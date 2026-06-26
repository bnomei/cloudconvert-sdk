DEVANA-FINDING: v1
Priority: P1 | Confidence: high | Security-sensitive: no | Status: fixed
Location: src/client.rs:685-703 | Slug: wait-socket-no-close-reconciliation

# wait_socket errors on channel close without final GET reconciliation

## Finding

When `next_event()` returns `None` (socket channel closed), `wait_socket` and `TasksResource::wait_socket` immediately return `Error::Socket("socket closed before … completed")` without a final REST `get()` to check whether the resource already reached a terminal state.

## Violated Invariant Or Contract

Managed wait helpers should return the terminal job/task once it has completed, including when the terminal socket event was missed (fast completion race, reconnect gap, or dropped buffer event).

## Oracle

`wait_socket` performs an initial `get()` only before entering the loop (`client.rs:679-682`, `843-846`). Terminal-event branch has a `None` payload fallback to `get()`, but channel close does not.

## Counterexample

1. `get()` returns `processing`.
2. Job finishes on the server; the terminal socket event is never received (subscribe race, reconnect, or dropped event).
3. Socket channel closes → `next_event()` returns `None`.
4. Helper returns `Err(Error::Socket(...))` instead of reconciling with a final `get()` that would show `finished`.

## Why It Might Matter

Callers treat completed jobs as failures, skip downloads/exports, or trigger unnecessary retries and alerts.

## Proof

**Control-flow trace:** `next_event().await.ok_or_else(|| Error::Socket(...))` at lines 686-689 and 850-852. No `get()` on the `None` branch. Contrasts with `None => self.get(&id).await?` inside the terminal-event match arm (698, 861).

## Counterevidence Checked

Initial pre-loop `get()` covers already-terminal resources. Terminal-event payload `None` fallback covers missing embedded objects, not channel close.

## Suggested Next Step

On channel close or prolonged silence, call `get()` (or `wait_response` on sync base) and return `Ok` when the resource is terminal.

## Agent Handoff

After working this report, preserve the original finding body. Update line 2 `Status: ...` and the final `DEVANA-SUMMARY:` status. Use one of: `open`, `fixed`, `invalid`, `stale`, `duplicate`, `wontfix`. Add dated notes below with the evidence checked.

## Status Notes

- 2026-06-25: open by Devana. Initial report written from static source inspection.
- 2026-06-26: fixed. Confirmed both `JobsResource::wait_socket` and `TasksResource::wait_socket` mapped `next_event() == None` straight to `Error::Socket(...)` with no reconciliation, unlike the terminal-event arm which already falls back to `get()`. Fix: replaced the `ok_or_else` on the channel-close branch with a final `get()`; if the resource is terminal it is returned `Ok`, otherwise the original socket-closed error is returned. This recovers fast-completion races, reconnect gaps, and dropped-buffer cases. Not unit-testable here (wait_socket connects to the hardcoded CloudConvert socket URL; no socket.io mock harness exists), so verified by compile + `cargo test --all-features` (all pass) + clippy clean; the fix mirrors the existing `None => self.get(&id).await?` reconciliation pattern.

DEVANA-KEY: src/client.rs:685-703 | P1 | wait-socket-no-close-reconciliation
DEVANA-SUMMARY: Status=fixed | P1 high src/client.rs:685-703 - wait_socket now does a final GET on channel close and returns Ok when the job/task is already terminal.