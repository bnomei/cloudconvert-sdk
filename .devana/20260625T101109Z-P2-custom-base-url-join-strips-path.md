DEVANA-FINDING: v1
Priority: P2 | Confidence: high | Security-sensitive: no | Status: fixed
Location: src/client.rs:521-523, src/config.rs:392-396 | Slug: custom-base-url-join-strips-path

# Custom API base URL without trailing slash drops path segment

## Finding

`api_url` resolves endpoints with `base.join(path)` and `with_base_urls` stores caller-provided `Url` values without normalizing a trailing slash. Per RFC 3986, joining `"jobs"` against `https://api.example.com/v2` (no trailing `/`) yields `https://api.example.com/jobs`, dropping the `v2` segment.

## Violated Invariant Or Contract

`with_base_urls(api, sync)` should resolve resource paths under the configured API root (e.g. `/v2/jobs`).

## Oracle

Default bases include trailing slashes (`config.rs:8-11`: `https://api.cloudconvert.com/v2/`). `api_url` (`client.rs:521-523`) performs bare `join`. Tests pass mock bases that already end with `/` (`tests/client.rs` `mock_client`).

## Counterexample

```rust
ClientBuilder::new(key)
    .with_base_urls(
        Url::parse("https://api.example.com/v2")?,
        Url::parse("https://sync.api.example.com/v2")?,
    )
    .build()?;
client.jobs().list(&JobListQuery::default()).await?;
// GET https://api.example.com/jobs instead of https://api.example.com/v2/jobs
```

## Why It Might Matter

404s, wrong-environment calls, or proxy misrouting for integrations that set custom base URLs without a trailing slash.

## Proof

**Counterexample value:** base `https://api.example.com/v2` + path `"jobs"`. **Contract mismatch:** documented defaults vs unnormalized custom URLs.

## Counterevidence Checked

Built-in `API_BASE` / `SANDBOX_API_BASE` constants end with `/`. `tests/security.rs` custom URL tests use trailing slashes. No normalization in `ClientBuilder::build`.

## Suggested Next Step

Normalize custom bases to end with `/` in `with_base_urls`, or document and validate the requirement.

## Agent Handoff

After working this report, preserve the original finding body. Update line 2 `Status: ...` and the final `DEVANA-SUMMARY:` status. Use one of: `open`, `fixed`, `invalid`, `stale`, `duplicate`, `wontfix`. Add dated notes below with the evidence checked.

## Status Notes

- 2026-06-25: open by Devana. Initial report written from static source inspection.
- 2026-06-26: fixed. Confirmed `api_url` (now src/client.rs:549-552) does a bare `base.join(path)` and `with_base_urls` stored caller URLs verbatim, so `https://api.example.com/v2` + `jobs` resolved to `.../jobs`, dropping `/v2`. Fix: added `ensure_trailing_slash` in config.rs and applied it in `ClientBuilder::build` to both resolved `api_base_url` and `sync_base_url` (covers custom URLs and any future entry point; defaults already end with `/` so they are unchanged). Added a regression assertion in tests/security.rs `builder_resolves_default_sandbox_region_and_custom_urls`: a base without a trailing slash is normalized to `.../v2/` and `join("jobs")` yields `.../v2/jobs`. Full suite green + clippy clean.

DEVANA-KEY: src/client.rs:521-523, src/config.rs:392-396 | P2 | custom-base-url-join-strips-path
DEVANA-SUMMARY: Status=fixed | P2 high src/client.rs:521-523, src/config.rs:392-396 - Custom base URLs are normalized to a trailing slash in build(), so path joins keep the configured root segment.