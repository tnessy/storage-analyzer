//! Fails if code outside `src/delete/**` and `src/history/**` can write to the filesystem (M9).

use std::fs;
use std::path::Path;

const FORBIDDEN_IDENTS: &[&str] = &[
    "remove_file",
    "remove_dir",
    "remove_dir_all",
    "unlinkat",
    "rename",
    "renameat",
    "set_permissions",
];
const FORBIDDEN_PATHS: &[&str] = &["File::create", "fs::write"];
const WRITE_CALLS: &[&str] = &[".write(", ".append(", ".create(", ".create_new("];
const ALLOWED_DIRS: &[&str] = &["delete", "history"];

/// Returns one message per forbidden construct found under `src_root`.
fn violations(src_root: &Path) -> Vec<String> {
    let mut files = Vec::new();
    collect_rs(src_root, &mut files);
    files.sort();
    let mut found = Vec::new();
    for file in files {
        let rel = file.strip_prefix(src_root).unwrap();
        let first = rel.components().next().unwrap().as_os_str();
        if ALLOWED_DIRS.iter().any(|d| first == *d) {
            continue;
        }
        let code = blank_comments_and_literals(&fs::read_to_string(&file).unwrap());
        for what in forbidden_in(&code) {
            found.push(format!("{}: {what}", rel.display()));
        }
    }
    found
}

fn collect_rs(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect_rs(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

fn forbidden_in(code: &str) -> Vec<String> {
    let mut found = Vec::new();
    let idents: Vec<&str> = code
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .collect();
    for word in FORBIDDEN_IDENTS {
        if idents.contains(word) {
            found.push((*word).to_string());
        }
    }
    let compact: String = code.chars().filter(|c| !c.is_whitespace()).collect();
    for path in FORBIDDEN_PATHS {
        if compact.contains(path) {
            found.push((*path).to_string());
        }
    }
    if compact.contains("OpenOptions") {
        for call in WRITE_CALLS {
            if compact.contains(call) {
                found.push(format!("OpenOptions{call}"));
            }
        }
    }
    found
}

/// Replaces comments, string literals and char literals with blanks, keeping newlines.
fn blank_comments_and_literals(src: &str) -> String {
    let b: Vec<char> = src.chars().collect();
    let at = |i: usize| b.get(i).copied().unwrap_or('\0');
    let mut out = String::new();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c == '/' && at(i + 1) == '/' {
            while i < b.len() && b[i] != '\n' {
                i += 1;
            }
        } else if c == '/' && at(i + 1) == '*' {
            let mut depth = 0;
            while i < b.len() {
                if b[i] == '/' && at(i + 1) == '*' {
                    depth += 1;
                    i += 2;
                } else if b[i] == '*' && at(i + 1) == '/' {
                    depth -= 1;
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    if b[i] == '\n' {
                        out.push('\n');
                    }
                    i += 1;
                }
            }
        } else if c == 'r'
            && matches!(at(i + 1), '"' | '#')
            && !out.ends_with(|p: char| p.is_alphanumeric() || p == '_')
            && raw_string_hashes(&b, i + 1).is_some()
        {
            let hashes = raw_string_hashes(&b, i + 1).unwrap();
            i += 2 + hashes;
            while i < b.len() && !(b[i] == '"' && (1..=hashes).all(|k| at(i + k) == '#')) {
                i += 1;
            }
            i += 1 + hashes;
            out.push(' ');
        } else if c == '"' {
            i += 1;
            while i < b.len() && b[i] != '"' {
                i += if b[i] == '\\' { 2 } else { 1 };
            }
            i += 1;
            out.push(' ');
        } else if c == '\'' && at(i + 1) == '\\' {
            i += 2;
            while i < b.len() && b[i] != '\'' {
                i += 1;
            }
            i += 1;
            out.push(' ');
        } else if c == '\'' && at(i + 2) == '\'' {
            i += 3;
            out.push(' ');
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
}

/// Number of `#` after an `r` that opens a raw string, or `None` if it isn't one.
fn raw_string_hashes(b: &[char], from: usize) -> Option<usize> {
    let hashes = b[from..].iter().take_while(|&&c| c == '#').count();
    (b.get(from + hashes) == Some(&'"')).then_some(hashes)
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
    let cases = [
        "std::fs::remove_dir(p)",
        "std::fs::remove_dir_all(p)",
        "rustix::fs::unlinkat(a, b, c)",
        "std::fs::File::create(p)",
        "std::fs::rename(a, b)",
        "rustix::fs::renameat(a, b, c, d)",
        "std::fs::write(p, d)",
        "std::fs::set_permissions(p, q)",
        "std::fs::OpenOptions::new().write(true).open(p)",
        "std::fs::OpenOptions::new().append(true).open(p)",
        "std::fs::OpenOptions::new().create(true).open(p)",
    ];
    for case in cases {
        let dir = write_tree(&[("model/x.rs", &format!("fn f() {{ {case}; }}"))]);
        assert_eq!(violations(dir.path()).len(), 1, "not caught: {case}");
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
fn comment_marker_inside_string_does_not_hide_a_violation() {
    let dir = write_tree(&[(
        "scan/x.rs",
        "fn f() { let _u = \"http://x\"; std::fs::remove_file(p); }",
    )]);
    assert_eq!(violations(dir.path()).len(), 1);
}
