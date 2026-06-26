DEVANA-FINDING: v1
Priority: P2 | Confidence: medium | Security-sensitive: no | Status: invalid
Location: src/socket.rs:403-467 | Slug: socket-buffer-drops-terminal-events

# Bounded socket channel silently drops terminal events

## Finding

Socket event callbacks push into a bounded `mpsc` channel and ignore send failures (`let _ = sender.send(...).await`). When the buffer fills, a terminal `job.finished` or `task.failed` event can be discarded while `wait_socket` continues waiting for a terminal event that will never arrive.

## Violated Invariant Or Contract

A terminal socket event for the subscribed resource must not be silently dropped while a managed wait is active.

## Oracle

Default connect uses buffer size 64 (`CloudConvertSocket::connect` → `connect_with_buffer(..., 64)`). Managed waits depend solely on buffered terminal events after an initial one-shot `get()`.

## Counterexample

A busy job emits many `job.updated` events that fill the 64-slot buffer. `job.finished` arrives next; `sender.send` fails and the event is dropped. Initial `get()` was non-terminal; the wait loop never sees a terminal event and blocks until socket close.

## Why It Might Matter

Indefinite waits or spurious socket-close errors for jobs that already completed, especially on high-churn channels or during event storms after reconnect.

## Proof

**Dataflow trace:** callback (`socket_event_callback:467`) → bounded mpsc → `wait_socket` loop (`client.rs:685-703`). Send failure is discarded; no watchdog or periodic `get()` after subscribe.

## Counterevidence Checked

Single-job `private-job.{id}` channels usually emit few events, lowering likelihood but not eliminating it. Larger buffers are available via `socket_with_buffer` but default managed waits use 64.

## Suggested Next Step

Use unbounded channels for terminal waits, surface send failures, or poll `get()` when sends fail or the buffer is near capacity.

## Agent Handoff

After working this report, preserve the original finding body. Update line 2 `Status: ...` and the final `DEVANA-SUMMARY:` status. Use one of: `open`, `fixed`, `invalid`, `stale`, `duplicate`, `wontfix`. Add dated notes below with the evidence checked.

## Status Notes

- 2026-06-25: open by Devana. Initial report written from static source inspection.
- 2026-06-26: invalid. The claimed mechanism (a full bounded buffer discarding a terminal event) does not occur. The callback uses `sender.send(...).await` (src/socket.rs:493), not `try_send`. tokio's bounded `mpsc::Sender::send().await` applies BACKPRESSURE when the channel is full — it waits for capacity — and returns `Err` only when the receiver has been dropped. It never drops the value on a full buffer. rust_socketio awaits the event callback inline in its receive loop (asynchronous/client/client.rs:383-384: `callback(payload.clone(), self.clone()).await;`) with no timeout or future cancellation, so a blocked send stalls that loop until the consumer drains rather than dropping the event. `wait_socket` continuously drains the receiver, so a slot frees and the (possibly terminal) event is delivered. The only `Err` (receiver dropped) case means no wait is active, so nothing is lost. Note the separate channel-close edge is independently covered by the now-fixed close-reconciliation (wait-socket-no-close-reconciliation). No code change.

DEVANA-KEY: src/socket.rs:403-467 | P2 | socket-buffer-drops-terminal-events
DEVANA-SUMMARY: Status=invalid | P2 medium src/socket.rs:403-467 - send().await is backpressure, not drop; tokio mpsc only errors when the receiver is gone, so terminal events are not silently discarded on a full buffer.