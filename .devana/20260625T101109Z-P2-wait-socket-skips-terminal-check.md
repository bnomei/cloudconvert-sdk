DEVANA-FINDING: v1
Priority: P2 | Confidence: medium | Security-sensitive: no | Status: fixed
Location: src/client.rs:691-702 | Slug: wait-socket-skips-terminal-check

# wait_socket returns without verifying resource terminal status

## Finding

After receiving a terminal-named socket event (`job.finished`, `task.failed`, etc.), `wait_socket` returns the embedded job/task without checking `is_terminal()` on the payload. Terminality is inferred only from the event name suffix, not from the deserialized status field.

## Violated Invariant Or Contract

`wait_socket` / `create_and_wait_socket` callers expect `Ok` results to be terminal resources. `tests/live_api.rs:142` asserts `finished.is_terminal()` after `wait_socket`.

## Oracle

`SocketEvent::is_terminal()` checks event name suffix only (`socket.rs:333-335`). `Job::is_terminal()` / `Task::is_terminal()` delegate to `JobStatus` / `TaskStatus`, where `Unknown` (from `#[serde(other)]`) is non-terminal (`jobs.rs:1989-1990`, `2028-2029`).

## Counterexample

Socket delivers `job.finished` with payload `{"job": {"id": "job_1", "status": "processing", ...}}` or any unrecognized status string that deserializes to `Unknown`. `event.is_terminal()` is true; `Some(job)` branch returns `Ok(job)` with `job.is_terminal() == false`.

## Why It Might Matter

Downstream code may call `export_urls()`, branch on `is_error()`, or chain further waits on a resource that is not actually complete.

## Proof

**Control-flow trace:** lines 691-702 return `Ok(job)` on `Some(job)` without `job.is_terminal()` guard. `create_and_wait_socket` early path (`client.rs:602-604`) correctly checks `job.is_terminal()` before the socket loop; post-event path does not.

## Counterevidence Checked

Live API test passes with consistent real payloads. `None` payload branch falls back to `get()` but does not validate terminality either. No re-fetch when embedded status disagrees with event name.

## Suggested Next Step

After a terminal-named event, require `resource.is_terminal()` or re-fetch via `get()`/`wait_response` until terminal.

## Agent Handoff

After working this report, preserve the original finding body. Update line 2 `Status: ...` and the final `DEVANA-SUMMARY:` status. Use one of: `open`, `fixed`, `invalid`, `stale`, `duplicate`, `wontfix`. Add dated notes below with the evidence checked.

## Status Notes

- 2026-06-25: open by Devana. Initial report written from static source inspection.
- 2026-06-26: fixed. Confirmed both `JobsResource::wait_socket` and `TasksResource::wait_socket` returned the embedded resource on a terminal-NAMED event without checking `is_terminal()` on the deserialized status (and `SocketEvent::is_terminal()` only inspects the event-name suffix; an unrecognized status deserializes to `Unknown`, which is non-terminal). Fix: in the terminal-event arm, only accept the embedded resource when `id` matches AND `resource.is_terminal()`; if the id matches but the payload is not actually terminal (stale/unknown status), reconcile with a `get()`; missing embedded resource still falls back to `get()`. A final `if !resource.is_terminal() { continue; }` guard keeps waiting for a later event if reconciliation still shows non-terminal, so an `Ok` result is always terminal. Applied to both Jobs and Tasks. Not unit-testable here (wait_socket uses the hardcoded CloudConvert socket URL; no socket.io mock harness); verified by compile + `cargo test --all-features` (all pass) + clippy clean.

DEVANA-KEY: src/client.rs:691-702 | P2 | wait-socket-skips-terminal-check
DEVANA-SUMMARY: Status=fixed | P2 medium src/client.rs:691-702 - wait_socket now requires the resource status to be terminal (re-fetching to reconcile, else waiting), so it never returns a non-terminal job/task.