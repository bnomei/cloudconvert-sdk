DEVANA-FINDING: v1
Priority: P2 | Confidence: high | Security-sensitive: no | Status: fixed
Location: src/jobs.rs:399-443 | Slug: duplicate-task-name-silent-overwrite

# Explicit duplicate task names silently overwrite prior tasks

## Finding

`JobBuilder::task`, `JobBuilder::add_named_task`, and `JobGraphBuilder::add_named_task` insert into the job's `BTreeMap<String, TaskRequest>` without checking for an existing key. A second insert with the same name overwrites the first task silently while earlier `input` references may still point at that name.

## Violated Invariant Or Contract

Each named task in the serialized `tasks` object must be retained. Duplicate explicit names should error or auto-deduplicate like `add_task` does for generated names.

## Oracle

`add_task` uses `generated_task_name` to suffix duplicates (`jobs.rs:424-429`, docs at 410-412). Explicit-name paths use bare `BTreeMap::insert` (`jobs.rs:401-403`, `439-441`).

## Counterexample

```rust
let mut b = JobCreateRequest::builder();
b.task("import-url", TaskRequest::import_url("https://a.test"));
b.task("import-url", TaskRequest::import_url("https://b.test"));
let req = b.build();
// req.tasks().len() == 1; only the second URL remains
```

## Why It Might Matter

Lost import sources, broken graphs, and hard-to-debug job payloads where dependency names reference tasks that no longer exist in the map.

## Proof

**Control-flow trace:** `insert` return value ignored; `last_task` updated to the latest entry. **Counterexample value:** duplicate `"import-url"` key.

## Counterevidence Checked

`generated_task_name` handles auto-named duplicates (`convert-2`, etc.). No validation on explicit `task()` / `add_named_task()`. Tests use unique explicit names only.

## Suggested Next Step

Return `InvalidBuilderState` on duplicate explicit names, or apply the same suffix strategy as `add_task`.

## Agent Handoff

After working this report, preserve the original finding body. Update line 2 `Status: ...` and the final `DEVANA-SUMMARY:` status. Use one of: `open`, `fixed`, `invalid`, `stale`, `duplicate`, `wontfix`. Add dated notes below with the evidence checked.

## Status Notes

- 2026-06-25: open by Devana. Initial report written from static source inspection.
- 2026-06-26: fixed. Confirmed `JobBuilder::task`, `JobBuilder::add_named_task`, and (by delegation) `JobGraphBuilder::add_named_task` did a bare `BTreeMap::insert`, silently overwriting an existing task. The builder methods return `Self`/`TaskName` and `build()` returns a plain `JobCreateRequest` (no Result), so erroring would force an API break; chose auto-dedup to match `add_task`. Extracted `deduplicated_task_name(base, existing)` from `generated_task_name` (which now normalizes then calls it) and routed all explicit-name inserts through it, so a duplicate name gets a `-2`/`-3` suffix instead of clobbering. `add_named_task` returns the actual (possibly suffixed) handle. Updated docs and added regression tests `duplicate_explicit_task_names_are_suffixed_not_overwritten` and `duplicate_named_task_handle_reflects_suffixed_name`. Full suite green + clippy clean.

DEVANA-KEY: src/jobs.rs:399-443 | P2 | duplicate-task-name-silent-overwrite
DEVANA-SUMMARY: Status=fixed | P2 high src/jobs.rs:399-443 - Explicit duplicate task names are now suffixed (name-2, name-3, ...) like generated names, so no task is silently overwritten.