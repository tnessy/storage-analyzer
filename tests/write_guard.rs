//! Fails if code outside `src/delete/**` and `src/history/**` can write to the filesystem (M9).
//!
//! The check is lexical: comments, string literals and char literals are blanked, then
//! identifier tokens and a few path/call shapes are matched. It does not parse Rust, so
//! it cannot see macro-generated calls or aliases (`use std::fs as f`).
//!
//! Known false positive: in a file that mentions `OpenOptions`, any `.write(` call
//! (for example `RwLock::write()`) is reported. Move that code or rename the call.

#[path = "support/src_tree.rs"]
mod src_tree;

use std::fs;
use std::path::Path;

use src_tree::{Literals, rs_files, scrub, write_tree};

const FORBIDDEN_IDENTS: &[&str] = &[
    "remove_file",
    "remove_dir",
    "remove_dir_all",
    "unlinkat",
    "rename",
    "renameat",
    "set_permissions",
    "create_dir",
    "create_dir_all",
    "hard_link",
    "soft_link",
    "symlink",
    "set_len",
    "set_times",
    "set_modified",
    "mkdirat",
    "linkat",
    "symlinkat",
    "fchmod",
    "fchmodat",
    "chmod",
    "ftruncate",
    "utimensat",
    "futimens",
];
const FORBIDDEN_PATHS: &[&str] = &["File::create", "File::options", "fs::write", "fs::copy"];
/// `std::fs` functions whose names are everyday words (`ui::copy` is a module here), so
/// they are matched as `fs::name`, as an import from `std::fs`, or as a bare call
/// after `use std::fs::*`.
const COMMON_FS_WRITERS: &[&str] = &["write", "copy"];
const WRITE_CALLS: &[&str] = &[".write(", ".append(", ".create(", ".create_new("];
/// Forbidden only in files that mention `OFlags` (they are common words otherwise).
const WRITE_OFLAGS: &[&str] = &["WRONLY", "RDWR", "CREATE", "TRUNC", "APPEND"];
const ALLOWED_DIRS: &[&str] = &["delete", "history"];

/// Returns one message per forbidden construct found under `src_root`.
fn violations(src_root: &Path) -> Vec<String> {
    let mut found = Vec::new();
    for file in rs_files(src_root) {
        let rel = file.strip_prefix(src_root).unwrap();
        let first = rel.components().next().unwrap().as_os_str();
        if ALLOWED_DIRS.iter().any(|d| first == *d) {
            continue;
        }
        let code = scrub(&fs::read_to_string(&file).unwrap(), Literals::Blank);
        let code = strip_test_modules(&code);
        for what in forbidden_in(&code) {
            found.push(format!("{}: {what}", rel.display()));
        }
    }
    found
}

fn identifiers(code: &str) -> Vec<&str> {
    code.split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .collect()
}

fn forbidden_in(code: &str) -> Vec<String> {
    let mut found = Vec::new();
    let idents = identifiers(code);
    let has = |word: &str| idents.contains(&word);
    for word in FORBIDDEN_IDENTS {
        if has(word) {
            found.push((*word).to_string());
        }
    }
    if has("OFlags") {
        for flag in WRITE_OFLAGS {
            if has(flag) {
                found.push(format!("OFlags::{flag}"));
            }
        }
    }
    let compact: String = code.chars().filter(|c| !c.is_whitespace()).collect();
    for path in FORBIDDEN_PATHS {
        if compact.contains(path) {
            found.push((*path).to_string());
        }
    }
    if has("OpenOptions") {
        for call in WRITE_CALLS {
            if compact.contains(call) {
                found.push(format!("OpenOptions{call}"));
            }
        }
    }
    found.extend(bare_fs_writers(&compact));
    found
}

/// Removes `#[cfg(test)] mod <name> { … }` blocks: test code does not ship and may build
/// fixtures. `mod <name>;` (a file module) and other cfg(test) items are left alone.
/// Expects literals and comments to be blanked already, so braces are balanced.
fn strip_test_modules(code: &str) -> String {
    let marker = "#[cfg(test)]";
    let mut out = String::new();
    let mut rest = code;
    while let Some(at) = rest.find(marker) {
        let after = &rest[at + marker.len()..];
        out.push_str(&rest[..at]);
        match test_module_body_len(after) {
            Some(len) => rest = &after[len..],
            None => {
                out.push_str(marker);
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// If `after` starts with `[pub] mod <ident> {`, the length up to and including the
/// matching `}`.
fn test_module_body_len(after: &str) -> Option<usize> {
    let head = after.trim_start();
    let head = head.strip_prefix("pub").map_or(head, str::trim_start);
    let head = head.strip_prefix("mod")?;
    let named = head.trim_start();
    if named.len() == head.len() {
        return None;
    }
    let ident_len = named
        .find(|c: char| !(c.is_alphanumeric() || c == '_'))
        .unwrap_or(named.len());
    let brace = named[ident_len..].trim_start();
    if ident_len == 0 || !brace.starts_with('{') {
        return None;
    }
    let open = after.len() - brace.len();
    let mut depth = 0;
    for (i, c) in after.char_indices().skip_while(|&(i, _)| i < open) {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i + 1);
                }
            }
            _ => {}
        }
    }
    Some(after.len())
}

/// Common-word writers (`write`, `copy`) imported from `std::fs`, or a glob import of
/// `std::fs` together with a bare call.
fn bare_fs_writers(compact: &str) -> Vec<String> {
    let mut found = Vec::new();
    for stmt in compact.split(';') {
        let Some(imported) = stmt.split("usestd::fs::").nth(1) else {
            continue;
        };
        let items: Vec<&str> = imported
            .trim_matches(|c| c == '{' || c == '}')
            .split(',')
            .collect();
        for name in COMMON_FS_WRITERS {
            let aliased = format!("{name}as");
            if items.iter().any(|i| i == name || i.starts_with(&aliased)) {
                found.push(format!("use std::fs::{name}"));
            }
            if items.contains(&"*") && has_bare_call(compact, name) {
                found.push(format!("{name}( via use std::fs::*"));
            }
        }
    }
    found
}

fn has_bare_call(compact: &str, name: &str) -> bool {
    let call = format!("{name}(");
    compact.match_indices(&call).any(|(at, _)| {
        let before = compact[..at].chars().next_back();
        !before.is_some_and(|c| c.is_alphanumeric() || matches!(c, '_' | '.' | ':'))
    })
}

#[test]
fn repo_sources_pass() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    assert_eq!(violations(&src), Vec::<String>::new());
}

#[test]
fn planted_violation_outside_allowed_modules_fails() {
    let dir = write_tree(&[(
        "scan/x.rs",
        "fn f() { std::fs::remove_file(\"a\").unwrap(); }",
    )]);
    let found = violations(dir.path());
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("scan/x.rs") && found[0].contains("remove_file"));
}

#[test]
fn same_call_inside_delete_or_history_passes() {
    let dir = write_tree(&[
        (
            "delete/x.rs",
            "fn f() { std::fs::remove_file(\"a\").unwrap(); }",
        ),
        (
            "history/y.rs",
            "fn f() { std::fs::rename(\"a\", \"b\").unwrap(); }",
        ),
    ]);
    assert!(violations(dir.path()).is_empty());
}

#[test]
fn forbidden_words_in_comments_and_strings_pass() {
    let dir = write_tree(&[(
        "ui/x.rs",
        "// remove_file is forbidden\n/* unlinkat\n rename */\n/// fs::write\nfn f() { let _ = \"File::create rename\"; }",
    )]);
    assert!(violations(dir.path()).is_empty());
}

#[test]
fn every_forbidden_form_is_caught() {
    // (code, text the report must mention)
    let cases = [
        ("std::fs::remove_dir(p)", "remove_dir"),
        ("std::fs::remove_dir_all(p)", "remove_dir_all"),
        ("rustix::fs::unlinkat(a, b, c)", "unlinkat"),
        ("std::fs::File::create(p)", "File::create"),
        ("std::fs::rename(a, b)", "rename"),
        ("rustix::fs::renameat(a, b, c, d)", "renameat"),
        ("std::fs::write(p, d)", "fs::write"),
        ("std::fs::set_permissions(p, q)", "set_permissions"),
        (
            "std::fs::OpenOptions::new().write(true).open(p)",
            "OpenOptions.write(",
        ),
        (
            "std::fs::OpenOptions::new().append(true).open(p)",
            "OpenOptions.append(",
        ),
        (
            "std::fs::OpenOptions::new().create(true).open(p)",
            "OpenOptions.create(",
        ),
        ("std::fs::File::options()", "File::options"),
        ("std::fs::File::create_new(p)", "File::create"),
        ("std::fs::create_dir(p)", "create_dir"),
        ("std::fs::create_dir_all(p)", "create_dir_all"),
        ("std::fs::copy(a, b)", "copy"),
        ("std::fs::hard_link(a, b)", "hard_link"),
        ("std::fs::soft_link(a, b)", "soft_link"),
        ("std::os::unix::fs::symlink(a, b)", "symlink"),
        ("f.set_len(0)", "set_len"),
        ("f.set_times(t)", "set_times"),
        ("f.set_modified(t)", "set_modified"),
        ("rustix::fs::mkdirat(a, b, m)", "mkdirat"),
        ("rustix::fs::linkat(a, b, c, d, e)", "linkat"),
        ("rustix::fs::symlinkat(a, b, c)", "symlinkat"),
        ("rustix::fs::fchmod(a, m)", "fchmod"),
        ("rustix::fs::fchmodat(a, b, m, f)", "fchmodat"),
        ("rustix::fs::chmod(a, m)", "chmod"),
        ("rustix::fs::ftruncate(a, 0)", "ftruncate"),
        ("rustix::fs::utimensat(a, b, t, f)", "utimensat"),
        ("rustix::fs::futimens(a, t)", "futimens"),
        ("let _ = OFlags::WRONLY", "OFlags::WRONLY"),
        ("let _ = OFlags::RDWR", "OFlags::RDWR"),
        ("let _ = OFlags::CREATE", "OFlags::CREATE"),
        ("let _ = OFlags::TRUNC", "OFlags::TRUNC"),
        ("let _ = OFlags::APPEND", "OFlags::APPEND"),
        ("let _ = OFlags::RDONLY | WRONLY", "OFlags::WRONLY"),
        ("use std::fs::write; write(p, d)", "use std::fs::write"),
        (
            "use std::fs::{self, write}; write(p, d)",
            "use std::fs::write",
        ),
        ("use std::fs::*; write(p, d)", "use std::fs::*"),
        ("use std::fs::copy; copy(a, b)", "use std::fs::copy"),
        ("use std::fs::*; copy(a, b)", "copy( via use std::fs::*"),
    ];
    for (case, needle) in cases {
        let dir = write_tree(&[("model/x.rs", &format!("fn f() {{ {case}; }}"))]);
        let found = violations(dir.path());
        assert!(
            found.iter().any(|m| m.contains(needle)),
            "not caught: {case} (wanted {needle}, got {found:?})"
        );
    }
}

#[test]
fn read_only_open_options_pass() {
    let dir = write_tree(&[(
        "scan/x.rs",
        "fn f() { OpenOptions::new().read(true).open(p); }",
    )]);
    assert!(violations(dir.path()).is_empty());
}

#[test]
fn read_only_open_flags_pass() {
    let dir = write_tree(&[(
        "scan/x.rs",
        "use rustix::fs::OFlags; fn f() { let _ = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW; }",
    )]);
    assert!(violations(dir.path()).is_empty());
}

#[test]
fn unrelated_write_copy_and_create_names_pass() {
    let dir = write_tree(&[(
        "ui/x.rs",
        "#[derive(Clone, Copy)] struct S;\n\
         fn f(l: &RwLock<u8>) { let _g = l.write(); let _ = std::fs::read(p); let _ = CREATE; }\n\
         use std::fs::{self, read};",
    )]);
    assert!(violations(dir.path()).is_empty());
}

#[test]
fn copy_as_a_module_or_derive_passes() {
    let dir = write_tree(&[(
        "ui/x.rs",
        "use super::copy; fn f() { let _ = copy::STARTING; }",
    )]);
    assert!(violations(dir.path()).is_empty());
}

#[test]
fn unit_test_modules_may_build_fixtures_but_production_code_may_not() {
    let test_only = "fn f() {}\n#[cfg(test)]\nmod tests {\n fn t() { std::fs::write(p, d); if x { } std::fs::create_dir(p); }\n}\n";
    let dir = write_tree(&[("app/x.rs", test_only)]);
    assert!(violations(dir.path()).is_empty());

    let after = format!("{test_only}fn g() {{ std::fs::remove_file(p); }}");
    let before = format!("fn g() {{ std::fs::remove_file(p); }}\n{test_only}");
    for code in [after, before] {
        let dir = write_tree(&[("app/x.rs", &code)]);
        assert_eq!(violations(dir.path()).len(), 1, "{code}");
    }
}

#[test]
fn a_file_module_marked_cfg_test_does_not_hide_the_next_item() {
    let code = "#[cfg(test)]\nmod tests;\n\npub fn evil(p: &Path) { std::fs::remove_file(p); }\n";
    let dir = write_tree(&[("scan/zz.rs", code)]);
    assert_eq!(violations(dir.path()).len(), 1);
}

#[test]
fn braces_of_a_stripped_test_module_end_where_it_ends() {
    let code = "#[cfg(test)]\nmod t { fn f() { std::fs::write(p, d); } }\n";
    let dir = write_tree(&[("scan/a.rs", code)]);
    assert!(violations(dir.path()).is_empty());
    let code = "#[cfg(test)]\npub mod t { fn f() { if x { std::fs::write(p, d); } } }\nfn g() { std::fs::rename(a, b); }\n";
    let dir = write_tree(&[("scan/b.rs", code)]);
    assert_eq!(violations(dir.path()).len(), 1);
}

#[test]
fn cfg_test_on_other_items_does_not_exempt_them() {
    let code = "#[cfg(test)]\nfn helper() { std::fs::remove_dir(p); }\n";
    let dir = write_tree(&[("scan/c.rs", code)]);
    assert_eq!(violations(dir.path()).len(), 1);
}

#[test]
fn bare_write_without_an_fs_import_passes() {
    let dir = write_tree(&[("ui/x.rs", "fn write(x: u8) {} fn f() { write(1); }")]);
    assert!(violations(dir.path()).is_empty());
}

#[test]
fn comment_marker_inside_string_does_not_hide_a_violation() {
    let dir = write_tree(&[(
        "scan/x.rs",
        "fn f() { let _u = \"http://x\"; std::fs::remove_file(p); }",
    )]);
    assert_eq!(violations(dir.path()).len(), 1);
}

#[test]
fn scrub_blanks_literals_and_drops_comments() {
    let blank = |s| scrub(s, Literals::Blank);
    assert_eq!(blank("a // c \"x\"\nb"), "a \nb");
    assert_eq!(blank("x /* a /* b */ c */ y"), "x  y");
    assert_eq!(blank("let s = \"a//b\"; // c"), "let s =  ; ");
    assert_eq!(blank("r#\"x\"y\"# z"), "  z");
    assert_eq!(blank("c = '\"'; d = '\\n';"), "c =  ; d =  ;");
    assert_eq!(blank("&'a str"), "&'a str");
    assert_eq!(
        blank("\"multi\nline\" x"),
        "\n  x",
        "line breaks inside literals survive"
    );
}
