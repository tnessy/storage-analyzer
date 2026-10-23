//! State shared between the main thread and scan workers (design §3.11).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{PoisonError, RwLock};

/// Every mutation bumps `generation`, so workers skip the locks when nothing changed.
#[derive(Debug, Default)]
pub struct ScanControl {
    cancel_all: AtomicBool,
    cancelled_prefixes: RwLock<Vec<PathBuf>>,
    focus: RwLock<Option<PathBuf>>,
    generation: AtomicU64,
}

impl ScanControl {
    pub fn new() -> Self {
        Self::default()
    }

    fn bump(&self) {
        self.generation.fetch_add(1, Ordering::Release);
    }

    pub fn cancel_prefix(&self, p: PathBuf) {
        self.cancelled_prefixes
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .push(p);
        self.bump();
    }

    /// Drops every cancelled prefix at or under `p` (plan D2).
    pub fn remove_prefixes_under(&self, p: &Path) {
        self.cancelled_prefixes
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .retain(|prefix| !prefix.starts_with(p));
        self.bump();
    }

    /// Matches whole path components: `/a/b` covers `/a/b/c` but not `/a/bc`.
    pub fn is_cancelled(&self, p: &Path) -> bool {
        self.cancelled_prefixes
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .any(|prefix| p.starts_with(prefix))
    }

    pub fn set_focus(&self, p: Option<PathBuf>) {
        *self.focus.write().unwrap_or_else(PoisonError::into_inner) = p;
        self.bump();
    }

    pub fn focus(&self) -> Option<PathBuf> {
        self.focus
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub fn cancel_all(&self) {
        self.cancel_all.store(true, Ordering::Release);
        self.bump();
    }

    pub fn reset_cancel_all(&self) {
        self.cancel_all.store(false, Ordering::Release);
        self.bump();
    }

    pub fn is_cancel_all(&self) -> bool {
        self.cancel_all.load(Ordering::Acquire)
    }

    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_idle() {
        let c = ScanControl::new();
        assert!(!c.is_cancel_all());
        assert!(!c.is_cancelled(Path::new("/a")));
        assert_eq!(c.focus(), None);
        assert_eq!(c.generation(), 0);
    }

    #[test]
    fn prefix_matches_whole_components() {
        let c = ScanControl::new();
        c.cancel_prefix("/a/b".into());
        assert!(c.is_cancelled(Path::new("/a/b")));
        assert!(c.is_cancelled(Path::new("/a/b/c/d")));
        assert!(!c.is_cancelled(Path::new("/a/bc")));
        assert!(!c.is_cancelled(Path::new("/a")));
    }

    #[test]
    fn remove_prefixes_under_clears_nested_only() {
        let c = ScanControl::new();
        c.cancel_prefix("/a/b".into());
        c.cancel_prefix("/a/b/c".into());
        c.cancel_prefix("/a/bc".into());
        c.cancel_prefix("/z".into());
        c.remove_prefixes_under(Path::new("/a/b"));
        assert!(!c.is_cancelled(Path::new("/a/b")));
        assert!(!c.is_cancelled(Path::new("/a/b/c")));
        assert!(c.is_cancelled(Path::new("/a/bc")));
        assert!(c.is_cancelled(Path::new("/z/q")));
    }

    #[test]
    fn focus_roundtrip() {
        let c = ScanControl::new();
        c.set_focus(Some("/a".into()));
        assert_eq!(c.focus(), Some(PathBuf::from("/a")));
        c.set_focus(None);
        assert_eq!(c.focus(), None);
    }

    #[test]
    fn cancel_all_roundtrip() {
        let c = ScanControl::new();
        c.cancel_all();
        assert!(c.is_cancel_all());
        c.reset_cancel_all();
        assert!(!c.is_cancel_all());
    }

    #[test]
    fn every_mutation_bumps_generation() {
        let c = ScanControl::new();
        let mutations: [&dyn Fn(&ScanControl); 6] = [
            &|c| c.cancel_prefix("/a".into()),
            &|c| c.remove_prefixes_under(Path::new("/a")),
            &|c| c.set_focus(Some("/a".into())),
            &|c| c.set_focus(None),
            &|c| c.cancel_all(),
            &|c| c.reset_cancel_all(),
        ];
        let mut last = c.generation();
        for m in mutations {
            m(&c);
            assert!(c.generation() > last);
            last = c.generation();
        }
    }
}
