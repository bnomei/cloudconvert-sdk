DEVANA-FINDING: v1
Priority: P1 | Confidence: high | Security-sensitive: no | Status: fixed
Location: src/tasks.rs:842-900 | Slug: extra-overwrites-task-core-fields

# Task option() can overwrite core task fields during serialization

## Finding

Typed task structs such as `ConvertTask` declare canonical fields (`input`, `output_format`, `engine`, etc.) and a `#[serde(flatten)]` `extra` map filled by `option()`. On serialization, flattened `extra` entries can collide with and overwrite struct fields in the JSON object that becomes the `TaskRequest` payload.

## Violated Invariant Or Contract

`option(key, value)` is for CloudConvert-specific extras. Canonical builder-set fields must remain authoritative in the emitted task payload.

## Oracle

`ConvertTask` layout (`tasks.rs:842-856`); `serialize_payload` uses `serde_json::to_value` (`tasks.rs:398-408`). Serde flatten collision semantics apply across task types sharing this pattern (`MergeTask`, `OptimizeTask`, etc.).

## Counterexample

```rust
ConvertTask::new("import-url", "pdf").option("input", "wrong-task")
```
Payload serializes `input` as `"wrong-task"` instead of `"import-url"`, breaking the task graph sent to CloudConvert.

## Why It Might Matter

Wrong task wiring in jobs, failed conversions, or subtle metadata corruption when callers use `option()` for forward-compatible fields that collide with SDK field names.

## Proof

**Dataflow trace:** `option` → `extra.insert` → `from_payload` → `serialize_payload` → flattened JSON. `TaskRequest::serialize` re-injects `operation` last but does not protect per-task fields from prior overwrite.

## Counterevidence Checked

`operation` is injected after payload clone in `TaskRequest::serialize` (`tasks.rs:363-367`). Per-task canonical fields are not protected. Tests cover benign `option()` usage only.

## Suggested Next Step

Reject reserved task field names in `option()`, or nest extras instead of flattening.

## Agent Handoff

After working this report, preserve the original finding body. Update line 2 `Status: ...` and the final `DEVANA-SUMMARY:` status. Use one of: `open`, `fixed`, `invalid`, `stale`, `duplicate`, `wontfix`. Add dated notes below with the evidence checked.

## Status Notes

- 2026-06-25: open by Devana. Initial report written from static source inspection.
- 2026-06-26: fixed. Reproduced the collision with a standalone serde test: a flattened `extra` map declared *after* the named fields makes `serde_json::to_value` keep the extra value (`{"input":"WRONG"}`); declared *before* the named fields, the canonical field wins (`{"input":"RIGHT"}`). serde emits fields in declaration order and serde_json keeps the last write per key. Fix: moved the `#[serde(flatten)] extra` field to be the FIRST field of every task struct (16 typed tasks + the `pdf_task!` macro covering 8 PDF tasks), so canonical builder-set fields always overwrite any colliding `option()` key at serialization. Added invariant comments at all 17 flatten sites and regression test `task_option_cannot_overwrite_canonical_fields` (covers ConvertTask and a macro-generated PdfATask). GenericTask is unaffected (raw `data` field map, no canonical/extra split). Full suite green (39 task tests + lib + doctests).

DEVANA-KEY: src/tasks.rs:842-900 | P1 | extra-overwrites-task-core-fields
DEVANA-SUMMARY: Status=fixed | P1 high src/tasks.rs:842-900 - Flattened task `extra` now declared first in every task struct so canonical fields (input/output_format/etc.) win over colliding option() keys at serialize time.