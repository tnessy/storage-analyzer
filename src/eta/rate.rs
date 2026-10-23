use std::time::Duration;

/// EWMA of entries per second, fed from the data tick.
#[derive(Clone, Debug)]
pub struct Rate {
    tau_s: f64,
    first: Option<(Duration, u64)>,
    last: Option<(Duration, u64)>,
    samples: u32,
    eps: f64,
}

impl Rate {
    pub fn new(tau: Duration) -> Self {
        Self {
            tau_s: tau.as_secs_f64(),
            first: None,
            last: None,
            samples: 0,
            eps: 0.0,
        }
    }

    pub fn sample(&mut self, now: Duration, seen: u64) {
        let Some((last_t, last_seen)) = self.last else {
            self.first = Some((now, seen));
            self.last = Some((now, seen));
            self.samples = 1;
            return;
        };
        let dt = now.saturating_sub(last_t).as_secs_f64();
        if dt <= 0.0 {
            return;
        }
        let instant = seen.saturating_sub(last_seen) as f64 / dt;
        self.eps = if self.samples == 1 {
            instant
        } else {
            let alpha = 1.0 - (-dt / self.tau_s).exp();
            alpha * instant + (1.0 - alpha) * self.eps
        };
        self.samples += 1;
        self.last = Some((now, seen));
    }

    /// `None` until at least 2 s and 3 samples have been seen.
    pub fn eps(&self) -> Option<f64> {
        let ((first_t, _), (last_t, _)) = (self.first?, self.last?);
        let settled = self.samples >= 3 && last_t.saturating_sub(first_t) >= Duration::from_secs(2);
        settled.then_some(self.eps)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(rate: &mut Rate, from_ms: u64, to_ms: u64, eps: u64) {
        for ms in (from_ms..=to_ms).step_by(100) {
            rate.sample(Duration::from_millis(ms), eps * ms / 1000);
        }
    }

    #[test]
    fn unsettled_until_two_seconds_and_three_samples() {
        let mut r = Rate::new(Duration::from_secs(5));
        assert_eq!(r.eps(), None);
        feed(&mut r, 0, 1900, 1000);
        assert_eq!(r.eps(), None);
        feed(&mut r, 2000, 2000, 1000);
        assert!(r.eps().is_some());
    }

    #[test]
    fn three_samples_are_required_even_after_two_seconds() {
        let mut r = Rate::new(Duration::from_secs(5));
        r.sample(Duration::from_secs(0), 0);
        r.sample(Duration::from_secs(3), 3000);
        assert_eq!(r.eps(), None);
        r.sample(Duration::from_secs(4), 4000);
        assert!(r.eps().is_some());
    }

    #[test]
    fn constant_input_converges_within_one_percent_after_five_tau() {
        let mut r = Rate::new(Duration::from_secs(5));
        feed(&mut r, 0, 25_000, 1234);
        let eps = r.eps().unwrap();
        assert!((eps / 1234.0 - 1.0).abs() < 0.01, "eps={eps}");
    }

    #[test]
    fn tracks_a_rate_change_with_time_constant_tau() {
        let mut r = Rate::new(Duration::from_secs(5));
        feed(&mut r, 0, 10_000, 1000);
        for ms in (10_100..=15_000).step_by(100) {
            r.sample(
                Duration::from_millis(ms),
                10_000 + 2000 * (ms - 10_000) / 1000,
            );
        }
        // After one tau of a step from 1000 to 2000, about 63 % of the way.
        let eps = r.eps().unwrap();
        assert!((1500.0..1750.0).contains(&eps), "eps={eps}");
    }

    #[test]
    fn repeated_timestamps_do_not_poison_the_rate() {
        let mut r = Rate::new(Duration::from_secs(5));
        feed(&mut r, 0, 3000, 1000);
        r.sample(Duration::from_millis(3000), 3000);
        let eps = r.eps().unwrap();
        assert!(eps.is_finite() && (eps - 1000.0).abs() < 1.0);
    }
}
