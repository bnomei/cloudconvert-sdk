# Changelog

## 0.2.0 - 2026-06-17

### Added

- Added recorded CloudConvert operation metadata contract fixtures and offline parsing tests.
- Added typed invalid-builder-state errors for invalid linear job builder shorthand order.

### Changed

- Linear job builder shorthand methods that infer input from the previous task now return `Result<Self>` instead of panicking.
- Updated examples and documentation to show fallible linear job builder chaining.
