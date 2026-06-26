DEVANA-FINDING: v1
Priority: P1 | Confidence: high | Security-sensitive: no | Status: fixed
Location: src/socket.rs:398-441 | Slug: socket-reconnect-loses-subscriptions

# Socket auto-reconnect does not re-emit subscribe events

## Finding

`CloudConvertSocket` enables `reconnect(true)` and `reconnect_on_disconnect(true)` but emits `"subscribe"` payloads only once during initial `connect_with_buffer`. After a transport disconnect mid-wait, the connection may restore without re-registering private channels, so managed wait helpers stop receiving job/task terminal events.

## Violated Invariant Or Contract

Managed socket waits (`wait_socket`, `create_and_wait_socket`) must complete when the target resource reaches a terminal state, including after transient disconnects. README documents subscribe-then-wait semantics for these helpers.

## Oracle

`socket_client_builder` enables reconnect (`src/socket.rs:439-441`). `connect_with_buffer` loops subscriptions once at lines 410-412. No reconnect handler re-calls `subscribe()`.

## Counterexample

`jobs().wait_socket("job_1")` on a long-running job. TCP drops after subscribe. `rust_socketio` reconnects transport, but the server never receives another `"subscribe"` for `private-job.job_1`. Terminal `job.finished` is not delivered to the mpsc receiver; the loop blocks until the channel closes and returns `Error::Socket("socket closed before job … completed")` even if the job finished on the server.

## Why It Might Matter

Spurious wait failures, hung integrations, and false error handling in production networks with blips or load-balancer idle timeouts.

## Proof

**State transition mismatch:** subscribe-once at connect; reconnect restores transport only. **Control-flow trace:** `wait_socket` blocks on `next_event()` (`src/client.rs:685-703`) with no post-reconnect resubscribe or polling fallback.

## Counterevidence Checked

Pre-loop `get()` handles jobs already terminal before subscribe (`client.rs:679-682`). `subscribe()` exists but is not invoked from any reconnect callback. `disconnect()` runs only on success paths.

## Suggested Next Step

Store subscriptions and re-emit on reconnect, or fall back to polling/`get()` after disconnect events.

## Agent Handoff

After working this report, preserve the original finding body. Update line 2 `Status: ...` and the final `DEVANA-SUMMARY:` status. Use one of: `open`, `fixed`, `invalid`, `stale`, `duplicate`, `wontfix`. Add dated notes below with the evidence checked.

## Status Notes

- 2026-06-25: open by Devana. Initial report written from static source inspection.
- 2026-06-26: fixed. Verified against rust_socketio 0.6.0 source: the async client fires `Event::Connect` whenever a Connect packet is received, including after each `reconnect()` (asynchronous/client/client.rs:507-509 calls `callback(&Event::Connect, "")`). Fix: `connect_with_buffer` now collects the subscriptions, pre-serializes them to payloads (so serialization errors surface before connecting), and passes them to `socket_client_builder`, which registers an `on(Event::Connect, resubscribe_callback)` handler. The handler re-emits every `subscribe` payload on each (re)connection, restoring private channels after a transport drop. The explicit initial subscribe loop is kept so initial subscribe emit errors still propagate from `connect_with_buffer`; the handler also re-emitting on the initial connect is harmless (Pusher-style channel subscribes are idempotent per connection, so no duplicate event delivery). No live-socket integration test exists in the suite (would require a socket.io mock server); verified via `cargo test --all-features` (all pass) + clippy clean + the library source trace above.

DEVANA-KEY: src/socket.rs:398-441 | P1 | socket-reconnect-loses-subscriptions
DEVANA-SUMMARY: Status=fixed | P1 high src/socket.rs:398-441 - An Event::Connect handler now re-emits all subscribe payloads on every (re)connection, so managed waits keep receiving terminal events after transient disconnects.