DEVANA-FINDING: v1
Priority: P2 | Confidence: high | Security-sensitive: no | Status: fixed
Location: src/operations.rs:269-278 | Slug: validate-array-option-rejects-valid-arrays

# Array-typed operation options reject every valid array value

## Finding

`OperationOption::validate_value_for_operation` enforces `possible_values` by
comparing the *entire* submitted value against each entry with
`possible_values.iter().any(|allowed| allowed == value)` (src/operations.rs:269-270).
For an option whose CloudConvert `type` is `array`, the metadata `possible_values`
enumerate the allowed *elements* of the array, not allowed whole-field values. So
a submitted JSON array is compared for equality against scalar strings and never
matches, and any array value is rejected as `InvalidOptionValue`.

## Violated Invariant Or Contract

For an `array` option, validation must accept a value that is an array whose
elements are all members of `possible_values`. The code already models arrays as
first class (`OperationOptionKind::Array`, `matches_value` accepts `is_array`),
so an all-valid array must pass `validate_task`, not fail it.

## Oracle

Recorded operation metadata fixture
`tests/fixtures/cloudconvert/operations-metadata-write.json:14-18` defines the
`metadata/write` option `remove` as
`{"type":"array","possible_values":["Author","Title","Subject","Keywords"]}`.
The documented valid input is an array such as `["Author","Title"]`.

## Counterexample

```rust
// operation = metadata/write Operation parsed from the recorded fixture
let task = TaskRequest::custom("metadata/write")
    .field("input", "import-file")
    .field("remove", serde_json::json!(["Author"]))
    .into();
operation.validate_task(&task); // => Err(InvalidOptionValue) for `remove`
```

`["Author"]` is composed solely of documented allowed members, yet it is rejected.

## Why It Might Matter

`validate_task` / `validate_task_strict` are public pre-flight helpers. They
falsely reject legitimate, API-valid task configurations for any array option
that carries `possible_values`, blocking correct usage and giving the caller a
misleading `InvalidOptionValue` error. The same defect applies to any future
`dictionary`/`array` option with enumerated member values.

## Proof

Control-flow + contract mismatch. In `validate_task_with_mode`
(src/operations.rs:180-193) each present option value reaches
`validate_value_for_operation`. With `kind = Array`, `matches_value` returns true
(`value.is_array()`, src/operations.rs:322), so flow reaches the `possible_values`
gate at 269-270. Each `allowed` is a JSON string (`"Author"`); `value` is a JSON
array (`["Author"]`); `Value::eq` between a string and an array is always false,
so `any(...)` is false and `invalid_value` is returned (272-277). No per-element
branch exists for `kind == Array`.

## Counterevidence Checked

- `matches_value` does not short-circuit the whitelist; it only checks the JSON
  shape, then control continues into the broken equality check.
- This is distinct from the two filed operations findings:
  `validate-strict-rejects-operation-fields` (strict-mode unknown keys) and
  `validate-task-ignores-format-identity` (format/engine identity). This one is
  about `possible_values` element-vs-whole-value semantics for array kinds.
- The existing fixture test only inspects the option shape; it never calls
  `validate_task` with a populated `remove` array, so the bug is untested.

## Suggested Next Step

When `kind == Array` (and the value is an array), validate each element against
`possible_values` instead of comparing the whole array; mirror the same for
`dictionary` member validation if applicable.

## Agent Handoff

After working this report, preserve the original finding body. Update line 2
`Status: ...` and the final `DEVANA-SUMMARY:` status. Use one of: `open`,
`fixed`, `invalid`, `stale`, `duplicate`, `wontfix`. Add dated notes below with
the evidence checked.

## Status Notes

- 2026-06-25: open by Devana. Initial report written from static source inspection; counterexample confirmed against recorded metadata-write fixture.
- 2026-06-26: fixed. Confirmed `validate_value_for_operation` compared the whole submitted value against each `possible_values` entry, so an `array`-kind option (whose `possible_values` enumerate allowed ELEMENTS) never matched and every array was rejected as `InvalidOptionValue`. Fix: when `kind == Array` and the value is a JSON array, validate that every element is a member of `possible_values` (`items.iter().all(|item| possible_values contains item)`); other kinds keep the whole-value comparison. Empty arrays validate vacuously. Added a regression test in tests/metadata_contract.rs against the recorded metadata-write fixture: `remove: ["Author","Title"]` (all documented) validates; `remove: ["Author","Bogus"]` returns InvalidOptionValue on `remove`. (Dictionary member semantics left unchanged — no fixture/clear contract; the metadata dictionary option carries no possible_values.) Full suite green + clippy clean.

DEVANA-KEY: src/operations.rs:269-278 | P2 | validate-array-option-rejects-valid-arrays
DEVANA-SUMMARY: Status=fixed | P2 high src/operations.rs:269-278 - Array options now validate each element against possible_values instead of comparing the whole array, so valid arrays pass.
