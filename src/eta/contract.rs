//! Inputs and outputs of the ETA estimator (design §3.6, §4).

use std::time::Duration;

use crate::types::ScanState;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EtaSource {
    History,
    SessionPrevious,
    FsInodes,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EtaInput {
    pub elapsed: Duration,
    pub seen: u64,
    pub pending_dirs: u64,
    /// `None` until the rate has settled.
    pub rate_eps: Option<f64>,
    /// Estimated total entries and where the estimate came from.
    pub source: Option<(u64, EtaSource)>,
    pub state: ScanState,
    pub top_done: u32,
    pub top_total: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EtaView {
    Estimating,
    Eta {
        remaining: Duration,
        fraction: f32,
        source: EtaSource,
    },
    OverEstimate,
    /// No reliable source: true quantities only.
    Indeterminate {
        rate_eps: Option<f64>,
        pending_dirs: u64,
        top_done: u32,
        top_total: u32,
    },
    Done {
        elapsed: Duration,
    },
    Stopped {
        elapsed: Duration,
    },
}
