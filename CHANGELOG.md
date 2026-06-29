# Changelog

## 0.3.0 - 2026-06-29

### Changed

- Raised the minimum supported Rust version to 1.87.
- Restricted automatic retries to idempotent HTTP methods.
- Normalized custom base URLs before joining API paths.
- Made duplicate explicit task names receive numeric suffixes instead of overwriting earlier tasks.
- Tightened operation metadata validation for format identity, array option values, and strict operation-specific structural fields.

### Fixed

- Preserved canonical task and job fields when callers pass colliding extra options.
- Re-subscribed socket channels after reconnects and reconciled socket waits with final API reads before reporting failures.
- Skipped null fields in presigned upload forms.
- Parsed HTTP-date `Retry-After` values and prevented retry backoff overflow panics.

## 0.2.0 - 2026-06-17

### Added

- Added recorded CloudConvert operation metadata contract fixtures and offline parsing tests.
- Added typed invalid-builder-state errors for invalid linear job builder shorthand order.

### Changed

- Linear job builder shorthand methods that infer input from the previous task now return `Result<Self>` instead of panicking.
- Updated examples and documentation to show fallible linear job builder chaining.
