use std::{collections::BTreeMap, fmt};

use serde::{Serialize, Serializer};
use serde_json::{Map, Value};

use crate::file_extension::normalize_file_extension;

pub type ExtraOptions = BTreeMap<String, Value>;

/// Input dependency for a CloudConvert task.
///
/// Most SDK methods accept `impl Into<Input>`, so callers can pass a task name,
/// a [`crate::TaskName`] handle, or a collection of task names for multi-input
/// operations such as `merge` and `export/url`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(untagged)]
#[non_exhaustive]
pub enum Input {
    Task(String),
    Tasks(Vec<String>),
}

impl From<String> for Input {
    fn from(value: String) -> Self {
        Self::Task(value)
    }
}

impl From<&str> for Input {
    fn from(value: &str) -> Self {
        Self::Task(value.to_string())
    }
}

impl From<Vec<String>> for Input {
    fn from(value: Vec<String>) -> Self {
        Self::Tasks(value)
    }
}

impl From<Vec<&str>> for Input {
    fn from(value: Vec<&str>) -> Self {
        Self::Tasks(value.into_iter().map(str::to_string).collect())
    }
}

/// Serialized task request used by job and standalone task APIs.
///
/// Most jobs can be built with [`crate::JobCreateRequest::linear`] or
/// [`crate::JobCreateRequest::graph`]. Use `TaskRequest` directly for
/// standalone task APIs, explicit low-level job construction, or operations that
/// do not have a first-class typed builder yet.
///
/// ```
/// use cloudconvert_sdk::{FileExtension, JobCreateRequest};
///
/// let request = JobCreateRequest::graph(|job| {
///     let import = job.import_url("https://example.test/input.docx");
///     job.convert(&import, FileExtension::Pdf);
/// })
/// .build();
///
/// let payload = serde_json::to_value(request).unwrap();
/// assert_eq!(payload["tasks"]["convert"]["input"], "import-url");
/// ```
#[derive(Clone)]
pub struct TaskRequest {
    operation: String,
    payload: Map<String, Value>,
}

mod sealed {
    pub trait Sealed {}
}

/// Sealed trait implemented by SDK-owned typed task builders.
///
/// Downstream code should use [`TaskRequest::custom`] or [`GenericTask`] for
/// operations that are not yet represented by this crate.
pub trait TaskPayload: sealed::Sealed + Serialize {
    /// CloudConvert operation name serialized into the task request.
    const OPERATION: &'static str;
}

impl TaskRequest {
    /// Creates an `import/url` task request.
    pub fn import_url(url: impl Into<String>) -> Self {
        ImportUrlTask::new(url).into()
    }

    /// Creates an `import/upload` task request.
    pub fn import_upload() -> Self {
        ImportUploadTask::default().into()
    }

    /// Creates an `import/base64` task request.
    pub fn import_base64(file: impl Into<String>, filename: impl Into<String>) -> Self {
        Base64ImportTask::new(file, filename).into()
    }

    /// Creates an `import/raw` task request.
    pub fn import_raw(file: impl Into<String>, filename: impl Into<String>) -> Self {
        RawImportTask::new(file, filename).into()
    }

    /// Creates an `import/s3` task request.
    pub fn import_s3(
        bucket: impl Into<String>,
        region: impl Into<String>,
        access_key_id: impl Into<String>,
        secret_access_key: impl Into<String>,
    ) -> Self {
        S3ImportTask::new(bucket, region, access_key_id, secret_access_key).into()
    }

    /// Creates an `import/azure/blob` task request.
    pub fn import_azure_blob(
        storage_account: impl Into<String>,
        container: impl Into<String>,
    ) -> Self {
        AzureBlobImportTask::new(storage_account, container).into()
    }

    /// Creates an `import/google-cloud-storage` task request.
    pub fn import_google_cloud_storage(
        project_id: impl Into<String>,
        bucket: impl Into<String>,
        client_email: impl Into<String>,
        private_key: impl Into<String>,
    ) -> Self {
        GoogleCloudStorageImportTask::new(project_id, bucket, client_email, private_key).into()
    }

    /// Creates an `import/openstack` task request.
    pub fn import_openstack(
        auth_url: impl Into<String>,
        username: impl Into<String>,
        password: impl Into<String>,
        region: impl Into<String>,
        container: impl Into<String>,
    ) -> Self {
        OpenStackImportTask::new(auth_url, username, password, region, container).into()
    }

    /// Creates an `import/sftp` task request.
    pub fn import_sftp(host: impl Into<String>, username: impl Into<String>) -> Self {
        SftpImportTask::new(host, username).into()
    }

    /// Creates a `convert` task request.
    pub fn convert(input: impl Into<Input>, output_format: impl Into<String>) -> Self {
        ConvertTask::new(input, output_format).into()
    }

    /// Creates an `optimize` task request.
    pub fn optimize(input: impl Into<Input>) -> Self {
        OptimizeTask::new(input).into()
    }

    /// Creates a `watermark` task request from a typed watermark task.
    pub fn watermark(task: WatermarkTask) -> Self {
        task.into()
    }

    /// Creates a `capture-website` task request.
    pub fn capture_website(url: impl Into<String>, output_format: impl Into<String>) -> Self {
        CaptureWebsiteTask::new(url, output_format).into()
    }

    /// Creates a `thumbnail` task request.
    pub fn thumbnail(input: impl Into<Input>, output_format: impl Into<String>) -> Self {
        ThumbnailTask::new(input, output_format).into()
    }

    /// Creates a `metadata` task request.
    pub fn metadata(input: impl Into<Input>) -> Self {
        MetadataTask::new(input).into()
    }

    /// Creates a `metadata/write` task request.
    pub fn metadata_write(input: impl Into<Input>) -> Self {
        MetadataWriteTask::new(input).into()
    }

    /// Creates a `merge` task request.
    pub fn merge(input: impl Into<Input>, output_format: impl Into<String>) -> Self {
        MergeTask::new(input, output_format).into()
    }

    /// Creates an `archive` task request.
    pub fn archive(input: impl Into<Input>, output_format: impl Into<String>) -> Self {
        ArchiveTask::new(input, output_format).into()
    }

    /// Creates a `command` task request.
    pub fn command(
        input: impl Into<Input>,
        engine: impl Into<String>,
        command: impl Into<String>,
        arguments: impl Into<String>,
    ) -> Self {
        CommandTask::new(input, engine, command, arguments).into()
    }

    /// Creates a `pdf/a` task request.
    pub fn pdf_a(input: impl Into<Input>) -> Self {
        PdfATask::new(input).into()
    }

    /// Creates a `pdf/x` task request.
    pub fn pdf_x(input: impl Into<Input>) -> Self {
        PdfXTask::new(input).into()
    }

    /// Creates a `pdf/ocr` task request.
    pub fn pdf_ocr(input: impl Into<Input>) -> Self {
        PdfOcrTask::new(input).into()
    }

    /// Creates a `pdf/encrypt` task request.
    pub fn pdf_encrypt(input: impl Into<Input>) -> Self {
        PdfEncryptTask::new(input).into()
    }

    /// Creates a `pdf/decrypt` task request.
    pub fn pdf_decrypt(input: impl Into<Input>) -> Self {
        PdfDecryptTask::new(input).into()
    }

    /// Creates a `pdf/split-pages` task request.
    pub fn pdf_split_pages(input: impl Into<Input>) -> Self {
        PdfSplitPagesTask::new(input).into()
    }

    /// Creates a `pdf/extract-pages` task request.
    pub fn pdf_extract_pages(input: impl Into<Input>) -> Self {
        PdfExtractPagesTask::new(input).into()
    }

    /// Creates a `pdf/rotate-pages` task request.
    pub fn pdf_rotate_pages(input: impl Into<Input>) -> Self {
        PdfRotatePagesTask::new(input).into()
    }

    /// Creates an `export/url` task request.
    pub fn export_url(input: impl Into<Input>) -> Self {
        ExportUrlTask::new(input).into()
    }

    /// Creates an `export/s3` task request.
    pub fn export_s3(
        input: impl Into<Input>,
        bucket: impl Into<String>,
        region: impl Into<String>,
        access_key_id: impl Into<String>,
        secret_access_key: impl Into<String>,
    ) -> Self {
        S3ExportTask::new(input, bucket, region, access_key_id, secret_access_key).into()
    }

    /// Creates an `export/azure/blob` task request.
    pub fn export_azure_blob(
        input: impl Into<Input>,
        storage_account: impl Into<String>,
        container: impl Into<String>,
    ) -> Self {
        AzureBlobExportTask::new(input, storage_account, container).into()
    }

    /// Creates an `export/google-cloud-storage` task request.
    pub fn export_google_cloud_storage(
        input: impl Into<Input>,
        project_id: impl Into<String>,
        bucket: impl Into<String>,
        client_email: impl Into<String>,
        private_key: impl Into<String>,
    ) -> Self {
        GoogleCloudStorageExportTask::new(input, project_id, bucket, client_email, private_key)
            .into()
    }

    /// Creates an `export/openstack` task request.
    pub fn export_openstack(
        input: impl Into<Input>,
        auth_url: impl Into<String>,
        username: impl Into<String>,
        password: impl Into<String>,
        region: impl Into<String>,
        container: impl Into<String>,
    ) -> Self {
        OpenStackExportTask::new(input, auth_url, username, password, region, container).into()
    }

    /// Creates an `export/sftp` task request.
    pub fn export_sftp(
        input: impl Into<Input>,
        host: impl Into<String>,
        username: impl Into<String>,
    ) -> Self {
        SftpExportTask::new(input, host, username).into()
    }

    /// Creates an `export/upload` task request.
    pub fn export_upload(input: impl Into<Input>, url: impl Into<String>) -> Self {
        ExportUploadTask::new(input, url).into()
    }

    /// Starts a custom task request for an operation not typed by the SDK.
    ///
    /// ```
    /// use cloudconvert_sdk::TaskRequest;
    /// use serde_json::json;
    ///
    /// let task = TaskRequest::custom("custom/op")
    ///     .field("input", "import-file")
    ///     .field("custom_option", json!(true));
    ///
    /// assert_eq!(task.operation(), "custom/op");
    /// ```
    pub fn custom(operation: impl Into<String>) -> GenericTask {
        GenericTask::new(operation)
    }

    /// Converts a typed task payload into a [`TaskRequest`].
    pub fn try_from_payload<T>(payload: T) -> crate::Result<Self>
    where
        T: TaskPayload,
    {
        Ok(Self {
            operation: T::OPERATION.to_string(),
            payload: serialize_payload(payload)?,
        })
    }

    /// Converts a typed task payload into a [`TaskRequest`].
    ///
    /// Panics only if an SDK-owned typed task serializes to something other than
    /// a JSON object.
    pub fn from_payload<T>(payload: T) -> Self
    where
        T: TaskPayload,
    {
        Self::try_from_payload(payload)
            .expect("task payload serialization should produce a JSON object")
    }

    /// Returns the CloudConvert operation name.
    pub fn operation(&self) -> &str {
        self.operation.as_str()
    }

    /// Returns the task payload without the injected `operation` field.
    pub fn payload(&self) -> &Map<String, Value> {
        &self.payload
    }
}

impl Serialize for TaskRequest {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut object = self.payload.clone();
        object.insert(
            "operation".to_string(),
            Value::String(self.operation.clone()),
        );

        object.serialize(serializer)
    }
}

impl fmt::Debug for TaskRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TaskRequest")
            .field("operation", &self.operation)
            .field("payload", &"REDACTED")
            .finish()
    }
}

macro_rules! task_payload {
    ($type:ty, $operation:literal) => {
        impl sealed::Sealed for $type {}

        impl TaskPayload for $type {
            const OPERATION: &'static str = $operation;
        }

        impl From<$type> for TaskRequest {
            fn from(value: $type) -> Self {
                Self::from_payload(value)
            }
        }
    };
}

fn serialize_payload<T>(payload: T) -> crate::Result<Map<String, Value>>
where
    T: Serialize,
{
    match serde_json::to_value(payload)? {
        Value::Object(object) => Ok(object),
        _ => Err(<serde_json::Error as serde::ser::Error>::custom(
            "task payload serialization must produce a JSON object",
        )
        .into()),
    }
}

fn insert_extra_object_field(
    extra: &mut ExtraOptions,
    object_key: &str,
    field_key: impl Into<String>,
    value: impl Into<Value>,
) {
    match extra
        .entry(object_key.to_string())
        .or_insert_with(|| Value::Object(Map::new()))
    {
        Value::Object(object) => {
            object.insert(field_key.into(), value.into());
        }
        existing => {
            let mut object = Map::new();
            object.insert(field_key.into(), value.into());
            *existing = Value::Object(object);
        }
    }
}

fn redacted_option<T>(value: &Option<T>) -> Option<&'static str> {
    value.as_ref().map(|_| "REDACTED")
}

struct RedactedStringMap<'a>(&'a BTreeMap<String, String>);

impl fmt::Debug for RedactedStringMap<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut debug = f.debug_map();
        for key in self.0.keys() {
            debug.entry(key, &"REDACTED");
        }
        debug.finish()
    }
}

macro_rules! debug_struct_redacted {
    (
        $name:ident {
            fields: [$($field:ident),* $(,)?],
            redacted: [$($redacted:ident),* $(,)?],
            redacted_options: [$($redacted_option:ident),* $(,)?],
            redacted_maps: [$($redacted_map:ident),* $(,)?]
        }
    ) => {
        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                let mut debug = f.debug_struct(stringify!($name));
                $(debug.field(stringify!($field), &self.$field);)*
                $(debug.field(stringify!($redacted), &"REDACTED");)*
                $(debug.field(stringify!($redacted_option), &redacted_option(&self.$redacted_option));)*
                $(debug.field(stringify!($redacted_map), &RedactedStringMap(&self.$redacted_map));)*
                debug.finish()
            }
        }
    };
}

#[derive(Clone, Serialize)]
pub struct ImportUrlTask {
    url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    filename: Option<String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    headers: BTreeMap<String, String>,
}

impl ImportUrlTask {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            filename: None,
            headers: BTreeMap::new(),
        }
    }

    pub fn filename(mut self, filename: impl Into<String>) -> Self {
        self.filename = Some(filename.into());
        self
    }

    pub fn header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(key.into(), value.into());
        self
    }
}

task_payload!(ImportUrlTask, "import/url");

#[derive(Clone, Debug, Default, Serialize)]
pub struct ImportUploadTask {
    #[serde(skip_serializing_if = "Option::is_none")]
    redirect: Option<String>,
}

impl ImportUploadTask {
    pub fn redirect(mut self, redirect: impl Into<String>) -> Self {
        self.redirect = Some(redirect.into());
        self
    }
}

task_payload!(ImportUploadTask, "import/upload");

#[derive(Clone, Debug, Serialize)]
pub struct Base64ImportTask {
    file: String,
    filename: String,
}

impl Base64ImportTask {
    pub fn new(file: impl Into<String>, filename: impl Into<String>) -> Self {
        Self {
            file: file.into(),
            filename: filename.into(),
        }
    }
}

task_payload!(Base64ImportTask, "import/base64");

#[derive(Clone, Debug, Serialize)]
pub struct RawImportTask {
    file: String,
    filename: String,
}

impl RawImportTask {
    pub fn new(file: impl Into<String>, filename: impl Into<String>) -> Self {
        Self {
            file: file.into(),
            filename: filename.into(),
        }
    }
}

task_payload!(RawImportTask, "import/raw");

#[derive(Clone, Serialize)]
pub struct S3ImportTask {
    bucket: String,
    region: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    endpoint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    key_prefix: Option<String>,
    access_key_id: String,
    secret_access_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    session_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    filename: Option<String>,
}

impl S3ImportTask {
    pub fn new(
        bucket: impl Into<String>,
        region: impl Into<String>,
        access_key_id: impl Into<String>,
        secret_access_key: impl Into<String>,
    ) -> Self {
        Self {
            bucket: bucket.into(),
            region: region.into(),
            endpoint: None,
            key: None,
            key_prefix: None,
            access_key_id: access_key_id.into(),
            secret_access_key: secret_access_key.into(),
            session_token: None,
            filename: None,
        }
    }

    pub fn endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.endpoint = Some(endpoint.into());
        self
    }

    pub fn key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }

    pub fn key_prefix(mut self, key_prefix: impl Into<String>) -> Self {
        self.key_prefix = Some(key_prefix.into());
        self
    }

    pub fn session_token(mut self, session_token: impl Into<String>) -> Self {
        self.session_token = Some(session_token.into());
        self
    }

    pub fn filename(mut self, filename: impl Into<String>) -> Self {
        self.filename = Some(filename.into());
        self
    }
}

task_payload!(S3ImportTask, "import/s3");

#[derive(Clone, Serialize)]
pub struct AzureBlobImportTask {
    storage_account: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    storage_access_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sas_token: Option<String>,
    container: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    blob: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    blob_prefix: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    filename: Option<String>,
}

impl AzureBlobImportTask {
    pub fn new(storage_account: impl Into<String>, container: impl Into<String>) -> Self {
        Self {
            storage_account: storage_account.into(),
            storage_access_key: None,
            sas_token: None,
            container: container.into(),
            blob: None,
            blob_prefix: None,
            filename: None,
        }
    }

    pub fn storage_access_key(mut self, storage_access_key: impl Into<String>) -> Self {
        self.storage_access_key = Some(storage_access_key.into());
        self
    }

    pub fn sas_token(mut self, sas_token: impl Into<String>) -> Self {
        self.sas_token = Some(sas_token.into());
        self
    }

    pub fn blob(mut self, blob: impl Into<String>) -> Self {
        self.blob = Some(blob.into());
        self
    }

    pub fn blob_prefix(mut self, blob_prefix: impl Into<String>) -> Self {
        self.blob_prefix = Some(blob_prefix.into());
        self
    }

    pub fn filename(mut self, filename: impl Into<String>) -> Self {
        self.filename = Some(filename.into());
        self
    }
}

task_payload!(AzureBlobImportTask, "import/azure/blob");

#[derive(Clone, Serialize)]
pub struct GoogleCloudStorageImportTask {
    project_id: String,
    bucket: String,
    client_email: String,
    private_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    file_prefix: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    filename: Option<String>,
}

impl GoogleCloudStorageImportTask {
    pub fn new(
        project_id: impl Into<String>,
        bucket: impl Into<String>,
        client_email: impl Into<String>,
        private_key: impl Into<String>,
    ) -> Self {
        Self {
            project_id: project_id.into(),
            bucket: bucket.into(),
            client_email: client_email.into(),
            private_key: private_key.into(),
            file: None,
            file_prefix: None,
            filename: None,
        }
    }

    pub fn file(mut self, file: impl Into<String>) -> Self {
        self.file = Some(file.into());
        self
    }

    pub fn file_prefix(mut self, file_prefix: impl Into<String>) -> Self {
        self.file_prefix = Some(file_prefix.into());
        self
    }

    pub fn filename(mut self, filename: impl Into<String>) -> Self {
        self.filename = Some(filename.into());
        self
    }
}

task_payload!(GoogleCloudStorageImportTask, "import/google-cloud-storage");

#[derive(Clone, Serialize)]
pub struct OpenStackImportTask {
    auth_url: String,
    username: String,
    password: String,
    region: String,
    container: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    file_prefix: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    filename: Option<String>,
}

impl OpenStackImportTask {
    pub fn new(
        auth_url: impl Into<String>,
        username: impl Into<String>,
        password: impl Into<String>,
        region: impl Into<String>,
        container: impl Into<String>,
    ) -> Self {
        Self {
            auth_url: auth_url.into(),
            username: username.into(),
            password: password.into(),
            region: region.into(),
            container: container.into(),
            file: None,
            file_prefix: None,
            filename: None,
        }
    }

    pub fn file(mut self, file: impl Into<String>) -> Self {
        self.file = Some(file.into());
        self
    }

    pub fn file_prefix(mut self, file_prefix: impl Into<String>) -> Self {
        self.file_prefix = Some(file_prefix.into());
        self
    }

    pub fn filename(mut self, filename: impl Into<String>) -> Self {
        self.filename = Some(filename.into());
        self
    }
}

task_payload!(OpenStackImportTask, "import/openstack");

#[derive(Clone, Serialize)]
pub struct SftpImportTask {
    host: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    port: Option<u16>,
    username: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    password: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    private_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    filename: Option<String>,
}

impl SftpImportTask {
    pub fn new(host: impl Into<String>, username: impl Into<String>) -> Self {
        Self {
            host: host.into(),
            port: None,
            username: username.into(),
            password: None,
            private_key: None,
            file: None,
            path: None,
            filename: None,
        }
    }

    pub fn port(mut self, port: u16) -> Self {
        self.port = Some(port);
        self
    }

    pub fn password(mut self, password: impl Into<String>) -> Self {
        self.password = Some(password.into());
        self
    }

    pub fn private_key(mut self, private_key: impl Into<String>) -> Self {
        self.private_key = Some(private_key.into());
        self
    }

    pub fn file(mut self, file: impl Into<String>) -> Self {
        self.file = Some(file.into());
        self
    }

    pub fn path(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }

    pub fn filename(mut self, filename: impl Into<String>) -> Self {
        self.filename = Some(filename.into());
        self
    }
}

task_payload!(SftpImportTask, "import/sftp");

#[derive(Clone, Debug, Serialize)]
pub struct ConvertTask {
    // Flattened extras MUST stay the first field: serde emits fields in
    // declaration order and serde_json keeps the last write on a key, so
    // canonical fields below win over any colliding option() key.
    #[serde(flatten, skip_serializing_if = "BTreeMap::is_empty")]
    extra: ExtraOptions,
    input: Input,
    #[serde(skip_serializing_if = "Option::is_none")]
    input_format: Option<String>,
    output_format: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    engine: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    engine_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    filename: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    timeout: Option<u64>,
}

impl ConvertTask {
    pub fn new(input: impl Into<Input>, output_format: impl Into<String>) -> Self {
        Self {
            input: input.into(),
            input_format: None,
            output_format: normalize_file_extension(output_format),
            engine: None,
            engine_version: None,
            filename: None,
            timeout: None,
            extra: BTreeMap::new(),
        }
    }

    pub fn input_format(mut self, input_format: impl Into<String>) -> Self {
        self.input_format = Some(normalize_file_extension(input_format));
        self
    }

    pub fn engine(mut self, engine: impl Into<String>) -> Self {
        self.engine = Some(engine.into());
        self
    }

    pub fn engine_version(mut self, engine_version: impl Into<String>) -> Self {
        self.engine_version = Some(engine_version.into());
        self
    }

    pub fn filename(mut self, filename: impl Into<String>) -> Self {
        self.filename = Some(filename.into());
        self
    }

    pub fn timeout(mut self, timeout: u64) -> Self {
        self.timeout = Some(timeout);
        self
    }

    pub fn option(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.extra.insert(key.into(), value.into());
        self
    }
}

task_payload!(ConvertTask, "convert");

#[derive(Clone, Debug, Serialize)]
pub struct OptimizeTask {
    // Flattened extras MUST stay the first field: serde emits fields in
    // declaration order and serde_json keeps the last write on a key, so
    // canonical fields below win over any colliding option() key.
    #[serde(flatten, skip_serializing_if = "BTreeMap::is_empty")]
    extra: ExtraOptions,
    input: Input,
    #[serde(skip_serializing_if = "Option::is_none")]
    input_format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    quality: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    profile: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    engine: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    engine_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    filename: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    timeout: Option<u64>,
}

impl OptimizeTask {
    pub fn new(input: impl Into<Input>) -> Self {
        Self {
            input: input.into(),
            input_format: None,
            quality: None,
            profile: None,
            engine: None,
            engine_version: None,
            filename: None,
            timeout: None,
            extra: BTreeMap::new(),
        }
    }

    pub fn input_format(mut self, input_format: impl Into<String>) -> Self {
        self.input_format = Some(normalize_file_extension(input_format));
        self
    }

    pub fn quality(mut self, quality: u8) -> Self {
        self.quality = Some(quality);
        self
    }

    pub fn profile(mut self, profile: impl Into<String>) -> Self {
        self.profile = Some(profile.into());
        self
    }

    pub fn engine(mut self, engine: impl Into<String>) -> Self {
        self.engine = Some(engine.into());
        self
    }

    pub fn engine_version(mut self, engine_version: impl Into<String>) -> Self {
        self.engine_version = Some(engine_version.into());
        self
    }

    pub fn filename(mut self, filename: impl Into<String>) -> Self {
        self.filename = Some(filename.into());
        self
    }

    pub fn timeout(mut self, timeout: u64) -> Self {
        self.timeout = Some(timeout);
        self
    }

    pub fn option(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.extra.insert(key.into(), value.into());
        self
    }
}

task_payload!(OptimizeTask, "optimize");

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum Layer {
    Above,
    Below,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum PositionVertical {
    Top,
    Center,
    Bottom,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum PositionHorizontal {
    Left,
    Center,
    Right,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum FontAlign {
    Left,
    Center,
    Right,
}

#[derive(Clone, Debug, Serialize)]
pub struct WatermarkTask {
    // Flattened extras MUST stay the first field: serde emits fields in
    // declaration order and serde_json keeps the last write on a key, so
    // canonical fields below win over any colliding option() key.
    #[serde(flatten, skip_serializing_if = "BTreeMap::is_empty")]
    extra: ExtraOptions,
    input: Input,
    #[serde(skip_serializing_if = "Option::is_none")]
    input_format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pages: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    layer: Option<Layer>,
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    font_size: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    font_width_percent: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    font_color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    font_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    font_align: Option<FontAlign>,
    #[serde(skip_serializing_if = "Option::is_none")]
    image: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    image_width: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    image_height: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    image_width_percent: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    position_vertical: Option<PositionVertical>,
    #[serde(skip_serializing_if = "Option::is_none")]
    position_horizontal: Option<PositionHorizontal>,
    #[serde(skip_serializing_if = "Option::is_none")]
    margin_vertical: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    margin_horizontal: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    opacity: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    rotation: Option<i16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    filename: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    engine: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    engine_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    timeout: Option<u64>,
}

impl WatermarkTask {
    pub fn text(input: impl Into<Input>, text: impl Into<String>) -> Self {
        Self::new(input).text_content(text)
    }

    pub fn image(input: impl Into<Input>, image_task_name: impl Into<String>) -> Self {
        Self::new(input).image_task(image_task_name)
    }

    fn new(input: impl Into<Input>) -> Self {
        Self {
            input: input.into(),
            input_format: None,
            pages: None,
            layer: None,
            text: None,
            font_size: None,
            font_width_percent: None,
            font_color: None,
            font_name: None,
            font_align: None,
            image: None,
            image_width: None,
            image_height: None,
            image_width_percent: None,
            position_vertical: None,
            position_horizontal: None,
            margin_vertical: None,
            margin_horizontal: None,
            opacity: None,
            rotation: None,
            filename: None,
            engine: None,
            engine_version: None,
            timeout: None,
            extra: BTreeMap::new(),
        }
    }

    fn text_content(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self
    }

    fn image_task(mut self, image_task_name: impl Into<String>) -> Self {
        self.image = Some(image_task_name.into());
        self
    }

    pub fn input_format(mut self, input_format: impl Into<String>) -> Self {
        self.input_format = Some(normalize_file_extension(input_format));
        self
    }

    pub fn pages(mut self, pages: impl Into<String>) -> Self {
        self.pages = Some(pages.into());
        self
    }

    pub fn layer(mut self, layer: Layer) -> Self {
        self.layer = Some(layer);
        self
    }

    pub fn font_size(mut self, font_size: u32) -> Self {
        self.font_size = Some(font_size);
        self
    }

    pub fn font_width_percent(mut self, font_width_percent: u8) -> Self {
        self.font_width_percent = Some(font_width_percent);
        self
    }

    pub fn font_color(mut self, font_color: impl Into<String>) -> Self {
        self.font_color = Some(font_color.into());
        self
    }

    pub fn font_name(mut self, font_name: impl Into<String>) -> Self {
        self.font_name = Some(font_name.into());
        self
    }

    pub fn font_align_left(mut self) -> Self {
        self.font_align = Some(FontAlign::Left);
        self
    }

    pub fn font_align_center(mut self) -> Self {
        self.font_align = Some(FontAlign::Center);
        self
    }

    pub fn font_align_right(mut self) -> Self {
        self.font_align = Some(FontAlign::Right);
        self
    }

    pub fn position(mut self, vertical: PositionVertical, horizontal: PositionHorizontal) -> Self {
        self.position_vertical = Some(vertical);
        self.position_horizontal = Some(horizontal);
        self
    }

    pub fn margins(mut self, vertical: u32, horizontal: u32) -> Self {
        self.margin_vertical = Some(vertical);
        self.margin_horizontal = Some(horizontal);
        self
    }

    pub fn opacity(mut self, opacity: u8) -> Self {
        self.opacity = Some(opacity);
        self
    }

    pub fn rotation(mut self, rotation: i16) -> Self {
        self.rotation = Some(rotation);
        self
    }

    pub fn image_width(mut self, image_width: u32) -> Self {
        self.image_width = Some(image_width);
        self
    }

    pub fn image_height(mut self, image_height: u32) -> Self {
        self.image_height = Some(image_height);
        self
    }

    pub fn image_width_percent(mut self, image_width_percent: u8) -> Self {
        self.image_width_percent = Some(image_width_percent);
        self
    }

    pub fn filename(mut self, filename: impl Into<String>) -> Self {
        self.filename = Some(filename.into());
        self
    }

    pub fn engine(mut self, engine: impl Into<String>) -> Self {
        self.engine = Some(engine.into());
        self
    }

    pub fn engine_version(mut self, engine_version: impl Into<String>) -> Self {
        self.engine_version = Some(engine_version.into());
        self
    }

    pub fn timeout(mut self, timeout: u64) -> Self {
        self.timeout = Some(timeout);
        self
    }

    pub fn option(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.extra.insert(key.into(), value.into());
        self
    }
}

task_payload!(WatermarkTask, "watermark");

#[derive(Clone, Debug, Serialize)]
pub struct CaptureWebsiteTask {
    // Flattened extras MUST stay the first field: serde emits fields in
    // declaration order and serde_json keeps the last write on a key, so
    // canonical fields below win over any colliding option() key.
    #[serde(flatten, skip_serializing_if = "BTreeMap::is_empty")]
    extra: ExtraOptions,
    url: String,
    output_format: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    engine: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    engine_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    filename: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    timeout: Option<u64>,
}

impl CaptureWebsiteTask {
    pub fn new(url: impl Into<String>, output_format: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            output_format: normalize_file_extension(output_format),
            engine: None,
            engine_version: None,
            filename: None,
            timeout: None,
            extra: BTreeMap::new(),
        }
    }

    pub fn engine(mut self, engine: impl Into<String>) -> Self {
        self.engine = Some(engine.into());
        self
    }

    pub fn engine_version(mut self, engine_version: impl Into<String>) -> Self {
        self.engine_version = Some(engine_version.into());
        self
    }

    pub fn filename(mut self, filename: impl Into<String>) -> Self {
        self.filename = Some(filename.into());
        self
    }

    pub fn timeout(mut self, timeout: u64) -> Self {
        self.timeout = Some(timeout);
        self
    }

    pub fn option(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.extra.insert(key.into(), value.into());
        self
    }
}

task_payload!(CaptureWebsiteTask, "capture-website");

#[derive(Clone, Debug, Serialize)]
pub struct ThumbnailTask {
    // Flattened extras MUST stay the first field: serde emits fields in
    // declaration order and serde_json keeps the last write on a key, so
    // canonical fields below win over any colliding option() key.
    #[serde(flatten, skip_serializing_if = "BTreeMap::is_empty")]
    extra: ExtraOptions,
    input: Input,
    #[serde(skip_serializing_if = "Option::is_none")]
    input_format: Option<String>,
    output_format: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    engine: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    engine_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    width: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    height: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    fit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    timestamp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    filename: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    timeout: Option<u64>,
}

impl ThumbnailTask {
    pub fn new(input: impl Into<Input>, output_format: impl Into<String>) -> Self {
        Self {
            input: input.into(),
            input_format: None,
            output_format: normalize_file_extension(output_format),
            engine: None,
            engine_version: None,
            width: None,
            height: None,
            fit: None,
            count: None,
            timestamp: None,
            filename: None,
            timeout: None,
            extra: BTreeMap::new(),
        }
    }

    pub fn input_format(mut self, input_format: impl Into<String>) -> Self {
        self.input_format = Some(normalize_file_extension(input_format));
        self
    }

    pub fn engine(mut self, engine: impl Into<String>) -> Self {
        self.engine = Some(engine.into());
        self
    }

    pub fn engine_version(mut self, engine_version: impl Into<String>) -> Self {
        self.engine_version = Some(engine_version.into());
        self
    }

    pub fn width(mut self, width: u32) -> Self {
        self.width = Some(width);
        self
    }

    pub fn height(mut self, height: u32) -> Self {
        self.height = Some(height);
        self
    }

    pub fn dimensions(mut self, width: u32, height: u32) -> Self {
        self.width = Some(width);
        self.height = Some(height);
        self
    }

    pub fn fit(mut self, fit: impl Into<String>) -> Self {
        self.fit = Some(fit.into());
        self
    }

    pub fn count(mut self, count: u32) -> Self {
        self.count = Some(count);
        self
    }

    pub fn timestamp(mut self, timestamp: impl Into<String>) -> Self {
        self.timestamp = Some(timestamp.into());
        self
    }

    pub fn filename(mut self, filename: impl Into<String>) -> Self {
        self.filename = Some(filename.into());
        self
    }

    pub fn timeout(mut self, timeout: u64) -> Self {
        self.timeout = Some(timeout);
        self
    }

    pub fn option(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.extra.insert(key.into(), value.into());
        self
    }
}

task_payload!(ThumbnailTask, "thumbnail");

#[derive(Clone, Debug, Serialize)]
pub struct MetadataTask {
    // Flattened extras MUST stay the first field: serde emits fields in
    // declaration order and serde_json keeps the last write on a key, so
    // canonical fields below win over any colliding option() key.
    #[serde(flatten, skip_serializing_if = "BTreeMap::is_empty")]
    extra: ExtraOptions,
    input: Input,
    #[serde(skip_serializing_if = "Option::is_none")]
    input_format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    engine: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    engine_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    timeout: Option<u64>,
}

impl MetadataTask {
    pub fn new(input: impl Into<Input>) -> Self {
        Self {
            input: input.into(),
            input_format: None,
            engine: None,
            engine_version: None,
            timeout: None,
            extra: BTreeMap::new(),
        }
    }

    pub fn input_format(mut self, input_format: impl Into<String>) -> Self {
        self.input_format = Some(normalize_file_extension(input_format));
        self
    }

    pub fn engine(mut self, engine: impl Into<String>) -> Self {
        self.engine = Some(engine.into());
        self
    }

    pub fn engine_version(mut self, engine_version: impl Into<String>) -> Self {
        self.engine_version = Some(engine_version.into());
        self
    }

    pub fn timeout(mut self, timeout: u64) -> Self {
        self.timeout = Some(timeout);
        self
    }

    pub fn option(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.extra.insert(key.into(), value.into());
        self
    }
}

task_payload!(MetadataTask, "metadata");

#[derive(Clone, Debug, Serialize)]
pub struct MetadataWriteTask {
    // Flattened extras MUST stay the first field: serde emits fields in
    // declaration order and serde_json keeps the last write on a key, so
    // canonical fields below win over any colliding option() key.
    #[serde(flatten, skip_serializing_if = "BTreeMap::is_empty")]
    extra: ExtraOptions,
    input: Input,
    #[serde(skip_serializing_if = "Option::is_none")]
    input_format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    engine: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    engine_version: Option<String>,
    metadata: BTreeMap<String, Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    filename: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    timeout: Option<u64>,
}

impl MetadataWriteTask {
    pub fn new(input: impl Into<Input>) -> Self {
        Self {
            input: input.into(),
            input_format: None,
            engine: None,
            engine_version: None,
            metadata: BTreeMap::new(),
            filename: None,
            timeout: None,
            extra: BTreeMap::new(),
        }
    }

    pub fn input_format(mut self, input_format: impl Into<String>) -> Self {
        self.input_format = Some(normalize_file_extension(input_format));
        self
    }

    pub fn engine(mut self, engine: impl Into<String>) -> Self {
        self.engine = Some(engine.into());
        self
    }

    pub fn engine_version(mut self, engine_version: impl Into<String>) -> Self {
        self.engine_version = Some(engine_version.into());
        self
    }

    pub fn metadata(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    pub fn metadata_map(mut self, metadata: BTreeMap<String, Value>) -> Self {
        self.metadata = metadata;
        self
    }

    pub fn filename(mut self, filename: impl Into<String>) -> Self {
        self.filename = Some(filename.into());
        self
    }

    pub fn timeout(mut self, timeout: u64) -> Self {
        self.timeout = Some(timeout);
        self
    }

    pub fn option(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.extra.insert(key.into(), value.into());
        self
    }
}

task_payload!(MetadataWriteTask, "metadata/write");

#[derive(Clone, Debug, Serialize)]
pub struct MergeTask {
    // Flattened extras MUST stay the first field: serde emits fields in
    // declaration order and serde_json keeps the last write on a key, so
    // canonical fields below win over any colliding option() key.
    #[serde(flatten, skip_serializing_if = "BTreeMap::is_empty")]
    extra: ExtraOptions,
    input: Input,
    output_format: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    engine: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    engine_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    filename: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    timeout: Option<u64>,
}

impl MergeTask {
    pub fn new(input: impl Into<Input>, output_format: impl Into<String>) -> Self {
        Self {
            input: input.into(),
            output_format: normalize_file_extension(output_format),
            engine: None,
            engine_version: None,
            filename: None,
            timeout: None,
            extra: BTreeMap::new(),
        }
    }

    pub fn engine(mut self, engine: impl Into<String>) -> Self {
        self.engine = Some(engine.into());
        self
    }

    pub fn engine_version(mut self, engine_version: impl Into<String>) -> Self {
        self.engine_version = Some(engine_version.into());
        self
    }

    pub fn filename(mut self, filename: impl Into<String>) -> Self {
        self.filename = Some(filename.into());
        self
    }

    pub fn timeout(mut self, timeout: u64) -> Self {
        self.timeout = Some(timeout);
        self
    }

    pub fn option(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.extra.insert(key.into(), value.into());
        self
    }
}

task_payload!(MergeTask, "merge");

#[derive(Clone, Debug, Serialize)]
pub struct ArchiveTask {
    // Flattened extras MUST stay the first field: serde emits fields in
    // declaration order and serde_json keeps the last write on a key, so
    // canonical fields below win over any colliding option() key.
    #[serde(flatten, skip_serializing_if = "BTreeMap::is_empty")]
    extra: ExtraOptions,
    input: Input,
    output_format: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    engine: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    engine_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    filename: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    timeout: Option<u64>,
}

impl ArchiveTask {
    pub fn new(input: impl Into<Input>, output_format: impl Into<String>) -> Self {
        Self {
            input: input.into(),
            output_format: normalize_file_extension(output_format),
            engine: None,
            engine_version: None,
            filename: None,
            timeout: None,
            extra: BTreeMap::new(),
        }
    }

    pub fn engine(mut self, engine: impl Into<String>) -> Self {
        self.engine = Some(engine.into());
        self
    }

    pub fn engine_version(mut self, engine_version: impl Into<String>) -> Self {
        self.engine_version = Some(engine_version.into());
        self
    }

    pub fn filename(mut self, filename: impl Into<String>) -> Self {
        self.filename = Some(filename.into());
        self
    }

    pub fn timeout(mut self, timeout: u64) -> Self {
        self.timeout = Some(timeout);
        self
    }

    pub fn option(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.extra.insert(key.into(), value.into());
        self
    }
}

task_payload!(ArchiveTask, "archive");

#[derive(Clone, Debug, Serialize)]
pub struct CommandTask {
    // Flattened extras MUST stay the first field: serde emits fields in
    // declaration order and serde_json keeps the last write on a key, so
    // canonical fields below win over any colliding option() key.
    #[serde(flatten, skip_serializing_if = "BTreeMap::is_empty")]
    extra: ExtraOptions,
    input: Input,
    engine: String,
    command: String,
    arguments: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    engine_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    capture_output: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    timeout: Option<u64>,
}

impl CommandTask {
    pub fn new(
        input: impl Into<Input>,
        engine: impl Into<String>,
        command: impl Into<String>,
        arguments: impl Into<String>,
    ) -> Self {
        Self {
            input: input.into(),
            engine: engine.into(),
            command: command.into(),
            arguments: arguments.into(),
            engine_version: None,
            capture_output: None,
            timeout: None,
            extra: BTreeMap::new(),
        }
    }

    pub fn engine_version(mut self, engine_version: impl Into<String>) -> Self {
        self.engine_version = Some(engine_version.into());
        self
    }

    pub fn capture_output(mut self, capture_output: bool) -> Self {
        self.capture_output = Some(capture_output);
        self
    }

    pub fn timeout(mut self, timeout: u64) -> Self {
        self.timeout = Some(timeout);
        self
    }

    pub fn option(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.extra.insert(key.into(), value.into());
        self
    }
}

task_payload!(CommandTask, "command");

macro_rules! pdf_task {
    ($type:ident, $operation:literal) => {
        #[derive(Clone, Debug, Serialize)]
        pub struct $type {
            // Flattened extras MUST stay the first field: serde emits fields in
            // declaration order and serde_json keeps the last write on a key, so
            // canonical fields below win over any colliding option() key.
            #[serde(flatten, skip_serializing_if = "BTreeMap::is_empty")]
            extra: ExtraOptions,
            input: Input,
            #[serde(skip_serializing_if = "Option::is_none")]
            engine: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            engine_version: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            filename: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            timeout: Option<u64>,
        }

        impl $type {
            pub fn new(input: impl Into<Input>) -> Self {
                Self {
                    input: input.into(),
                    engine: None,
                    engine_version: None,
                    filename: None,
                    timeout: None,
                    extra: BTreeMap::new(),
                }
            }

            pub fn engine(mut self, engine: impl Into<String>) -> Self {
                self.engine = Some(engine.into());
                self
            }

            pub fn engine_version(mut self, engine_version: impl Into<String>) -> Self {
                self.engine_version = Some(engine_version.into());
                self
            }

            pub fn filename(mut self, filename: impl Into<String>) -> Self {
                self.filename = Some(filename.into());
                self
            }

            pub fn timeout(mut self, timeout: u64) -> Self {
                self.timeout = Some(timeout);
                self
            }

            pub fn option(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
                self.extra.insert(key.into(), value.into());
                self
            }
        }

        task_payload!($type, $operation);
    };
}

pdf_task!(PdfATask, "pdf/a");
pdf_task!(PdfXTask, "pdf/x");
pdf_task!(PdfOcrTask, "pdf/ocr");
pdf_task!(PdfEncryptTask, "pdf/encrypt");
pdf_task!(PdfDecryptTask, "pdf/decrypt");
pdf_task!(PdfSplitPagesTask, "pdf/split-pages");
pdf_task!(PdfExtractPagesTask, "pdf/extract-pages");
pdf_task!(PdfRotatePagesTask, "pdf/rotate-pages");

#[derive(Clone, Debug, Serialize)]
pub struct ExportUrlTask {
    input: Input,
    #[serde(skip_serializing_if = "Option::is_none")]
    inline: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    archive_multiple_files: Option<bool>,
}

impl ExportUrlTask {
    pub fn new(input: impl Into<Input>) -> Self {
        Self {
            input: input.into(),
            inline: None,
            archive_multiple_files: None,
        }
    }

    pub fn inline(mut self, inline: bool) -> Self {
        self.inline = Some(inline);
        self
    }

    pub fn archive_multiple_files(mut self, archive_multiple_files: bool) -> Self {
        self.archive_multiple_files = Some(archive_multiple_files);
        self
    }
}

task_payload!(ExportUrlTask, "export/url");

#[derive(Clone, Serialize)]
pub struct S3ExportTask {
    // Flattened extras MUST stay the first field: serde emits fields in
    // declaration order and serde_json keeps the last write on a key, so
    // canonical fields below win over any colliding option() key.
    #[serde(flatten, skip_serializing_if = "BTreeMap::is_empty")]
    extra: ExtraOptions,
    input: Input,
    bucket: String,
    region: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    endpoint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    key_prefix: Option<String>,
    access_key_id: String,
    secret_access_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    session_token: Option<String>,
}

impl S3ExportTask {
    pub fn new(
        input: impl Into<Input>,
        bucket: impl Into<String>,
        region: impl Into<String>,
        access_key_id: impl Into<String>,
        secret_access_key: impl Into<String>,
    ) -> Self {
        Self {
            input: input.into(),
            bucket: bucket.into(),
            region: region.into(),
            endpoint: None,
            key: None,
            key_prefix: None,
            access_key_id: access_key_id.into(),
            secret_access_key: secret_access_key.into(),
            session_token: None,
            extra: BTreeMap::new(),
        }
    }

    pub fn endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.endpoint = Some(endpoint.into());
        self
    }

    pub fn key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }

    pub fn key_prefix(mut self, key_prefix: impl Into<String>) -> Self {
        self.key_prefix = Some(key_prefix.into());
        self
    }

    pub fn session_token(mut self, session_token: impl Into<String>) -> Self {
        self.session_token = Some(session_token.into());
        self
    }

    pub fn acl(self, acl: impl Into<String>) -> Self {
        self.option("acl", acl.into())
    }

    pub fn cache_control(self, cache_control: impl Into<String>) -> Self {
        self.option("cache_control", cache_control.into())
    }

    pub fn content_disposition(self, content_disposition: impl Into<String>) -> Self {
        self.option("content_disposition", content_disposition.into())
    }

    pub fn content_type(self, content_type: impl Into<String>) -> Self {
        self.option("content_type", content_type.into())
    }

    pub fn server_side_encryption(self, server_side_encryption: impl Into<String>) -> Self {
        self.option("server_side_encryption", server_side_encryption.into())
    }

    pub fn metadata(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        insert_extra_object_field(&mut self.extra, "metadata", key, value);
        self
    }

    pub fn tag(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        insert_extra_object_field(&mut self.extra, "tagging", key, value);
        self
    }

    pub fn option(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.extra.insert(key.into(), value.into());
        self
    }
}

task_payload!(S3ExportTask, "export/s3");

#[derive(Clone, Serialize)]
pub struct AzureBlobExportTask {
    // Flattened extras MUST stay the first field: serde emits fields in
    // declaration order and serde_json keeps the last write on a key, so
    // canonical fields below win over any colliding option() key.
    #[serde(flatten, skip_serializing_if = "BTreeMap::is_empty")]
    extra: ExtraOptions,
    input: Input,
    storage_account: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    storage_access_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sas_token: Option<String>,
    container: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    blob: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    blob_prefix: Option<String>,
}

impl AzureBlobExportTask {
    pub fn new(
        input: impl Into<Input>,
        storage_account: impl Into<String>,
        container: impl Into<String>,
    ) -> Self {
        Self {
            input: input.into(),
            storage_account: storage_account.into(),
            storage_access_key: None,
            sas_token: None,
            container: container.into(),
            blob: None,
            blob_prefix: None,
            extra: BTreeMap::new(),
        }
    }

    pub fn storage_access_key(mut self, storage_access_key: impl Into<String>) -> Self {
        self.storage_access_key = Some(storage_access_key.into());
        self
    }

    pub fn sas_token(mut self, sas_token: impl Into<String>) -> Self {
        self.sas_token = Some(sas_token.into());
        self
    }

    pub fn blob(mut self, blob: impl Into<String>) -> Self {
        self.blob = Some(blob.into());
        self
    }

    pub fn blob_prefix(mut self, blob_prefix: impl Into<String>) -> Self {
        self.blob_prefix = Some(blob_prefix.into());
        self
    }

    pub fn metadata(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        insert_extra_object_field(&mut self.extra, "metadata", key, value);
        self
    }

    pub fn option(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.extra.insert(key.into(), value.into());
        self
    }
}

task_payload!(AzureBlobExportTask, "export/azure/blob");

#[derive(Clone, Serialize)]
pub struct GoogleCloudStorageExportTask {
    // Flattened extras MUST stay the first field: serde emits fields in
    // declaration order and serde_json keeps the last write on a key, so
    // canonical fields below win over any colliding option() key.
    #[serde(flatten, skip_serializing_if = "BTreeMap::is_empty")]
    extra: ExtraOptions,
    input: Input,
    project_id: String,
    bucket: String,
    client_email: String,
    private_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    file_prefix: Option<String>,
}

impl GoogleCloudStorageExportTask {
    pub fn new(
        input: impl Into<Input>,
        project_id: impl Into<String>,
        bucket: impl Into<String>,
        client_email: impl Into<String>,
        private_key: impl Into<String>,
    ) -> Self {
        Self {
            input: input.into(),
            project_id: project_id.into(),
            bucket: bucket.into(),
            client_email: client_email.into(),
            private_key: private_key.into(),
            file: None,
            file_prefix: None,
            extra: BTreeMap::new(),
        }
    }

    pub fn file(mut self, file: impl Into<String>) -> Self {
        self.file = Some(file.into());
        self
    }

    pub fn file_prefix(mut self, file_prefix: impl Into<String>) -> Self {
        self.file_prefix = Some(file_prefix.into());
        self
    }

    pub fn option(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.extra.insert(key.into(), value.into());
        self
    }
}

task_payload!(GoogleCloudStorageExportTask, "export/google-cloud-storage");

#[derive(Clone, Serialize)]
pub struct OpenStackExportTask {
    // Flattened extras MUST stay the first field: serde emits fields in
    // declaration order and serde_json keeps the last write on a key, so
    // canonical fields below win over any colliding option() key.
    #[serde(flatten, skip_serializing_if = "BTreeMap::is_empty")]
    extra: ExtraOptions,
    input: Input,
    auth_url: String,
    username: String,
    password: String,
    region: String,
    container: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    file_prefix: Option<String>,
}

impl OpenStackExportTask {
    pub fn new(
        input: impl Into<Input>,
        auth_url: impl Into<String>,
        username: impl Into<String>,
        password: impl Into<String>,
        region: impl Into<String>,
        container: impl Into<String>,
    ) -> Self {
        Self {
            input: input.into(),
            auth_url: auth_url.into(),
            username: username.into(),
            password: password.into(),
            region: region.into(),
            container: container.into(),
            file: None,
            file_prefix: None,
            extra: BTreeMap::new(),
        }
    }

    pub fn file(mut self, file: impl Into<String>) -> Self {
        self.file = Some(file.into());
        self
    }

    pub fn file_prefix(mut self, file_prefix: impl Into<String>) -> Self {
        self.file_prefix = Some(file_prefix.into());
        self
    }

    pub fn option(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.extra.insert(key.into(), value.into());
        self
    }
}

task_payload!(OpenStackExportTask, "export/openstack");

#[derive(Clone, Serialize)]
pub struct SftpExportTask {
    // Flattened extras MUST stay the first field: serde emits fields in
    // declaration order and serde_json keeps the last write on a key, so
    // canonical fields below win over any colliding option() key.
    #[serde(flatten, skip_serializing_if = "BTreeMap::is_empty")]
    extra: ExtraOptions,
    input: Input,
    host: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    port: Option<u16>,
    username: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    password: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    private_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<String>,
}

impl SftpExportTask {
    pub fn new(
        input: impl Into<Input>,
        host: impl Into<String>,
        username: impl Into<String>,
    ) -> Self {
        Self {
            input: input.into(),
            host: host.into(),
            port: None,
            username: username.into(),
            password: None,
            private_key: None,
            file: None,
            path: None,
            extra: BTreeMap::new(),
        }
    }

    pub fn port(mut self, port: u16) -> Self {
        self.port = Some(port);
        self
    }

    pub fn password(mut self, password: impl Into<String>) -> Self {
        self.password = Some(password.into());
        self
    }

    pub fn private_key(mut self, private_key: impl Into<String>) -> Self {
        self.private_key = Some(private_key.into());
        self
    }

    pub fn file(mut self, file: impl Into<String>) -> Self {
        self.file = Some(file.into());
        self
    }

    pub fn path(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }

    pub fn option(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.extra.insert(key.into(), value.into());
        self
    }
}

task_payload!(SftpExportTask, "export/sftp");

#[derive(Clone, Serialize)]
pub struct ExportUploadTask {
    // Flattened extras MUST stay the first field: serde emits fields in
    // declaration order and serde_json keeps the last write on a key, so
    // canonical fields below win over any colliding option() key.
    #[serde(flatten, skip_serializing_if = "BTreeMap::is_empty")]
    extra: ExtraOptions,
    input: Input,
    url: String,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    headers: BTreeMap<String, String>,
}

impl ExportUploadTask {
    pub fn new(input: impl Into<Input>, url: impl Into<String>) -> Self {
        Self {
            input: input.into(),
            url: url.into(),
            headers: BTreeMap::new(),
            extra: BTreeMap::new(),
        }
    }

    pub fn header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(key.into(), value.into());
        self
    }

    pub fn option(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.extra.insert(key.into(), value.into());
        self
    }
}

task_payload!(ExportUploadTask, "export/upload");

/// Builder for a custom CloudConvert task operation.
///
/// Use this when the API supports an operation or option that is not yet typed
/// by this SDK.
///
/// ```
/// use cloudconvert_sdk::TaskRequest;
///
/// let task = TaskRequest::custom("custom/op")
///     .field("input", "import-file")
///     .field("answer", 42);
///
/// assert_eq!(task.operation(), "custom/op");
/// ```
#[derive(Clone)]
pub struct GenericTask {
    operation: String,
    data: Map<String, Value>,
}

impl GenericTask {
    /// Starts a custom task for the given CloudConvert operation name.
    pub fn new(operation: impl Into<String>) -> Self {
        Self {
            operation: operation.into(),
            data: Map::new(),
        }
    }

    /// Adds a field to the custom task payload.
    pub fn field(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.data.insert(key.into(), value.into());
        self
    }

    /// Returns the custom operation name.
    pub fn operation(&self) -> &str {
        self.operation.as_str()
    }

    /// Returns the custom payload fields.
    pub fn data(&self) -> &Map<String, Value> {
        &self.data
    }
}

impl From<GenericTask> for TaskRequest {
    fn from(value: GenericTask) -> Self {
        Self {
            operation: value.operation,
            payload: value.data,
        }
    }
}

impl fmt::Debug for GenericTask {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GenericTask")
            .field("operation", &self.operation)
            .field("data", &"REDACTED")
            .finish()
    }
}

debug_struct_redacted!(ImportUrlTask {
    fields: [url, filename],
    redacted: [],
    redacted_options: [],
    redacted_maps: [headers]
});

debug_struct_redacted!(S3ImportTask {
    fields: [bucket, region, endpoint, key, key_prefix, filename],
    redacted: [access_key_id, secret_access_key],
    redacted_options: [session_token],
    redacted_maps: []
});

debug_struct_redacted!(AzureBlobImportTask {
    fields: [storage_account, container, blob, blob_prefix, filename],
    redacted: [],
    redacted_options: [storage_access_key, sas_token],
    redacted_maps: []
});

debug_struct_redacted!(GoogleCloudStorageImportTask {
    fields: [
        project_id,
        bucket,
        client_email,
        file,
        file_prefix,
        filename
    ],
    redacted: [private_key],
    redacted_options: [],
    redacted_maps: []
});

debug_struct_redacted!(OpenStackImportTask {
    fields: [
        auth_url,
        username,
        region,
        container,
        file,
        file_prefix,
        filename
    ],
    redacted: [password],
    redacted_options: [],
    redacted_maps: []
});

debug_struct_redacted!(SftpImportTask {
    fields: [host, port, username, file, path, filename],
    redacted: [],
    redacted_options: [password, private_key],
    redacted_maps: []
});

debug_struct_redacted!(S3ExportTask {
    fields: [input, bucket, region, endpoint, key, key_prefix, extra],
    redacted: [access_key_id, secret_access_key],
    redacted_options: [session_token],
    redacted_maps: []
});

debug_struct_redacted!(AzureBlobExportTask {
    fields: [input, storage_account, container, blob, blob_prefix, extra],
    redacted: [],
    redacted_options: [storage_access_key, sas_token],
    redacted_maps: []
});

debug_struct_redacted!(GoogleCloudStorageExportTask {
    fields: [
        input,
        project_id,
        bucket,
        client_email,
        file,
        file_prefix,
        extra
    ],
    redacted: [private_key],
    redacted_options: [],
    redacted_maps: []
});

debug_struct_redacted!(OpenStackExportTask {
    fields: [
        input,
        auth_url,
        username,
        region,
        container,
        file,
        file_prefix,
        extra
    ],
    redacted: [password],
    redacted_options: [],
    redacted_maps: []
});

debug_struct_redacted!(SftpExportTask {
    fields: [input, host, port, username, file, path, extra],
    redacted: [],
    redacted_options: [password, private_key],
    redacted_maps: []
});

debug_struct_redacted!(ExportUploadTask {
    fields: [input, url, extra],
    redacted: [],
    redacted_options: [],
    redacted_maps: [headers]
});
