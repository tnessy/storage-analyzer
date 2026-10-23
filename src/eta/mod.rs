//! Scan progress and ETA estimator (T013).

pub mod contract;

pub use contract::{EtaInput, EtaSource, EtaView};

pub fn view(_input: &EtaInput) -> EtaView {
    todo!("T013")
}
