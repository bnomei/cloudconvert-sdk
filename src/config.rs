use std::{env, fmt, sync::Arc, time::Duration};

use secrecy::{ExposeSecret, SecretString};
use url::Url;

use crate::{Error, Result};

const API_BASE: &str = "https://api.cloudconvert.com/v2/";
const SANDBOX_API_BASE: &str = "https://api.sandbox.cloudconvert.com/v2/";
const SYNC_API_BASE: &str = "https://sync.api.cloudconvert.com/v2/";
const SANDBOX_SYNC_API_BASE: &str = "https://sync.api.sandbox.cloudconvert.com/v2/";

#[derive(Clone)]
pub struct ApiKey(Arc<SecretString>);

impl ApiKey {
    pub fn new(value: impl Into<String>) -> Self {
        Self(Arc::new(SecretString::from(value.into())))
    }

    pub(crate) fn expose(&self) -> &str {
        self.0.expose_secret()
    }

    pub fn from_env() -> Result<Self> {
        env::var("CLOUDCONVERT_API_KEY")
            .map(Self::new)
            .map_err(|_| Error::MissingEnv("CLOUDCONVERT_API_KEY"))
    }
}

impl fmt::Debug for ApiKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ApiKey(REDACTED)")
    }
}

#[derive(Clone)]
pub struct SigningSecret(Arc<SecretString>);

impl SigningSecret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(Arc::new(SecretString::from(value.into())))
    }

    pub(crate) fn expose(&self) -> &str {
        self.0.expose_secret()
    }
}

impl fmt::Debug for SigningSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SigningSecret(REDACTED)")
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Region {
    EuCentral,
    UsEast,
    Custom(String),
}

impl Region {
    fn prefix(&self) -> Result<&str> {
        match self {
            Self::EuCentral => Ok("eu-central"),
            Self::UsEast => Ok("us-east"),
            Self::Custom(region) => validate_region_prefix(region),
        }
    }
}

#[derive(Clone)]
pub struct CloudConvertConfig {
    pub(crate) api_key: ApiKey,
    pub(crate) api_base_url: Url,
    pub(crate) sync_base_url: Url,
    pub(crate) sandbox: bool,
    pub(crate) region: Option<Region>,
    #[cfg(feature = "retry")]
    pub(crate) retry_policy: Option<RetryPolicy>,
}

impl CloudConvertConfig {
    pub fn api_base_url(&self) -> &Url {
        &self.api_base_url
    }

    pub fn sync_base_url(&self) -> &Url {
        &self.sync_base_url
    }

    pub fn sandbox(&self) -> bool {
        self.sandbox
    }

    pub fn region(&self) -> Option<&Region> {
        self.region.as_ref()
    }
}

impl fmt::Debug for CloudConvertConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut debug = f.debug_struct("CloudConvertConfig");
        debug
            .field("api_key", &self.api_key)
            .field("api_base_url", &self.api_base_url)
            .field("sync_base_url", &self.sync_base_url)
            .field("sandbox", &self.sandbox)
            .field("region", &self.region);
        #[cfg(feature = "retry")]
        debug.field("retry_policy", &self.retry_policy);
        debug.finish()
    }
}

#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct TransportConfig {
    request_timeout: Option<Duration>,
    connect_timeout: Option<Duration>,
    pool_idle_timeout: Option<Duration>,
    user_agent: Option<String>,
}

impl TransportConfig {
    pub fn request_timeout_value(&self) -> Option<Duration> {
        self.request_timeout
    }

    pub fn request_timeout(mut self, request_timeout: Duration) -> Self {
        self.request_timeout = Some(request_timeout);
        self
    }

    pub fn connect_timeout_value(&self) -> Option<Duration> {
        self.connect_timeout
    }

    pub fn connect_timeout(mut self, connect_timeout: Duration) -> Self {
        self.connect_timeout = Some(connect_timeout);
        self
    }

    pub fn pool_idle_timeout_value(&self) -> Option<Duration> {
        self.pool_idle_timeout
    }

    pub fn pool_idle_timeout(mut self, pool_idle_timeout: Duration) -> Self {
        self.pool_idle_timeout = Some(pool_idle_timeout);
        self
    }

    pub fn user_agent_value(&self) -> Option<&str> {
        self.user_agent.as_deref()
    }

    pub fn user_agent(mut self, user_agent: impl Into<String>) -> Self {
        self.user_agent = Some(user_agent.into());
        self
    }
}

#[cfg(feature = "retry")]
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct RetryPolicy {
    max_attempts: u32,
    initial_delay: Duration,
    max_delay: Duration,
    backoff_factor: f64,
    respect_retry_after: bool,
}

#[cfg(feature = "retry")]
impl RetryPolicy {
    pub fn new(max_attempts: u32) -> Self {
        Self {
            max_attempts: max_attempts.max(1),
            ..Self::default()
        }
    }

    pub fn max_attempts_value(&self) -> u32 {
        self.max_attempts
    }

    pub fn max_attempts(mut self, max_attempts: u32) -> Self {
        self.max_attempts = max_attempts.max(1);
        self
    }

    pub fn initial_delay_value(&self) -> Duration {
        self.initial_delay
    }

    pub fn initial_delay(mut self, initial_delay: Duration) -> Self {
        self.initial_delay = initial_delay;
        self
    }

    pub fn max_delay_value(&self) -> Duration {
        self.max_delay
    }

    pub fn max_delay(mut self, max_delay: Duration) -> Self {
        self.max_delay = max_delay;
        self
    }

    pub fn backoff_factor_value(&self) -> f64 {
        self.backoff_factor
    }

    pub fn backoff_factor(mut self, backoff_factor: f64) -> Self {
        self.backoff_factor = backoff_factor.max(1.0);
        self
    }

    pub fn respect_retry_after_value(&self) -> bool {
        self.respect_retry_after
    }

    pub fn respect_retry_after(mut self, respect_retry_after: bool) -> Self {
        self.respect_retry_after = respect_retry_after;
        self
    }
}

#[cfg(feature = "retry")]
impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_delay: Duration::from_millis(250),
            max_delay: Duration::from_secs(10),
            backoff_factor: 2.0,
            respect_retry_after: true,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ClientBuilder {
    api_key: ApiKey,
    sandbox: bool,
    region: Option<Region>,
    api_base_url: Option<Url>,
    sync_base_url: Option<Url>,
    http_client: Option<reqwest::Client>,
    redirectless_http_client: Option<reqwest::Client>,
    transport_config: Option<TransportConfig>,
    #[cfg(feature = "retry")]
    retry_policy: Option<RetryPolicy>,
}

impl ClientBuilder {
    pub fn new(api_key: ApiKey) -> Self {
        Self {
            api_key,
            sandbox: false,
            region: None,
            api_base_url: None,
            sync_base_url: None,
            http_client: None,
            redirectless_http_client: None,
            transport_config: None,
            #[cfg(feature = "retry")]
            retry_policy: None,
        }
    }

    pub fn sandbox(mut self, sandbox: bool) -> Self {
        self.sandbox = sandbox;
        self
    }

    pub fn region(mut self, region: Region) -> Self {
        self.region = Some(region);
        self
    }

    pub fn with_base_urls(mut self, api_base_url: Url, sync_base_url: Url) -> Self {
        self.api_base_url = Some(api_base_url);
        self.sync_base_url = Some(sync_base_url);
        self
    }

    pub fn http_client(mut self, http_client: reqwest::Client) -> Self {
        self.http_client = Some(http_client);
        self
    }

    pub fn http_clients(
        mut self,
        http_client: reqwest::Client,
        redirectless_http_client: reqwest::Client,
    ) -> Self {
        self.http_client = Some(http_client);
        self.redirectless_http_client = Some(redirectless_http_client);
        self
    }

    pub fn transport_config(mut self, transport_config: TransportConfig) -> Self {
        self.transport_config = Some(transport_config);
        self
    }

    #[cfg(feature = "retry")]
    pub fn retry_policy(mut self, retry_policy: RetryPolicy) -> Self {
        self.retry_policy = Some(retry_policy);
        self
    }

    pub fn build(self) -> Result<crate::CloudConvertClient> {
        let api_base_url = match self.api_base_url {
            Some(url) => url,
            None => default_api_url(self.sandbox, self.region.as_ref())?,
        };
        let sync_base_url = match self.sync_base_url {
            Some(url) => url,
            None => default_sync_url(self.sandbox, self.region.as_ref())?,
        };

        let config = CloudConvertConfig {
            api_key: self.api_key,
            api_base_url,
            sync_base_url,
            sandbox: self.sandbox,
            region: self.region,
            #[cfg(feature = "retry")]
            retry_policy: self.retry_policy,
        };

        let (http_client, redirectless_http_client) = match (
            self.http_client,
            self.redirectless_http_client,
            self.transport_config,
        ) {
            (Some(http_client), Some(redirectless_http_client), _) => {
                (http_client, redirectless_http_client)
            }
            (Some(http_client), None, _) => (http_client, redirectless_client(None)?),
            (None, Some(redirectless_http_client), transport_config) => (
                http_client(transport_config.as_ref())?,
                redirectless_http_client,
            ),
            (None, None, transport_config) => (
                http_client(transport_config.as_ref())?,
                redirectless_client(transport_config.as_ref())?,
            ),
        };

        crate::CloudConvertClient::from_parts(config, http_client, redirectless_http_client)
    }
}

fn http_client(transport_config: Option<&TransportConfig>) -> Result<reqwest::Client> {
    http_client_builder(transport_config)
        .build()
        .map_err(Error::Http)
}

fn redirectless_client(transport_config: Option<&TransportConfig>) -> Result<reqwest::Client> {
    http_client_builder(transport_config)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(Error::Http)
}

fn http_client_builder(transport_config: Option<&TransportConfig>) -> reqwest::ClientBuilder {
    let mut builder = reqwest::Client::builder();

    if let Some(transport_config) = transport_config {
        if let Some(timeout) = transport_config.request_timeout {
            builder = builder.timeout(timeout);
        }
        if let Some(timeout) = transport_config.connect_timeout {
            builder = builder.connect_timeout(timeout);
        }
        if let Some(timeout) = transport_config.pool_idle_timeout {
            builder = builder.pool_idle_timeout(timeout);
        }
        if let Some(user_agent) = &transport_config.user_agent {
            builder = builder.user_agent(user_agent);
        }
    }

    builder
}

fn default_api_url(sandbox: bool, region: Option<&Region>) -> Result<Url> {
    if sandbox {
        return Url::parse(SANDBOX_API_BASE).map_err(Error::Url);
    }

    match region {
        Some(region) => {
            let prefix = region.prefix()?;
            Url::parse(&format!("https://{prefix}.api.cloudconvert.com/v2/")).map_err(Error::Url)
        }
        None => Url::parse(API_BASE).map_err(Error::Url),
    }
}

fn default_sync_url(sandbox: bool, region: Option<&Region>) -> Result<Url> {
    if sandbox {
        return Url::parse(SANDBOX_SYNC_API_BASE).map_err(Error::Url);
    }

    match region {
        Some(region) => {
            let prefix = region.prefix()?;
            Url::parse(&format!("https://{prefix}.sync.api.cloudconvert.com/v2/"))
                .map_err(Error::Url)
        }
        None => Url::parse(SYNC_API_BASE).map_err(Error::Url),
    }
}

fn validate_region_prefix(prefix: &str) -> Result<&str> {
    let valid = !prefix.is_empty()
        && !prefix.starts_with('-')
        && !prefix.ends_with('-')
        && prefix
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-');

    if valid {
        Ok(prefix)
    } else {
        Err(Error::InvalidRegion)
    }
}

#[cfg(test)]
mod tests {
    use super::validate_region_prefix;

    #[test]
    fn validates_custom_region_prefixes() {
        assert_eq!(
            validate_region_prefix("ap-southeast").unwrap(),
            "ap-southeast"
        );
        assert!(validate_region_prefix("attacker.test").is_err());
        assert!(validate_region_prefix("attacker/../api").is_err());
        assert!(validate_region_prefix("-us-east").is_err());
        assert!(validate_region_prefix("us-east-").is_err());
    }
}
