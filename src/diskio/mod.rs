//! Disk read speed: where the numbers come from (the source) and how they become one value a
//! second (the meter). The real source lives with T036b; the app starts with `NoSource`.

mod meter;
pub mod resolve;
pub mod sys;
pub mod thread;

pub use meter::ReadRateMeter;

/// Where the read-speed numbers come from. Implemented for real (/proc, /sys) and by fakes in
/// tests. Must be cheap and non-blocking: it is called from the main loop at most once per
/// second.
pub trait ReadBytesSource {
    /// Cumulative bytes read so far by whatever the source measures, or `None` if the
    /// source can't be read right now (the field is then hidden for that second).
    fn read_bytes(&mut self) -> Option<u64>;
}

/// No source available (the default until the real one lands, and when the platform has none).
pub struct NoSource;

impl ReadBytesSource for NoSource {
    fn read_bytes(&mut self) -> Option<u64> {
        None
    }
}
