DEVANA-FINDING: v1
Priority: P2 | Confidence: high | Security-sensitive: no | Status: fixed
Location: src/operations.rs:195-204,545-557 | Slug: validate-strict-rejects-operation-fields

# validate_task_strict rejects required operation-specific payload fields

## Finding

In strict mode, `validate_task_with_mode` flags any payload key not present in `operation.options` and not listed in `is_common_task_field`. Operation-specific required fields such as `url` on `import/url` are not in that allowlist, so valid SDK-built tasks fail strict validation whenever the operation record has a non-empty options map.

## Violated Invariant Or Contract

`validate_task_strict` should accept SDK-built tasks that serialize to valid CloudConvert payloads. It should reject only undocumented extras, not canonical operation fields.

## Oracle

`is_common_task_field` (`operations.rs:545-557`) lists only `input`, `ignore_error`, formats, `engine`, `engine_version`, `filename`, `timeout`. `ImportUrlTask` serializes a top-level `url` field (`tasks.rs:471-477`). Strict tests use `convert` tasks only (`tests/client.rs:230-266`); `metadata/write` test whitelists `input` and `metadata` (`tests/metadata_contract.rs:105-106`).

## Counterexample

```rust
let operation = /* import/url metadata with options: { "filename": {...} } */;
let task = TaskRequest::from(ImportUrlTask::new("https://example.test/in.pdf"));
operation.validate_task_strict(&task);
// Err(UnknownOption) on "url"
```

## Why It Might Matter

Metadata-driven integrations cannot safely use strict validation on import/export and most non-convert tasks, undermining the strict mode contract.

## Proof

**Dataflow trace:** `task.payload().keys()` → strict loop (`195-204`) → `UnknownOption` when `url` ∉ `options` ∧ ¬`is_common_task_field`.

## Counterevidence Checked

Lenient mode passes. Convert-only strict tests do not cover `import/url`. `headers` on `ImportUrlTask` would also fail under the same rule.

## Suggested Next Step

Expand `is_common_task_field` per operation family, or derive allowed core fields from operation name instead of options map keys alone.

## Agent Handoff

After working this report, preserve the original finding body. Update line 2 `Status: ...` and the final `DEVANA-SUMMARY:` status. Use one of: `open`, `fixed`, `invalid`, `stale`, `duplicate`, `wontfix`. Add dated notes below with the evidence checked.

## Status Notes

- 2026-06-25: open by Devana. Initial report written from static source inspection.
- 2026-06-26: fixed. Confirmed the strict loop (operations.rs:195-204) flags any payload key not in `operation.options` and not in `is_common_task_field`, which only listed 8 shared fields — so structural fields like `url`/`headers` (import/url), provider credentials (s3/azure/gcs/openstack/sftp), and `command`/`arguments`/`metadata` failed strict validation. Fix: enumerated every non-option struct field across tasks.rs and expanded `is_common_task_field` with the structural set (input/output references, object-storage location+credential fields, and the primary required payloads of command/metadata-write), grouped with comments. Deliberately did NOT add tunable options (convert/watermark/optimize/thumbnail settings such as width/fit/text/font_*/opacity/profile) so strict mode still validates those against the options map. Added test `strict_validation_accepts_structural_operation_fields` (import/url task with url+headers passes strict against an operation whose options omit them; a genuine unknown option still errors). Full suite green + clippy clean.

DEVANA-KEY: src/operations.rs:195-204,545-557 | P2 | validate-strict-rejects-operation-fields
DEVANA-SUMMARY: Status=fixed | P2 high src/operations.rs:195-204,545-557 - is_common_task_field now recognizes structural import/export/credential/command/metadata fields, so strict validation no longer rejects canonical task fields while still catching unknown options.