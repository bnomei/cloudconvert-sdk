//! Async Rust SDK primitives for the CloudConvert API v2.
//!
//! This crate exposes typed request and response models for CloudConvert jobs
//! and tasks, plus a [`CloudConvertClient`] for calling the API from async Rust
//! applications. Operation-specific drift is handled through `option(...)`
//! builder methods, `extra` maps, and [`TaskRequest::custom`].
//!
//! # Build jobs
//!
//! Use [`JobCreateRequest::linear`] when each task feeds into the next task.
//!
//! ```
//! use cloudconvert_sdk::{FileExtension, JobCreateRequest};
//!
//! let request = JobCreateRequest::linear()
//!     .import_url("https://example.test/input.docx")
//!     .convert(FileExtension::Pdf)
//!     .export_url()
//!     .build();
//!
//! let payload = serde_json::to_value(request).unwrap();
//! assert_eq!(payload["tasks"]["import-url"]["operation"], "import/url");
//! assert_eq!(payload["tasks"]["convert"]["input"], "import-url");
//! assert_eq!(payload["tasks"]["export-url"]["input"], "convert");
//! ```
//!
//! Use `*_with(...)` methods to configure task-specific options while keeping a
//! serial pipeline.
//!
//! ```
//! use cloudconvert_sdk::{FileExtension, JobCreateRequest};
//!
//! let request = JobCreateRequest::linear()
//!     .import_url_with("https://example.test/input.docx", |task| {
//!         task.filename("input.docx")
//!     })
//!     .convert_with(FileExtension::Pdf, |task| {
//!         task.input_format(FileExtension::Docx)
//!     })
//!     .export_url()
//!     .build();
//!
//! let payload = serde_json::to_value(request).unwrap();
//! assert_eq!(payload["tasks"]["import-url"]["filename"], "input.docx");
//! assert_eq!(payload["tasks"]["convert"]["input_format"], "docx");
//! ```
//!
//! Use [`JobCreateRequest::graph`] when a job branches, joins multiple inputs,
//! or needs to reference a non-adjacent task.
//!
//! ```
//! use cloudconvert_sdk::{FileExtension, JobCreateRequest};
//!
//! let request = JobCreateRequest::graph(|job| {
//!     let import = job.import_url("https://example.test/input.docx");
//!     let pdf = job.convert(&import, FileExtension::Pdf);
//!     let png = job.convert(&import, FileExtension::Png);
//!     job.export_url([&pdf, &png]);
//! })
//! .build();
//!
//! let payload = serde_json::to_value(request).unwrap();
//! assert_eq!(payload["tasks"]["convert"]["input"], "import-url");
//! assert_eq!(payload["tasks"]["convert-2"]["input"], "import-url");
//! assert_eq!(
//!     payload["tasks"]["export-url"]["input"],
//!     serde_json::json!(["convert", "convert-2"])
//! );
//! ```
//!
//! # Call the API
//!
//! Live API calls need a CloudConvert API key. `ApiKey::from_env()` reads
//! `CLOUDCONVERT_API_KEY`.
//!
//! ```no_run
//! use cloudconvert_sdk::{ApiKey, CloudConvertClient, FileExtension, JobCreateRequest};
//!
//! # async fn run() -> cloudconvert_sdk::Result<()> {
//! let client = CloudConvertClient::builder(ApiKey::from_env()?).build()?;
//! let request = JobCreateRequest::linear()
//!     .import_url("https://example.test/input.docx")
//!     .convert(FileExtension::Pdf)
//!     .export_url()
//!     .build();
//!
//! let job = client.jobs().create(request).await?;
//! let finished = client.jobs().wait(&job.id).await?;
//! for file in finished.export_urls() {
//!     if let Some(url) = &file.url {
//!         let bytes = client.download(url).await?;
//!         println!("downloaded {} bytes as {}", bytes.len(), file.filename);
//!     }
//! }
//! # Ok(())
//! # }
//! ```

mod client;
mod config;
mod error;
mod file_extension;
mod jobs;
mod oauth;
mod operations;
mod resources;
mod signed_url;
mod socket;
mod tasks;
mod webhook;

pub use client::{
    CloudConvertClient, JobsResource, OperationsResource, TasksResource, UsersResource,
    WebhooksResource,
};
#[cfg(feature = "retry")]
pub use config::RetryPolicy;
pub use config::{
    ApiKey, ClientBuilder, CloudConvertConfig, OAuthAccessToken, OAuthClientSecret,
    OAuthRefreshToken, Region, SigningSecret, TransportConfig,
};
pub use error::{ApiError, Error, Result};
pub use file_extension::{FileExtension, ParseFileExtensionError};
pub use jobs::{
    ApiResponse, FileResult, Job, JobBuilder, JobCreateRequest, JobGetQuery, JobGraphBuilder,
    JobListQuery, JobStatus, JobTask, Page, PaginationLinks, PaginationMeta, RateLimit, Task,
    TaskGetQuery, TaskListQuery, TaskName, TaskResult, TaskStatus, UploadForm,
};
pub use oauth::{OAuthClient, OAuthScope, OAuthTokenResponse};
pub use operations::{
    Operation, OperationEngineVersion, OperationListQuery, OperationOption, OperationOptionKind,
    OperationValidationError, OperationValidationErrorKind, OperationValidationMode,
    OperationValidationResult,
};
pub use resources::{User, Webhook, WebhookCreateRequest, WebhookEvent, WebhookListQuery};
pub use signed_url::sign_job_url;
#[cfg(feature = "socket")]
pub use socket::{CloudConvertSocket, SocketEvent};
pub use socket::{
    JobSocketEvent, SocketChannel, SocketEventKind, SocketSubscription, TaskSocketEvent,
    socket_base_url,
};
pub use tasks::{
    ArchiveTask, AzureBlobExportTask, AzureBlobImportTask, Base64ImportTask, CaptureWebsiteTask,
    CommandTask, ConvertTask, ExportUploadTask, ExportUrlTask, ExtraOptions, FontAlign,
    GenericTask, GoogleCloudStorageExportTask, GoogleCloudStorageImportTask, ImportUploadTask,
    ImportUrlTask, Input, Layer, MergeTask, MetadataTask, MetadataWriteTask, OpenStackExportTask,
    OpenStackImportTask, OptimizeTask, PdfATask, PdfDecryptTask, PdfEncryptTask,
    PdfExtractPagesTask, PdfOcrTask, PdfRotatePagesTask, PdfSplitPagesTask, PdfXTask,
    PositionHorizontal, PositionVertical, RawImportTask, S3ExportTask, S3ImportTask,
    SftpExportTask, SftpImportTask, TaskPayload, TaskRequest, ThumbnailTask, WatermarkTask,
};
pub use webhook::{sign_payload, verify_signature};
