DEVANA-FINDING: v1
Priority: P3 | Confidence: high | Security-sensitive: no | Status: fixed
Location: src/client.rs:556-558 | Slug: retry-backoff-factor-mulf64-panic

# Retry backoff multiply can panic on a builder-accepted backoff factor

## Finding

`next_retry_delay` computes `current.mul_f64(policy.backoff_factor_value())`
(src/client.rs:557). `Duration::mul_f64` panics if the factor is non-finite
(`NaN`/`±inf`) or if the result overflows `Duration`. The builder setter
`RetryPolicy::backoff_factor` only clamps the lower bound with
`backoff_factor.max(1.0)` (src/config.rs:317), which does not exclude
`f64::INFINITY` (`f64::INFINITY.max(1.0) == f64::INFINITY`). The `.min(max_delay)`
guard is applied only *after* the multiply (src/client.rs:558), so it cannot
prevent the panic.

## Violated Invariant Or Contract

The retry loop must not panic on configuration the builder accepted. The
`.max(1.0)` clamp signals an intent to sanitize `backoff_factor`, but it leaves
non-finite values intact.

## Oracle

`std::time::Duration::mul_f64` documented contract: it panics if the result is
negative, overflows `Duration`, or is non-finite. The builder
(src/config.rs:316-319) accepts and stores the offending value.

## Counterexample

```rust
let policy = RetryPolicy::default().backoff_factor(f64::INFINITY);
// first retryable response (e.g. 429/503) -> next_retry_delay runs:
// Duration::from_millis(250).mul_f64(f64::INFINITY) -> panic
```

Alternatively `initial_delay(Duration::from_secs(u64::MAX / 2)).backoff_factor(2.0)`
overflows `Duration` on the second multiply and panics.

## Why It Might Matter

A panic aborts the in-flight request task instead of retrying or returning an
error. It is gated behind the non-default `retry` feature and an unusual config
value, hence P3, but the panic is reachable from safe public API with no
`unsafe`, and the partial `.max(1.0)` sanitization makes the gap easy to miss.

## Proof

Dataflow + contract mismatch. Setter clamps only the lower bound
(src/config.rs:317) -> stores `f64::INFINITY` -> retry loop calls
`next_retry_delay` after each attempt (src/client.rs:414) -> `mul_f64` hits its
documented non-finite panic path (src/client.rs:557) before any `.min` clamp.

## Counterevidence Checked

- `NaN` is neutralized: `f64::NAN.max(1.0) == 1.0` in Rust, so only `+inf` and
  huge-`initial_delay` overflow remain as triggers.
- `max_delay` / `.min()` clamp runs after the multiply, so it does not guard it.
- The `retry-after` sleep path uses `Duration::from_secs(u64)` and is panic-free;
  only the exponential branch is affected.
- Distinct from the filed `retry-replays-non-idempotent-posts` finding (that is
  about replay semantics, not a panic).

## Suggested Next Step

In `backoff_factor`, reject or clamp non-finite input (e.g.
`if !backoff_factor.is_finite() { backoff_factor = 1.0 }` alongside `.max(1.0)`),
and/or use `Duration::saturating_mul`-style saturating math in `next_retry_delay`.

## Agent Handoff

After working this report, preserve the original finding body. Update line 2
`Status: ...` and the final `DEVANA-SUMMARY:` status. Use one of: `open`,
`fixed`, `invalid`, `stale`, `duplicate`, `wontfix`. Add dated notes below with
the evidence checked.

## Status Notes

- 2026-06-25: open by Devana. Initial report written from static source inspection (retry feature path).
- 2026-06-26: fixed. Confirmed `backoff_factor` only did `.max(1.0)` (which leaves `+inf` intact; NaN already collapses to 1.0) and `next_retry_delay` called `Duration::mul_f64`, which panics on a non-finite or overflowing product before the `.min(max_delay)` clamp. Two defensive fixes: (1) the `backoff_factor` setter now rejects non-finite input (`is_finite()` else 1.0) before flooring at 1.0; (2) `next_retry_delay` replaces `mul_f64` with checked float math — `Duration::try_from_secs_f64(current.as_secs_f64() * factor).unwrap_or(max_delay).min(max_delay)` — so a non-finite or overflowing result saturates to `max_delay` instead of panicking (also covers the huge-`initial_delay` overflow case the report raised). Added unit tests `backoff_factor_rejects_non_finite_values` and `next_retry_delay_saturates_instead_of_panicking` (including a 1e308 factor with `u64::MAX/2` delay). Default build unaffected; full suite green + clippy clean.

DEVANA-KEY: src/client.rs:556-558 | P3 | retry-backoff-factor-mulf64-panic
DEVANA-SUMMARY: Status=fixed | P3 high src/client.rs:556-558 - backoff_factor rejects non-finite input and next_retry_delay uses saturating try_from_secs_f64 math, so the retry loop never panics on a bad factor or overflow.
