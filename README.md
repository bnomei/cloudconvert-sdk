# cloudconvert-sdk

[![Crates.io Version](https://img.shields.io/crates/v/cloudconvert-sdk)](https://crates.io/crates/cloudconvert-sdk)
[![Docs.rs](https://img.shields.io/docsrs/cloudconvert-sdk)](https://docs.rs/cloudconvert-sdk)
[![CI](https://img.shields.io/github/actions/workflow/status/bnomei/cloudconvert-sdk/ci.yml?branch=main)](https://github.com/bnomei/cloudconvert-sdk/actions/workflows/ci.yml)
[![Crates.io Downloads](https://img.shields.io/crates/d/cloudconvert-sdk)](https://crates.io/crates/cloudconvert-sdk)
[![License](https://img.shields.io/crates/l/cloudconvert-sdk)](https://crates.io/crates/cloudconvert-sdk)
[![Discord](https://flat.badgen.net/badge/discord/bnomei?color=7289da&icon=discord&label)](https://discordapp.com/users/bnomei)
[![Buymecoffee](https://flat.badgen.net/badge/icon/donate?icon=buymeacoffee&color=FF813F&label)](https://www.buymeacoffee.com/bnomei)

Async Rust SDK primitives for the [CloudConvert](https://cloudconvert.com)
API v2.

`cloudconvert-sdk` gives Tokio applications typed CloudConvert clients, job and
task builders, upload and download helpers, operation metadata validation,
OAuth 2.0 flows, webhook signature helpers, signed job URLs, and optional retry
and Socket.IO support.

This is an unofficial library. Use the
[official CloudConvert API documentation](https://cloudconvert.com/docs) and the
[CloudConvert Job Builder](https://cloudconvert.com/job-builder) as the source
of truth for service behavior, scopes, formats, engines, regions, sandbox usage,
and operation-specific options.

## Requirements

- Rust 1.87 or newer.
- Tokio for async API calls.
- A CloudConvert API key for live API calls. Payload builders, examples, and
  most tests run offline without credentials.

## Installation

Add the crate to your application:

```toml
[dependencies]
cloudconvert-sdk = "0.3"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

The crate is a library only. It does not install a binary.

Default features are empty. Enable optional features only when you need them:

```toml
cloudconvert-sdk = { version = "0.3", features = ["retry", "socket"] }
```

| Feature | Adds |
| --- | --- |
| `retry` | `RetryPolicy` and automatic retry for idempotent CloudConvert API requests. |
| `socket` | Socket.IO clients, streams, and managed socket wait helpers. |

## Configure the client

Create an API key in the
[CloudConvert dashboard](https://cloudconvert.com/dashboard/api/v2/keys), then
store it outside your source code:

```sh
export CLOUDCONVERT_API_KEY=<API_KEY>
```

Build a client from the environment:

```rust
use cloudconvert_sdk::{ApiKey, CloudConvertClient};

let client = CloudConvertClient::builder(ApiKey::from_env()?).build()?;
```

The builder also supports sandbox mode, regional production hosts, custom base
URLs, custom `reqwest` clients, and transport timeouts:

```rust
use std::time::Duration;

use cloudconvert_sdk::{ApiKey, CloudConvertClient, Region, TransportConfig};

let client = CloudConvertClient::builder(ApiKey::from_env()?)
    .region(Region::EuCentral)
    .transport_config(
        TransportConfig::default()
            .connect_timeout(Duration::from_secs(10))
            .request_timeout(Duration::from_secs(120))
            .user_agent("my-app/1.0"),
    )
    .build()?;
```

Use `.sandbox(true)` to route requests to CloudConvert sandbox hosts. Use
`Region::EuCentral`, `Region::UsEast`, or `Region::Custom(...)` for regional
production API hosts. Use `.with_base_urls(...)` for tests, proxies, or
non-standard deployments.

Secret-bearing types such as `ApiKey`, `OAuthAccessToken`, `OAuthRefreshToken`,
`OAuthClientSecret`, and `SigningSecret` redact their `Debug` output.

## Quick start

This example creates a URL import job, converts a DOCX file to PDF, waits for
the job to finish, and downloads each `export/url` result.

```rust
use cloudconvert_sdk::{ApiKey, CloudConvertClient, FileExtension, JobCreateRequest};

#[tokio::main]
async fn main() -> cloudconvert_sdk::Result<()> {
    let client = CloudConvertClient::builder(ApiKey::from_env()?).build()?;

    let request = JobCreateRequest::linear()
        .import_url("https://example.test/input.docx")
        .convert(FileExtension::Pdf)?
        .export_url()?
        .build();

    let job = client.jobs().create(request).await?;
    let finished = client.jobs().wait(&job.id).await?;

    for file in finished.export_urls() {
        if let Some(url) = &file.url {
            let bytes = client.download(url).await?;
            println!("downloaded {} bytes as {}", bytes.len(), file.filename);
        }
    }

    Ok(())
}
```

Expected result:

```txt
downloaded <bytes> bytes as <filename>
```

For production workflows, prefer CloudConvert webhooks or Socket.IO waits over
long blocking waits.

## Build jobs

CloudConvert jobs serialize tasks as an object keyed by task name. The SDK
generates task names unless you provide explicit names.

Use `JobCreateRequest::linear()` when every task feeds into the next task:

```rust
use cloudconvert_sdk::{FileExtension, JobCreateRequest};

fn build_request() -> cloudconvert_sdk::Result<JobCreateRequest> {
    let request = JobCreateRequest::linear()
        .import_url("https://example.test/input.docx")
        .convert(FileExtension::Pdf)?
        .export_url()?
        .build();

    Ok(request)
}
```

Linear methods that infer their input from the previous task return
`Error::InvalidBuilderState` when they are called before a source task exists.

Use `*_with(...)` methods when a serial task needs options:

```rust
use cloudconvert_sdk::{FileExtension, JobCreateRequest};

fn build_request() -> cloudconvert_sdk::Result<JobCreateRequest> {
    let request = JobCreateRequest::linear()
        .import_url_with("https://example.test/input.docx", |task| {
            task.filename("input.docx")
        })
        .convert_with(FileExtension::Pdf, |task| {
            task.input_format(FileExtension::Docx)
                .engine("office")
                .filename("converted.pdf")
        })?
        .export_url_with(|task| task.inline(false))?
        .build();

    Ok(request)
}
```

Use `JobCreateRequest::graph(|job| ...)` when a job branches, joins multiple
inputs, or references a non-adjacent task. Each graph method returns a
`TaskName` handle:

```rust
use cloudconvert_sdk::{FileExtension, JobCreateRequest};

let request = JobCreateRequest::graph(|job| {
    let source = job.import_url("https://example.test/input.docx");
    let pdf = job.convert(&source, FileExtension::Pdf);
    let png = job.convert(&source, FileExtension::Png);
    job.export_url([&pdf, &png]);
})
.tag("branch-demo")
.build();
```

`TaskName` handles also work for multi-input operations such as `merge`,
watermarks that use an imported image file, and `export/url` tasks that archive
multiple outputs:

```rust
use cloudconvert_sdk::{
    FileExtension, JobCreateRequest, Layer, PositionHorizontal, PositionVertical,
};

let request = JobCreateRequest::graph(|job| {
    let cover_docx = job.import_url_with("https://example.test/report-cover.docx", |task| {
        task.filename("cover.docx")
    });
    let body_docx = job.import_url_with("https://example.test/report-body.docx", |task| {
        task.filename("body.docx")
    });
    let logo_png = job.import_url_with("https://example.test/logo.png", |task| {
        task.filename("logo.png")
    });

    let cover_pdf = job.convert_with(&cover_docx, FileExtension::Pdf, |task| {
        task.input_format(FileExtension::Docx).filename("cover.pdf")
    });
    let body_pdf = job.convert_with(&body_docx, FileExtension::Pdf, |task| {
        task.input_format(FileExtension::Docx).filename("body.pdf")
    });
    let merged = job.merge_with([&cover_pdf, &body_pdf], FileExtension::Pdf, |task| {
        task.filename("report.pdf")
    });
    let watermarked = job.watermark_image_with(&merged, &logo_png, |task| {
        task.input_format(FileExtension::Pdf)
            .layer(Layer::Above)
            .image_width(180)
            .position(PositionVertical::Bottom, PositionHorizontal::Right)
            .margins(24, 24)
            .opacity(80)
            .filename("report-watermarked.pdf")
    });

    job.export_url_with([&cover_pdf, &body_pdf, &watermarked], |task| {
        task.archive_multiple_files(true)
    });
})
.tag("report-package")
.build();
```

When the task name itself matters, use `JobBuilder::task(...)`,
`JobBuilder::add_named_task(...)`, or `JobGraphBuilder::add_named_task(...)`.
For operations not yet typed by this SDK, use `TaskRequest::custom(...)`.

Typed builders cover imports from URL, upload, base64, raw content, S3, Azure
Blob, Google Cloud Storage, OpenStack, and SFTP; conversion, optimization,
watermarking, website capture, thumbnails, metadata, metadata writes, merge,
archive, command, and PDF tasks; and exports to URL, S3, Azure Blob, Google
Cloud Storage, OpenStack, SFTP, and upload callbacks.

## File extensions

Use `FileExtension` for known CloudConvert format tokens:

```rust
use cloudconvert_sdk::{ConvertTask, FileExtension};

let task = ConvertTask::new("upload-file", FileExtension::Pdf)
    .input_format(FileExtension::Docx);
```

Format setters still accept strings for forward compatibility. Strings are
normalized by trimming leading dots and lowercasing ASCII, so `.PDF` and `PDF`
serialize as `pdf`.

## Upload and download files

Use `import/upload` when your application already has the input file locally.
The job creation response contains the signed upload form; the SDK handles the
multipart upload helper.

```rust
use std::path::Path;

use cloudconvert_sdk::{ApiKey, CloudConvertClient, FileExtension, JobCreateRequest};

async fn run() -> cloudconvert_sdk::Result<()> {
    let client = CloudConvertClient::builder(ApiKey::from_env()?).build()?;
    let request = JobCreateRequest::linear()
        .import_upload()
        .convert_with(FileExtension::Pdf, |task| {
            task.input_format(FileExtension::Txt)
        })?
        .export_url()?
        .build();

    let job = client.jobs().create(request).await?;
    let upload_task_id = job
        .tasks
        .iter()
        .find(|task| task.operation == "import/upload")
        .and_then(|task| task.id.as_deref())
        .expect("import/upload task should have an id");

    let upload_task = client.tasks().get(upload_task_id).await?;
    client.upload_path(&upload_task, "input.txt").await?;

    let finished = client.jobs().wait(&job.id).await?;
    tokio::fs::create_dir_all("downloads").await?;

    for file in finished.export_urls() {
        if let Some(url) = &file.url {
            let filename = Path::new(&file.filename)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("download");
            client.download_to_path(url, Path::new("downloads").join(filename)).await?;
        }
    }

    Ok(())
}
```

Download helpers never attach CloudConvert bearer credentials to signed storage
URLs. Upload helpers submit to the signed form action returned by CloudConvert.

## API overview

The crate exports typed resource clients from `CloudConvertClient`:

| Client method | Purpose |
| --- | --- |
| `jobs()` | Create, list, fetch, wait for, redirect, and delete jobs. |
| `tasks()` | Create standalone tasks, list, fetch, wait for, cancel, retry, and delete tasks. |
| `operations()` | List operation metadata, options, and engine versions; validate task payloads against returned metadata. |
| `users()` | Read the authenticated user and, with `socket`, open user-scoped event streams. |
| `webhooks()` | Create, list, and delete webhooks. |

Useful helpers:

- `download(...)`, `download_stream(...)`, and `download_to_path(...)`.
- `upload_bytes(...)`, `upload_body(...)`, `upload_stream(...)`, and
  `upload_path(...)`.
- `sign_payload(...)` and `verify_signature(...)` for webhook signatures.
- `sign_job_url(...)` for signed job-template URLs.
- `socket_base_url(...)`, `SocketChannel`, `JobSocketEvent`, and
  `TaskSocketEvent` for Socket.IO payloads.

## OAuth 2.0

Use API keys for server-side integrations owned by one CloudConvert account. Use
OAuth when your app acts on behalf of CloudConvert users.

```rust
use cloudconvert_sdk::{
    JobListQuery, OAuthClient, OAuthClientSecret, OAuthScope,
};

async fn run() -> cloudconvert_sdk::Result<()> {
    let oauth = OAuthClient::new("client-id", OAuthClientSecret::new("client-secret"))?;
    let redirect = oauth.authorization_code_url_with_state(
        "https://app.example.test/cloudconvert/callback",
        [OAuthScope::TaskRead, OAuthScope::TaskWrite],
        "state-from-your-app",
    )?;

    // Redirect the user to `redirect`, then exchange the returned code.
    let token = oauth
        .exchange_code("authorization-code", "https://app.example.test/cloudconvert/callback")
        .await?;
    let client = token.into_client_builder().build()?;

    let _jobs = client.jobs().list(&JobListQuery::default()).await?;
    Ok(())
}
```

`OAuthAccessToken::from_env()`, `OAuthRefreshToken::from_env()`, and
`OAuthClientSecret::from_env()` read `CLOUDCONVERT_OAUTH_ACCESS_TOKEN`,
`CLOUDCONVERT_OAUTH_REFRESH_TOKEN`, and `CLOUDCONVERT_OAUTH_CLIENT_SECRET`.
OAuth-backed clients use the same SDK resources and Socket.IO helpers as
API-key clients.

## Webhooks and signed job URLs

Use `verify_signature(...)` to check inbound CloudConvert webhook payloads with
your webhook signing secret:

```rust
use cloudconvert_sdk::{SigningSecret, verify_signature};

let valid = verify_signature(
    br#"{"event":"job.finished"}"#,
    "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
    &SigningSecret::new("webhook-signing-secret"),
)?;
```

Use `sign_payload(...)` for tests that need CloudConvert-compatible webhook
signatures. Use `sign_job_url(...)` when you embed a signed job template into a
hosted CloudConvert job URL.

## Operation metadata

For metadata-driven integrations, call `operations().list(...)` with
`include_options()` or `include_options_and_engine_versions()`:

```rust
use cloudconvert_sdk::{ConvertTask, FileExtension, OperationListQuery, TaskRequest};

async fn validate(client: cloudconvert_sdk::CloudConvertClient) -> cloudconvert_sdk::Result<()> {
    let operation = client.operations().list(
        &OperationListQuery::default()
            .operation("convert")
            .input_format("docx")
            .output_format("pdf")
            .include_options_and_engine_versions(),
    ).await?.remove(0);

    let task = TaskRequest::from(
        ConvertTask::new("import-file", FileExtension::Pdf)
            .input_format(FileExtension::Docx),
    );
    operation.validate_task(&task).expect("task should match operation metadata");
    Ok(())
}
```

Use `option(...)` builder methods, `extra` maps, or `TaskRequest::custom(...)`
for operation-specific options that are not yet typed by this SDK.

### Recorded metadata fixtures

Normal CI parses committed fixtures from `tests/fixtures/cloudconvert/` and does
not require CloudConvert credentials. Refresh those fixtures only when you want
to intentionally review upstream metadata drift.

```sh
export CLOUDCONVERT_API_KEY=<API_KEY>

curl --get "https://api.cloudconvert.com/v2/operations" \
  --header "Authorization: Bearer ${CLOUDCONVERT_API_KEY}" \
  --data-urlencode "filter[operation]=convert" \
  --data-urlencode "filter[input_format]=docx" \
  --data-urlencode "filter[output_format]=pdf" \
  --data-urlencode "include=options,engine_versions" \
  --data-urlencode "alternatives=true" \
  --output tests/fixtures/cloudconvert/operations-convert-docx-pdf.json

curl --get "https://api.cloudconvert.com/v2/operations" \
  --header "Authorization: Bearer ${CLOUDCONVERT_API_KEY}" \
  --data-urlencode "filter[operation]=metadata" \
  --data-urlencode "filter[input_format]=pdf" \
  --data-urlencode "include=options,engine_versions" \
  --output tests/fixtures/cloudconvert/operations-metadata.json

curl --get "https://api.cloudconvert.com/v2/operations" \
  --header "Authorization: Bearer ${CLOUDCONVERT_API_KEY}" \
  --data-urlencode "filter[operation]=metadata/write" \
  --data-urlencode "filter[input_format]=pdf" \
  --data-urlencode "include=options,engine_versions" \
  --output tests/fixtures/cloudconvert/operations-metadata-write.json
```

Review fixture diffs before committing them. If the response shape changed,
update `tests/metadata_contract.rs` with an assertion for the new contract
instead of weakening existing checks.

## Retry

Automatic retry is off by default. Enable the optional feature and set a policy:

```toml
cloudconvert-sdk = { version = "0.3", features = ["retry"] }
```

```rust
use std::time::Duration;

use cloudconvert_sdk::{ApiKey, CloudConvertClient, RetryPolicy, TransportConfig};

let client = CloudConvertClient::builder(ApiKey::from_env()?)
    .transport_config(
        TransportConfig::default()
            .connect_timeout(Duration::from_secs(10))
            .request_timeout(Duration::from_secs(120)),
    )
    .retry_policy(
        RetryPolicy::new(3)
            .initial_delay(Duration::from_millis(250))
            .max_delay(Duration::from_secs(10)),
    )
    .build()?;
```

Retry applies only to idempotent CloudConvert API requests. It retries transient
statuses `429`, `500`, `502`, `503`, and `504`, plus connect and timeout errors.
It does not retry job or task creation, signed `import/upload` form
submissions, or `export/url` downloads.

## Socket.IO waits

Enable the optional feature when an async application wants lower-latency
completion than polling and does not expose a public webhook receiver:

```toml
cloudconvert-sdk = { version = "0.3", features = ["socket"] }
```

```rust
use cloudconvert_sdk::{ApiKey, CloudConvertClient, FileExtension, JobCreateRequest};

async fn run() -> cloudconvert_sdk::Result<()> {
    let client = CloudConvertClient::builder(ApiKey::from_env()?).build()?;
    let request = JobCreateRequest::linear()
        .import_url("https://example.test/input.docx")
        .convert(FileExtension::Pdf)?
        .export_url()?
        .build();

    let finished = client.jobs().create_and_wait_socket(request).await?;
    for file in finished.export_urls() {
        println!("{}", file.filename);
    }

    Ok(())
}
```

The managed wait helpers subscribe, check the current resource state to avoid
missing fast completions, wait for a terminal Socket.IO event, and disconnect.
Use webhooks when CloudConvert can call your service directly.

For streams, use `client.socket(...)` with `SocketChannel`,
`jobs().task_events_socket(job_id)`, or `users().events_socket()`.

## Runnable examples

These examples build request payloads and print JSON. They do not call the live
CloudConvert API, so they are safe to run without credentials:

```sh
cargo run --example build_job
cargo run --example linear_options_job
cargo run --example branch_job
cargo run --example advanced_job
cargo run --example file_extensions
```

## Build and test

Run the local validation loop before opening a pull request:

```sh
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo check --workspace --all-targets --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-targets --locked
cargo test --workspace --all-targets --all-features --locked
cargo test --doc --all-features --locked
```

CI also generates an `llvm-cov` HTML coverage artifact and enforces the current
line coverage threshold.

## Live API tests

Live CloudConvert tests are ignored by default so normal CI and `cargo test` do
not consume API credits.

Put a real key in `.env` or the process environment:

```sh
CLOUDCONVERT_API_KEY=<API_KEY>
```

Run the live group explicitly:

```sh
cargo test --test live_api -- --ignored
```

The live group keeps API usage small. It creates and deletes live tasks and
jobs, including a watermark job shape, and has one ignored
upload-convert-export flow with a tiny generated text file. It needs task and
job scopes, not `user.read`.

## Source map

- Public exports and crate-level examples: [`src/lib.rs`](src/lib.rs)
- Client configuration and optional retry policy: [`src/config.rs`](src/config.rs)
- Resource clients, upload helpers, and download helpers: [`src/client.rs`](src/client.rs)
- Job builders and response models: [`src/jobs.rs`](src/jobs.rs)
- Task payload builders: [`src/tasks.rs`](src/tasks.rs)
- Operation metadata validation: [`src/operations.rs`](src/operations.rs)
- OAuth 2.0 flows: [`src/oauth.rs`](src/oauth.rs)
- Webhook and signed URL helpers: [`src/webhook.rs`](src/webhook.rs),
  [`src/signed_url.rs`](src/signed_url.rs)
