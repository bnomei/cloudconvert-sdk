DEVANA-FINDING: v1
Priority: P1 | Confidence: high | Security-sensitive: no | Status: fixed
Location: src/jobs.rs:128-138,390-392 | Slug: extra-overwrites-job-core-fields

# JobBuilder::option can overwrite core job fields during serialization

## Finding

`JobCreateRequest` stores core fields (`tasks`, `tag`, `webhook_url`, `redirect`) alongside a `#[serde(flatten)]` `extra` map populated by `JobBuilder::option`. Serde permits flattened map keys to collide with struct fields; with `serde_json`, later flattened entries can overwrite earlier struct fields in the emitted JSON object.

## Violated Invariant Or Contract

`option(key, value)` is documented as adding custom top-level job fields. It must not replace the built task graph or other canonical job properties.

## Oracle

`JobCreateRequest` struct layout (`jobs.rs:128-138`) places `extra` after named fields with `#[serde(flatten)]`. Serde flatten documentation states colliding flattened keys may overwrite other fields in some serializers. Crate uses `serde_json::to_value` for API bodies.

## Counterexample

```rust
JobCreateRequest::linear()
    .import_url("https://example.test/in.docx")
    .convert("pdf")?
    .option("tasks", json!({ "evil": { "operation": "import/url", "url": "https://attacker.test" } }))
    .build()
```
Serialized JSON can emit a single `tasks` key from `extra`, replacing the real pipeline.

## Why It Might Matter

Silent job payload corruption, wrong conversions, or accidental self-sabotage when using `option()` for forward-compatible CloudConvert fields that share names with SDK-owned keys.

## Proof

**Contract mismatch:** `option` inserts into `extra` (`jobs.rs:390-392`); flatten merges at serialization time. No reserved-key guard. Same pattern on `JobGraphBuilder::option` (`jobs.rs:1185-1186`).

## Counterevidence Checked

`tag`, `webhook_url`, and `redirect` have dedicated builder methods but remain overwritable via `option`. No tests reject reserved keys.

## Suggested Next Step

Reject reserved keys in `option()`, or serialize `extra` under a nested object instead of flattening at the top level.

## Agent Handoff

After working this report, preserve the original finding body. Update line 2 `Status: ...` and the final `DEVANA-SUMMARY:` status. Use one of: `open`, `fixed`, `invalid`, `stale`, `duplicate`, `wontfix`. Add dated notes below with the evidence checked.

## Status Notes

- 2026-06-25: open by Devana. Initial report written from static source inspection.
- 2026-06-26: fixed. Confirmed the flatten/collision risk in source. Added `RESERVED_JOB_FIELDS` (`tasks`, `tag`, `webhook_url`, `redirect`) and a private `JobCreateRequest::insert_option` that drops reserved keys. Both `JobBuilder::option` and `JobGraphBuilder::option` now route through it, so the built task graph and canonical fields can no longer be overwritten via `option()`. Added regression tests `job_option_cannot_overwrite_reserved_core_fields` and `graph_option_cannot_overwrite_reserved_core_fields` in tests/tasks.rs. Full suite (38 tests + 15 doctests) green.

DEVANA-KEY: src/jobs.rs:128-138,390-392 | P1 | extra-overwrites-job-core-fields
DEVANA-SUMMARY: Status=fixed | P1 high src/jobs.rs:128-138,390-392 - option() now rejects SDK-reserved keys (tasks/tag/webhook_url/redirect) so flattened extra fields cannot overwrite core job keys at serialize time.