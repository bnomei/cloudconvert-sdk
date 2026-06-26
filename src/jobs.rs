use std::{collections::BTreeMap, convert::identity, fmt};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::InvalidBuilderState;
use crate::tasks::{
    ArchiveTask, AzureBlobExportTask, AzureBlobImportTask, Base64ImportTask, CaptureWebsiteTask,
    CommandTask, ConvertTask, ExportUploadTask, ExportUrlTask, GoogleCloudStorageExportTask,
    GoogleCloudStorageImportTask, ImportUploadTask, ImportUrlTask, Input, MergeTask, MetadataTask,
    MetadataWriteTask, OpenStackExportTask, OpenStackImportTask, OptimizeTask, PdfATask,
    PdfDecryptTask, PdfEncryptTask, PdfExtractPagesTask, PdfOcrTask, PdfRotatePagesTask,
    PdfSplitPagesTask, PdfXTask, RawImportTask, S3ExportTask, S3ImportTask, SftpExportTask,
    SftpImportTask, TaskRequest, ThumbnailTask, WatermarkTask,
};

fn redacted_option<T>(value: &Option<T>) -> Option<&'static str> {
    value.as_ref().map(|_| "REDACTED")
}

struct RedactedValueMap<'a>(&'a BTreeMap<String, Value>);

impl fmt::Debug for RedactedValueMap<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut debug = f.debug_map();
        for key in self.0.keys() {
            debug.entry(key, &"REDACTED");
        }
        debug.finish()
    }
}

/// Rate limit information extracted from CloudConvert response headers.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[non_exhaustive]
pub struct RateLimit {
    #[serde(default)]
    pub limit: Option<u64>,
    #[serde(default)]
    pub remaining: Option<u64>,
    #[serde(default)]
    pub reset: Option<u64>,
    #[serde(default)]
    pub retry_after: Option<u64>,
}

/// Pagination links returned by CloudConvert list endpoints.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[non_exhaustive]
pub struct PaginationLinks {
    #[serde(default)]
    pub first: Option<String>,
    #[serde(default)]
    pub last: Option<String>,
    #[serde(default)]
    pub prev: Option<String>,
    #[serde(default)]
    pub next: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// Pagination metadata returned by CloudConvert list endpoints.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[non_exhaustive]
pub struct PaginationMeta {
    #[serde(default)]
    pub current_page: Option<u32>,
    #[serde(default)]
    pub from: Option<u32>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub per_page: Option<u32>,
    #[serde(default)]
    pub to: Option<u32>,
    #[serde(default)]
    pub total: Option<u32>,
    #[serde(default)]
    pub last_page: Option<u32>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// A paginated API response.
#[derive(Clone, Debug, Default, Serialize)]
#[non_exhaustive]
pub struct Page<T> {
    pub data: Vec<T>,
    pub links: PaginationLinks,
    pub meta: PaginationMeta,
    #[serde(skip)]
    pub rate_limit: Option<RateLimit>,
}

/// A non-paginated API response that preserves envelope metadata.
#[derive(Clone, Debug, Serialize)]
#[non_exhaustive]
pub struct ApiResponse<T> {
    pub data: T,
    pub links: PaginationLinks,
    pub meta: PaginationMeta,
    #[serde(skip)]
    pub rate_limit: Option<RateLimit>,
}

/// Request body for `POST /v2/jobs`.
///
/// Use [`JobCreateRequest::linear`] for serial pipelines and
/// [`JobCreateRequest::graph`] for branches or joins. Both builders generate
/// the task names CloudConvert requires.
///
/// ```
/// use cloudconvert_sdk::{FileExtension, JobCreateRequest};
///
/// # fn main() -> cloudconvert_sdk::Result<()> {
/// let request = JobCreateRequest::linear()
///     .import_url("https://example.test/input.docx")
///     .convert(FileExtension::Pdf)?
///     .export_url()?
///     .build();
///
/// let payload = serde_json::to_value(request).unwrap();
/// assert_eq!(payload["tasks"]["convert"]["input"], "import-url");
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Default, Serialize)]
pub struct JobCreateRequest {
    tasks: BTreeMap<String, TaskRequest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    webhook_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) redirect: Option<bool>,
    #[serde(flatten, skip_serializing_if = "BTreeMap::is_empty")]
    extra: BTreeMap<String, Value>,
}

/// Core job fields owned by the SDK that `option()` must never overwrite.
///
/// These keys are serialized from dedicated struct fields, and the
/// `#[serde(flatten)]` `extra` map would otherwise be able to replace them at
/// serialization time (serde permits flattened keys to collide with named
/// fields). Inserting any of these via `option()` is rejected so the built task
/// graph and other canonical properties cannot be silently corrupted.
const RESERVED_JOB_FIELDS: [&str; 4] = ["tasks", "tag", "webhook_url", "redirect"];

impl JobCreateRequest {
    /// Inserts a custom top-level field, ignoring SDK-reserved keys.
    fn insert_option(&mut self, key: String, value: Value) {
        if RESERVED_JOB_FIELDS.contains(&key.as_str()) {
            return;
        }
        self.extra.insert(key, value);
    }
}

/// Name assigned to a task in a CloudConvert job request.
///
/// `TaskName` is the serialized task-map key. Pass handles returned by
/// [`JobGraphBuilder`] methods as dependency inputs for later graph tasks.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TaskName(String);

impl TaskName {
    fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Returns the task name string used in the serialized job payload.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for TaskName {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for TaskName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl From<TaskName> for String {
    fn from(value: TaskName) -> Self {
        value.0
    }
}

impl From<&TaskName> for String {
    fn from(value: &TaskName) -> Self {
        value.as_str().to_string()
    }
}

impl From<TaskName> for Input {
    fn from(value: TaskName) -> Self {
        Self::from(value.0)
    }
}

impl From<&TaskName> for Input {
    fn from(value: &TaskName) -> Self {
        Self::from(value.as_str())
    }
}

impl From<Vec<TaskName>> for Input {
    fn from(value: Vec<TaskName>) -> Self {
        Self::Tasks(value.into_iter().map(String::from).collect())
    }
}

impl From<Vec<&TaskName>> for Input {
    fn from(value: Vec<&TaskName>) -> Self {
        Self::Tasks(value.into_iter().map(String::from).collect())
    }
}

impl<const N: usize> From<[TaskName; N]> for Input {
    fn from(value: [TaskName; N]) -> Self {
        Self::Tasks(value.into_iter().map(String::from).collect())
    }
}

impl<const N: usize> From<[&TaskName; N]> for Input {
    fn from(value: [&TaskName; N]) -> Self {
        Self::Tasks(value.into_iter().map(String::from).collect())
    }
}

impl JobCreateRequest {
    /// Starts a general-purpose job builder.
    ///
    /// Prefer [`JobCreateRequest::linear`] for serial pipelines and
    /// [`JobCreateRequest::graph`] for branch or join jobs.
    pub fn builder() -> JobBuilder {
        JobBuilder::default()
    }

    /// Starts a job builder for a serial task pipeline.
    pub fn linear() -> JobBuilder {
        Self::builder()
    }

    /// Builds a job graph with task handles scoped to a closure.
    ///
    /// Each graph method returns a [`TaskName`] that can be passed as the input
    /// for later tasks. The returned [`JobBuilder`] can still be used to set
    /// top-level job options before calling [`JobBuilder::build`].
    ///
    /// ```
    /// use cloudconvert_sdk::{FileExtension, JobCreateRequest};
    ///
    /// let request = JobCreateRequest::graph(|job| {
    ///     let source = job.import_url("https://example.test/input.docx");
    ///     let pdf = job.convert(&source, FileExtension::Pdf);
    ///     let png = job.convert(&source, FileExtension::Png);
    ///     job.export_url([&pdf, &png]);
    /// })
    /// .tag("branch-demo")
    /// .build();
    ///
    /// let payload = serde_json::to_value(request).unwrap();
    /// assert_eq!(payload["tasks"]["convert"]["input"], "import-url");
    /// assert_eq!(payload["tasks"]["convert-2"]["input"], "import-url");
    /// ```
    pub fn graph(configure: impl FnOnce(&mut JobGraphBuilder)) -> JobBuilder {
        let mut graph = JobGraphBuilder::default();
        configure(&mut graph);
        graph.into_builder()
    }

    /// Returns the keyed task map that will be serialized as `tasks`.
    pub fn tasks(&self) -> &BTreeMap<String, TaskRequest> {
        &self.tasks
    }

    /// Returns the optional job tag.
    pub fn tag(&self) -> Option<&str> {
        self.tag.as_deref()
    }

    /// Returns the optional webhook URL.
    pub fn webhook_url(&self) -> Option<&str> {
        self.webhook_url.as_deref()
    }

    /// Returns whether CloudConvert should use redirect responses for the job.
    pub fn redirect(&self) -> Option<bool> {
        self.redirect
    }

    /// Returns additional job fields set through [`JobBuilder::option`].
    pub fn extra(&self) -> &BTreeMap<String, Value> {
        &self.extra
    }
}

impl fmt::Debug for JobCreateRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("JobCreateRequest")
            .field("tasks", &self.tasks)
            .field("tag", &self.tag)
            .field("webhook_url", &self.webhook_url)
            .field("redirect", &self.redirect)
            .field("extra", &RedactedValueMap(&self.extra))
            .finish()
    }
}

#[derive(Clone, Debug, Default)]
/// Builder for serial [`JobCreateRequest`] pipelines.
///
/// For linear jobs, use the fluent task shorthands and let the SDK wire each
/// task to the previous task.
///
/// ```
/// use cloudconvert_sdk::{FileExtension, JobCreateRequest};
///
/// # fn main() -> cloudconvert_sdk::Result<()> {
/// let request = JobCreateRequest::linear()
///     .import_url("https://example.test/input.docx")
///     .convert(FileExtension::Pdf)?
///     .export_url()?
///     .build();
///
/// let payload = serde_json::to_value(request).unwrap();
/// assert_eq!(payload["tasks"]["export-url"]["input"], "convert");
/// # Ok(())
/// # }
/// ```
///
/// Use `*_with(...)` methods when a task needs options. Use
/// [`JobCreateRequest::graph`] when a later task needs a handle to a specific
/// earlier task.
pub struct JobBuilder {
    request: JobCreateRequest,
    last_task: Option<TaskName>,
}

macro_rules! linear_pdf_task_methods {
    ($method:ident, $with_method:ident, $task_type:ident) => {
        /// Appends a PDF operation task using the previous task as input.
        pub fn $method(self) -> crate::Result<Self> {
            let input = self.previous_input(stringify!($method))?;
            Ok(self.append_task(TaskRequest::$method(input)))
        }

        /// Appends and configures a PDF operation task using the previous task as input.
        pub fn $with_method<F>(self, configure: F) -> crate::Result<Self>
        where
            F: FnOnce($task_type) -> $task_type,
        {
            let input = self.previous_input(stringify!($with_method))?;
            Ok(self.append_configured_task($task_type::new(input), configure))
        }
    };
}

macro_rules! graph_pdf_task_methods {
    ($method:ident, $with_method:ident, $task_type:ident) => {
        /// Adds a PDF operation task.
        pub fn $method(&mut self, input: impl Into<Input>) -> TaskName {
            self.$with_method(input, identity)
        }

        /// Adds and configures a PDF operation task.
        pub fn $with_method<F>(&mut self, input: impl Into<Input>, configure: F) -> TaskName
        where
            F: FnOnce($task_type) -> $task_type,
        {
            self.add_configured_task($task_type::new(input), configure)
        }
    };
}

impl JobBuilder {
    /// Creates an empty job builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets an optional job tag.
    pub fn tag(mut self, tag: impl Into<String>) -> Self {
        self.request.tag = Some(tag.into());
        self
    }

    /// Sets the webhook URL CloudConvert should call for this job.
    pub fn webhook_url(mut self, webhook_url: impl Into<String>) -> Self {
        self.request.webhook_url = Some(webhook_url.into());
        self
    }

    /// Sets whether CloudConvert should redirect on synchronous job completion.
    pub fn redirect(mut self, redirect: bool) -> Self {
        self.request.redirect = Some(redirect);
        self
    }

    /// Adds a custom top-level job field.
    ///
    /// Keys that collide with SDK-owned job fields (`tasks`, `tag`,
    /// `webhook_url`, `redirect`) are ignored so the built task graph and other
    /// canonical properties cannot be overwritten. Use the dedicated builder
    /// methods for those fields.
    pub fn option(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.request.insert_option(key.into(), value.into());
        self
    }

    /// Adds a task with an explicit CloudConvert task name.
    ///
    /// This keeps compatibility with CloudConvert's keyed task object and with
    /// existing code that already depends on task names. A name that is already
    /// taken receives a numeric suffix (`name-2`, `name-3`, ...) so an earlier
    /// task is never silently overwritten.
    pub fn task(mut self, name: impl Into<String>, task: impl Into<TaskRequest>) -> Self {
        let name = deduplicated_task_name(&name.into(), &self.request.tasks);
        self.request
            .tasks
            .insert(name.as_str().to_string(), task.into());
        self.last_task = Some(name);
        self
    }

    /// Adds a task with a generated name and returns that name as a handle.
    ///
    /// Generated names are derived from the task operation, such as
    /// `import-url`, `convert`, or `export-url`. Duplicate operation names get
    /// numeric suffixes.
    ///
    /// ```
    /// use cloudconvert_sdk::{ConvertTask, FileExtension, JobCreateRequest, TaskRequest};
    ///
    /// let mut builder = JobCreateRequest::builder();
    /// let import = builder.add_task(TaskRequest::import_url("https://example.test/input.docx"));
    /// let convert = builder.add_task(ConvertTask::new(&import, FileExtension::Pdf));
    ///
    /// assert_eq!(import.as_str(), "import-url");
    /// assert_eq!(convert.as_str(), "convert");
    /// ```
    pub fn add_task(&mut self, task: impl Into<TaskRequest>) -> TaskName {
        let task = task.into();
        let name = generated_task_name(task.operation(), &self.request.tasks);
        self.request.tasks.insert(name.as_str().to_string(), task);
        self.last_task = Some(name.clone());
        name
    }

    /// Adds a task with an explicit name and returns that name as a handle.
    ///
    /// If the name is already taken it receives a numeric suffix; the returned
    /// handle reflects the actual name stored in the job.
    pub fn add_named_task(
        &mut self,
        name: impl Into<String>,
        task: impl Into<TaskRequest>,
    ) -> TaskName {
        let name = deduplicated_task_name(&name.into(), &self.request.tasks);
        self.request
            .tasks
            .insert(name.as_str().to_string(), task.into());
        self.last_task = Some(name.clone());
        name
    }

    /// Appends an `import/url` task.
    pub fn import_url(self, url: impl Into<String>) -> Self {
        self.append_task(TaskRequest::import_url(url))
    }

    /// Appends an `import/upload` task.
    pub fn import_upload(self) -> Self {
        self.append_task(TaskRequest::import_upload())
    }

    /// Appends an `import/base64` task.
    pub fn import_base64(self, file: impl Into<String>, filename: impl Into<String>) -> Self {
        self.append_task(TaskRequest::import_base64(file, filename))
    }

    /// Appends an `import/raw` task.
    pub fn import_raw(self, file: impl Into<String>, filename: impl Into<String>) -> Self {
        self.append_task(TaskRequest::import_raw(file, filename))
    }

    /// Appends an `import/s3` task.
    pub fn import_s3(
        self,
        bucket: impl Into<String>,
        region: impl Into<String>,
        access_key_id: impl Into<String>,
        secret_access_key: impl Into<String>,
    ) -> Self {
        self.append_task(TaskRequest::import_s3(
            bucket,
            region,
            access_key_id,
            secret_access_key,
        ))
    }

    /// Appends an `import/azure/blob` task.
    pub fn import_azure_blob(
        self,
        storage_account: impl Into<String>,
        container: impl Into<String>,
    ) -> Self {
        self.append_task(TaskRequest::import_azure_blob(storage_account, container))
    }

    /// Appends an `import/google-cloud-storage` task.
    pub fn import_google_cloud_storage(
        self,
        project_id: impl Into<String>,
        bucket: impl Into<String>,
        client_email: impl Into<String>,
        private_key: impl Into<String>,
    ) -> Self {
        self.append_task(TaskRequest::import_google_cloud_storage(
            project_id,
            bucket,
            client_email,
            private_key,
        ))
    }

    /// Appends an `import/openstack` task.
    pub fn import_openstack(
        self,
        auth_url: impl Into<String>,
        username: impl Into<String>,
        password: impl Into<String>,
        region: impl Into<String>,
        container: impl Into<String>,
    ) -> Self {
        self.append_task(TaskRequest::import_openstack(
            auth_url, username, password, region, container,
        ))
    }

    /// Appends an `import/sftp` task.
    pub fn import_sftp(self, host: impl Into<String>, username: impl Into<String>) -> Self {
        self.append_task(TaskRequest::import_sftp(host, username))
    }

    /// Appends a `convert` task using the previous task as input.
    pub fn convert(self, output_format: impl Into<String>) -> crate::Result<Self> {
        let input = self.previous_input("convert")?;
        Ok(self.append_task(TaskRequest::convert(input, output_format)))
    }

    /// Appends a `convert` task with an explicit input format.
    pub fn convert_with_input_format(
        self,
        input_format: impl Into<String>,
        output_format: impl Into<String>,
    ) -> crate::Result<Self> {
        let input = self.previous_input("convert_with_input_format")?;
        Ok(self.append_task(ConvertTask::new(input, output_format).input_format(input_format)))
    }

    /// Appends an `optimize` task using the previous task as input.
    pub fn optimize(self) -> crate::Result<Self> {
        let input = self.previous_input("optimize")?;
        Ok(self.append_task(TaskRequest::optimize(input)))
    }

    /// Appends a text `watermark` task using the previous task as input.
    pub fn watermark_text(self, text: impl Into<String>) -> crate::Result<Self> {
        let input = self.previous_input("watermark_text")?;
        Ok(
            self.append_task(TaskRequest::watermark(crate::tasks::WatermarkTask::text(
                input, text,
            ))),
        )
    }

    /// Appends an image `watermark` task using the previous task as input.
    pub fn watermark_image(self, image_task_name: impl Into<String>) -> crate::Result<Self> {
        let input = self.previous_input("watermark_image")?;
        Ok(
            self.append_task(TaskRequest::watermark(crate::tasks::WatermarkTask::image(
                input,
                image_task_name,
            ))),
        )
    }

    /// Appends a `capture-website` task.
    pub fn capture_website(self, url: impl Into<String>, output_format: impl Into<String>) -> Self {
        self.append_task(TaskRequest::capture_website(url, output_format))
    }

    /// Appends a `thumbnail` task using the previous task as input.
    pub fn thumbnail(self, output_format: impl Into<String>) -> crate::Result<Self> {
        let input = self.previous_input("thumbnail")?;
        Ok(self.append_task(TaskRequest::thumbnail(input, output_format)))
    }

    /// Appends a `metadata` task using the previous task as input.
    pub fn metadata(self) -> crate::Result<Self> {
        let input = self.previous_input("metadata")?;
        Ok(self.append_task(TaskRequest::metadata(input)))
    }

    /// Appends a `metadata/write` task using the previous task as input.
    pub fn metadata_write(self) -> crate::Result<Self> {
        let input = self.previous_input("metadata_write")?;
        Ok(self.append_task(TaskRequest::metadata_write(input)))
    }

    /// Appends a `merge` task using the previous task as input.
    pub fn merge(self, output_format: impl Into<String>) -> crate::Result<Self> {
        let input = self.previous_input("merge")?;
        Ok(self.append_task(TaskRequest::merge(input, output_format)))
    }

    /// Appends an `archive` task using the previous task as input.
    pub fn archive(self, output_format: impl Into<String>) -> crate::Result<Self> {
        let input = self.previous_input("archive")?;
        Ok(self.append_task(TaskRequest::archive(input, output_format)))
    }

    /// Appends a `command` task using the previous task as input.
    pub fn command(
        self,
        engine: impl Into<String>,
        command: impl Into<String>,
        arguments: impl Into<String>,
    ) -> crate::Result<Self> {
        let input = self.previous_input("command")?;
        Ok(self.append_task(TaskRequest::command(input, engine, command, arguments)))
    }

    linear_pdf_task_methods!(pdf_a, pdf_a_with, PdfATask);
    linear_pdf_task_methods!(pdf_x, pdf_x_with, PdfXTask);
    linear_pdf_task_methods!(pdf_ocr, pdf_ocr_with, PdfOcrTask);
    linear_pdf_task_methods!(pdf_encrypt, pdf_encrypt_with, PdfEncryptTask);
    linear_pdf_task_methods!(pdf_decrypt, pdf_decrypt_with, PdfDecryptTask);
    linear_pdf_task_methods!(pdf_split_pages, pdf_split_pages_with, PdfSplitPagesTask);
    linear_pdf_task_methods!(
        pdf_extract_pages,
        pdf_extract_pages_with,
        PdfExtractPagesTask
    );
    linear_pdf_task_methods!(pdf_rotate_pages, pdf_rotate_pages_with, PdfRotatePagesTask);

    /// Appends an `export/url` task using the previous task as input.
    pub fn export_url(self) -> crate::Result<Self> {
        let input = self.previous_input("export_url")?;
        Ok(self.append_task(TaskRequest::export_url(input)))
    }

    /// Appends an `export/s3` task using the previous task as input.
    pub fn export_s3(
        self,
        bucket: impl Into<String>,
        region: impl Into<String>,
        access_key_id: impl Into<String>,
        secret_access_key: impl Into<String>,
    ) -> crate::Result<Self> {
        let input = self.previous_input("export_s3")?;
        Ok(self.append_task(TaskRequest::export_s3(
            input,
            bucket,
            region,
            access_key_id,
            secret_access_key,
        )))
    }

    /// Appends an `export/azure/blob` task using the previous task as input.
    pub fn export_azure_blob(
        self,
        storage_account: impl Into<String>,
        container: impl Into<String>,
    ) -> crate::Result<Self> {
        let input = self.previous_input("export_azure_blob")?;
        Ok(self.append_task(TaskRequest::export_azure_blob(
            input,
            storage_account,
            container,
        )))
    }

    /// Appends an `export/google-cloud-storage` task using the previous task as input.
    pub fn export_google_cloud_storage(
        self,
        project_id: impl Into<String>,
        bucket: impl Into<String>,
        client_email: impl Into<String>,
        private_key: impl Into<String>,
    ) -> crate::Result<Self> {
        let input = self.previous_input("export_google_cloud_storage")?;
        Ok(self.append_task(TaskRequest::export_google_cloud_storage(
            input,
            project_id,
            bucket,
            client_email,
            private_key,
        )))
    }

    /// Appends an `export/openstack` task using the previous task as input.
    pub fn export_openstack(
        self,
        auth_url: impl Into<String>,
        username: impl Into<String>,
        password: impl Into<String>,
        region: impl Into<String>,
        container: impl Into<String>,
    ) -> crate::Result<Self> {
        let input = self.previous_input("export_openstack")?;
        Ok(self.append_task(TaskRequest::export_openstack(
            input, auth_url, username, password, region, container,
        )))
    }

    /// Appends an `export/sftp` task using the previous task as input.
    pub fn export_sftp(
        self,
        host: impl Into<String>,
        username: impl Into<String>,
    ) -> crate::Result<Self> {
        let input = self.previous_input("export_sftp")?;
        Ok(self.append_task(TaskRequest::export_sftp(input, host, username)))
    }

    /// Appends an `export/upload` task using the previous task as input.
    pub fn export_upload(self, url: impl Into<String>) -> crate::Result<Self> {
        let input = self.previous_input("export_upload")?;
        Ok(self.append_task(TaskRequest::export_upload(input, url)))
    }

    /// Appends and configures an `import/url` task.
    pub fn import_url_with<F>(self, url: impl Into<String>, configure: F) -> Self
    where
        F: FnOnce(ImportUrlTask) -> ImportUrlTask,
    {
        self.append_configured_task(ImportUrlTask::new(url), configure)
    }

    /// Appends and configures an `import/upload` task.
    pub fn import_upload_with<F>(self, configure: F) -> Self
    where
        F: FnOnce(ImportUploadTask) -> ImportUploadTask,
    {
        self.append_configured_task(ImportUploadTask::default(), configure)
    }

    /// Appends and configures an `import/s3` task.
    pub fn import_s3_with<F>(
        self,
        bucket: impl Into<String>,
        region: impl Into<String>,
        access_key_id: impl Into<String>,
        secret_access_key: impl Into<String>,
        configure: F,
    ) -> Self
    where
        F: FnOnce(S3ImportTask) -> S3ImportTask,
    {
        self.append_configured_task(
            S3ImportTask::new(bucket, region, access_key_id, secret_access_key),
            configure,
        )
    }

    /// Appends and configures an `import/azure/blob` task.
    pub fn import_azure_blob_with<F>(
        self,
        storage_account: impl Into<String>,
        container: impl Into<String>,
        configure: F,
    ) -> Self
    where
        F: FnOnce(AzureBlobImportTask) -> AzureBlobImportTask,
    {
        self.append_configured_task(
            AzureBlobImportTask::new(storage_account, container),
            configure,
        )
    }

    /// Appends and configures an `import/google-cloud-storage` task.
    pub fn import_google_cloud_storage_with<F>(
        self,
        project_id: impl Into<String>,
        bucket: impl Into<String>,
        client_email: impl Into<String>,
        private_key: impl Into<String>,
        configure: F,
    ) -> Self
    where
        F: FnOnce(GoogleCloudStorageImportTask) -> GoogleCloudStorageImportTask,
    {
        self.append_configured_task(
            GoogleCloudStorageImportTask::new(project_id, bucket, client_email, private_key),
            configure,
        )
    }

    /// Appends and configures an `import/openstack` task.
    pub fn import_openstack_with<F>(
        self,
        auth_url: impl Into<String>,
        username: impl Into<String>,
        password: impl Into<String>,
        region: impl Into<String>,
        container: impl Into<String>,
        configure: F,
    ) -> Self
    where
        F: FnOnce(OpenStackImportTask) -> OpenStackImportTask,
    {
        self.append_configured_task(
            OpenStackImportTask::new(auth_url, username, password, region, container),
            configure,
        )
    }

    /// Appends and configures an `import/sftp` task.
    pub fn import_sftp_with<F>(
        self,
        host: impl Into<String>,
        username: impl Into<String>,
        configure: F,
    ) -> Self
    where
        F: FnOnce(SftpImportTask) -> SftpImportTask,
    {
        self.append_configured_task(SftpImportTask::new(host, username), configure)
    }

    /// Appends and configures a `convert` task using the previous task as input.
    pub fn convert_with<F>(
        self,
        output_format: impl Into<String>,
        configure: F,
    ) -> crate::Result<Self>
    where
        F: FnOnce(ConvertTask) -> ConvertTask,
    {
        let input = self.previous_input("convert_with")?;
        Ok(self.append_configured_task(ConvertTask::new(input, output_format), configure))
    }

    /// Appends and configures an `optimize` task using the previous task as input.
    pub fn optimize_with<F>(self, configure: F) -> crate::Result<Self>
    where
        F: FnOnce(OptimizeTask) -> OptimizeTask,
    {
        let input = self.previous_input("optimize_with")?;
        Ok(self.append_configured_task(OptimizeTask::new(input), configure))
    }

    /// Appends and configures a text `watermark` task using the previous task as input.
    pub fn watermark_text_with<F>(
        self,
        text: impl Into<String>,
        configure: F,
    ) -> crate::Result<Self>
    where
        F: FnOnce(WatermarkTask) -> WatermarkTask,
    {
        let input = self.previous_input("watermark_text_with")?;
        Ok(self.append_configured_task(WatermarkTask::text(input, text), configure))
    }

    /// Appends and configures an image `watermark` task using the previous task as input.
    pub fn watermark_image_with<F>(
        self,
        image_task_name: impl Into<String>,
        configure: F,
    ) -> crate::Result<Self>
    where
        F: FnOnce(WatermarkTask) -> WatermarkTask,
    {
        let input = self.previous_input("watermark_image_with")?;
        Ok(self.append_configured_task(WatermarkTask::image(input, image_task_name), configure))
    }

    /// Appends and configures a `capture-website` task.
    pub fn capture_website_with<F>(
        self,
        url: impl Into<String>,
        output_format: impl Into<String>,
        configure: F,
    ) -> Self
    where
        F: FnOnce(CaptureWebsiteTask) -> CaptureWebsiteTask,
    {
        self.append_configured_task(CaptureWebsiteTask::new(url, output_format), configure)
    }

    /// Appends and configures a `thumbnail` task using the previous task as input.
    pub fn thumbnail_with<F>(
        self,
        output_format: impl Into<String>,
        configure: F,
    ) -> crate::Result<Self>
    where
        F: FnOnce(ThumbnailTask) -> ThumbnailTask,
    {
        let input = self.previous_input("thumbnail_with")?;
        Ok(self.append_configured_task(ThumbnailTask::new(input, output_format), configure))
    }

    /// Appends and configures a `metadata` task using the previous task as input.
    pub fn metadata_with<F>(self, configure: F) -> crate::Result<Self>
    where
        F: FnOnce(MetadataTask) -> MetadataTask,
    {
        let input = self.previous_input("metadata_with")?;
        Ok(self.append_configured_task(MetadataTask::new(input), configure))
    }

    /// Appends and configures a `metadata/write` task using the previous task as input.
    pub fn metadata_write_with<F>(self, configure: F) -> crate::Result<Self>
    where
        F: FnOnce(MetadataWriteTask) -> MetadataWriteTask,
    {
        let input = self.previous_input("metadata_write_with")?;
        Ok(self.append_configured_task(MetadataWriteTask::new(input), configure))
    }

    /// Appends and configures a `merge` task using the previous task as input.
    pub fn merge_with<F>(
        self,
        output_format: impl Into<String>,
        configure: F,
    ) -> crate::Result<Self>
    where
        F: FnOnce(MergeTask) -> MergeTask,
    {
        let input = self.previous_input("merge_with")?;
        Ok(self.append_configured_task(MergeTask::new(input, output_format), configure))
    }

    /// Appends and configures an `archive` task using the previous task as input.
    pub fn archive_with<F>(
        self,
        output_format: impl Into<String>,
        configure: F,
    ) -> crate::Result<Self>
    where
        F: FnOnce(ArchiveTask) -> ArchiveTask,
    {
        let input = self.previous_input("archive_with")?;
        Ok(self.append_configured_task(ArchiveTask::new(input, output_format), configure))
    }

    /// Appends and configures a `command` task using the previous task as input.
    pub fn command_with<F>(
        self,
        engine: impl Into<String>,
        command: impl Into<String>,
        arguments: impl Into<String>,
        configure: F,
    ) -> crate::Result<Self>
    where
        F: FnOnce(CommandTask) -> CommandTask,
    {
        let input = self.previous_input("command_with")?;
        Ok(self.append_configured_task(
            CommandTask::new(input, engine, command, arguments),
            configure,
        ))
    }

    /// Appends and configures an `export/url` task using the previous task as input.
    pub fn export_url_with<F>(self, configure: F) -> crate::Result<Self>
    where
        F: FnOnce(ExportUrlTask) -> ExportUrlTask,
    {
        let input = self.previous_input("export_url_with")?;
        Ok(self.append_configured_task(ExportUrlTask::new(input), configure))
    }

    /// Appends and configures an `export/s3` task using the previous task as input.
    pub fn export_s3_with<F>(
        self,
        bucket: impl Into<String>,
        region: impl Into<String>,
        access_key_id: impl Into<String>,
        secret_access_key: impl Into<String>,
        configure: F,
    ) -> crate::Result<Self>
    where
        F: FnOnce(S3ExportTask) -> S3ExportTask,
    {
        let input = self.previous_input("export_s3_with")?;
        Ok(self.append_configured_task(
            S3ExportTask::new(input, bucket, region, access_key_id, secret_access_key),
            configure,
        ))
    }

    /// Appends and configures an `export/azure/blob` task using the previous task as input.
    pub fn export_azure_blob_with<F>(
        self,
        storage_account: impl Into<String>,
        container: impl Into<String>,
        configure: F,
    ) -> crate::Result<Self>
    where
        F: FnOnce(AzureBlobExportTask) -> AzureBlobExportTask,
    {
        let input = self.previous_input("export_azure_blob_with")?;
        Ok(self.append_configured_task(
            AzureBlobExportTask::new(input, storage_account, container),
            configure,
        ))
    }

    /// Appends and configures an `export/google-cloud-storage` task using the previous task as input.
    pub fn export_google_cloud_storage_with<F>(
        self,
        project_id: impl Into<String>,
        bucket: impl Into<String>,
        client_email: impl Into<String>,
        private_key: impl Into<String>,
        configure: F,
    ) -> crate::Result<Self>
    where
        F: FnOnce(GoogleCloudStorageExportTask) -> GoogleCloudStorageExportTask,
    {
        let input = self.previous_input("export_google_cloud_storage_with")?;
        Ok(self.append_configured_task(
            GoogleCloudStorageExportTask::new(input, project_id, bucket, client_email, private_key),
            configure,
        ))
    }

    /// Appends and configures an `export/openstack` task using the previous task as input.
    pub fn export_openstack_with<F>(
        self,
        auth_url: impl Into<String>,
        username: impl Into<String>,
        password: impl Into<String>,
        region: impl Into<String>,
        container: impl Into<String>,
        configure: F,
    ) -> crate::Result<Self>
    where
        F: FnOnce(OpenStackExportTask) -> OpenStackExportTask,
    {
        let input = self.previous_input("export_openstack_with")?;
        Ok(self.append_configured_task(
            OpenStackExportTask::new(input, auth_url, username, password, region, container),
            configure,
        ))
    }

    /// Appends and configures an `export/sftp` task using the previous task as input.
    pub fn export_sftp_with<F>(
        self,
        host: impl Into<String>,
        username: impl Into<String>,
        configure: F,
    ) -> crate::Result<Self>
    where
        F: FnOnce(SftpExportTask) -> SftpExportTask,
    {
        let input = self.previous_input("export_sftp_with")?;
        Ok(self.append_configured_task(SftpExportTask::new(input, host, username), configure))
    }

    /// Appends and configures an `export/upload` task using the previous task as input.
    pub fn export_upload_with<F>(self, url: impl Into<String>, configure: F) -> crate::Result<Self>
    where
        F: FnOnce(ExportUploadTask) -> ExportUploadTask,
    {
        let input = self.previous_input("export_upload_with")?;
        Ok(self.append_configured_task(ExportUploadTask::new(input, url), configure))
    }

    /// Finishes the builder and returns the job creation request.
    pub fn build(self) -> JobCreateRequest {
        self.request
    }

    fn append_task(mut self, task: impl Into<TaskRequest>) -> Self {
        self.add_task(task);
        self
    }

    fn append_configured_task<T, F>(self, task: T, configure: F) -> Self
    where
        T: Into<TaskRequest>,
        F: FnOnce(T) -> T,
    {
        self.append_task(configure(task))
    }

    fn previous_input(&self, method: &'static str) -> crate::Result<Input> {
        self.last_task
            .as_ref()
            .map(Input::from)
            .ok_or_else(|| InvalidBuilderState::missing_previous_task(method).into())
    }
}

fn generated_task_name(operation: &str, existing: &BTreeMap<String, TaskRequest>) -> TaskName {
    deduplicated_task_name(&task_name_base(operation), existing)
}

/// Returns `base` if unused, otherwise the first free `base-N` (N >= 2).
///
/// Explicit and generated task names share this so a duplicate name never
/// silently overwrites an earlier task in the job's `tasks` map.
fn deduplicated_task_name(base: &str, existing: &BTreeMap<String, TaskRequest>) -> TaskName {
    if !existing.contains_key(base) {
        return TaskName::new(base);
    }

    let mut counter = 2;
    loop {
        let candidate = format!("{base}-{counter}");
        if !existing.contains_key(&candidate) {
            return TaskName::new(candidate);
        }
        counter += 1;
    }
}

fn task_name_base(operation: &str) -> String {
    let mut name = String::new();
    let mut previous_was_separator = false;

    for byte in operation.bytes() {
        if byte.is_ascii_alphanumeric() {
            name.push(byte.to_ascii_lowercase() as char);
            previous_was_separator = false;
        } else if !previous_was_separator && !name.is_empty() {
            name.push('-');
            previous_was_separator = true;
        }
    }

    while name.ends_with('-') {
        name.pop();
    }

    if name.is_empty() {
        "task".to_string()
    } else {
        name
    }
}

impl From<JobBuilder> for JobCreateRequest {
    fn from(builder: JobBuilder) -> Self {
        builder.build()
    }
}

/// Builder for branched CloudConvert job graphs.
///
/// `JobGraphBuilder` is usually used through [`JobCreateRequest::graph`]. Each
/// task method appends one task and returns a [`TaskName`] handle that can be
/// passed to later graph methods.
///
/// ```
/// use cloudconvert_sdk::{FileExtension, JobCreateRequest};
///
/// let request = JobCreateRequest::graph(|job| {
///     let imported = job.import_url("https://example.test/input.docx");
///     let pdf = job.convert(&imported, FileExtension::Pdf);
///     let png = job.convert(&imported, FileExtension::Png);
///     job.export_url(vec![&pdf, &png]);
/// })
/// .build();
///
/// let payload = serde_json::to_value(request).unwrap();
/// assert_eq!(payload["tasks"]["export-url"]["input"], serde_json::json!(["convert", "convert-2"]));
/// ```
#[derive(Clone, Debug, Default)]
pub struct JobGraphBuilder {
    builder: JobBuilder,
}

impl JobGraphBuilder {
    /// Creates an empty graph builder.
    ///
    /// Prefer [`JobCreateRequest::graph`] when the graph can be configured in a
    /// single closure.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets an optional job tag.
    pub fn tag(&mut self, tag: impl Into<String>) -> &mut Self {
        self.builder.request.tag = Some(tag.into());
        self
    }

    /// Sets the webhook URL CloudConvert should call for this job.
    pub fn webhook_url(&mut self, webhook_url: impl Into<String>) -> &mut Self {
        self.builder.request.webhook_url = Some(webhook_url.into());
        self
    }

    /// Sets whether CloudConvert should redirect on synchronous job completion.
    pub fn redirect(&mut self, redirect: bool) -> &mut Self {
        self.builder.request.redirect = Some(redirect);
        self
    }

    /// Adds a custom top-level job field.
    ///
    /// Keys that collide with SDK-owned job fields (`tasks`, `tag`,
    /// `webhook_url`, `redirect`) are ignored so the built task graph and other
    /// canonical properties cannot be overwritten. Use the dedicated builder
    /// methods for those fields.
    pub fn option(&mut self, key: impl Into<String>, value: impl Into<Value>) -> &mut Self {
        self.builder.request.insert_option(key.into(), value.into());
        self
    }

    /// Adds a task with a generated task name and returns that name as a handle.
    pub fn add_task(&mut self, task: impl Into<TaskRequest>) -> TaskName {
        self.builder.add_task(task)
    }

    /// Adds a task with an explicit task name and returns that name as a handle.
    pub fn add_named_task(
        &mut self,
        name: impl Into<String>,
        task: impl Into<TaskRequest>,
    ) -> TaskName {
        self.builder.add_named_task(name, task)
    }

    /// Adds an `import/url` task.
    pub fn import_url(&mut self, url: impl Into<String>) -> TaskName {
        self.import_url_with(url, identity)
    }

    /// Adds and configures an `import/url` task.
    pub fn import_url_with<F>(&mut self, url: impl Into<String>, configure: F) -> TaskName
    where
        F: FnOnce(ImportUrlTask) -> ImportUrlTask,
    {
        self.add_configured_task(ImportUrlTask::new(url), configure)
    }

    /// Adds an `import/upload` task.
    pub fn import_upload(&mut self) -> TaskName {
        self.import_upload_with(identity)
    }

    /// Adds and configures an `import/upload` task.
    pub fn import_upload_with<F>(&mut self, configure: F) -> TaskName
    where
        F: FnOnce(ImportUploadTask) -> ImportUploadTask,
    {
        self.add_configured_task(ImportUploadTask::default(), configure)
    }

    /// Adds an `import/base64` task.
    pub fn import_base64(
        &mut self,
        file: impl Into<String>,
        filename: impl Into<String>,
    ) -> TaskName {
        self.add_task(Base64ImportTask::new(file, filename))
    }

    /// Adds an `import/raw` task.
    pub fn import_raw(&mut self, file: impl Into<String>, filename: impl Into<String>) -> TaskName {
        self.add_task(RawImportTask::new(file, filename))
    }

    /// Adds an `import/s3` task.
    pub fn import_s3(
        &mut self,
        bucket: impl Into<String>,
        region: impl Into<String>,
        access_key_id: impl Into<String>,
        secret_access_key: impl Into<String>,
    ) -> TaskName {
        self.import_s3_with(bucket, region, access_key_id, secret_access_key, identity)
    }

    /// Adds and configures an `import/s3` task.
    pub fn import_s3_with<F>(
        &mut self,
        bucket: impl Into<String>,
        region: impl Into<String>,
        access_key_id: impl Into<String>,
        secret_access_key: impl Into<String>,
        configure: F,
    ) -> TaskName
    where
        F: FnOnce(S3ImportTask) -> S3ImportTask,
    {
        self.add_configured_task(
            S3ImportTask::new(bucket, region, access_key_id, secret_access_key),
            configure,
        )
    }

    /// Adds an `import/azure/blob` task.
    pub fn import_azure_blob(
        &mut self,
        storage_account: impl Into<String>,
        container: impl Into<String>,
    ) -> TaskName {
        self.import_azure_blob_with(storage_account, container, identity)
    }

    /// Adds and configures an `import/azure/blob` task.
    pub fn import_azure_blob_with<F>(
        &mut self,
        storage_account: impl Into<String>,
        container: impl Into<String>,
        configure: F,
    ) -> TaskName
    where
        F: FnOnce(AzureBlobImportTask) -> AzureBlobImportTask,
    {
        self.add_configured_task(
            AzureBlobImportTask::new(storage_account, container),
            configure,
        )
    }

    /// Adds an `import/google-cloud-storage` task.
    pub fn import_google_cloud_storage(
        &mut self,
        project_id: impl Into<String>,
        bucket: impl Into<String>,
        client_email: impl Into<String>,
        private_key: impl Into<String>,
    ) -> TaskName {
        self.import_google_cloud_storage_with(
            project_id,
            bucket,
            client_email,
            private_key,
            identity,
        )
    }

    /// Adds and configures an `import/google-cloud-storage` task.
    pub fn import_google_cloud_storage_with<F>(
        &mut self,
        project_id: impl Into<String>,
        bucket: impl Into<String>,
        client_email: impl Into<String>,
        private_key: impl Into<String>,
        configure: F,
    ) -> TaskName
    where
        F: FnOnce(GoogleCloudStorageImportTask) -> GoogleCloudStorageImportTask,
    {
        self.add_configured_task(
            GoogleCloudStorageImportTask::new(project_id, bucket, client_email, private_key),
            configure,
        )
    }

    /// Adds an `import/openstack` task.
    pub fn import_openstack(
        &mut self,
        auth_url: impl Into<String>,
        username: impl Into<String>,
        password: impl Into<String>,
        region: impl Into<String>,
        container: impl Into<String>,
    ) -> TaskName {
        self.import_openstack_with(auth_url, username, password, region, container, identity)
    }

    /// Adds and configures an `import/openstack` task.
    pub fn import_openstack_with<F>(
        &mut self,
        auth_url: impl Into<String>,
        username: impl Into<String>,
        password: impl Into<String>,
        region: impl Into<String>,
        container: impl Into<String>,
        configure: F,
    ) -> TaskName
    where
        F: FnOnce(OpenStackImportTask) -> OpenStackImportTask,
    {
        self.add_configured_task(
            OpenStackImportTask::new(auth_url, username, password, region, container),
            configure,
        )
    }

    /// Adds an `import/sftp` task.
    pub fn import_sftp(
        &mut self,
        host: impl Into<String>,
        username: impl Into<String>,
    ) -> TaskName {
        self.import_sftp_with(host, username, identity)
    }

    /// Adds and configures an `import/sftp` task.
    pub fn import_sftp_with<F>(
        &mut self,
        host: impl Into<String>,
        username: impl Into<String>,
        configure: F,
    ) -> TaskName
    where
        F: FnOnce(SftpImportTask) -> SftpImportTask,
    {
        self.add_configured_task(SftpImportTask::new(host, username), configure)
    }

    /// Adds a `convert` task.
    pub fn convert(
        &mut self,
        input: impl Into<Input>,
        output_format: impl Into<String>,
    ) -> TaskName {
        self.convert_with(input, output_format, identity)
    }

    /// Adds and configures a `convert` task.
    pub fn convert_with<F>(
        &mut self,
        input: impl Into<Input>,
        output_format: impl Into<String>,
        configure: F,
    ) -> TaskName
    where
        F: FnOnce(ConvertTask) -> ConvertTask,
    {
        self.add_configured_task(ConvertTask::new(input, output_format), configure)
    }

    /// Adds an `optimize` task.
    pub fn optimize(&mut self, input: impl Into<Input>) -> TaskName {
        self.optimize_with(input, identity)
    }

    /// Adds and configures an `optimize` task.
    pub fn optimize_with<F>(&mut self, input: impl Into<Input>, configure: F) -> TaskName
    where
        F: FnOnce(OptimizeTask) -> OptimizeTask,
    {
        self.add_configured_task(OptimizeTask::new(input), configure)
    }

    /// Adds a text `watermark` task.
    pub fn watermark_text(&mut self, input: impl Into<Input>, text: impl Into<String>) -> TaskName {
        self.watermark_text_with(input, text, identity)
    }

    /// Adds and configures a text `watermark` task.
    pub fn watermark_text_with<F>(
        &mut self,
        input: impl Into<Input>,
        text: impl Into<String>,
        configure: F,
    ) -> TaskName
    where
        F: FnOnce(WatermarkTask) -> WatermarkTask,
    {
        self.add_configured_task(WatermarkTask::text(input, text), configure)
    }

    /// Adds an image `watermark` task.
    pub fn watermark_image(
        &mut self,
        input: impl Into<Input>,
        image_task_name: impl Into<String>,
    ) -> TaskName {
        self.watermark_image_with(input, image_task_name, identity)
    }

    /// Adds and configures an image `watermark` task.
    pub fn watermark_image_with<F>(
        &mut self,
        input: impl Into<Input>,
        image_task_name: impl Into<String>,
        configure: F,
    ) -> TaskName
    where
        F: FnOnce(WatermarkTask) -> WatermarkTask,
    {
        self.add_configured_task(WatermarkTask::image(input, image_task_name), configure)
    }

    /// Adds a `capture-website` task.
    pub fn capture_website(
        &mut self,
        url: impl Into<String>,
        output_format: impl Into<String>,
    ) -> TaskName {
        self.capture_website_with(url, output_format, identity)
    }

    /// Adds and configures a `capture-website` task.
    pub fn capture_website_with<F>(
        &mut self,
        url: impl Into<String>,
        output_format: impl Into<String>,
        configure: F,
    ) -> TaskName
    where
        F: FnOnce(CaptureWebsiteTask) -> CaptureWebsiteTask,
    {
        self.add_configured_task(CaptureWebsiteTask::new(url, output_format), configure)
    }

    /// Adds a `thumbnail` task.
    pub fn thumbnail(
        &mut self,
        input: impl Into<Input>,
        output_format: impl Into<String>,
    ) -> TaskName {
        self.thumbnail_with(input, output_format, identity)
    }

    /// Adds and configures a `thumbnail` task.
    pub fn thumbnail_with<F>(
        &mut self,
        input: impl Into<Input>,
        output_format: impl Into<String>,
        configure: F,
    ) -> TaskName
    where
        F: FnOnce(ThumbnailTask) -> ThumbnailTask,
    {
        self.add_configured_task(ThumbnailTask::new(input, output_format), configure)
    }

    /// Adds a `metadata` task.
    pub fn metadata(&mut self, input: impl Into<Input>) -> TaskName {
        self.metadata_with(input, identity)
    }

    /// Adds and configures a `metadata` task.
    pub fn metadata_with<F>(&mut self, input: impl Into<Input>, configure: F) -> TaskName
    where
        F: FnOnce(MetadataTask) -> MetadataTask,
    {
        self.add_configured_task(MetadataTask::new(input), configure)
    }

    /// Adds a `metadata/write` task.
    pub fn metadata_write(&mut self, input: impl Into<Input>) -> TaskName {
        self.metadata_write_with(input, identity)
    }

    /// Adds and configures a `metadata/write` task.
    pub fn metadata_write_with<F>(&mut self, input: impl Into<Input>, configure: F) -> TaskName
    where
        F: FnOnce(MetadataWriteTask) -> MetadataWriteTask,
    {
        self.add_configured_task(MetadataWriteTask::new(input), configure)
    }

    /// Adds a `merge` task.
    pub fn merge(&mut self, input: impl Into<Input>, output_format: impl Into<String>) -> TaskName {
        self.merge_with(input, output_format, identity)
    }

    /// Adds and configures a `merge` task.
    pub fn merge_with<F>(
        &mut self,
        input: impl Into<Input>,
        output_format: impl Into<String>,
        configure: F,
    ) -> TaskName
    where
        F: FnOnce(MergeTask) -> MergeTask,
    {
        self.add_configured_task(MergeTask::new(input, output_format), configure)
    }

    /// Adds an `archive` task.
    pub fn archive(
        &mut self,
        input: impl Into<Input>,
        output_format: impl Into<String>,
    ) -> TaskName {
        self.archive_with(input, output_format, identity)
    }

    /// Adds and configures an `archive` task.
    pub fn archive_with<F>(
        &mut self,
        input: impl Into<Input>,
        output_format: impl Into<String>,
        configure: F,
    ) -> TaskName
    where
        F: FnOnce(ArchiveTask) -> ArchiveTask,
    {
        self.add_configured_task(ArchiveTask::new(input, output_format), configure)
    }

    /// Adds a `command` task.
    pub fn command(
        &mut self,
        input: impl Into<Input>,
        engine: impl Into<String>,
        command: impl Into<String>,
        arguments: impl Into<String>,
    ) -> TaskName {
        self.command_with(input, engine, command, arguments, identity)
    }

    /// Adds and configures a `command` task.
    pub fn command_with<F>(
        &mut self,
        input: impl Into<Input>,
        engine: impl Into<String>,
        command: impl Into<String>,
        arguments: impl Into<String>,
        configure: F,
    ) -> TaskName
    where
        F: FnOnce(CommandTask) -> CommandTask,
    {
        self.add_configured_task(
            CommandTask::new(input, engine, command, arguments),
            configure,
        )
    }

    graph_pdf_task_methods!(pdf_a, pdf_a_with, PdfATask);
    graph_pdf_task_methods!(pdf_x, pdf_x_with, PdfXTask);
    graph_pdf_task_methods!(pdf_ocr, pdf_ocr_with, PdfOcrTask);
    graph_pdf_task_methods!(pdf_encrypt, pdf_encrypt_with, PdfEncryptTask);
    graph_pdf_task_methods!(pdf_decrypt, pdf_decrypt_with, PdfDecryptTask);
    graph_pdf_task_methods!(pdf_split_pages, pdf_split_pages_with, PdfSplitPagesTask);
    graph_pdf_task_methods!(
        pdf_extract_pages,
        pdf_extract_pages_with,
        PdfExtractPagesTask
    );
    graph_pdf_task_methods!(pdf_rotate_pages, pdf_rotate_pages_with, PdfRotatePagesTask);

    /// Adds an `export/url` task.
    pub fn export_url(&mut self, input: impl Into<Input>) -> TaskName {
        self.export_url_with(input, identity)
    }

    /// Adds and configures an `export/url` task.
    pub fn export_url_with<F>(&mut self, input: impl Into<Input>, configure: F) -> TaskName
    where
        F: FnOnce(ExportUrlTask) -> ExportUrlTask,
    {
        self.add_configured_task(ExportUrlTask::new(input), configure)
    }

    /// Adds an `export/s3` task.
    pub fn export_s3(
        &mut self,
        input: impl Into<Input>,
        bucket: impl Into<String>,
        region: impl Into<String>,
        access_key_id: impl Into<String>,
        secret_access_key: impl Into<String>,
    ) -> TaskName {
        self.export_s3_with(
            input,
            bucket,
            region,
            access_key_id,
            secret_access_key,
            identity,
        )
    }

    /// Adds and configures an `export/s3` task.
    pub fn export_s3_with<F>(
        &mut self,
        input: impl Into<Input>,
        bucket: impl Into<String>,
        region: impl Into<String>,
        access_key_id: impl Into<String>,
        secret_access_key: impl Into<String>,
        configure: F,
    ) -> TaskName
    where
        F: FnOnce(S3ExportTask) -> S3ExportTask,
    {
        self.add_configured_task(
            S3ExportTask::new(input, bucket, region, access_key_id, secret_access_key),
            configure,
        )
    }

    /// Adds an `export/azure/blob` task.
    pub fn export_azure_blob(
        &mut self,
        input: impl Into<Input>,
        storage_account: impl Into<String>,
        container: impl Into<String>,
    ) -> TaskName {
        self.export_azure_blob_with(input, storage_account, container, identity)
    }

    /// Adds and configures an `export/azure/blob` task.
    pub fn export_azure_blob_with<F>(
        &mut self,
        input: impl Into<Input>,
        storage_account: impl Into<String>,
        container: impl Into<String>,
        configure: F,
    ) -> TaskName
    where
        F: FnOnce(AzureBlobExportTask) -> AzureBlobExportTask,
    {
        self.add_configured_task(
            AzureBlobExportTask::new(input, storage_account, container),
            configure,
        )
    }

    /// Adds an `export/google-cloud-storage` task.
    pub fn export_google_cloud_storage(
        &mut self,
        input: impl Into<Input>,
        project_id: impl Into<String>,
        bucket: impl Into<String>,
        client_email: impl Into<String>,
        private_key: impl Into<String>,
    ) -> TaskName {
        self.export_google_cloud_storage_with(
            input,
            project_id,
            bucket,
            client_email,
            private_key,
            identity,
        )
    }

    /// Adds and configures an `export/google-cloud-storage` task.
    pub fn export_google_cloud_storage_with<F>(
        &mut self,
        input: impl Into<Input>,
        project_id: impl Into<String>,
        bucket: impl Into<String>,
        client_email: impl Into<String>,
        private_key: impl Into<String>,
        configure: F,
    ) -> TaskName
    where
        F: FnOnce(GoogleCloudStorageExportTask) -> GoogleCloudStorageExportTask,
    {
        self.add_configured_task(
            GoogleCloudStorageExportTask::new(input, project_id, bucket, client_email, private_key),
            configure,
        )
    }

    /// Adds an `export/openstack` task.
    pub fn export_openstack(
        &mut self,
        input: impl Into<Input>,
        auth_url: impl Into<String>,
        username: impl Into<String>,
        password: impl Into<String>,
        region: impl Into<String>,
        container: impl Into<String>,
    ) -> TaskName {
        self.export_openstack_with(
            input, auth_url, username, password, region, container, identity,
        )
    }

    /// Adds and configures an `export/openstack` task.
    #[allow(clippy::too_many_arguments)]
    pub fn export_openstack_with<F>(
        &mut self,
        input: impl Into<Input>,
        auth_url: impl Into<String>,
        username: impl Into<String>,
        password: impl Into<String>,
        region: impl Into<String>,
        container: impl Into<String>,
        configure: F,
    ) -> TaskName
    where
        F: FnOnce(OpenStackExportTask) -> OpenStackExportTask,
    {
        self.add_configured_task(
            OpenStackExportTask::new(input, auth_url, username, password, region, container),
            configure,
        )
    }

    /// Adds an `export/sftp` task.
    pub fn export_sftp(
        &mut self,
        input: impl Into<Input>,
        host: impl Into<String>,
        username: impl Into<String>,
    ) -> TaskName {
        self.export_sftp_with(input, host, username, identity)
    }

    /// Adds and configures an `export/sftp` task.
    pub fn export_sftp_with<F>(
        &mut self,
        input: impl Into<Input>,
        host: impl Into<String>,
        username: impl Into<String>,
        configure: F,
    ) -> TaskName
    where
        F: FnOnce(SftpExportTask) -> SftpExportTask,
    {
        self.add_configured_task(SftpExportTask::new(input, host, username), configure)
    }

    /// Adds an `export/upload` task.
    pub fn export_upload(&mut self, input: impl Into<Input>, url: impl Into<String>) -> TaskName {
        self.export_upload_with(input, url, identity)
    }

    /// Adds and configures an `export/upload` task.
    pub fn export_upload_with<F>(
        &mut self,
        input: impl Into<Input>,
        url: impl Into<String>,
        configure: F,
    ) -> TaskName
    where
        F: FnOnce(ExportUploadTask) -> ExportUploadTask,
    {
        self.add_configured_task(ExportUploadTask::new(input, url), configure)
    }

    /// Finishes the graph builder and returns a regular [`JobBuilder`].
    pub fn into_builder(self) -> JobBuilder {
        self.builder
    }

    /// Finishes the graph builder and returns the job creation request.
    pub fn build(self) -> JobCreateRequest {
        self.into_builder().build()
    }

    fn add_configured_task<T, F>(&mut self, task: T, configure: F) -> TaskName
    where
        T: Into<TaskRequest>,
        F: FnOnce(T) -> T,
    {
        self.add_task(configure(task))
    }
}

impl From<JobGraphBuilder> for JobBuilder {
    fn from(builder: JobGraphBuilder) -> Self {
        builder.into_builder()
    }
}

impl From<JobGraphBuilder> for JobCreateRequest {
    fn from(builder: JobGraphBuilder) -> Self {
        builder.build()
    }
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct JobListQuery {
    #[serde(rename = "filter[status]", skip_serializing_if = "Option::is_none")]
    filter_status: Option<JobStatus>,
    #[serde(rename = "filter[tag]", skip_serializing_if = "Option::is_none")]
    filter_tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    include: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    per_page: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    page: Option<u32>,
}

impl JobListQuery {
    pub fn status(mut self, status: JobStatus) -> Self {
        self.filter_status = Some(status);
        self
    }

    pub fn tag(mut self, tag: impl Into<String>) -> Self {
        self.filter_tag = Some(tag.into());
        self
    }

    pub fn include(mut self, include: impl Into<String>) -> Self {
        self.include = Some(include.into());
        self
    }

    pub fn per_page(mut self, per_page: u32) -> Self {
        self.per_page = Some(per_page);
        self
    }

    pub fn page(mut self, page: u32) -> Self {
        self.page = Some(page);
        self
    }
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct JobGetQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    include: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    redirect: Option<bool>,
}

impl JobGetQuery {
    pub fn include(mut self, include: impl Into<String>) -> Self {
        self.include = Some(include.into());
        self
    }

    pub fn redirect(mut self, redirect: bool) -> Self {
        self.redirect = Some(redirect);
        self
    }
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct TaskListQuery {
    #[serde(rename = "filter[job_id]", skip_serializing_if = "Option::is_none")]
    filter_job_id: Option<String>,
    #[serde(rename = "filter[status]", skip_serializing_if = "Option::is_none")]
    filter_status: Option<TaskStatus>,
    #[serde(rename = "filter[operation]", skip_serializing_if = "Option::is_none")]
    filter_operation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    include: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    per_page: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    page: Option<u32>,
}

impl TaskListQuery {
    pub fn job_id(mut self, job_id: impl Into<String>) -> Self {
        self.filter_job_id = Some(job_id.into());
        self
    }

    pub fn status(mut self, status: TaskStatus) -> Self {
        self.filter_status = Some(status);
        self
    }

    pub fn operation(mut self, operation: impl Into<String>) -> Self {
        self.filter_operation = Some(operation.into());
        self
    }

    pub fn include(mut self, include: impl Into<String>) -> Self {
        self.include = Some(include.into());
        self
    }

    pub fn per_page(mut self, per_page: u32) -> Self {
        self.per_page = Some(per_page);
        self
    }

    pub fn page(mut self, page: u32) -> Self {
        self.page = Some(page);
        self
    }
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct TaskGetQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    include: Option<String>,
}

impl TaskGetQuery {
    pub fn include(mut self, include: impl Into<String>) -> Self {
        self.include = Some(include.into());
        self
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum JobStatus {
    Waiting,
    Processing,
    Finished,
    Error,
    #[serde(other)]
    Unknown,
}

impl JobStatus {
    pub fn is_waiting(&self) -> bool {
        matches!(self, Self::Waiting)
    }

    pub fn is_processing(&self) -> bool {
        matches!(self, Self::Processing)
    }

    pub fn is_finished(&self) -> bool {
        matches!(self, Self::Finished)
    }

    pub fn is_error(&self) -> bool {
        matches!(self, Self::Error)
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Finished | Self::Error)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum TaskStatus {
    Waiting,
    Queued,
    Processing,
    Finished,
    Error,
    #[serde(other)]
    Unknown,
}

impl TaskStatus {
    pub fn is_waiting(&self) -> bool {
        matches!(self, Self::Waiting)
    }

    pub fn is_queued(&self) -> bool {
        matches!(self, Self::Queued)
    }

    pub fn is_processing(&self) -> bool {
        matches!(self, Self::Processing)
    }

    pub fn is_finished(&self) -> bool {
        matches!(self, Self::Finished)
    }

    pub fn is_error(&self) -> bool {
        matches!(self, Self::Error)
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Finished | Self::Error)
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[non_exhaustive]
pub struct Job {
    pub id: String,
    #[serde(default)]
    pub tag: Option<String>,
    pub status: JobStatus,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub started_at: Option<String>,
    #[serde(default)]
    pub ended_at: Option<String>,
    #[serde(default)]
    pub tasks: Vec<JobTask>,
    #[serde(default)]
    pub links: BTreeMap<String, Value>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl Job {
    pub fn is_finished(&self) -> bool {
        self.status.is_finished()
    }

    pub fn is_error(&self) -> bool {
        self.status.is_error()
    }

    pub fn is_terminal(&self) -> bool {
        self.status.is_terminal()
    }

    pub fn export_tasks(&self) -> impl Iterator<Item = &JobTask> {
        self.tasks.iter().filter(|task| task.is_export_url())
    }

    pub fn finished_export_tasks(&self) -> impl Iterator<Item = &JobTask> {
        self.export_tasks().filter(|task| task.is_finished())
    }

    pub fn export_urls(&self) -> Vec<&FileResult> {
        self.finished_export_tasks()
            .flat_map(JobTask::files)
            .collect()
    }
}

impl fmt::Debug for Job {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Job")
            .field("id", &self.id)
            .field("tag", &self.tag)
            .field("status", &self.status)
            .field("created_at", &self.created_at)
            .field("started_at", &self.started_at)
            .field("ended_at", &self.ended_at)
            .field("tasks", &self.tasks)
            .field("links", &self.links)
            .field("extra", &RedactedValueMap(&self.extra))
            .finish()
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[non_exhaustive]
pub struct JobTask {
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    pub operation: String,
    pub status: TaskStatus,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub code: Option<String>,
    #[serde(default)]
    pub credits: Option<f64>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub started_at: Option<String>,
    #[serde(default)]
    pub ended_at: Option<String>,
    #[serde(default)]
    pub result: Option<TaskResult>,
    #[serde(default)]
    pub payload: Option<Value>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl JobTask {
    pub fn is_finished(&self) -> bool {
        self.status.is_finished()
    }

    pub fn is_error(&self) -> bool {
        self.status.is_error()
    }

    pub fn is_terminal(&self) -> bool {
        self.status.is_terminal()
    }

    pub fn is_export_url(&self) -> bool {
        self.operation == "export/url"
    }

    pub fn files(&self) -> impl Iterator<Item = &FileResult> {
        self.result
            .as_ref()
            .into_iter()
            .flat_map(|result| result.files.iter())
    }
}

impl fmt::Debug for JobTask {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("JobTask")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("operation", &self.operation)
            .field("status", &self.status)
            .field("message", &self.message)
            .field("code", &self.code)
            .field("credits", &self.credits)
            .field("created_at", &self.created_at)
            .field("started_at", &self.started_at)
            .field("ended_at", &self.ended_at)
            .field("result", &self.result)
            .field("payload", &redacted_option(&self.payload))
            .field("extra", &RedactedValueMap(&self.extra))
            .finish()
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[non_exhaustive]
pub struct Task {
    pub id: String,
    #[serde(default)]
    pub job_id: Option<String>,
    pub operation: String,
    pub status: TaskStatus,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub code: Option<String>,
    #[serde(default)]
    pub credits: Option<f64>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub started_at: Option<String>,
    #[serde(default)]
    pub ended_at: Option<String>,
    #[serde(default)]
    pub depends_on_tasks: BTreeMap<String, String>,
    #[serde(default)]
    pub result: Option<TaskResult>,
    #[serde(default)]
    pub payload: Option<Value>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl Task {
    pub fn is_finished(&self) -> bool {
        self.status.is_finished()
    }

    pub fn is_error(&self) -> bool {
        self.status.is_error()
    }

    pub fn is_terminal(&self) -> bool {
        self.status.is_terminal()
    }

    pub fn is_import_upload(&self) -> bool {
        self.operation == "import/upload"
    }

    pub fn upload_form(&self) -> Option<&UploadForm> {
        self.result
            .as_ref()
            .and_then(|result| result.form.as_ref())
            .filter(|_| self.is_import_upload())
    }

    pub fn is_upload_ready(&self) -> bool {
        self.upload_form().is_some()
    }

    pub fn files(&self) -> impl Iterator<Item = &FileResult> {
        self.result
            .as_ref()
            .into_iter()
            .flat_map(|result| result.files.iter())
    }
}

impl fmt::Debug for Task {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Task")
            .field("id", &self.id)
            .field("job_id", &self.job_id)
            .field("operation", &self.operation)
            .field("status", &self.status)
            .field("message", &self.message)
            .field("code", &self.code)
            .field("credits", &self.credits)
            .field("created_at", &self.created_at)
            .field("started_at", &self.started_at)
            .field("ended_at", &self.ended_at)
            .field("depends_on_tasks", &self.depends_on_tasks)
            .field("result", &self.result)
            .field("payload", &redacted_option(&self.payload))
            .field("extra", &RedactedValueMap(&self.extra))
            .finish()
    }
}

#[derive(Clone, Default, Deserialize, Serialize)]
#[non_exhaustive]
pub struct TaskResult {
    #[serde(default)]
    pub files: Vec<FileResult>,
    #[serde(default)]
    pub form: Option<UploadForm>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl fmt::Debug for TaskResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TaskResult")
            .field("files", &self.files)
            .field("form", &self.form)
            .field("extra", &RedactedValueMap(&self.extra))
            .finish()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[non_exhaustive]
pub struct FileResult {
    #[serde(default)]
    pub dir: Option<String>,
    pub filename: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Deserialize, Serialize)]
#[non_exhaustive]
pub struct UploadForm {
    pub url: String,
    #[serde(default)]
    pub parameters: BTreeMap<String, Value>,
}

impl fmt::Debug for UploadForm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UploadForm")
            .field("url", &self.url)
            .field("parameters", &RedactedValueMap(&self.parameters))
            .finish()
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct DataEnvelope<T> {
    pub data: T,
    #[serde(default)]
    pub links: PaginationLinks,
    #[serde(default)]
    pub meta: PaginationMeta,
}
