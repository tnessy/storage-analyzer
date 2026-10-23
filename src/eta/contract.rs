//! Inputs and outputs of the ETA estimator (design §3.6, §4).

use std::time::Duration;

use crate::types::ScanState;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EtaSource {
    History,
    FsInodes,
    Probe,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EtaInput {
    pub elapsed: Duration,
    pub seen: u64,
    pub pending_dirs: u64,
    pub rate_eps: f64,
    /// Estimated total entries and where the estimate came from.
    pub source: Option<(u64, EtaSource)>,
    pub state: ScanState,
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
    Done {
        elapsed: Duration,
    },
    Stopped {
        elapsed: Duration,
    },
}
