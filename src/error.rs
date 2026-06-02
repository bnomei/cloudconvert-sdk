use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::jobs::RateLimit;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("CloudConvert API returned HTTP {status}: {message}")]
    Api {
        status: u16,
        message: String,
        code: Option<String>,
        errors: Option<Box<Value>>,
        rate_limit: Option<Box<RateLimit>>,
    },

    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("local IO failed: {0}")]
    Io(#[from] std::io::Error),

    #[error("URL parsing failed: {0}")]
    Url(#[from] url::ParseError),

    #[error("JSON serialization failed: {0}")]
    Json(#[from] serde_json::Error),

    #[error("missing required environment variable {0}")]
    MissingEnv(&'static str),

    #[error("task is not an import/upload task that is ready for upload")]
    UploadTaskNotReady,

    #[error("CloudConvert redirect response did not include a valid Location header")]
    MissingRedirectLocation,

    #[error("webhook signature is not valid hex")]
    InvalidSignatureHex,

    #[error("invalid CloudConvert API path")]
    InvalidApiPath,

    #[error("invalid CloudConvert resource id")]
    InvalidResourceId,

    #[error("invalid CloudConvert region")]
    InvalidRegion,

    #[cfg(feature = "socket")]
    #[error("Socket.IO operation failed: {0}")]
    Socket(String),
}

impl Error {
    pub fn api_error(&self) -> Option<ApiError<'_>> {
        match self {
            Self::Api {
                status,
                message,
                code,
                errors,
                rate_limit,
            } => Some(ApiError {
                status: *status,
                message,
                code: code.as_deref(),
                errors: errors.as_deref(),
                rate_limit: rate_limit.as_deref(),
                retry_after: rate_limit
                    .as_ref()
                    .and_then(|rate_limit| rate_limit.retry_after),
            }),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct ApiError<'a> {
    pub status: u16,
    pub message: &'a str,
    pub code: Option<&'a str>,
    pub errors: Option<&'a Value>,
    pub rate_limit: Option<&'a RateLimit>,
    pub retry_after: Option<u64>,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct ApiErrorBody {
    pub message: Option<String>,
    pub code: Option<String>,
    pub errors: Option<Value>,
}
