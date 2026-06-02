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

The crate is built for Tokio applications that need to create CloudConvert jobs,
upload files, wait for results, download `export/url` outputs, inspect operation
metadata, verify webhooks, or use OAuth access tokens without hand-building
every request.

This is an unofficial library. For service behavior, scopes, formats, engines,
regions, sandbox usage, and operation-specific options, use the
[official CloudConvert API documentation](https://cloudconvert.com/docs) and the
[CloudConvert Job Builder](https://cloudconvert.com/job-builder).

## Installation

```toml
[dependencies]
cloudconvert-sdk = "0.1"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

The crate is a library only. It does not install a binary.

## Quick Start

Create an API key in the
[CloudConvert dashboard](https://cloudconvert.com/dashboard/api/v2/keys) and
export it:

```sh
export CLOUDCONVERT_API_KEY=...
```

Then build a job, wait for it, and download the `export/url` result:

```rust
use cloudconvert_sdk::{ApiKey, CloudConvertClient, FileExtension, JobCreateRequest};

#[tokio::main]
async fn main() -> cloudconvert_sdk::Result<()> {
    let client = CloudConvertClient::builder(ApiKey::from_env()?).build()?;

    let request = JobCreateRequest::linear()
        .import_url("https://example.test/input.docx")
        .convert(FileExtension::Pdf)
        .export_url()
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

For production workflows, prefer CloudConvert webhooks or Socket.io waits over
long blocking waits.

## Job Builders

CloudConvert jobs serialize tasks as an object keyed by task name. The SDK
generates those names unless you choose to provide explicit names.

### Linear Jobs

Use `JobCreateRequest::linear()` when every task feeds into the next task:

```rust
use cloudconvert_sdk::{FileExtension, JobCreateRequest};

let request = JobCreateRequest::linear()
    .import_url("https://example.test/input.docx")
    .convert(FileExtension::Pdf)
    .export_url()
    .build();
```

Use `*_with(...)` methods when a task needs options but the job is still
serial:

```rust
let request = JobCreateRequest::linear()
    .import_url_with("https://example.test/input.docx", |task| {
        task.filename("input.docx")
    })
    .convert_with(FileExtension::Pdf, |task| {
        task.input_format(FileExtension::Docx)
            .engine("office")
            .filename("converted.pdf")
    })
    .export_url_with(|task| task.inline(false))
    .build();
```

### Graph Jobs

Use `JobCreateRequest::graph(|job| ...)` when a job branches, joins multiple
inputs, or needs to reference a non-adjacent task. Each graph method returns a
`TaskName` handle.

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
For operations not yet typed by the SDK, use `TaskRequest::custom(...)`.

## File Extensions

Use `FileExtension` for known CloudConvert format tokens:

```rust
use cloudconvert_sdk::{ConvertTask, FileExtension};

let task = ConvertTask::new("upload-file", FileExtension::Pdf)
    .input_format(FileExtension::Docx);
```

Format setters still accept strings for forward compatibility. Strings are
normalized by trimming leading dots and lowercasing ASCII, so `.PDF` and `PDF`
serialize as `pdf`.

## Uploads And Downloads

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
        })
        .export_url()
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

## API Overview

The crate exports typed resource clients from `CloudConvertClient`:

- `jobs()` creates, lists, fetches, waits for, redirects, and deletes jobs.
- `tasks()` creates standalone tasks, lists, fetches, waits for, cancels,
  retries, and deletes tasks.
- `operations()` lists operation metadata, options, engine versions, and can
  validate task payloads against returned metadata.
- `users()` reads the authenticated user.
- `webhooks()` creates, lists, and deletes webhooks.

Useful helpers:

- `download(...)`, `download_stream(...)`, and `download_to_path(...)`.
- `upload_bytes(...)`, `upload_body(...)`, `upload_stream(...)`, and
  `upload_path(...)`.
- `sign_payload(...)` and `verify_signature(...)` for webhook signatures.
- `sign_job_url(...)` for signed job-template URLs.
- `socket_base_url(...)`, `SocketChannel`, `JobSocketEvent`, and
  `TaskSocketEvent` for Socket.io payloads.

Client setup supports API keys, OAuth access tokens, sandbox mode, custom
regions, custom base URLs, custom reqwest clients, transport timeouts, and the
optional `retry` and `socket` features.

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

`OAuthAccessToken`, `OAuthRefreshToken`, and `OAuthClientSecret` redact debug
output. OAuth-backed clients use the same SDK resources and Socket.io helpers as
API-key clients.

## Operation Metadata

For metadata-driven integrations, call `operations().list(...)` with
`include_options()` or `include_options_and_engine_versions()`:

```rust
use cloudconvert_sdk::{ConvertTask, OperationListQuery, TaskRequest};

async fn validate(client: cloudconvert_sdk::CloudConvertClient) -> cloudconvert_sdk::Result<()> {
    let operation = client.operations().list(
        &OperationListQuery::default()
            .operation("convert")
            .input_format("docx")
            .output_format("pdf")
            .include_options_and_engine_versions(),
    ).await?.remove(0);

    let task = TaskRequest::from(ConvertTask::new("import-file", "pdf"));
    operation.validate_task(&task).expect("task should match operation metadata");
    Ok(())
}
```

Use `option(...)` builder methods, `extra` maps, or `TaskRequest::custom(...)`
for operation-specific options that are not yet typed by this SDK.

## Retry

Automatic retry is off by default. Enable the optional feature and set a policy:

```toml
cloudconvert-sdk = { version = "0.1", features = ["retry"] }
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

Retry covers CloudConvert API and synchronous API requests for transient
statuses `429`, `500`, `502`, `503`, and `504`, plus connect and timeout
errors. Signed `import/upload` form submissions and `export/url` downloads stay
outside that retry boundary.

## Socket.io Waits

Enable the optional feature when an async application wants lower-latency
completion than polling and does not expose a public webhook receiver:

```toml
cloudconvert-sdk = { version = "0.1", features = ["socket"] }
```

```rust
use cloudconvert_sdk::{ApiKey, CloudConvertClient, FileExtension, JobCreateRequest};

async fn run() -> cloudconvert_sdk::Result<()> {
    let client = CloudConvertClient::builder(ApiKey::from_env()?).build()?;
    let request = JobCreateRequest::linear()
        .import_url("https://example.test/input.docx")
        .convert(FileExtension::Pdf)
        .export_url()
        .build();

    let finished = client.jobs().create_and_wait_socket(request).await?;
    for file in finished.export_urls() {
        println!("{}", file.filename);
    }

    Ok(())
}
```

The managed wait helpers subscribe, check the current resource state to avoid
missing fast completions, wait for a terminal Socket.io event, and disconnect.
Use webhooks when CloudConvert can call your service directly.

For streams, use `client.socket(...)` with `SocketChannel`,
`jobs().task_events_socket(job_id)`, or `users().events_socket()`.

## Runnable Examples

These examples build request payloads and print JSON. They do not call the live
CloudConvert API, so they are safe to run without credentials:

```sh
cargo run --example build_job
cargo run --example linear_options_job
cargo run --example branch_job
cargo run --example advanced_job
cargo run --example file_extensions
```

## Build Tasks

```sh
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo check --workspace --all-targets --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-targets --locked
cargo test --workspace --all-targets --all-features --locked
```

CI also generates an `llvm-cov` HTML coverage artifact and enforces the current
coverage threshold.

## Live API Tests

Live CloudConvert tests are ignored by default so normal CI and `cargo test` do
not consume API credits.

Put a real key in `.env` or the process environment:

```sh
CLOUDCONVERT_API_KEY=...
```

Run the live group explicitly:

```sh
cargo test --test live_api -- --ignored
```

The live group keeps API usage small. It creates and deletes live tasks/jobs,
including a watermark job shape, and has one ignored upload/convert/export flow
with a tiny generated text file. It needs task/job scopes, not `user.read`.
