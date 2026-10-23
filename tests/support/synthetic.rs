//! Synthetic `ScanMsg` streams and fixtures shared by the app tests.

#![allow(dead_code)]

use storage_analyzer::model::{Model, ModelOptions};
use storage_analyzer::scan::{RawEntry, ScanMsg};
use storage_analyzer::types::{Kind, NodeId};

pub const ROOT_PATH: &str = "/synthetic/root";

pub fn entry(name: &str, kind: Kind, bytes: u64) -> RawEntry {
    RawEntry {
        name: name.as_bytes().into(),
        kind,
        disk_bytes: bytes,
        apparent_bytes: bytes,
        ino: 1,
        dev: 1,
        nlink: 1,
        issue: None,
    }
}

pub fn root_entry() -> RawEntry {
    entry(ROOT_PATH, Kind::Dir, 4096)
}

pub fn model() -> Model {
    Model::new(ROOT_PATH.into(), root_entry(), ModelOptions { top_n: 100 })
}

pub fn listing(dir: NodeId, first_id: u32, entries: Vec<RawEntry>) -> ScanMsg {
    ScanMsg::Listing {
        dir,
        first_id: NodeId(first_id),
        entries,
    }
}

pub fn done(cancelled: bool) -> ScanMsg {
    ScanMsg::Done { cancelled }
}

/// A small finished scan: root with a directory and a file.
pub fn small_tree() -> Vec<ScanMsg> {
    vec![
        listing(
            NodeId::ROOT,
            1,
            vec![
                entry("docs", Kind::Dir, 4096),
                entry("a.bin", Kind::File, 10_000),
            ],
        ),
        listing(NodeId(1), 3, vec![entry("b.txt", Kind::File, 500)]),
    ]
}

/// `total_dirs + 1000 + 1` small messages (`total_dirs` must be a multiple of 1,000): the root lists
/// 1,000 directories, each of those lists `total_dirs / 1000` directories, and each of those lists
/// one file. No single message is large.
pub fn flood(total_dirs: u32) -> impl Iterator<Item = ScanMsg> {
    const TOP: u32 = 1000;
    let per_top = total_dirs / TOP;
    let top = (0..TOP)
        .map(|i| entry(&format!("t{i}"), Kind::Dir, 4096))
        .collect();
    let level2 = (0..TOP).map(move |t| {
        let dirs = (0..per_top)
            .map(|i| entry(&format!("d{i}"), Kind::Dir, 4096))
            .collect();
        listing(NodeId(1 + t), 1 + TOP + t * per_top, dirs)
    });
    let files = (0..total_dirs).map(move |d| {
        listing(
            NodeId(1 + TOP + d),
            1 + TOP + total_dirs + d,
            vec![entry("f", Kind::File, 100)],
        )
    });
    std::iter::once(listing(NodeId::ROOT, 1, top))
        .chain(level2)
        .chain(files)
}

/// An endless stream of cheap messages (re-listing an already listed directory), for sustained load.
pub fn endless() -> impl Iterator<Item = ScanMsg> {
    std::iter::repeat_with(|| listing(NodeId::ROOT, 1, Vec::new()))
}
