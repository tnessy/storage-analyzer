//! Entry-count probe thread (T019).

/// Probe thread → main thread (plan D9).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeMsg {
    Estimate { total_entries: u64 },
}
