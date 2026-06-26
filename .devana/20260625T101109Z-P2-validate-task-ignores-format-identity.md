DEVANA-FINDING: v1
Priority: P2 | Confidence: high | Security-sensitive: no | Status: fixed
Location: src/operations.rs:168-207 | Slug: validate-task-ignores-format-identity

# validate_task does not compare task formats to operation record

## Finding

`Operation::validate_task_with_mode` checks that `task.operation()` matches `self.operation` and validates entries in `self.options`, but never compares the task payload's `input_format`, `output_format`, `engine`, or `engine_version` against the corresponding fields on the `Operation` record.

## Violated Invariant Or Contract

Validating a task against a specific `Operation` metadata row should reject tasks whose formats or engine disagree with that row.

## Oracle

Fixture `tests/fixtures/cloudconvert/operations-convert-docx-pdf.json` describes `docx → pdf` with engine `office`. `Operation` deserializes `input_format`, `output_format`, and `engine` (`operations.rs:104-112`) but validation never reads them.

## Counterexample

```rust
let operation = /* convert docx→pdf fixture */;
let task = TaskRequest::from(ConvertTask::new("import-file", "png"));
operation.validate_task(&task); // Ok(()) — wrong output_format for this record
```

## Why It Might Matter

False confidence in metadata-driven validation; wrong engine/format combinations reach the API despite passing local checks.

## Proof

**Contract mismatch:** caller expects operation record to bound task shape; implementation only checks option-level constraints. **Control-flow trace:** no comparison of `self.input_format` / `self.output_format` / `self.engine` to `task.payload()`.

## Counterevidence Checked

Option required/type/value tests in `tests/client.rs`. Fixture tests assert deserialization only (`tests/metadata_contract.rs`). No cross-field format matching tests.

## Suggested Next Step

Compare canonical payload fields to `Operation` identity fields when present, or document that callers must filter operations before validation.

## Agent Handoff

After working this report, preserve the original finding body. Update line 2 `Status: ...` and the final `DEVANA-SUMMARY:` status. Use one of: `open`, `fixed`, `invalid`, `stale`, `duplicate`, `wontfix`. Add dated notes below with the evidence checked.

## Status Notes

- 2026-06-25: open by Devana. Initial report written from static source inspection.
- 2026-06-26: fixed. Confirmed `validate_task_with_mode` only checked the operation name and the options map, never the `Operation` record's `input_format`/`output_format`/`engine`/`engine_version`. Fix: added a new `OperationValidationErrorKind::FormatMismatch` variant (enum is `#[non_exhaustive]`, so non-breaking) with a `format_mismatch` constructor and a `check_identity_field` helper. After the operation-name check (so it applies in both lenient and strict modes), the task payload's `input_format`, `output_format`, `engine`, and `engine_version` are compared against the record; a disagreement (when both sides specify the field) returns `FormatMismatch` with expected/actual. Compare-only-when-both-present keeps optional/defaulted fields from false-positiving. Added a regression assertion to `operation_metadata_validates_task_requests_when_requested` (output_format jpg vs the record's png → FormatMismatch on output_format). Existing convert/metadata tests still pass (their formats match the record). Full suite green + clippy clean.

DEVANA-KEY: src/operations.rs:168-207 | P2 | validate-task-ignores-format-identity
DEVANA-SUMMARY: Status=fixed | P2 high src/operations.rs:168-207 - validate_task now compares task input/output_format and engine/engine_version against the Operation record and returns FormatMismatch on disagreement.