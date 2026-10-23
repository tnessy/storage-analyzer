//! Fails if the `APP_ID` value is spelled out anywhere under `src/` except its definition.

use std::fs;
use std::path::{Path, PathBuf};

use storage_analyzer::APP_ID;

/// Returns `file:line` for each line under `src_root` whose code part contains `app_id`,
/// except the line defining `APP_ID`.
fn literal_uses(src_root: &Path, app_id: &str) -> Vec<String> {
    let mut files = Vec::new();
    collect_rs(src_root, &mut files);
    files.sort();
    let mut found = Vec::new();
    for file in files {
        let rel = file.strip_prefix(src_root).unwrap();
        for (n, line) in fs::read_to_string(&file).unwrap().lines().enumerate() {
            let code = line.split("//").next().unwrap();
            if code.contains(app_id) && !code.contains("const APP_ID") {
                found.push(format!("{}:{}", rel.display(), n + 1));
            }
        }
    }
    found
}

fn collect_rs(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect_rs(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

fn write_tree(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (rel, body) in files {
        let path = dir.path().join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, body).unwrap();
    }
    dir
}

#[test]
fn repo_sources_pass() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    assert_eq!(literal_uses(&src, APP_ID), Vec::<String>::new());
}

#[test]
fn planted_literal_fails() {
    let dir = write_tree(&[
        ("lib.rs", "pub const APP_ID: &str = \"tool-x\";\n"),
        ("ui/msg.rs", "const USAGE: &str = \"usage: tool-x PATH\";\n"),
    ]);
    let found = literal_uses(dir.path(), "tool-x");
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("ui/msg.rs:1"));
}

#[test]
fn definition_and_comments_pass() {
    let dir = write_tree(&[
        ("lib.rs", "pub const APP_ID: &str = \"tool-x\";\n"),
        ("ui/msg.rs", "// run tool-x\n/// tool-x help\nfn f() {}\n"),
    ]);
    assert!(literal_uses(dir.path(), "tool-x").is_empty());
}

#[test]
fn second_definition_like_line_still_fails() {
    let dir = write_tree(&[
        ("lib.rs", "pub const APP_ID: &str = \"tool-x\";\n"),
        ("ui/msg.rs", "const NAME: &str = \"tool-x\";\n"),
    ]);
    assert_eq!(literal_uses(dir.path(), "tool-x").len(), 1);
}
