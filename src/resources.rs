//! Account and webhook resource models returned by CloudConvert user endpoints.
//!
//! [`User`] represents the authenticated account. Webhook types model event
//! subscriptions CloudConvert delivers to caller-controlled URLs.

use std::{collections::BTreeMap, fmt};

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

/// Authenticated CloudConvert account returned by `GET /v2/users/me`.
#[derive(Clone, Deserialize, Serialize)]
#[non_exhaustive]
pub struct User {
    pub id: String,
    pub username: String,
    pub email: String,
    pub credits: f64,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl fmt::Debug for User {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("User")
            .field("id", &self.id)
            .field("username", &self.username)
            .field("email", &"REDACTED")
            .field("credits", &self.credits)
            .field("created_at", &self.created_at)
            .field("extra", &self.extra)
            .finish()
    }
}

/// Event name CloudConvert can deliver to a registered webhook URL.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum WebhookEvent {
    JobCreated,
    JobFinished,
    JobFailed,
    TaskCreated,
    TaskFinished,
    TaskFailed,
    Other(String),
}

impl WebhookEvent {
    pub fn as_str(&self) -> &str {
        match self {
            Self::JobCreated => "job.created",
            Self::JobFinished => "job.finished",
            Self::JobFailed => "job.failed",
            Self::TaskCreated => "task.created",
            Self::TaskFinished => "task.finished",
            Self::TaskFailed => "task.failed",
            Self::Other(value) => value.as_str(),
        }
    }
}

impl Serialize for WebhookEvent {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for WebhookEvent {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Ok(match value.as_str() {
            "job.created" => Self::JobCreated,
            "job.finished" => Self::JobFinished,
            "job.failed" => Self::JobFailed,
            "task.created" => Self::TaskCreated,
            "task.finished" => Self::TaskFinished,
            "task.failed" => Self::TaskFailed,
            _ => Self::Other(value),
        })
    }
}

/// Request body for `POST /v2/webhooks`.
#[derive(Clone, Debug, Serialize)]
pub struct WebhookCreateRequest {
    url: String,
    events: Vec<WebhookEvent>,
}

impl WebhookCreateRequest {
    pub fn new(url: impl Into<String>, events: impl Into<Vec<WebhookEvent>>) -> Self {
        Self {
            url: url.into(),
            events: events.into(),
        }
    }

    pub fn url(&self) -> &str {
        self.url.as_str()
    }

    pub fn events(&self) -> &[WebhookEvent] {
        &self.events
    }
}

/// Query parameters for listing registered webhooks.
#[derive(Clone, Debug, Default, Serialize)]
pub struct WebhookListQuery {
    #[serde(rename = "filter[url]", skip_serializing_if = "Option::is_none")]
    filter_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    per_page: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    page: Option<u32>,
}

impl WebhookListQuery {
    pub fn url(mut self, url: impl Into<String>) -> Self {
        self.filter_url = Some(url.into());
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

/// Registered webhook subscription, including optional signing secret metadata.
#[derive(Clone, Deserialize, Serialize)]
#[non_exhaustive]
pub struct Webhook {
    pub id: String,
    pub url: String,
    #[serde(default)]
    pub disabled: Option<bool>,
    #[serde(default)]
    pub events: Vec<WebhookEvent>,
    #[serde(default)]
    pub failing: Option<bool>,
    #[serde(default, skip_serializing)]
    pub signing_secret: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
    #[serde(default)]
    pub links: BTreeMap<String, Value>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl fmt::Debug for Webhook {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Webhook")
            .field("id", &self.id)
            .field("url", &self.url)
            .field("disabled", &self.disabled)
            .field("events", &self.events)
            .field("failing", &self.failing)
            .field(
                "signing_secret",
                &self.signing_secret.as_ref().map(|_| "REDACTED"),
            )
            .field("created_at", &self.created_at)
            .field("updated_at", &self.updated_at)
            .field("links", &self.links)
            .field("extra", &self.extra)
            .finish()
    }
}
