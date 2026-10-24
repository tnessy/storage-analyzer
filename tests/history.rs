//! The history file (T019): where it lives, how it is written, what it keeps.

use std::ffi::OsString;
use std::fs;

use storage_analyzer::APP_ID;
use storage_analyzer::history::{
    FileHistory, HistoryEntry, HistoryError, HistoryStore, MAX_ROOTS, lookup, state_dir, upsert,
};

fn entry(root: &str, entries: u64) -> HistoryEntry {
    HistoryEntry {
        root: root.into(),
        one_filesystem: true,
        entries,
        dirs: entries / 10,
        bytes: entries * 100,
        duration_ms: 1234,
    }
}

#[test]
fn state_dir_prefers_xdg_state_home_then_the_home_default() {
    let xdg = Some(OsString::from("/state"));
    let home = Some(OsString::from("/home/u"));
    assert_eq!(
        state_dir(xdg.clone(), home.clone())
            .unwrap()
            .to_str()
            .unwrap(),
        format!("/state/{APP_ID}")
    );
    assert_eq!(
        state_dir(None, home.clone()).unwrap().to_str().unwrap(),
        format!("/home/u/.local/state/{APP_ID}")
    );
    // An empty XDG_STATE_HOME counts as unset (XDG base directory spec).
    assert_eq!(
        state_dir(Some(OsString::new()), home)
            .unwrap()
            .to_str()
            .unwrap(),
        format!("/home/u/.local/state/{APP_ID}")
    );
    assert!(state_dir(None, None).is_none());
}

#[test]
fn a_missing_file_loads_as_empty() {
    let dir = tempfile::tempdir().unwrap();
    let store = FileHistory::new(dir.path().join(APP_ID));
    assert!(store.load().unwrap().is_empty());
}

#[test]
fn save_then_load_roundtrips_and_creates_the_directory() {
    let dir = tempfile::tempdir().unwrap();
    let store = FileHistory::new(dir.path().join("nested").join(APP_ID));
    let saved = vec![entry("/a", 10), entry("/b", 20)];
    store.save(&saved).unwrap();
    assert_eq!(store.load().unwrap(), saved);
}

#[test]
fn saving_is_atomic_and_leaves_no_temp_file() {
    let dir = tempfile::tempdir().unwrap();
    let store = FileHistory::new(dir.path().to_path_buf());
    store.save(&[entry("/a", 1)]).unwrap();
    store.save(&[entry("/a", 2)]).unwrap();
    let names: Vec<_> = fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["history.json"], "only the history file remains");
    assert_eq!(store.load().unwrap()[0].entries, 2);
}

#[test]
fn a_corrupt_file_is_reported_and_left_alone_until_the_next_save() {
    let dir = tempfile::tempdir().unwrap();
    let store = FileHistory::new(dir.path().to_path_buf());
    fs::write(store.path(), b"{ not json").unwrap();
    assert!(matches!(store.load(), Err(HistoryError::Corrupt(_))));
    assert_eq!(
        fs::read(store.path()).unwrap(),
        b"{ not json",
        "loading does not touch it"
    );
    store.save(&[entry("/a", 3)]).unwrap();
    assert_eq!(store.load().unwrap().len(), 1, "the next save replaces it");
}

#[test]
fn upsert_replaces_the_same_root_and_options_and_moves_it_last() {
    let mut list = vec![entry("/a", 1), entry("/b", 2)];
    upsert(&mut list, entry("/a", 99));
    assert_eq!(
        list.iter()
            .map(|e| (e.root.as_str(), e.entries))
            .collect::<Vec<_>>(),
        [("/b", 2), ("/a", 99)]
    );
    // The same root with different options is a different record.
    let mut other = entry("/a", 5);
    other.one_filesystem = false;
    upsert(&mut list, other);
    assert_eq!(list.len(), 3);
}

#[test]
fn the_file_keeps_at_most_fifty_roots_dropping_the_oldest() {
    let mut list = Vec::new();
    for i in 0..(MAX_ROOTS + 5) {
        upsert(&mut list, entry(&format!("/root{i}"), i as u64));
    }
    assert_eq!(list.len(), MAX_ROOTS);
    assert_eq!(list[0].root, "/root5", "the five oldest are gone");
    assert_eq!(list.last().unwrap().root, format!("/root{}", MAX_ROOTS + 4));
}

#[test]
fn lookup_matches_root_and_options() {
    let mut list = vec![entry("/a", 1)];
    let mut other = entry("/a", 7);
    other.one_filesystem = false;
    list.push(other);
    assert_eq!(lookup(&list, "/a", true).unwrap().entries, 1);
    assert_eq!(lookup(&list, "/a", false).unwrap().entries, 7);
    assert!(lookup(&list, "/b", true).is_none());
}
