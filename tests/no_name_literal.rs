//! Fails if the `APP_ID` value is spelled out anywhere under `src/` except its definition.
//!
//! Only the exact line `pub const APP_ID: &str = "<value>";` in `src/lib.rs` is exempt.
//! Comments are ignored; string literals are not, even if they contain `//`.

#[path = "support/src_tree.rs"]
mod src_tree;

use std::fs;
use std::path::Path;

use src_tree::{Literals, rs_files, scrub, write_tree};
use storage_analyzer::APP_ID;

/// Returns `file:line` for each line under `src_root` whose code (comments removed)
/// contains `app_id`, except the definition line in `lib.rs`.
fn literal_uses(src_root: &Path, app_id: &str) -> Vec<String> {
    let definition = format!("pub const APP_ID: &str = \"{app_id}\";");
    let mut found = Vec::new();
    for file in rs_files(src_root) {
        let rel = file.strip_prefix(src_root).unwrap();
        let code = scrub(&fs::read_to_string(&file).unwrap(), Literals::Keep);
        for (n, line) in code.lines().enumerate() {
            let is_definition = rel == Path::new("lib.rs") && line.trim() == definition;
            if line.contains(app_id) && !is_definition {
                found.push(format!("{}:{}", rel.display(), n + 1));
            }
        }
    }
    found
}

const DEFINITION: &str = "pub const APP_ID: &str = \"tool-x\";\n";

#[test]
fn repo_sources_pass() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    assert_eq!(literal_uses(&src, APP_ID), Vec::<String>::new());
}

#[test]
fn planted_literal_fails() {
    let dir = write_tree(&[
        ("lib.rs", DEFINITION),
        ("ui/msg.rs", "const USAGE: &str = \"usage: tool-x PATH\";\n"),
    ]);
    assert_eq!(
        literal_uses(dir.path(), "tool-x"),
        vec!["ui/msg.rs:1".to_string()]
    );
}

#[test]
fn definition_and_comments_pass() {
    let dir = write_tree(&[
        ("lib.rs", DEFINITION),
        ("ui/msg.rs", "// run tool-x\n/// tool-x help\nfn f() {}\n"),
    ]);
    assert!(literal_uses(dir.path(), "tool-x").is_empty());
}

#[test]
fn another_constant_with_the_value_still_fails() {
    let dir = write_tree(&[
        ("lib.rs", DEFINITION),
        ("ui/msg.rs", "const NAME: &str = \"tool-x\";\n"),
    ]);
    assert_eq!(literal_uses(dir.path(), "tool-x").len(), 1);
}

#[test]
fn only_the_exact_definition_in_lib_rs_is_exempt() {
    let dir = write_tree(&[(
        "lib.rs",
        &format!("{DEFINITION}const APP_ID_X: &str = \"tool-x\";\n"),
    )]);
    assert_eq!(
        literal_uses(dir.path(), "tool-x"),
        vec!["lib.rs:2".to_string()]
    );

    let dir = write_tree(&[("lib.rs", DEFINITION), ("ui/x.rs", DEFINITION)]);
    assert_eq!(
        literal_uses(dir.path(), "tool-x"),
        vec!["ui/x.rs:1".to_string()]
    );
}

#[test]
fn slashes_inside_a_string_do_not_hide_the_literal() {
    let dir = write_tree(&[("ui/x.rs", "const U: &str = \"https://x tool-x\";\n")]);
    assert_eq!(literal_uses(dir.path(), "tool-x").len(), 1);
}

#[test]
fn block_comments_and_trailing_comments_pass() {
    let dir = write_tree(&[(
        "ui/x.rs",
        "/* tool-x\n tool-x */\nfn f() {} // tool-x\nfn g() { let _ = \"a\"; /* tool-x */ }\n",
    )]);
    assert!(literal_uses(dir.path(), "tool-x").is_empty());
}
