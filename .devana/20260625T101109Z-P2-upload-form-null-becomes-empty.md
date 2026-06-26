DEVANA-FINDING: v1
Priority: P2 | Confidence: medium | Security-sensitive: no | Status: fixed
Location: src/client.rs:1058-1065,241-243 | Slug: upload-form-null-becomes-empty

# Upload form serializes JSON null parameters as empty text fields

## Finding

`upload_part` converts every entry in `UploadForm.parameters` through `form_value`, which maps `Value::Null` to an empty string and sends it via `multipart.text`. Optional parameters present as JSON `null` in the task result therefore become extra empty form fields instead of being omitted.

## Violated Invariant Or Contract

Presigned upload multipart fields must match the signed form parameters exactly. Null or absent optional fields should not be altered relative to the policy signature set.

## Oracle

Test fixture `upload_task()` in `tests/client.rs:1210-1218` includes `"empty": null` alongside other heterogeneous values. Upload tests assert signature and filename presence only, not per-field encoding (`tests/client.rs:931-935`).

## Counterexample

Task result contains `"parameters": { "optional": null }`. `form_value` returns `""`; `multipart.text("optional", "")` submits an extra empty field that may not be included in the S3/policy signature set, causing signature mismatch failures at upload time.

## Why It Might Matter

Intermittent upload failures when CloudConvert returns null-valued optional form parameters, especially across storage backends with strict signature validation.

## Proof

**Dataflow trace:** `upload_part` (`241-243`) → `form_value` Null arm (`1063`) → `multipart.text`. **Counterexample value:** `"empty": null` from test fixture.

## Counterevidence Checked

String and number parameters likely serialize correctly. `skip_serializing_if` on Rust `Option::None` applies to outbound task JSON, not inbound deserialized `Value::Null` in upload forms. No omission branch for null parameters.

## Suggested Next Step

Skip `Value::Null` parameters in the multipart loop, or preserve the signed representation expected by each storage backend.

## Agent Handoff

After working this report, preserve the original finding body. Update line 2 `Status: ...` and the final `DEVANA-SUMMARY:` status. Use one of: `open`, `fixed`, `invalid`, `stale`, `duplicate`, `wontfix`. Add dated notes below with the evidence checked.

## Status Notes

- 2026-06-25: open by Devana. Initial report written from static source inspection.
- 2026-06-26: fixed. Confirmed `upload_part` iterated all `form.parameters` and `form_value` maps `Value::Null` to `""`, so a null parameter became an extra empty multipart field outside the signed parameter set. Fix: skip `value.is_null()` parameters in the multipart loop (treat null as absent), so only real signed fields are submitted. Kept `form_value`'s explicit `Null => ""` arm as a defensive fallback (its catch-all would otherwise stringify null to "null"). Extended `uploads_to_presigned_form_without_bearer_auth` (fixture already carries `"empty": null`) to assert the `signature`/`enabled` fields are present and `name="empty"` is absent from the multipart body. Full suite green + clippy clean.

DEVANA-KEY: src/client.rs:1058-1065,241-243 | P2 | upload-form-null-becomes-empty
DEVANA-SUMMARY: Status=fixed | P2 medium src/client.rs:1058-1065,241-243 - Null upload form parameters are now skipped instead of submitted as empty multipart fields.