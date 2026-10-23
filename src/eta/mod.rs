//! Scan progress and ETA estimator (T013).

use std::time::Duration;

use crate::types::ScanState;

pub mod contract;
mod rate;

pub use contract::{EtaInput, EtaSource, EtaView};
pub use rate::Rate;

const MIN_ELAPSED: Duration = Duration::from_secs(2);
const OVER_ESTIMATE_FACTOR: f64 = 1.02;
const MAX_FRACTION: f64 = 0.99;

pub fn view(input: &EtaInput) -> EtaView {
    match input.state {
        ScanState::Done => {
            return EtaView::Done {
                elapsed: input.elapsed,
            };
        }
        ScanState::Stopped => {
            return EtaView::Stopped {
                elapsed: input.elapsed,
            };
        }
        ScanState::Starting | ScanState::Scanning => {}
    }
    let Some((source_total, source)) = input.source else {
        return EtaView::Indeterminate {
            rate_eps: input.rate_eps,
            pending_dirs: input.pending_dirs,
            top_done: input.top_done,
            top_total: input.top_total,
        };
    };
    let rate = match input.rate_eps {
        Some(r) if input.elapsed >= MIN_ELAPSED && r.is_finite() && r > 0.0 => r,
        _ => return EtaView::Estimating,
    };
    let seen = input.seen as f64;
    if seen >= source_total as f64 * OVER_ESTIMATE_FACTOR {
        return EtaView::OverEstimate;
    }
    let est_total = (source_total as f64).max(seen + input.pending_dirs as f64);
    let fraction = (seen / est_total).clamp(0.0, MAX_FRACTION);
    match Duration::try_from_secs_f64((est_total - seen) / rate) {
        Ok(remaining) => EtaView::Eta {
            remaining,
            fraction: fraction as f32,
            source,
        },
        Err(_) => EtaView::Estimating,
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::types::ScanState;

    fn input() -> EtaInput {
        EtaInput {
            elapsed: Duration::from_secs(30),
            seen: 1000,
            pending_dirs: 10,
            rate_eps: Some(100.0),
            source: Some((2000, EtaSource::History)),
            state: ScanState::Scanning,
            top_done: 3,
            top_total: 7,
        }
    }

    #[test]
    fn done_and_stopped_win_over_everything() {
        let elapsed = Duration::from_secs(9);
        for source in [None, Some((10, EtaSource::FsInodes))] {
            let i = EtaInput {
                state: ScanState::Done,
                elapsed,
                source,
                ..input()
            };
            assert_eq!(view(&i), EtaView::Done { elapsed });
            let i = EtaInput {
                state: ScanState::Stopped,
                elapsed,
                source,
                ..input()
            };
            assert_eq!(view(&i), EtaView::Stopped { elapsed });
        }
    }

    #[test]
    fn no_source_is_indeterminate_never_estimating() {
        for secs in [0.5, 30.0] {
            for rate_eps in [None, Some(50.0)] {
                let i = EtaInput {
                    elapsed: Duration::from_secs_f64(secs),
                    source: None,
                    rate_eps,
                    ..input()
                };
                assert_eq!(
                    view(&i),
                    EtaView::Indeterminate {
                        rate_eps,
                        pending_dirs: 10,
                        top_done: 3,
                        top_total: 7
                    }
                );
            }
        }
    }

    #[test]
    fn source_but_early_or_unsettled_is_estimating() {
        let early = EtaInput {
            elapsed: Duration::from_millis(500),
            ..input()
        };
        assert_eq!(view(&early), EtaView::Estimating);
        let unsettled = EtaInput {
            rate_eps: None,
            ..input()
        };
        assert_eq!(view(&unsettled), EtaView::Estimating);
        let zero_rate = EtaInput {
            rate_eps: Some(0.0),
            ..input()
        };
        assert_eq!(view(&zero_rate), EtaView::Estimating);
    }

    #[test]
    fn settled_source_gives_eta() {
        let EtaView::Eta {
            remaining,
            fraction,
            source,
        } = view(&input())
        else {
            panic!("expected Eta")
        };
        // est_total 2000, seen 1000, 100 eps.
        assert_eq!(remaining, Duration::from_secs(10));
        assert_eq!(fraction, 0.5);
        assert_eq!(source, EtaSource::History);
    }

    #[test]
    fn over_estimate_at_one_point_oh_two() {
        let at = EtaInput {
            seen: 2039,
            source: Some((2000, EtaSource::FsInodes)),
            ..input()
        };
        assert!(matches!(view(&at), EtaView::Eta { .. }));
        let past = EtaInput { seen: 2040, ..at };
        assert_eq!(view(&past), EtaView::OverEstimate);
    }

    #[test]
    fn guard_uses_seen_plus_pending_when_source_is_smaller() {
        let i = EtaInput {
            seen: 1000,
            pending_dirs: 100,
            source: Some((1010, EtaSource::SessionPrevious)),
            ..input()
        };
        let EtaView::Eta {
            remaining,
            fraction,
            ..
        } = view(&i)
        else {
            panic!("expected Eta")
        };
        assert_eq!(remaining, Duration::from_secs(1));
        assert!((fraction - 1000.0 / 1100.0).abs() < 1e-6 && fraction < 1.0);
    }

    #[test]
    fn fraction_is_clamped_below_one_before_done() {
        let i = EtaInput {
            seen: 2000,
            pending_dirs: 0,
            ..input()
        };
        let EtaView::Eta { fraction, .. } = view(&i) else {
            panic!("expected Eta")
        };
        assert_eq!(fraction, 0.99);
    }

    fn state_at(t_ms: u64, seen: u64, rate: &Rate, source: Option<(u64, EtaSource)>) -> EtaView {
        view(&EtaInput {
            elapsed: Duration::from_millis(t_ms),
            seen,
            pending_dirs: 0,
            rate_eps: rate.eps(),
            source,
            state: ScanState::Scanning,
            top_done: 0,
            top_total: 0,
        })
    }

    #[test]
    fn source_arriving_mid_scan_never_goes_back_to_indeterminate() {
        let mut rate = Rate::new(Duration::from_secs(5));
        let mut views = Vec::new();
        for ms in (0..=20_000).step_by(100) {
            let seen = ms * 1000 / 1000;
            rate.sample(Duration::from_millis(ms), seen);
            let source = (ms >= 10_000).then_some((100_000, EtaSource::History));
            views.push((ms, state_at(ms, seen, &rate, source)));
        }
        for (ms, v) in &views {
            match (*ms < 10_000, v) {
                (true, EtaView::Indeterminate { .. }) => {}
                (false, EtaView::Estimating | EtaView::Eta { .. }) => {}
                _ => panic!("t={ms}: {v:?}"),
            }
        }
        assert!(matches!(views.last().unwrap().1, EtaView::Eta { .. }));
    }

    #[test]
    fn constant_rate_scan_with_exact_source_converges_to_done() {
        let truth = 60_000u64;
        let mut rate = Rate::new(Duration::from_secs(5));
        let mut prev: Option<Duration> = None;
        let mut etas = 0;
        for ms in (0..60_000).step_by(100) {
            let seen = ms; // 1000 entries/s
            rate.sample(Duration::from_millis(ms), seen);
            if let EtaView::Eta {
                remaining,
                fraction,
                ..
            } = state_at(ms, seen, &rate, Some((truth, EtaSource::History)))
            {
                etas += 1;
                assert!(fraction <= 0.99);
                if let Some(p) = prev {
                    assert!(
                        remaining.as_secs_f64() <= p.as_secs_f64() * 1.05 + 0.2,
                        "{p:?} -> {remaining:?}"
                    );
                }
                prev = Some(remaining);
            }
        }
        assert!(etas > 500);
        assert!(prev.unwrap() < Duration::from_millis(500));
        let done = EtaInput {
            state: ScanState::Done,
            elapsed: Duration::from_secs(60),
            ..input()
        };
        assert_eq!(
            view(&done),
            EtaView::Done {
                elapsed: Duration::from_secs(60)
            }
        );
    }

    proptest! {
        #[test]
        fn never_non_finite_and_fraction_capped(
            elapsed_ms in 0u64..10_000_000,
            seen in any::<u64>(),
            pending in any::<u64>(),
            rate in prop::option::of(prop_oneof![Just(0.0), Just(f64::MIN_POSITIVE), 0.0f64..1e12, Just(1e300)]),
            source in prop::option::of((any::<u64>(), 0usize..3)),
            state in prop_oneof![Just(ScanState::Starting), Just(ScanState::Scanning)],
        ) {
            let i = EtaInput {
                elapsed: Duration::from_millis(elapsed_ms),
                seen,
                pending_dirs: pending,
                rate_eps: rate,
                source: source.map(|(t, k)| (t, [EtaSource::History, EtaSource::SessionPrevious, EtaSource::FsInodes][k])),
                state,
                top_done: 0,
                top_total: 0,
            };
            if let EtaView::Eta { remaining, fraction, .. } = view(&i) {
                prop_assert!(fraction.is_finite() && (0.0..=0.99).contains(&fraction));
                prop_assert!(remaining.as_secs_f64().is_finite());
            }
        }
    }
}
