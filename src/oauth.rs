//! CloudConvert OAuth authorization and token exchange helpers.
//!
//! [`OAuthClient`] builds authorization URLs and exchanges authorization codes
//! or refresh tokens for [`OAuthTokenResponse`] values that can seed a
//! [`CloudConvertClient`] through [`OAuthTokenResponse::client_builder`].

use std::{collections::BTreeMap, fmt};

use reqwest::header::CONTENT_TYPE;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use url::Url;

use crate::{
    ClientBuilder, CloudConvertClient, Error, Result,
    config::{OAuthAccessToken, OAuthClientSecret, OAuthRefreshToken},
};

const OAUTH_AUTHORIZE_URL: &str = "https://cloudconvert.com/oauth/authorize";
const OAUTH_TOKEN_URL: &str = "https://cloudconvert.com/oauth/token";

/// OAuth scope token requested during authorization.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum OAuthScope {
    UserRead,
    UserWrite,
    TaskRead,
    TaskWrite,
    WebhookRead,
    WebhookWrite,
    Other(String),
}

impl OAuthScope {
    pub fn as_str(&self) -> &str {
        match self {
            Self::UserRead => "user.read",
            Self::UserWrite => "user.write",
            Self::TaskRead => "task.read",
            Self::TaskWrite => "task.write",
            Self::WebhookRead => "webhook.read",
            Self::WebhookWrite => "webhook.write",
            Self::Other(value) => value.as_str(),
        }
    }
}

impl From<&str> for OAuthScope {
    fn from(value: &str) -> Self {
        match value {
            "user.read" => Self::UserRead,
            "user.write" => Self::UserWrite,
            "task.read" => Self::TaskRead,
            "task.write" => Self::TaskWrite,
            "webhook.read" => Self::WebhookRead,
            "webhook.write" => Self::WebhookWrite,
            _ => Self::Other(value.to_string()),
        }
    }
}

impl From<String> for OAuthScope {
    fn from(value: String) -> Self {
        Self::from(value.as_str())
    }
}

impl Serialize for OAuthScope {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

/// OAuth client used to start authorization flows and exchange tokens.
#[derive(Clone)]
pub struct OAuthClient {
    client_id: String,
    client_secret: OAuthClientSecret,
    authorize_url: Url,
    token_url: Url,
    http: reqwest::Client,
}

impl OAuthClient {
    pub fn new(client_id: impl Into<String>, client_secret: OAuthClientSecret) -> Result<Self> {
        Self::with_http_client(client_id, client_secret, reqwest::Client::new())
    }

    pub fn with_http_client(
        client_id: impl Into<String>,
        client_secret: OAuthClientSecret,
        http: reqwest::Client,
    ) -> Result<Self> {
        Ok(Self {
            client_id: client_id.into(),
            client_secret,
            authorize_url: Url::parse(OAUTH_AUTHORIZE_URL)?,
            token_url: Url::parse(OAUTH_TOKEN_URL)?,
            http,
        })
    }

    pub fn with_endpoints(mut self, authorize_url: Url, token_url: Url) -> Self {
        self.authorize_url = authorize_url;
        self.token_url = token_url;
        self
    }

    pub fn client_id(&self) -> &str {
        self.client_id.as_str()
    }

    pub fn authorize_url(&self) -> &Url {
        &self.authorize_url
    }

    pub fn token_url(&self) -> &Url {
        &self.token_url
    }

    /// Builds an authorization-code flow URL for the given redirect URI and scopes.
    pub fn authorization_code_url(
        &self,
        redirect_uri: impl AsRef<str>,
        scopes: impl IntoIterator<Item = OAuthScope>,
    ) -> Result<Url> {
        self.authorization_url("code", redirect_uri.as_ref(), scopes, None)
    }

    pub fn authorization_code_url_with_state(
        &self,
        redirect_uri: impl AsRef<str>,
        scopes: impl IntoIterator<Item = OAuthScope>,
        state: impl Into<String>,
    ) -> Result<Url> {
        self.authorization_url("code", redirect_uri.as_ref(), scopes, Some(state.into()))
    }

    pub fn implicit_url(
        &self,
        redirect_uri: impl AsRef<str>,
        scopes: impl IntoIterator<Item = OAuthScope>,
    ) -> Result<Url> {
        self.authorization_url("token", redirect_uri.as_ref(), scopes, None)
    }

    pub fn implicit_url_with_state(
        &self,
        redirect_uri: impl AsRef<str>,
        scopes: impl IntoIterator<Item = OAuthScope>,
        state: impl Into<String>,
    ) -> Result<Url> {
        self.authorization_url("token", redirect_uri.as_ref(), scopes, Some(state.into()))
    }

    /// Exchanges an authorization code for access and optional refresh tokens.
    pub async fn exchange_code(
        &self,
        code: impl Into<String>,
        redirect_uri: impl Into<String>,
    ) -> Result<OAuthTokenResponse> {
        self.send_token_request(vec![
            ("grant_type", "authorization_code".to_string()),
            ("code", code.into()),
            ("redirect_uri", redirect_uri.into()),
            ("client_id", self.client_id.clone()),
            ("client_secret", self.client_secret.expose().to_string()),
        ])
        .await
    }

    /// Refreshes an access token using a stored refresh token.
    pub async fn refresh_access_token(
        &self,
        refresh_token: &OAuthRefreshToken,
    ) -> Result<OAuthTokenResponse> {
        self.send_token_request(vec![
            ("grant_type", "refresh_token".to_string()),
            ("refresh_token", refresh_token.expose().to_string()),
            ("client_id", self.client_id.clone()),
            ("client_secret", self.client_secret.expose().to_string()),
        ])
        .await
    }

    fn authorization_url(
        &self,
        response_type: &str,
        redirect_uri: &str,
        scopes: impl IntoIterator<Item = OAuthScope>,
        state: Option<String>,
    ) -> Result<Url> {
        let mut url = self.authorize_url.clone();
        let scope = scopes
            .into_iter()
            .map(|scope| scope.as_str().to_string())
            .collect::<Vec<_>>()
            .join(" ");

        {
            let mut query = url.query_pairs_mut();
            query
                .append_pair("response_type", response_type)
                .append_pair("client_id", &self.client_id)
                .append_pair("redirect_uri", redirect_uri);
            if !scope.is_empty() {
                query.append_pair("scope", &scope);
            }
            if let Some(state) = state {
                query.append_pair("state", &state);
            }
        }

        Ok(url)
    }

    async fn send_token_request(
        &self,
        form: Vec<(&'static str, String)>,
    ) -> Result<OAuthTokenResponse> {
        let body = form_body(&form);
        let response = self
            .http
            .post(self.token_url.clone())
            .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(body)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(oauth_api_error(response).await);
        }

        let raw = response.json::<RawOAuthTokenResponse>().await?;
        Ok(raw.into())
    }
}

impl fmt::Debug for OAuthClient {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OAuthClient")
            .field("client_id", &self.client_id)
            .field("client_secret", &self.client_secret)
            .field("authorize_url", &self.authorize_url)
            .field("token_url", &self.token_url)
            .field("http", &"reqwest::Client")
            .finish()
    }
}

/// Token payload returned by CloudConvert OAuth token endpoints.
#[derive(Clone)]
#[non_exhaustive]
pub struct OAuthTokenResponse {
    pub access_token: OAuthAccessToken,
    pub refresh_token: Option<OAuthRefreshToken>,
    pub token_type: Option<String>,
    pub expires_in: Option<u64>,
    pub scope: Option<String>,
    pub extra: BTreeMap<String, Value>,
}

impl OAuthTokenResponse {
    pub fn access_token(&self) -> &OAuthAccessToken {
        &self.access_token
    }

    pub fn refresh_token(&self) -> Option<&OAuthRefreshToken> {
        self.refresh_token.as_ref()
    }

    pub fn into_access_token(self) -> OAuthAccessToken {
        self.access_token
    }

    /// Starts a [`CloudConvertClient`] builder authenticated with this access token.
    pub fn client_builder(&self) -> ClientBuilder {
        CloudConvertClient::builder_with_access_token(self.access_token.clone())
    }

    pub fn into_client_builder(self) -> ClientBuilder {
        CloudConvertClient::builder_with_access_token(self.access_token)
    }
}

impl fmt::Debug for OAuthTokenResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OAuthTokenResponse")
            .field("access_token", &self.access_token)
            .field("refresh_token", &self.refresh_token)
            .field("token_type", &self.token_type)
            .field("expires_in", &self.expires_in)
            .field("scope", &self.scope)
            .field("extra", &self.extra)
            .finish()
    }
}

#[derive(Deserialize)]
struct RawOAuthTokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    token_type: Option<String>,
    #[serde(default)]
    expires_in: Option<u64>,
    #[serde(default)]
    scope: Option<String>,
    #[serde(flatten)]
    extra: BTreeMap<String, Value>,
}

impl From<RawOAuthTokenResponse> for OAuthTokenResponse {
    fn from(value: RawOAuthTokenResponse) -> Self {
        Self {
            access_token: OAuthAccessToken::new(value.access_token),
            refresh_token: value.refresh_token.map(OAuthRefreshToken::new),
            token_type: value.token_type,
            expires_in: value.expires_in,
            scope: value.scope,
            extra: value.extra,
        }
    }
}

fn form_body(form: &[(&'static str, String)]) -> String {
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    for (key, value) in form {
        serializer.append_pair(key, value);
    }
    serializer.finish()
}

async fn oauth_api_error(response: reqwest::Response) -> Error {
    let status = response.status().as_u16();
    let body = response.text().await.unwrap_or_default();
    let parsed = serde_json::from_str::<Value>(&body).ok();
    let message = parsed
        .as_ref()
        .and_then(|body| {
            body.get("error_description")
                .or_else(|| body.get("message"))
                .and_then(Value::as_str)
        })
        .filter(|message| !message.is_empty())
        .unwrap_or("OAuth token request failed")
        .to_string();
    let code = parsed
        .as_ref()
        .and_then(|body| {
            body.get("error")
                .or_else(|| body.get("code"))
                .and_then(Value::as_str)
        })
        .map(ToString::to_string);

    Error::Api {
        status,
        message,
        code,
        errors: parsed.map(Box::new),
        rate_limit: None,
    }
}
