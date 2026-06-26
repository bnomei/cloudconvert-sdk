DEVANA-FINDING: v1
Priority: P3 | Confidence: medium | Security-sensitive: no | Status: fixed
Location: src/client.rs:391-403 | Slug: retry-after-http-date-dropped

# Retry-After HTTP-date form is silently dropped, retrying far too soon

## Finding

When `respect_retry_after` is enabled (default true, src/config.rs:339), the
retry loop reads the `Retry-After` header and parses it only as integer seconds:
`value.parse::<u64>().ok()` (src/client.rs:396). RFC 7231 §7.1.3 allows
`Retry-After` to be *either* delta-seconds *or* an HTTP-date. A date-form value
fails `parse::<u64>`, collapses through `.flatten()` to `None`, and
`unwrap_or(delay)` falls back to the small exponential backoff delay
(src/client.rs:399-402). The server's explicit back-off instruction is discarded.

## Violated Invariant Or Contract

With `respect_retry_after = true`, a valid server `Retry-After` must be honored —
the client should not retry before the indicated time (bounded by policy). An
RFC-permitted HTTP-date value must not be treated as "no header".

## Oracle

RFC 7231 §7.1.3 (`Retry-After = HTTP-date / delay-seconds`) and the
`respect_retry_after` flag (src/config.rs:325-339), whose name promises the
server directive is respected.

## Counterexample

Server returns `429` with `Retry-After: Wed, 25 Jun 2026 12:00:00 GMT` (≈60s
out). `"Wed, 25 Jun 2026 12:00:00 GMT".parse::<u64>()` fails -> inner `Option` is
`None` -> `retry_after = Some(None)` -> `.flatten()` -> `None` ->
`unwrap_or(delay)` uses the exponential delay (initial 250 ms, capped at
`max_delay` 10 s). The client retries ~40x sooner than instructed.

## Why It Might Matter

The client re-hits a rate-limited endpoint well inside the server's stated
window, can exhaust `max_attempts`, and surfaces a 429 a correct backoff would
have avoided. CloudConvert itself sends integer-seconds `Retry-After`, so this
fires mainly behind an RFC-compliant proxy/gateway that emits the date form —
hence P3 — but it is a concrete, silent contract gap.

## Proof

Control-flow + contract mismatch. The header value never reaches a date parser;
only `u64` is attempted (src/client.rs:396), and failure is indistinguishable
from a missing header in the `.flatten().unwrap_or(delay)` chain
(src/client.rs:399-402).

Related: even for the seconds form, the value is capped with
`.min(max_delay_value())` (src/client.rs:401), so a `Retry-After: 30` is
truncated to the default 10 s ceiling and the client still retries inside the
server's window.

## Counterevidence Checked

- The seconds-form path works and is bounded by `max_delay`; the defect is
  specific to the date form (dropped) and to directives exceeding `max_delay`
  (truncated).
- `respect_retry_after` can be disabled, but it defaults to on and advertises the
  opposite behavior.
- Distinct from the filed `retry-replays-non-idempotent-posts` finding, which
  concerns replay safety, not timing.

## Suggested Next Step

Parse the HTTP-date form of `Retry-After` (compute the delay from now), and
reconsider applying `max_delay` to an explicit server `Retry-After` for 429
responses rather than truncating the server's directive.

## Agent Handoff

After working this report, preserve the original finding body. Update line 2
`Status: ...` and the final `DEVANA-SUMMARY:` status. Use one of: `open`,
`fixed`, `invalid`, `stale`, `duplicate`, `wontfix`. Add dated notes below with
the evidence checked.

## Status Notes

- 2026-06-25: open by Devana. Initial report written from static source inspection (retry feature path).
- 2026-06-26: fixed (date-form drop). Confirmed the retry loop parsed `Retry-After` only as `u64` seconds, so an RFC 7231 HTTP-date value failed `parse::<u64>` and collapsed to the small exponential delay. Fix: added `parse_retry_after` which tries delta-seconds first, then `httpdate::parse_http_date` (computing the delay from `SystemTime::now()`; a past date yields `Duration::ZERO`). Added `httpdate` as an optional dependency gated by the `retry` feature (it was already in the tree transitively via hyper). Added 4 unit tests (`retry_after_tests`) covering delta-seconds, a future HTTP-date, a past HTTP-date, and garbage. The default (no-retry) build is unaffected; verified with `cargo test --all-features` + clippy clean.

  Secondary point (max_delay truncating an explicit Retry-After) was intentionally NOT changed: the cap is a deliberate, tested safety bound (`retry_after_headers_are_capped_by_max_delay`) protecting against pathological/huge directives. The parsed date delta flows through the same `.min(max_delay)` cap as the seconds form, which is consistent. Callers wanting to honor large server windows can raise `max_delay`.

DEVANA-KEY: src/client.rs:391-403 | P3 | retry-after-http-date-dropped
DEVANA-SUMMARY: Status=fixed | P3 medium src/client.rs:391-403 - Retry-After now parses both delta-seconds and the HTTP-date form (via httpdate, retry feature); the max_delay cap on explicit directives is kept by design.
