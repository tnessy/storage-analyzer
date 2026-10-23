# T013 report (engineer-2)

Status: DONE

Files: `src/eta/{contract,mod,rate}.rs`, deleted `src/probe/mod.rs`, removed `pub mod probe;` from `src/lib.rs`.
Part A done as specified (`EtaSource` with `SessionPrevious`, `Probe` removed; `rate_eps: Option<f64>`, `top_done`/`top_total` on the input; `EtaView::Indeterminate`). Nothing else referenced `probe` or the old types.

Red: with stubs (`view` returned `Estimating`, `Rate` returned `None`), 13 of 15 eta tests failed. Green: 15 pass, including the property test.
Fresh: fmt clean; clippy `-D warnings` clean; `cargo test --all-targets` all pass (49 lib + 14 + 4 + 7); doc tests ok; release build ok.

Decisions:
- A source with `rate_eps` of `Some(0.0)` or a non-finite value returns `Estimating`, as does a `remaining` that doesn't fit a `Duration`. This keeps `Eta` always finite.
- `OverEstimate` is checked before `Eta`, after the 2 s / settled-rate gate, per the precedence list.
- `Rate`: the first rate sample sets the EWMA directly; later samples use alpha = 1 − exp(−dt/τ). Samples with dt = 0 are ignored. Settled = ≥3 samples and ≥2 s between the first and last.
- The mid-scan-arrival test accepts `Estimating` or `Eta` after the source appears, because by 10 s the rate is already settled, so the sequence goes straight to `Eta`.
