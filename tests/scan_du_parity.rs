//! M8 / M9 / M10 end to end: real files -> walker -> model, compared with `du` (T007).
//!
//! Runs on the default temp dir, and on btrfs too when `SA_TEST_BTRFS_DIR` names a
//! writable directory there.

#[path = "support/fixtures.rs"]
mod fixtures;

use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use fixtures::{du_with, locations, m8_tree, scan_to_done};
use storage_analyzer::model::{Flags, Model};
use storage_analyzer::types::{IssueKind, Kind, NodeId, Op, SizeMode};

const D: SizeMode = SizeMode::Disk;
const A: SizeMode = SizeMode::Apparent;

fn all_nodes(m: &Model) -> impl Iterator<Item = NodeId> + '_ {
    (0..=m.totals().entries_seen as u32)
        .map(NodeId)
        .filter(|&n| m.exists(n))
}

fn find_by_name(m: &Model, name: &[u8]) -> Option<NodeId> {
    all_nodes(m).find(|&n| m.name(n) == name)
}

#[test]
fn totals_equal_du_in_disk_apparent_and_one_filesystem_modes() {
    for (place, base) in locations() {
        for threads in [1, 4] {
            let tree = m8_tree(base.as_deref());
            let m = scan_to_done(&tree.root, threads, true);
            let at = format!("{place}, {threads} thread(s)");
            assert_eq!(
                m.size(NodeId::ROOT, D),
                du_with(&tree.root, false, false),
                "disk {at}"
            );
            assert_eq!(
                m.size(NodeId::ROOT, D),
                du_with(&tree.root, false, true),
                "disk -x {at}"
            );
            assert_eq!(
                m.size(NodeId::ROOT, A),
                du_with(&tree.root, true, false),
                "apparent {at}"
            );
            assert_eq!(
                m.size(NodeId::ROOT, A),
                du_with(&tree.root, true, true),
                "apparent -x {at}"
            );
            assert!(m.is_complete(NodeId::ROOT), "{at}");
        }
    }
}

#[test]
fn crossing_filesystems_allowed_changes_nothing_on_a_single_filesystem() {
    let tree = m8_tree(None);
    let one = scan_to_done(&tree.root, 4, true);
    let all = scan_to_done(&tree.root, 4, false);
    assert_eq!(one.size(NodeId::ROOT, D), all.size(NodeId::ROOT, D));
    assert_eq!(one.counts(NodeId::ROOT), all.counts(NodeId::ROOT));
}

#[test]
fn directories_have_no_apparent_size_like_du() {
    // du --apparent-size leaves directories out; on btrfs they do have a st_size.
    for (place, base) in locations() {
        let tree = m8_tree(base.as_deref());
        let dir_len = fs::symlink_metadata(tree.root.join("plain")).unwrap().len();
        println!("{place}: st_size of a directory is {dir_len}");
        let m = scan_to_done(&tree.root, 2, true);
        let plain = find_by_name(&m, b"plain").unwrap();
        let files: u64 = m
            .children(plain)
            .iter()
            .filter(|&&c| m.kind(c) == Kind::File)
            .map(|&c| m.size(c, A))
            .sum();
        assert_eq!(
            m.size(plain, A),
            files,
            "{place}: only files add apparent bytes"
        );
    }
}

#[test]
fn hard_links_are_counted_once_with_one_duplicate_per_extra_link() {
    let tree = m8_tree(None);
    let m = scan_to_done(&tree.root, 4, true);
    let dups = all_nodes(&m)
        .filter(|&n| m.flags(n).contains(Flags::HARDLINK_DUP))
        .count();
    assert_eq!(dups, tree.extra_links);
    for n in all_nodes(&m).filter(|&n| m.flags(n).contains(Flags::HARDLINK_DUP)) {
        assert_eq!(m.size(n, D), 0);
        assert!(m.nlink(n) > 1);
    }
    // Per inode: all links but one are duplicates.
    let mut groups: std::collections::HashMap<(u64, u64), (usize, usize)> = Default::default();
    for n in all_nodes(&m).filter(|&n| m.kind(n) == Kind::File && m.nlink(n) > 1) {
        let g = groups.entry(m.identity(n)).or_default();
        g.0 += 1;
        g.1 += usize::from(m.flags(n).contains(Flags::HARDLINK_DUP));
    }
    assert_eq!(groups.len(), 2);
    for (links, dups) in groups.values() {
        assert_eq!(*dups, links - 1);
    }
}

#[test]
fn symlinks_are_leaves_and_never_followed() {
    let tree = m8_tree(None);
    let m = scan_to_done(&tree.root, 4, true);
    let links: Vec<NodeId> = all_nodes(&m)
        .filter(|&n| m.kind(n) == Kind::Symlink)
        .collect();
    assert_eq!(links.len(), 8);
    for n in links {
        assert!(m.children(n).is_empty());
        assert!(m.is_complete(n));
    }
    assert!(
        find_by_name(&m, b"big.dat").is_none(),
        "the file outside the root is not scanned"
    );
    assert!(all_nodes(&m).filter(|&n| m.name(n) == b"b.bin").count() == 1);
}

#[test]
fn awkward_names_and_deep_nesting_survive() {
    let tree = m8_tree(None);
    let m = scan_to_done(&tree.root, 4, true);
    let bad = find_by_name(&m, b"bad\xff\xfename").expect("invalid UTF-8 name");
    assert_eq!(m.size(bad, A), 5000);
    let newline = find_by_name(&m, b"line\nbreak").expect("newline name");
    assert_eq!(m.size(newline, A), 70);
    let expected = tree
        .root
        .join("weird")
        .join(<std::ffi::OsStr as std::os::unix::ffi::OsStrExt>::from_bytes(b"bad\xff\xfename"));
    assert_eq!(m.path(bad), expected);
    let bottom = find_by_name(&m, b"bottom.txt").unwrap();
    assert_eq!(m.depth(bottom), 202);
    assert_eq!(m.size(bottom, A), 300);
    assert_eq!(m.path(bottom), tree.root.join(deep_rel()));
}

fn deep_rel() -> PathBuf {
    let mut p = PathBuf::from("deep");
    for _ in 0..200 {
        p.push("d");
    }
    p.push("bottom.txt");
    p
}

#[test]
fn an_unreadable_directory_is_one_issue_and_the_rest_completes() {
    let tree = m8_tree(None);
    if tree.locked.is_none() {
        println!("SKIP: permissions are not enforced for root");
        return;
    }
    let m = scan_to_done(&tree.root, 4, true);
    assert_eq!(m.issues().len(), 1, "{:?}", m.issues());
    let issue = m.issues()[0];
    assert_eq!(
        (issue.op, issue.kind),
        (Op::Open, IssueKind::PermissionDenied)
    );
    assert_eq!(m.rel_path(issue.node), PathBuf::from("locked"));
    assert!(m.is_complete(NodeId::ROOT));
    assert!(m.children(issue.node).is_empty());
}

/// Every entry's path, size, kind, mode and times; unreadable directories are not entered.
fn tree_state(root: &Path) -> Vec<(PathBuf, u64, u32, i64, i64, i64, i64)> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for e in entries {
            let path = e.unwrap().path();
            let m = fs::symlink_metadata(&path).unwrap();
            out.push((
                path.clone(),
                m.len(),
                m.mode(),
                m.atime(),
                m.atime_nsec(),
                m.mtime(),
                m.mtime_nsec(),
            ));
            if m.is_dir() {
                stack.push(path);
            }
        }
    }
    out.sort();
    out
}

#[test]
fn scanning_changes_no_access_or_modification_time() {
    for (place, base) in locations() {
        let tree = m8_tree(base.as_deref());
        let before = tree_state(&tree.root);
        let _ = scan_to_done(&tree.root, 4, true);
        let after = tree_state(&tree.root);
        assert_eq!(before.len(), after.len(), "{place}");
        assert_eq!(
            before, after,
            "{place}: atime/mtime/size/mode changed by the scan"
        );
    }
}
