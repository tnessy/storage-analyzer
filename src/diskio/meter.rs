//! Turns cumulative byte counts into one bytes-per-second value a second. No I/O.

use std::time::Duration;

/// A full window is this long; the shown value changes at most once per window.
const WINDOW: Duration = Duration::from_secs(1);

#[derive(Debug, Default)]
pub struct ReadRateMeter {
    window: Option<(Duration, u64)>,
    rate: Option<u64>,
}

impl ReadRateMeter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feeds one reading. `cumulative` is `None` when the source can't be read. A missing
    /// reading or a total that went down gives no value that second; a new window opens and
    /// values resume after the next full second.
    pub fn sample(&mut self, now: Duration, cumulative: Option<u64>) {
        let Some(bytes) = cumulative else {
            self.rate = None;
            self.window = None;
            return;
        };
        let Some((started, first)) = self.window else {
            self.window = Some((now, bytes));
            return;
        };
        if bytes < first {
            // The counter wrapped or was reset: this window can't be trusted.
            self.rate = None;
            self.window = Some((now, bytes));
            return;
        }
        let elapsed = now.saturating_sub(started);
        if elapsed >= WINDOW {
            self.rate = Some(((bytes - first) as f64 / elapsed.as_secs_f64()).round() as u64);
            self.window = Some((now, bytes));
        }
    }

    /// Bytes per second over the last full window; `None` while there is nothing to show.
    pub fn rate(&self) -> Option<u64> {
        self.rate
    }

    /// Forgets everything (a new scan starts).
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(ms: u64) -> Duration {
        Duration::from_millis(ms)
    }

    #[test]
    fn nothing_until_the_first_full_window_then_bytes_per_second() {
        let mut m = ReadRateMeter::new();
        assert_eq!(m.rate(), None);
        m.sample(at(100), Some(1_000));
        assert_eq!(m.rate(), None, "the first reading only opens a window");
        m.sample(at(600), Some(500_000));
        assert_eq!(m.rate(), None, "half a window is not enough");
        m.sample(at(1_100), Some(2_001_000));
        assert_eq!(m.rate(), Some(2_000_000));
    }

    #[test]
    fn a_measured_zero_is_a_value() {
        let mut m = ReadRateMeter::new();
        m.sample(at(0), Some(10));
        m.sample(at(1_000), Some(10));
        assert_eq!(m.rate(), Some(0));
    }

    #[test]
    fn the_value_changes_at_most_once_per_second_with_readings_every_100_ms() {
        let mut m = ReadRateMeter::new();
        let mut changes = 0;
        let mut last = None;
        for i in 0..=50u64 {
            // The source speeds up all the time, so every reading would give a new number.
            m.sample(at(i * 100), Some(i * i * 1_000));
            if m.rate() != last {
                changes += 1;
                last = m.rate();
            }
        }
        // 5 seconds: the value appears after 1 s and changes once per later second.
        assert!(changes <= 5, "{changes} changes");
        assert!(changes >= 4);
    }

    #[test]
    fn a_counter_going_backwards_gives_no_value_that_second_and_values_resume() {
        let mut m = ReadRateMeter::new();
        m.sample(at(0), Some(5_000));
        m.sample(at(1_000), Some(9_000));
        assert_eq!(m.rate(), Some(4_000));
        m.sample(at(2_000), Some(100));
        assert_eq!(m.rate(), None, "no value that second");
        m.sample(at(3_000), Some(200_100));
        assert_eq!(
            m.rate(),
            Some(200_000),
            "the next full second gives a value again"
        );
    }

    #[test]
    fn a_missing_reading_gives_no_value_that_second_and_values_resume() {
        let mut m = ReadRateMeter::new();
        m.sample(at(0), Some(0));
        m.sample(at(1_000), Some(1_000));
        assert_eq!(m.rate(), Some(1_000));
        m.sample(at(1_100), None);
        assert_eq!(m.rate(), None);
        // The reading is back; it opens a new window, and the value follows a full second later.
        m.sample(at(2_000), Some(2_000));
        assert_eq!(m.rate(), None);
        m.sample(at(3_000), Some(7_000));
        assert_eq!(m.rate(), Some(5_000));
    }

    #[test]
    fn reset_starts_over() {
        let mut m = ReadRateMeter::new();
        m.sample(at(0), Some(0));
        m.sample(at(1_000), Some(1_000));
        m.reset();
        assert_eq!(m.rate(), None);
        m.sample(at(10_000), Some(0));
        m.sample(at(11_000), Some(7));
        assert_eq!(m.rate(), Some(7));
    }
}
