//! Helpers shared by the tests that scan `src/` lexically (`write_guard`, `no_name_literal`).
#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};

/// All `.rs` files under `dir`, sorted.
pub fn rs_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_rs(dir, &mut files);
    files.sort();
    files
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

/// A temp source tree: `(relative path, contents)` pairs.
pub fn write_tree(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (rel, body) in files {
        let path = dir.path().join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, body).unwrap();
    }
    dir
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Literals {
    Keep,
    Blank,
}

/// Removes comments and, with `Literals::Blank`, replaces string and char literals by a
/// space. Line breaks are always kept, so line numbers survive.
pub fn scrub(src: &str, literals: Literals) -> String {
    let b: Vec<char> = src.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < b.len() {
        let (end, is_comment) = match token_end(&b, i) {
            Some(t) => t,
            None => {
                out.push(b[i]);
                i += 1;
                continue;
            }
        };
        for &c in &b[i..end] {
            if c == '\n' {
                out.push('\n');
            } else if !is_comment && literals == Literals::Keep {
                out.push(c);
            }
        }
        if !is_comment && literals == Literals::Blank {
            out.push(' ');
        }
        i = end;
    }
    out
}

/// If a comment or literal starts at `i`: where it ends, and whether it is a comment.
fn token_end(b: &[char], i: usize) -> Option<(usize, bool)> {
    let at = |k: usize| b.get(k).copied().unwrap_or('\0');
    match b[i] {
        '/' if at(i + 1) == '/' => {
            let end = b[i..]
                .iter()
                .position(|&c| c == '\n')
                .map_or(b.len(), |p| i + p);
            Some((end, true))
        }
        '/' if at(i + 1) == '*' => Some((block_comment_end(b, i), true)),
        'r' if raw_string_end(b, i).is_some() && !continues_identifier(b, i) => {
            raw_string_end(b, i).map(|end| (end, false))
        }
        '"' => Some((string_end(b, i), false)),
        '\'' if at(i + 1) == '\\' => {
            let end = b[i + 2..]
                .iter()
                .position(|&c| c == '\'')
                .map_or(b.len(), |p| i + 3 + p);
            Some((end, false))
        }
        '\'' if at(i + 2) == '\'' => Some((i + 3, false)),
        _ => None,
    }
}

/// True if the `r` at `i` is the tail of an identifier (but `br"…"` is a raw byte string).
fn continues_identifier(b: &[char], i: usize) -> bool {
    let is_ident = |c: char| c.is_alphanumeric() || c == '_';
    match i.checked_sub(1).map(|p| b[p]) {
        Some('b') => i >= 2 && is_ident(b[i - 2]),
        Some(c) => is_ident(c),
        None => false,
    }
}

fn block_comment_end(b: &[char], start: usize) -> usize {
    let at = |k: usize| b.get(k).copied().unwrap_or('\0');
    let (mut i, mut depth) = (start, 0);
    while i < b.len() {
        if b[i] == '/' && at(i + 1) == '*' {
            depth += 1;
            i += 2;
        } else if b[i] == '*' && at(i + 1) == '/' {
            depth -= 1;
            i += 2;
            if depth == 0 {
                return i;
            }
        } else {
            i += 1;
        }
    }
    b.len()
}

fn string_end(b: &[char], start: usize) -> usize {
    let mut i = start + 1;
    while i < b.len() && b[i] != '"' {
        i += if b[i] == '\\' { 2 } else { 1 };
    }
    (i + 1).min(b.len())
}

/// End of the raw string `r#"…"#` starting at `start`, or `None` if there isn't one.
fn raw_string_end(b: &[char], start: usize) -> Option<usize> {
    let hashes = b[start + 1..].iter().take_while(|&&c| c == '#').count();
    if b.get(start + 1 + hashes) != Some(&'"') {
        return None;
    }
    let mut i = start + 2 + hashes;
    while i < b.len() {
        if b[i] == '"' && (1..=hashes).all(|k| b.get(i + k) == Some(&'#')) {
            return Some(i + 1 + hashes);
        }
        i += 1;
    }
    Some(b.len())
}
