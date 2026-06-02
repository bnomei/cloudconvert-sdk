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

CloudConvert already provides official SDKs for several ecosystems. This crate
exists to make the same API comfortable from Tokio-based Rust applications: build
jobs and tasks, upload files, wait for results, download `export/url` outputs,
discover operation options, verify webhooks, and create signed job URLs without
manually shaping every HTTP request.

This is an unofficial library. For service-level behavior, scopes, formats,
engines, regions, sandbox usage, and operation-specific options, use the
[official CloudConvert API documentation](https://cloudconvert.com/docs) and the
[CloudConvert Job Builder](https://cloudconvert.com/job-builder). The project
lives at <https://github.com/bnomei/cloudconvert-sdk>.

## Installation

Add the library to your application:

```toml
[dependencies]
cloudconvert-sdk = "0.1"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

The crate is a library only. It does not install a binary.

## Runnable Examples

The examples build request payloads and print JSON. They do not call the live
CloudConvert API, so they are safe to run without credentials:

```sh
cargo run --example build_job
cargo run --example linear_options_job
cargo run --example branch_job
cargo run --example advanced_job
cargo run --example file_extensions
```

## Quick Start

Create an API key in the
[CloudConvert dashboard](https://cloudconvert.com/dashboard/api/v2/keys) and put
it in the process environment:

```sh
export CLOUDCONVERT_API_KEY=...
```

### Convert a URL to PDF

This mirrors the
[official CloudConvert quickstart](https://cloudconvert.com/docs/getting-started/quickstart-guide)
flow: import a file from a URL, convert it, create an `export/url`, wait for the
job, and download the result.

```rust
use std::path::Path;

use cloudconvert_sdk::{ApiKey, CloudConvertClient, FileExtension, JobCreateRequest};

#[tokio::main]
async fn main() -> cloudconvert_sdk::Result<()> {
    let client = CloudConvertClient::builder(ApiKey::from_env()?).build()?;

    let request = JobCreateRequest::linear()
        .import_url("https://my.url/file.docx")
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

For production systems, prefer CloudConvert webhooks over blocking `wait` calls.

### Upload, convert, export

Use `import/upload` when your application already has the input file locally.
The job creation response contains the signed upload form; the SDK handles the
multipart upload helper.

```rust
use cloudconvert_sdk::{ApiKey, CloudConvertClient, FileExtension, JobCreateRequest};

#[tokio::main]
async fn main() -> cloudconvert_sdk::Result<()> {
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
    tokio::fs::create_dir_all("downloads").await?;
    for file in finished.export_urls() {
        if let Some(url) = &file.url {
            let safe_filename = Path::new(&file.filename)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("download");
            let destination = Path::new("downloads").join(safe_filename);
            client.download_to_path(url, destination).await?;
        }
    }

    Ok(())
}
```

### Multi-file job graph with task handles

CloudConvert jobs are graphs of tasks, not only linear pipelines. The
[introduction](https://cloudconvert.com/docs/getting-started/introduction)
notes that a single job can contain multiple conversions and export tasks. Some
operations naturally need multiple named inputs: `merge` takes input task
name(s) according to the
[merge docs](https://cloudconvert.com/docs/operations/merge-files), image
watermarks need a second imported file in the
[watermark docs](https://cloudconvert.com/docs/operations/add-watermarks), and
[`export/url`](https://cloudconvert.com/docs/import-export/export-files) can
export multiple input task name(s), optionally as one ZIP archive.

That is where `graph(|job| ...)` and `TaskName` handles are useful. Each graph
method returns the generated task name for that operation, so later tasks can
join branches without manually naming every task. This example imports two DOCX
files and a logo, converts both documents to PDF, merges the PDFs, adds the logo
as a watermark, and exports the individual PDFs plus the final watermarked
report as one archive.

```rust
use cloudconvert_sdk::{
    FileExtension, JobCreateRequest, Layer, PositionHorizontal, PositionVertical,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
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

    println!("{}", serde_json::to_string_pretty(&request)?);

    Ok(())
}
```

Run the same payload builder locally with:

```sh
cargo run --example advanced_job
```

## Public API Surface

The crate exports high-level resource clients from `CloudConvertClient` and
typed request/response models from the crate root.

### Client setup

- `CloudConvertClient::builder(ApiKey)` creates clients for live API usage.
- `ApiKey::from_env()` reads `CLOUDCONVERT_API_KEY`.
- `OAuthClient` builds CloudConvert OAuth authorization URLs and exchanges or
  refreshes OAuth tokens.
- `CloudConvertClient::builder_with_access_token(OAuthAccessToken)` creates API
  clients from OAuth access tokens.
- `sandbox(true)` switches to CloudConvert sandbox endpoints.
- `region(Region::EuCentral | Region::UsEast | Region::Custom(_))` selects a
  region-specific API base.
- `with_base_urls(...)`, `http_client(...)`, `http_clients(...)`, and
  `transport_config(...)` allow custom transport setup.
- The optional `retry` feature exposes `RetryPolicy` for transient HTTP status
  retries.
- The optional `socket` feature exposes a managed Socket.io client for
  event-driven waits.

### REST resources

Jobs:

- `POST /v2/jobs` via `jobs().create(...)` for async job creation.
- `POST /v2/jobs` on the synchronous API base via `jobs().create_and_wait(...)`.
- With the `socket` feature, `jobs().create_and_wait_socket(...)` creates a job,
  subscribes to its Socket.io channel, and returns when CloudConvert emits a
  terminal event.
- `GET /v2/jobs` via `jobs().list(...)` and `jobs().list_page(...)`.
- `GET /v2/jobs/{id}` via `jobs().get(...)` and `jobs().get_with_query(...)`.
- `GET /v2/jobs/{id}` on the synchronous API base via `jobs().wait(...)` and
  `jobs().wait_with_query(...)`.
- With the `socket` feature, `jobs().wait_socket(...)` waits for a job through
  Socket.io events instead of the synchronous API base.
- `jobs().get_redirect_url(...)`, `jobs().wait_redirect_url(...)`, and
  `jobs().create_and_wait_redirect_url(...)` expose CloudConvert redirect
  helpers for `export/url` outputs.
- `DELETE /v2/jobs/{id}` via `jobs().delete(...)`.

Tasks:

- `POST /v2/{operation}` via `tasks().create(...)` for standalone task
  creation.
- `GET /v2/tasks` via `tasks().list(...)` and `tasks().list_page(...)`.
- `GET /v2/tasks/{id}` via `tasks().get(...)` and `tasks().get_with_query(...)`.
- `GET /v2/tasks/{id}` on the synchronous API base via `tasks().wait(...)` and
  `tasks().wait_with_query(...)`.
- With the `socket` feature, `tasks().wait_socket(...)` waits for a task through
  Socket.io events.
- `POST /v2/tasks/{id}/cancel` via `tasks().cancel(...)`.
- `POST /v2/tasks/{id}/retry` via `tasks().retry(...)`.
- `DELETE /v2/tasks/{id}` via `tasks().delete(...)`.

Other resources:

- `GET /v2/operations` via `operations().list(...)` and
  `operations().list_page(...)`.
- Operation metadata includes option kinds, possible values, engine versions, and
  opt-in `Operation::validate_task(...)` helpers.
- `GET /v2/users/me` via `users().me()`.
- `POST /v2/webhooks` via `webhooks().create(...)`.
- `GET /v2/users/me/webhooks` via `webhooks().list(...)` and
  `webhooks().list_page(...)`.
- `DELETE /v2/webhooks/{id}` via `webhooks().delete(...)`.

Helpers:

- `download(...)`, `download_stream(...)`, and `download_to_path(...)` read
  `export/url` files from signed storage URLs.
- `upload_bytes(...)`, `upload_body(...)`, `upload_stream(...)`, and
  `upload_path(...)` upload to the signed form action returned by
  `import/upload` tasks.
- `sign_payload(...)` and `verify_signature(...)` handle webhook signatures.
- `sign_job_url(...)` creates signed URLs for job templates.
- `socket_base_url(...)`, `socket_subscription(...)`, `SocketChannel`,
  `JobSocketEvent`, `TaskSocketEvent`, and `SocketEventKind` model CloudConvert
  Socket.io payloads.
- With the `socket` feature, `CloudConvertSocket` and `SocketEvent` manage the
  Socket.io connection and decode job/task event payloads.

### Typed task builders

Import tasks:

- `import/url`
- `import/upload`
- `import/base64`
- `import/raw`
- `import/s3`
- `import/azure/blob`
- `import/google-cloud-storage`
- `import/openstack`
- `import/sftp`

Processing tasks:

- `convert`
- `optimize`
- `watermark`
- `capture-website`
- `thumbnail`
- `metadata`
- `metadata/write`
- `merge`
- `archive`
- `command`
- `pdf/a`
- `pdf/x`
- `pdf/ocr`
- `pdf/encrypt`
- `pdf/decrypt`
- `pdf/split-pages`
- `pdf/extract-pages`
- `pdf/rotate-pages`

Export tasks:

- `export/url`
- `export/s3`
- `export/azure/blob`
- `export/google-cloud-storage`
- `export/openstack`
- `export/sftp`
- `export/upload`

Use `TaskRequest::custom(...)` or `GenericTask::new(...)` for CloudConvert
operations that are not yet represented by a typed builder.

### Task names

CloudConvert jobs serialize tasks as an object keyed by task name, and
dependency inputs refer to those names. For linear jobs, use `linear()` and let
the SDK wire each task to the previous one:

```rust
use cloudconvert_sdk::{FileExtension, JobCreateRequest};

let request = JobCreateRequest::linear()
    .import_url("https://example.test/input.docx")
    .convert(FileExtension::Pdf)
    .export_url()
    .build();
```

Generated names are based on the operation (`import-url`, `convert`,
`export-url`) with numeric suffixes for duplicates. Use `*_with(...)` methods
when a linear task needs options:

```rust
let request = JobCreateRequest::linear()
    .import_url_with("https://example.test/input.docx", |task| {
        task.filename("input.docx")
    })
    .convert_with(FileExtension::Pdf, |task| {
        task.input_format(FileExtension::Docx)
    })
    .export_url_with(|task| task.inline(false))
    .build();
```

Use `JobCreateRequest::graph(|job| ...)` when branches or joins need task
handles:

```rust
let request = JobCreateRequest::graph(|job| {
    let import = job.import_url("https://example.test/input.docx");
    let pdf = job.convert(&import, FileExtension::Pdf);
    let png = job.convert(&import, FileExtension::Png);
    job.export_url([&pdf, &png]);
})
.build();
```

When the name itself matters, `JobBuilder::task("name", ...)`,
`JobBuilder::add_named_task(...)`, and `JobGraphBuilder::add_named_task(...)`
still preserve explicit CloudConvert task names.

### File extensions

Use `FileExtension` for known CloudConvert format tokens instead of spelling
strings by hand:

```rust
use cloudconvert_sdk::{ConvertTask, FileExtension};

let task = ConvertTask::new("upload-file", FileExtension::Pdf)
    .input_format(FileExtension::Docx);
```

Format setters still accept strings for forward compatibility. Those string
inputs are normalized by trimming leading dots and using lowercase, so `.PDF`
and `PDF` serialize as `pdf`.

## OAuth 2.0

Use API keys for server-side integrations owned by one CloudConvert account. Use
OAuth when your app acts on behalf of CloudConvert users.

```rust
use cloudconvert_sdk::{
    JobListQuery, OAuthClient, OAuthClientSecret, OAuthScope,
};

# async fn run() -> cloudconvert_sdk::Result<()> {
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
# Ok(())
# }
```

`OAuthAccessToken`, `OAuthRefreshToken`, and `OAuthClientSecret` redact their
debug output. OAuth-backed clients use the same SDK resources and Socket.io
subscription helpers as API-key clients.

## Extensibility

CloudConvert exposes many engine-specific options. This crate types the common
task shape and keeps operation-specific drift available through `extra` maps,
`option(...)` builder methods, and `GenericTask`.

The `TaskPayload` trait is sealed for SDK-owned typed task builders. Downstream
code that needs a new operation should use `TaskRequest::custom(...)` until the
crate adds a first-class builder.

For metadata-driven integrations, call `operations().list(...)` with
`include_options()` or `include_options_and_engine_versions()`. The returned
`Operation` can validate a `TaskRequest` before sending it:

```rust
use cloudconvert_sdk::{ConvertTask, OperationListQuery, TaskRequest};

# async fn run(client: cloudconvert_sdk::CloudConvertClient) -> cloudconvert_sdk::Result<()> {
let operation = client.operations().list(
    &OperationListQuery::default()
        .operation("convert")
        .input_format("docx")
        .output_format("pdf")
        .include_options_and_engine_versions(),
).await?.remove(0);

let task = TaskRequest::from(ConvertTask::new("import-file", "pdf"));
operation.validate_task(&task).expect("task should match operation metadata");
# Ok(())
# }
```

## Transport And Retry

By default, no automatic client-side retry/backoff is enabled. To opt in, enable
the `retry` feature and set a policy:

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

Retry covers transient API statuses `429`, `500`, `502`, `503`, and `504`, plus
connect and timeout errors. `Retry-After` seconds are respected by default.

Retry applies to CloudConvert API and synchronous API requests. Signed
`import/upload` form submissions and `export/url` downloads use storage URLs
returned by CloudConvert and are kept as a separate transport boundary.

## Socket.io Waits

CloudConvert also publishes job and task lifecycle events over Socket.io. This
is useful when an async application wants lower-latency completion than periodic
polling, but does not want to expose a public webhook receiver.

Enable the optional feature:

```toml
cloudconvert-sdk = { version = "0.1", features = ["socket"] }
```

Then use the managed wait helpers:

```rust
use cloudconvert_sdk::{ApiKey, CloudConvertClient, FileExtension, JobCreateRequest};

# async fn run() -> cloudconvert_sdk::Result<()> {
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
# Ok(())
# }
```

The SDK connects, subscribes, checks the current resource state to avoid missing
fast completions, waits for a terminal Socket.io event, and disconnects. Prefer
webhooks for production workflows where CloudConvert can call your service
directly.

For event streams instead of one-shot waits, use `client.socket(...)` with any
`SocketChannel`, `jobs().task_events_socket(job_id)` for all task events in a
job, or `users().events_socket()` for user-wide job and task events. User-wide
helpers fetch `users/me` and subscribe to the correct private channels under the
hood.

## Build Tasks

The CI workflow runs the library checks against default features and all
features:

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

The live group keeps API usage small: it creates then deletes live tasks/jobs,
including a watermark job shape, and has one ignored upload/convert/export flow
with a tiny generated text file. It needs task/job scopes, not `user.read`.
