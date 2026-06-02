//! Async Rust SDK primitives for the CloudConvert API v2.
//!
//! This crate exposes typed request and response models for common CloudConvert
//! jobs and tasks, plus a [`CloudConvertClient`] for calling the API from async
//! Rust applications. Operation-specific drift is handled through `option(...)`
//! builder methods and `extra` maps.
//!
//! # Build a job request
//!
//! For a simple linear job, use [`JobCreateRequest::linear`] and job-level task
//! shorthands. The SDK still creates the task names required by CloudConvert,
//! but callers do not need to spell them out.
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
//! # Branch a job graph
//!
//! Use [`JobCreateRequest::graph`] when later tasks need to reference specific
//! earlier tasks, for example in a branch or join.
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
    ApiKey, ClientBuilder, CloudConvertConfig, Region, SigningSecret, TransportConfig,
};
pub use error::{ApiError, Error, Result};
pub use file_extension::{FileExtension, ParseFileExtensionError};
pub use jobs::{
    ApiResponse, FileResult, Job, JobBuilder, JobCreateRequest, JobGetQuery, JobGraphBuilder,
    JobListQuery, JobStatus, JobTask, Page, PaginationLinks, PaginationMeta, RateLimit, Task,
    TaskGetQuery, TaskListQuery, TaskName, TaskResult, TaskStatus, UploadForm,
};
pub use operations::{Operation, OperationListQuery, OperationOption};
pub use resources::{User, Webhook, WebhookCreateRequest, WebhookEvent, WebhookListQuery};
pub use signed_url::sign_job_url;
#[cfg(feature = "socket")]
pub use socket::{CloudConvertSocket, SocketEvent};
pub use socket::{
    JobSocketEvent, SocketChannel, SocketSubscription, TaskSocketEvent, socket_base_url,
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
