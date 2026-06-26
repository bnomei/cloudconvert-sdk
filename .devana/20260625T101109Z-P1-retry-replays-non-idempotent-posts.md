DEVANA-FINDING: v1
Priority: P1 | Confidence: high | Security-sensitive: no | Status: fixed
Location: src/client.rs:359-418 | Slug: retry-replays-non-idempotent-posts

# Retry policy replays non-idempotent POST requests

## Finding

When the `retry` feature is enabled, `send_api_with_retry` retries any failed API call—including POST creates and other state-mutating endpoints—without checking HTTP method or idempotency. A transient timeout or 503 after the server already accepted a POST can produce a second live resource while the caller believes only one was created.

## Violated Invariant Or Contract

Automatic retries must not re-execute requests whose success is ambiguous. POST job/task/webhook creation, sync `create_and_wait`, task `cancel`/`retry`, and similar mutations should not be blindly replayed.

## Oracle

README documents retry for CloudConvert API calls (`README.md` ~407–410). HTTP semantics and the SDK's own README boundary for uploads/downloads imply POST safety is not guaranteed. Retry tests only exercise GET `jobs().get("flaky")` (`tests/client.rs:844-861`), not POST paths.

## Counterexample

`client.jobs().create(request)` with `RetryPolicy::new(3)`. The server creates `job_A` and the response is lost (503 or client timeout). `send_api_with_retry` rebuilds the identical POST; the server creates `job_B`. The caller receives one `Job` id but two jobs exist and may be billed twice.

## Why It Might Matter

Duplicate jobs/tasks/webhooks, double billing, orphaned resources, and broken caller assumptions about single-create semantics.

## Proof

**Control-flow trace:** `post_response` → `send_api` → `send_api_with_retry` for `JobsResource::create`, `create_and_wait_response`, `TasksResource::create`/`cancel`/`retry`, `WebhooksResource::create`. `is_retryable_status` matches 429/5xx; `is_retryable_error` matches connect/timeout. No method or path guard; `build()` reconstructs the same body each attempt.

## Counterevidence Checked

Upload/download paths bypass `send_api`. No idempotency-key support in request builders. Retry feature is opt-in, but once enabled all API POSTs share the same replay loop.

## Suggested Next Step

Restrict automatic retry to safe methods (GET/HEAD/DELETE) or require explicit opt-in per endpoint; document POST behavior if replay is intentional.

## Agent Handoff

After working this report, preserve the original finding body. Update line 2 `Status: ...` and the final `DEVANA-SUMMARY:` status. Use one of: `open`, `fixed`, `invalid`, `stale`, `duplicate`, `wontfix`. Add dated notes below with the evidence checked.

## Status Notes

- 2026-06-25: open by Devana. Initial report written from static source inspection.
- 2026-06-26: fixed. Confirmed `send_api`/`send_api_with_retry` is the shared path for GET, POST and DELETE and had no method guard. Fix: `send_api_with_retry` now builds the request once to read its HTTP method and only allows multiple attempts for idempotent methods (`is_idempotent_method`: GET/HEAD/OPTIONS/TRACE/PUT/DELETE). POST/PATCH (job/task/webhook creates, cancel/retry, sync create_and_wait) are sent exactly once; a transient 5xx/timeout surfaces as an error instead of being replayed, so no duplicate resources are created. If the method can't be determined the code falls back to no retries. Added test `retry_feature_does_not_replay_post_creates` (mock create returns 503; asserts a single attempt and an error) and kept the existing GET-retry tests green. Verified with `cargo test --all-features` (all suites pass) plus fmt + clippy clean.

DEVANA-KEY: src/client.rs:359-418 | P1 | retry-replays-non-idempotent-posts
DEVANA-SUMMARY: Status=fixed | P1 high src/client.rs:359-418 - Automatic retry is now restricted to idempotent HTTP methods; POST/PATCH creates and mutations are never replayed.