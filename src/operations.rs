use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::file_extension::normalize_file_extension;

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
    pub options: Option<BTreeMap<String, OperationOption>>,
    #[serde(default)]
    pub engine_versions: Vec<String>,
    #[serde(default)]
    pub alternatives: Vec<Operation>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[non_exhaustive]
pub struct OperationOption {
    #[serde(default, rename = "type")]
    pub kind: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub required: Option<bool>,
    #[serde(default)]
    pub default: Option<Value>,
    #[serde(default)]
    pub values: Vec<Value>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}
