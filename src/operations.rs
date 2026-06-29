//! CloudConvert operations metadata and local task validation helpers.
//!
//! [`Operation`] records describe supported engines, formats, and documented
//! options returned by `GET /v2/operations`. [`Operation::validate_task`]
//! checks a [`TaskRequest`] against that metadata before sending it to the API.

use std::collections::BTreeMap;

use std::{error::Error as StdError, fmt};

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

use crate::file_extension::normalize_file_extension;
use crate::tasks::TaskRequest;

/// Query parameters for `GET /v2/operations`.
#[derive(Clone, Debug, Default, Serialize)]
pub struct OperationListQuery {
    #[serde(rename = "filter[operation]", skip_serializing_if = "Option::is_none")]
    operation: Option<String>,
    #[serde(
        rename = "filter[input_format]",
        skip_serializing_if = "Option::is_none"
    )]
    input_format: Option<String>,
    #[serde(
        rename = "filter[output_format]",
        skip_serializing_if = "Option::is_none"
    )]
    output_format: Option<String>,
    #[serde(rename = "filter[engine]", skip_serializing_if = "Option::is_none")]
    engine: Option<String>,
    #[serde(
        rename = "filter[engine_version]",
        skip_serializing_if = "Option::is_none"
    )]
    engine_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    include: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    alternatives: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    page: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    per_page: Option<u32>,
}

impl OperationListQuery {
    pub fn operation(mut self, operation: impl Into<String>) -> Self {
        self.operation = Some(operation.into());
        self
    }

    pub fn input_format(mut self, input_format: impl Into<String>) -> Self {
        self.input_format = Some(normalize_file_extension(input_format));
        self
    }

    pub fn output_format(mut self, output_format: impl Into<String>) -> Self {
        self.output_format = Some(normalize_file_extension(output_format));
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

    pub fn include(mut self, include: impl Into<String>) -> Self {
        self.include = Some(include.into());
        self
    }

    pub fn include_options(self) -> Self {
        self.include("options")
    }

    pub fn include_engine_versions(self) -> Self {
        self.include("engine_versions")
    }

    pub fn include_options_and_engine_versions(self) -> Self {
        self.include("options,engine_versions")
    }

    pub fn alternatives(mut self, alternatives: bool) -> Self {
        self.alternatives = Some(alternatives);
        self
    }

    pub fn page(mut self, page: u32) -> Self {
        self.page = Some(page);
        self
    }

    pub fn per_page(mut self, per_page: u32) -> Self {
        self.per_page = Some(per_page);
        self
    }
}

/// One supported CloudConvert operation with engines, options, and alternatives.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[non_exhaustive]
pub struct Operation {
    pub operation: String,
    #[serde(default)]
    pub input_format: Option<String>,
    #[serde(default)]
    pub output_format: Option<String>,
    #[serde(default)]
    pub engine: Option<String>,
    #[serde(default)]
    pub engine_version: Option<String>,
    #[serde(default)]
    pub credits: Option<u64>,
    #[serde(default, deserialize_with = "deserialize_options")]
    pub options: BTreeMap<String, OperationOption>,
    #[serde(default, deserialize_with = "deserialize_engine_versions")]
    pub engine_versions: Vec<OperationEngineVersion>,
    #[serde(default)]
    pub alternatives: Vec<Operation>,
    #[serde(default)]
    pub deprecated: Option<bool>,
    #[serde(default)]
    pub experimental: Option<bool>,
    #[serde(default)]
    pub meta: Option<BTreeMap<String, Value>>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl Operation {
    pub fn option(&self, name: &str) -> Option<&OperationOption> {
        self.options.get(name)
    }

    pub fn options(&self) -> impl Iterator<Item = (&str, &OperationOption)> {
        self.options
            .iter()
            .map(|(name, option)| (name.as_str(), option))
    }

    pub fn engine_version_values(&self) -> impl Iterator<Item = &str> {
        self.engine_versions
            .iter()
            .map(|version| version.version.as_str())
    }

    pub fn default_engine_version(&self) -> Option<&OperationEngineVersion> {
        self.engine_versions
            .iter()
            .find(|version| version.default.unwrap_or(false))
    }

    pub fn latest_engine_version(&self) -> Option<&OperationEngineVersion> {
        self.engine_versions
            .iter()
            .find(|version| version.latest.unwrap_or(false))
    }

    /// Validates a task against this operation record, ignoring unknown options.
    pub fn validate_task(&self, task: &TaskRequest) -> OperationValidationResult {
        self.validate_task_with_mode(task, OperationValidationMode::Lenient)
    }

    /// Validates a task and rejects options not documented for this operation.
    pub fn validate_task_strict(&self, task: &TaskRequest) -> OperationValidationResult {
        self.validate_task_with_mode(task, OperationValidationMode::Strict)
    }

    fn check_identity_field(
        &self,
        task: &TaskRequest,
        field: &str,
        expected: &Option<String>,
    ) -> OperationValidationResult {
        if let (Some(expected), Some(actual)) = (
            expected.as_deref(),
            task.payload().get(field).and_then(Value::as_str),
        ) {
            if expected != actual {
                return Err(OperationValidationError::format_mismatch(
                    &self.operation,
                    field,
                    expected,
                    actual,
                ));
            }
        }
        Ok(())
    }

    pub fn validate_task_with_mode(
        &self,
        task: &TaskRequest,
        mode: OperationValidationMode,
    ) -> OperationValidationResult {
        if task.operation() != self.operation {
            return Err(OperationValidationError::operation_mismatch(
                &self.operation,
                task.operation(),
            ));
        }

        self.check_identity_field(task, "input_format", &self.input_format)?;
        self.check_identity_field(task, "output_format", &self.output_format)?;
        self.check_identity_field(task, "engine", &self.engine)?;
        self.check_identity_field(task, "engine_version", &self.engine_version)?;

        for (name, option) in &self.options {
            let value = task.payload().get(name);
            if option.required.unwrap_or(false) && value.is_none_or(Value::is_null) {
                return Err(OperationValidationError::missing_required(
                    &self.operation,
                    name,
                ));
            }

            let Some(value) = value else {
                continue;
            };
            option.validate_value_for_operation(&self.operation, name, value)?;
        }

        if matches!(mode, OperationValidationMode::Strict) && !self.options.is_empty() {
            for name in task.payload().keys() {
                if !self.options.contains_key(name)
                    && !is_structural_task_field(&self.operation, name)
                {
                    return Err(OperationValidationError::unknown_option(
                        &self.operation,
                        name,
                    ));
                }
            }
        }

        Ok(())
    }
}

/// Documented option schema for a CloudConvert operation.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[non_exhaustive]
pub struct OperationOption {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default, rename = "type")]
    pub kind: Option<OperationOptionKind>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub required: Option<bool>,
    #[serde(default)]
    pub default: Option<Value>,
    #[serde(default, alias = "values")]
    pub possible_values: Vec<Value>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl OperationOption {
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    pub fn kind(&self) -> Option<&OperationOptionKind> {
        self.kind.as_ref()
    }

    pub fn is_required(&self) -> bool {
        self.required.unwrap_or(false)
    }

    pub fn possible_values(&self) -> &[Value] {
        &self.possible_values
    }

    pub fn validate_value(&self, value: &Value) -> OperationValidationResult {
        self.validate_value_for_operation("", self.name.as_deref().unwrap_or("option"), value)
    }

    fn validate_value_for_operation(
        &self,
        operation: &str,
        name: &str,
        value: &Value,
    ) -> OperationValidationResult {
        if let Some(kind) = &self.kind
            && !kind.matches_value(value)
        {
            return Err(OperationValidationError::invalid_type(
                operation,
                name,
                kind.as_str(),
                value_type(value),
            ));
        }

        if !self.possible_values.is_empty() {
            let allowed = match (&self.kind, value) {
                (Some(OperationOptionKind::Array), Value::Array(items)) => items
                    .iter()
                    .all(|item| self.possible_values.iter().any(|allowed| allowed == item)),
                _ => self.possible_values.iter().any(|allowed| allowed == value),
            };
            if !allowed {
                return Err(OperationValidationError::invalid_value(
                    operation,
                    name,
                    "one of the documented possible_values",
                    value,
                ));
            }
        }

        Ok(())
    }
}

/// Declared value type for an operation option in metadata responses.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum OperationOptionKind {
    String,
    Boolean,
    Integer,
    Float,
    Enum,
    Dictionary,
    Array,
    Other(String),
}

impl OperationOptionKind {
    pub fn as_str(&self) -> &str {
        match self {
            Self::String => "string",
            Self::Boolean => "boolean",
            Self::Integer => "integer",
            Self::Float => "float",
            Self::Enum => "enum",
            Self::Dictionary => "dictionary",
            Self::Array => "array",
            Self::Other(value) => value.as_str(),
        }
    }

    fn matches_value(&self, value: &Value) -> bool {
        match self {
            Self::String => value.is_string(),
            Self::Boolean => value.is_boolean(),
            Self::Integer => value
                .as_i64()
                .or_else(|| value.as_u64().and_then(|value| i64::try_from(value).ok()))
                .is_some(),
            Self::Float => value.is_number(),
            Self::Enum => value.is_string(),
            Self::Dictionary => value.is_object(),
            Self::Array => value.is_array(),
            Self::Other(_) => true,
        }
    }
}

impl Serialize for OperationOptionKind {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for OperationOptionKind {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Ok(match value.as_str() {
            "string" => Self::String,
            "boolean" => Self::Boolean,
            "integer" => Self::Integer,
            "float" => Self::Float,
            "enum" => Self::Enum,
            "dictionary" => Self::Dictionary,
            "array" => Self::Array,
            _ => Self::Other(value),
        })
    }
}

/// Engine version entry attached to an [`Operation`] metadata record.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[non_exhaustive]
pub struct OperationEngineVersion {
    pub version: String,
    #[serde(default)]
    pub default: Option<bool>,
    #[serde(default)]
    pub latest: Option<bool>,
    #[serde(default)]
    pub deprecated: Option<bool>,
    #[serde(default)]
    pub experimental: Option<bool>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// Controls whether undocumented task options are accepted during validation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum OperationValidationMode {
    Lenient,
    Strict,
}

/// Result of validating a [`TaskRequest`] against an [`Operation`] record.
pub type OperationValidationResult = std::result::Result<(), OperationValidationError>;

/// Validation failure describing which operation field or option was rejected.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperationValidationError {
    pub kind: OperationValidationErrorKind,
    pub operation: String,
    pub option: Option<String>,
    pub expected: Option<String>,
    pub actual: Option<String>,
}

impl OperationValidationError {
    fn operation_mismatch(expected: &str, actual: &str) -> Self {
        Self {
            kind: OperationValidationErrorKind::OperationMismatch,
            operation: expected.to_string(),
            option: None,
            expected: Some(expected.to_string()),
            actual: Some(actual.to_string()),
        }
    }

    fn format_mismatch(operation: &str, field: &str, expected: &str, actual: &str) -> Self {
        Self {
            kind: OperationValidationErrorKind::FormatMismatch,
            operation: operation.to_string(),
            option: Some(field.to_string()),
            expected: Some(expected.to_string()),
            actual: Some(actual.to_string()),
        }
    }

    fn missing_required(operation: &str, option: &str) -> Self {
        Self {
            kind: OperationValidationErrorKind::MissingRequiredOption,
            operation: operation.to_string(),
            option: Some(option.to_string()),
            expected: Some("present value".to_string()),
            actual: Some("missing".to_string()),
        }
    }

    fn invalid_type(operation: &str, option: &str, expected: &str, actual: &str) -> Self {
        Self {
            kind: OperationValidationErrorKind::InvalidOptionType,
            operation: operation.to_string(),
            option: Some(option.to_string()),
            expected: Some(expected.to_string()),
            actual: Some(actual.to_string()),
        }
    }

    fn invalid_value(operation: &str, option: &str, expected: &str, actual: &Value) -> Self {
        Self {
            kind: OperationValidationErrorKind::InvalidOptionValue,
            operation: operation.to_string(),
            option: Some(option.to_string()),
            expected: Some(expected.to_string()),
            actual: Some(actual.to_string()),
        }
    }

    fn unknown_option(operation: &str, option: &str) -> Self {
        Self {
            kind: OperationValidationErrorKind::UnknownOption,
            operation: operation.to_string(),
            option: Some(option.to_string()),
            expected: Some("documented operation option".to_string()),
            actual: Some("unknown option".to_string()),
        }
    }
}

impl fmt::Display for OperationValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.option {
            Some(option) => write!(
                formatter,
                "operation {} option {} failed validation: expected {}, got {}",
                self.operation,
                option,
                self.expected.as_deref().unwrap_or("valid value"),
                self.actual.as_deref().unwrap_or("invalid value")
            ),
            None => write!(
                formatter,
                "operation {} failed validation: expected {}, got {}",
                self.operation,
                self.expected.as_deref().unwrap_or("valid value"),
                self.actual.as_deref().unwrap_or("invalid value")
            ),
        }
    }
}

impl StdError for OperationValidationError {}

/// Specific validation failure category for an operation or option check.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum OperationValidationErrorKind {
    OperationMismatch,
    FormatMismatch,
    MissingRequiredOption,
    InvalidOptionType,
    InvalidOptionValue,
    UnknownOption,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum OperationOptionsWire {
    Map(BTreeMap<String, OperationOption>),
    List(Vec<OperationOption>),
}

fn deserialize_options<'de, D>(
    deserializer: D,
) -> std::result::Result<BTreeMap<String, OperationOption>, D::Error>
where
    D: Deserializer<'de>,
{
    let wire = Option::<OperationOptionsWire>::deserialize(deserializer)?;
    let mut options = BTreeMap::new();

    match wire {
        Some(OperationOptionsWire::Map(map)) => {
            for (name, mut option) in map {
                if option.name.is_none() {
                    option.name = Some(name.clone());
                }
                options.insert(name, option);
            }
        }
        Some(OperationOptionsWire::List(list)) => {
            for option in list {
                if let Some(name) = option.name.clone() {
                    options.insert(name, option);
                }
            }
        }
        None => {}
    }

    Ok(options)
}

#[derive(Deserialize)]
#[serde(untagged)]
enum OperationEngineVersionWire {
    String(String),
    Object(OperationEngineVersion),
}

fn deserialize_engine_versions<'de, D>(
    deserializer: D,
) -> std::result::Result<Vec<OperationEngineVersion>, D::Error>
where
    D: Deserializer<'de>,
{
    let wire = Option::<Vec<OperationEngineVersionWire>>::deserialize(deserializer)?;
    Ok(wire
        .unwrap_or_default()
        .into_iter()
        .map(|version| match version {
            OperationEngineVersionWire::String(version) => OperationEngineVersion {
                version,
                default: None,
                latest: None,
                deprecated: None,
                experimental: None,
                extra: BTreeMap::new(),
            },
            OperationEngineVersionWire::Object(version) => version,
        })
        .collect())
}

fn is_structural_task_field(operation: &str, name: &str) -> bool {
    if matches!(name, "ignore_error") {
        return true;
    }

    match operation {
        "import/url" => matches!(name, "url" | "filename" | "headers"),
        "import/upload" => matches!(name, "redirect"),
        "import/base64" | "import/raw" => matches!(name, "file" | "filename"),
        "import/s3" => matches!(
            name,
            "bucket"
                | "region"
                | "endpoint"
                | "key"
                | "key_prefix"
                | "access_key_id"
                | "secret_access_key"
                | "session_token"
                | "filename"
        ),
        "import/azure/blob" => matches!(
            name,
            "storage_account"
                | "storage_access_key"
                | "sas_token"
                | "container"
                | "blob"
                | "blob_prefix"
                | "filename"
        ),
        "import/google-cloud-storage" => matches!(
            name,
            "project_id"
                | "bucket"
                | "client_email"
                | "private_key"
                | "file"
                | "file_prefix"
                | "filename"
        ),
        "import/openstack" => matches!(
            name,
            "auth_url"
                | "username"
                | "password"
                | "region"
                | "container"
                | "file"
                | "file_prefix"
                | "filename"
        ),
        "import/sftp" => matches!(
            name,
            "host"
                | "port"
                | "username"
                | "password"
                | "private_key"
                | "file"
                | "path"
                | "filename"
        ),
        "convert" => is_file_processing_field(name),
        "optimize" | "metadata" => matches!(
            name,
            "input" | "input_format" | "engine" | "engine_version" | "filename" | "timeout"
        ),
        "watermark" => matches!(
            name,
            "input" | "input_format" | "engine" | "engine_version" | "filename" | "timeout"
        ),
        "capture-website" => matches!(
            name,
            "url" | "output_format" | "engine" | "engine_version" | "filename" | "timeout"
        ),
        "thumbnail" => is_file_processing_field(name),
        "metadata/write" => matches!(
            name,
            "input"
                | "input_format"
                | "engine"
                | "engine_version"
                | "metadata"
                | "filename"
                | "timeout"
        ),
        "merge" | "archive" => matches!(
            name,
            "input" | "output_format" | "engine" | "engine_version" | "filename" | "timeout"
        ),
        "command" => matches!(
            name,
            "input"
                | "engine"
                | "command"
                | "arguments"
                | "engine_version"
                | "capture_output"
                | "timeout"
        ),
        "pdf/a" | "pdf/x" | "pdf/ocr" | "pdf/encrypt" | "pdf/decrypt" | "pdf/split-pages"
        | "pdf/extract-pages" | "pdf/rotate-pages" => matches!(
            name,
            "input" | "engine" | "engine_version" | "filename" | "timeout"
        ),
        "export/url" => matches!(name, "input" | "inline" | "archive_multiple_files"),
        "export/s3" => matches!(
            name,
            "input"
                | "bucket"
                | "region"
                | "endpoint"
                | "key"
                | "key_prefix"
                | "access_key_id"
                | "secret_access_key"
                | "session_token"
        ),
        "export/azure/blob" => matches!(
            name,
            "input"
                | "storage_account"
                | "storage_access_key"
                | "sas_token"
                | "container"
                | "blob"
                | "blob_prefix"
        ),
        "export/google-cloud-storage" => matches!(
            name,
            "input"
                | "project_id"
                | "bucket"
                | "client_email"
                | "private_key"
                | "file"
                | "file_prefix"
        ),
        "export/openstack" => matches!(
            name,
            "input"
                | "auth_url"
                | "username"
                | "password"
                | "region"
                | "container"
                | "file"
                | "file_prefix"
        ),
        "export/sftp" => matches!(
            name,
            "input" | "host" | "port" | "username" | "password" | "private_key" | "file" | "path"
        ),
        "export/upload" => matches!(name, "input" | "url" | "headers"),
        _ => matches!(
            name,
            "input"
                | "input_format"
                | "output_format"
                | "engine"
                | "engine_version"
                | "filename"
                | "timeout"
        ),
    }
}

fn is_file_processing_field(name: &str) -> bool {
    matches!(
        name,
        "input"
            | "input_format"
            | "output_format"
            | "engine"
            | "engine_version"
            | "filename"
            | "timeout"
    )
}

fn value_type(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(number) if number.is_i64() || number.is_u64() => "integer",
        Value::Number(_) => "float",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "dictionary",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structural_task_fields_are_operation_specific() {
        const FILE_PROCESSING_FIELDS: &[&str] = &[
            "input",
            "input_format",
            "output_format",
            "engine",
            "engine_version",
            "filename",
            "timeout",
        ];
        const INPUT_PROCESSING_FIELDS: &[&str] = &[
            "input",
            "input_format",
            "engine",
            "engine_version",
            "filename",
            "timeout",
        ];
        const OUTPUT_PROCESSING_FIELDS: &[&str] = &[
            "input",
            "output_format",
            "engine",
            "engine_version",
            "filename",
            "timeout",
        ];
        const PDF_FIELDS: &[&str] = &["input", "engine", "engine_version", "filename", "timeout"];

        let cases: &[(&str, &[&str])] = &[
            ("import/url", &["url", "filename", "headers"]),
            ("import/upload", &["redirect"]),
            ("import/base64", &["file", "filename"]),
            ("import/raw", &["file", "filename"]),
            (
                "import/s3",
                &[
                    "bucket",
                    "region",
                    "endpoint",
                    "key",
                    "key_prefix",
                    "access_key_id",
                    "secret_access_key",
                    "session_token",
                    "filename",
                ],
            ),
            (
                "import/azure/blob",
                &[
                    "storage_account",
                    "storage_access_key",
                    "sas_token",
                    "container",
                    "blob",
                    "blob_prefix",
                    "filename",
                ],
            ),
            (
                "import/google-cloud-storage",
                &[
                    "project_id",
                    "bucket",
                    "client_email",
                    "private_key",
                    "file",
                    "file_prefix",
                    "filename",
                ],
            ),
            (
                "import/openstack",
                &[
                    "auth_url",
                    "username",
                    "password",
                    "region",
                    "container",
                    "file",
                    "file_prefix",
                    "filename",
                ],
            ),
            (
                "import/sftp",
                &[
                    "host",
                    "port",
                    "username",
                    "password",
                    "private_key",
                    "file",
                    "path",
                    "filename",
                ],
            ),
            ("convert", FILE_PROCESSING_FIELDS),
            ("optimize", INPUT_PROCESSING_FIELDS),
            ("metadata", INPUT_PROCESSING_FIELDS),
            ("watermark", INPUT_PROCESSING_FIELDS),
            (
                "capture-website",
                &[
                    "url",
                    "output_format",
                    "engine",
                    "engine_version",
                    "filename",
                    "timeout",
                ],
            ),
            ("thumbnail", FILE_PROCESSING_FIELDS),
            (
                "metadata/write",
                &[
                    "input",
                    "input_format",
                    "engine",
                    "engine_version",
                    "metadata",
                    "filename",
                    "timeout",
                ],
            ),
            ("merge", OUTPUT_PROCESSING_FIELDS),
            ("archive", OUTPUT_PROCESSING_FIELDS),
            (
                "command",
                &[
                    "input",
                    "engine",
                    "command",
                    "arguments",
                    "engine_version",
                    "capture_output",
                    "timeout",
                ],
            ),
            ("pdf/a", PDF_FIELDS),
            ("pdf/x", PDF_FIELDS),
            ("pdf/ocr", PDF_FIELDS),
            ("pdf/encrypt", PDF_FIELDS),
            ("pdf/decrypt", PDF_FIELDS),
            ("pdf/split-pages", PDF_FIELDS),
            ("pdf/extract-pages", PDF_FIELDS),
            ("pdf/rotate-pages", PDF_FIELDS),
            ("export/url", &["input", "inline", "archive_multiple_files"]),
            (
                "export/s3",
                &[
                    "input",
                    "bucket",
                    "region",
                    "endpoint",
                    "key",
                    "key_prefix",
                    "access_key_id",
                    "secret_access_key",
                    "session_token",
                ],
            ),
            (
                "export/azure/blob",
                &[
                    "input",
                    "storage_account",
                    "storage_access_key",
                    "sas_token",
                    "container",
                    "blob",
                    "blob_prefix",
                ],
            ),
            (
                "export/google-cloud-storage",
                &[
                    "input",
                    "project_id",
                    "bucket",
                    "client_email",
                    "private_key",
                    "file",
                    "file_prefix",
                ],
            ),
            (
                "export/openstack",
                &[
                    "input",
                    "auth_url",
                    "username",
                    "password",
                    "region",
                    "container",
                    "file",
                    "file_prefix",
                ],
            ),
            (
                "export/sftp",
                &[
                    "input",
                    "host",
                    "port",
                    "username",
                    "password",
                    "private_key",
                    "file",
                    "path",
                ],
            ),
            ("export/upload", &["input", "url", "headers"]),
            ("future/custom-op", FILE_PROCESSING_FIELDS),
        ];

        for &(operation, fields) in cases {
            assert!(is_structural_task_field(operation, "ignore_error"));
            for &field in fields {
                assert!(
                    is_structural_task_field(operation, field),
                    "{operation} should accept structural field {field}"
                );
            }
        }

        for &(operation, field) in &[
            ("convert", "url"),
            ("convert", "bucket"),
            ("import/url", "bucket"),
            ("export/s3", "url"),
            ("future/custom-op", "url"),
            ("future/custom-op", "not_a_real_field"),
        ] {
            assert!(
                !is_structural_task_field(operation, field),
                "{operation} should reject unrelated field {field}"
            );
        }
    }
}
