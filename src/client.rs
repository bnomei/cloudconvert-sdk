#[cfg(feature = "retry")]
use std::time::Duration;
use std::{
    error::Error as StdError,
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use bytes::Bytes;
use futures_util::{Stream, StreamExt, TryStream};
use reqwest::{
    Body, Method, Response,
    header::{HeaderMap, LOCATION},
    multipart,
};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use tokio::io::AsyncWriteExt;
use url::Url;

use crate::{
    Result,
    config::{ApiKey, ClientBuilder, CloudConvertConfig, OAuthAccessToken},
    error::{ApiErrorBody, Error},
    jobs::{
        ApiResponse, DataEnvelope, Job, JobCreateRequest, JobGetQuery, JobListQuery, Page,
        RateLimit, Task, TaskGetQuery, TaskListQuery, UploadForm,
    },
    operations::{Operation, OperationListQuery},
    resources::{User, Webhook, WebhookCreateRequest, WebhookListQuery},
    socket::{SocketChannel, SocketSubscription},
    tasks::TaskRequest,
};

#[cfg(feature = "socket")]
use crate::socket::CloudConvertSocket;

#[derive(Clone, Debug)]
pub struct CloudConvertClient {
    config: Arc<CloudConvertConfig>,
    http: reqwest::Client,
    redirectless_http: reqwest::Client,
}

impl CloudConvertClient {
    pub fn builder(api_key: ApiKey) -> ClientBuilder {
        ClientBuilder::new(api_key)
    }

    pub fn builder_with_access_token(access_token: OAuthAccessToken) -> ClientBuilder {
        ClientBuilder::new_with_access_token(access_token)
    }

    pub(crate) fn from_parts(
        config: CloudConvertConfig,
        http: reqwest::Client,
        redirectless_http: reqwest::Client,
    ) -> Result<Self> {
        Ok(Self {
            config: Arc::new(config),
            http,
            redirectless_http,
        })
    }

    pub fn config(&self) -> &CloudConvertConfig {
        &self.config
    }

    pub fn jobs(&self) -> JobsResource {
        JobsResource {
            client: self.clone(),
        }
    }

    pub fn tasks(&self) -> TasksResource {
        TasksResource {
            client: self.clone(),
        }
    }

    pub fn users(&self) -> UsersResource {
        UsersResource {
            client: self.clone(),
        }
    }

    pub fn webhooks(&self) -> WebhooksResource {
        WebhooksResource {
            client: self.clone(),
        }
    }

    pub fn operations(&self) -> OperationsResource {
        OperationsResource {
            client: self.clone(),
        }
    }

    pub fn socket_base_url(&self) -> &'static str {
        crate::socket::socket_base_url(self.config.sandbox())
    }

    pub fn socket_subscription(&self, channel: SocketChannel) -> SocketSubscription {
        SocketSubscription::new(channel.name().into_owned(), self.config.credential.expose())
    }

    #[cfg(feature = "socket")]
    pub async fn socket(
        &self,
        channels: impl IntoIterator<Item = SocketChannel>,
    ) -> Result<CloudConvertSocket> {
        let subscriptions = channels
            .into_iter()
            .map(|channel| self.socket_subscription(channel));
        CloudConvertSocket::connect(self.socket_base_url(), subscriptions).await
    }

    #[cfg(feature = "socket")]
    pub async fn socket_with_buffer(
        &self,
        channels: impl IntoIterator<Item = SocketChannel>,
        buffer: usize,
    ) -> Result<CloudConvertSocket> {
        let subscriptions = channels
            .into_iter()
            .map(|channel| self.socket_subscription(channel));
        CloudConvertSocket::connect_with_buffer(self.socket_base_url(), subscriptions, buffer).await
    }

    pub async fn download(&self, url: impl AsRef<str>) -> Result<Bytes> {
        let response = self.http.get(url.as_ref()).send().await?;
        Self::ensure_success(response)
            .await?
            .bytes()
            .await
            .map_err(Error::Http)
    }

    pub async fn download_stream(
        &self,
        url: impl AsRef<str>,
    ) -> Result<impl Stream<Item = Result<Bytes>>> {
        let response = self.http.get(url.as_ref()).send().await?;
        Ok(Self::ensure_success(response)
            .await?
            .bytes_stream()
            .map(|chunk| chunk.map_err(Error::Http)))
    }

    pub async fn download_to_path(
        &self,
        url: impl AsRef<str>,
        destination: impl AsRef<Path>,
    ) -> Result<()> {
        let destination = destination.as_ref();
        let temp_path = temporary_download_path(destination);
        self.download_to_temporary_path(url.as_ref(), destination, temp_path)
            .await
    }

    pub async fn upload_bytes(
        &self,
        task: &Task,
        filename: impl Into<String>,
        bytes: impl Into<Bytes>,
    ) -> Result<()> {
        self.upload_part(
            task,
            "file",
            multipart::Part::stream(Body::from(bytes.into())).file_name(filename.into()),
        )
        .await
    }

    pub async fn upload_body(
        &self,
        task: &Task,
        filename: impl Into<String>,
        body: Body,
    ) -> Result<()> {
        self.upload_part(
            task,
            "file",
            multipart::Part::stream(body).file_name(filename.into()),
        )
        .await
    }

    pub async fn upload_stream<S, B, E>(
        &self,
        task: &Task,
        filename: impl Into<String>,
        stream: S,
    ) -> Result<()>
    where
        S: TryStream<Ok = B, Error = E> + Send + 'static,
        Bytes: From<B>,
        E: Into<Box<dyn StdError + Send + Sync>>,
    {
        self.upload_body(task, filename, Body::wrap_stream(stream))
            .await
    }

    pub async fn upload_path(&self, task: &Task, path: impl AsRef<Path>) -> Result<()> {
        self.upload_part(task, "file", multipart::Part::file(path).await?)
            .await
    }

    async fn download_to_temporary_path(
        &self,
        url: &str,
        destination: &Path,
        temp_path: PathBuf,
    ) -> Result<()> {
        let mut temp_file = TemporaryDownload::new(temp_path);
        let mut stream = self.download_stream(url).await?;
        let mut file = tokio::fs::File::create(temp_file.path()).await?;

        while let Some(chunk) = stream.next().await {
            file.write_all(&chunk?).await?;
        }

        file.flush().await?;
        drop(file);
        tokio::fs::rename(temp_file.path(), destination).await?;
        temp_file.commit();
        Ok(())
    }

    async fn upload_part(
        &self,
        task: &Task,
        field_name: &str,
        part: multipart::Part,
    ) -> Result<()> {
        let form = Self::upload_form(task)?;
        let mut multipart = multipart::Form::new();
        for (key, value) in &form.parameters {
            multipart = multipart.text(key.clone(), form_value(value));
        }
        multipart = multipart.part(field_name.to_string(), part);

        let response = self
            .http
            .post(&form.url)
            .multipart(multipart)
            .send()
            .await?;
        Self::ensure_success(response).await?;
        Ok(())
    }

    fn upload_form(task: &Task) -> Result<&UploadForm> {
        task.upload_form().ok_or(Error::UploadTaskNotReady)
    }

    async fn get_response<T>(&self, base: &Url, path: &str) -> Result<ApiResponse<T>>
    where
        T: DeserializeOwned,
    {
        let url = api_url(base, path)?;
        let response = self
            .send_api(|| {
                self.http
                    .request(Method::GET, url.clone())
                    .bearer_auth(self.config.credential.expose())
            })
            .await?;
        Self::decode_response(response).await
    }

    async fn get_with_query_response<T, Q>(
        &self,
        base: &Url,
        path: &str,
        query: &Q,
    ) -> Result<ApiResponse<T>>
    where
        T: DeserializeOwned,
        Q: Serialize + ?Sized,
    {
        let url = api_url(base, path)?;
        let response = self
            .send_api(|| {
                self.http
                    .request(Method::GET, url.clone())
                    .bearer_auth(self.config.credential.expose())
                    .query(query)
            })
            .await?;
        Self::decode_response(response).await
    }

    async fn post_response<T, B>(&self, base: &Url, path: &str, body: &B) -> Result<ApiResponse<T>>
    where
        T: DeserializeOwned,
        B: Serialize + ?Sized,
    {
        let url = api_url(base, path)?;
        let response = self
            .send_api(|| {
                self.http
                    .request(Method::POST, url.clone())
                    .bearer_auth(self.config.credential.expose())
                    .json(body)
            })
            .await?;
        Self::decode_response(response).await
    }

    async fn delete(&self, base: &Url, path: &str) -> Result<()> {
        let url = api_url(base, path)?;
        let response = self
            .send_api(|| {
                self.http
                    .request(Method::DELETE, url.clone())
                    .bearer_auth(self.config.credential.expose())
            })
            .await?;
        Self::ensure_success(response).await?;
        Ok(())
    }

    async fn get_redirect_location<Q>(&self, base: &Url, path: &str, query: &Q) -> Result<Url>
    where
        Q: Serialize + ?Sized,
    {
        let url = api_url(base, path)?;
        let response = self
            .send_api(|| {
                self.redirectless_http
                    .request(Method::GET, url.clone())
                    .bearer_auth(self.config.credential.expose())
                    .query(query)
            })
            .await?;
        Self::decode_redirect_location(response).await
    }

    async fn post_redirect_location<B>(&self, base: &Url, path: &str, body: &B) -> Result<Url>
    where
        B: Serialize + ?Sized,
    {
        let url = api_url(base, path)?;
        let response = self
            .send_api(|| {
                self.redirectless_http
                    .request(Method::POST, url.clone())
                    .bearer_auth(self.config.credential.expose())
                    .json(body)
            })
            .await?;
        Self::decode_redirect_location(response).await
    }

    async fn send_api<F>(&self, build: F) -> Result<Response>
    where
        F: Fn() -> reqwest::RequestBuilder,
    {
        #[cfg(feature = "retry")]
        if let Some(policy) = &self.config.retry_policy {
            return self.send_api_with_retry(build, policy).await;
        }

        Ok(build().send().await?)
    }

    #[cfg(feature = "retry")]
    async fn send_api_with_retry<F>(
        &self,
        build: F,
        policy: &crate::RetryPolicy,
    ) -> Result<Response>
    where
        F: Fn() -> reqwest::RequestBuilder,
    {
        // Only idempotent requests may be replayed. A POST/PATCH that already
        // reached the server before a transient timeout or 5xx would otherwise
        // be re-sent, creating duplicate jobs/tasks/webhooks. When the method
        // cannot be determined (build error), fall back to no retries.
        let idempotent = build()
            .build()
            .map(|request| Self::is_idempotent_method(request.method()))
            .unwrap_or(false);
        let attempts = if idempotent {
            policy.max_attempts_value().max(1)
        } else {
            1
        };
        let mut delay = policy.initial_delay_value();

        for attempt in 1..=attempts {
            match build().send().await {
                Ok(response) => {
                    if attempt == attempts || !Self::is_retryable_status(response.status().as_u16())
                    {
                        return Ok(response);
                    }

                    let retry_after = policy.respect_retry_after_value().then(|| {
                        response
                            .headers()
                            .get("retry-after")
                            .and_then(|value| value.to_str().ok())
                            .and_then(|value| value.parse::<u64>().ok())
                            .map(Duration::from_secs)
                    });
                    let sleep_for = retry_after
                        .flatten()
                        .map(|duration| duration.min(policy.max_delay_value()))
                        .unwrap_or(delay);
                    tokio::time::sleep(sleep_for).await;
                }
                Err(error) => {
                    if attempt == attempts || !Self::is_retryable_error(&error) {
                        return Err(Error::Http(error));
                    }

                    tokio::time::sleep(delay).await;
                }
            }

            delay = next_retry_delay(delay, policy);
        }

        unreachable!("retry loop always returns on the final attempt")
    }

    #[cfg(feature = "retry")]
    fn is_retryable_status(status: u16) -> bool {
        matches!(status, 429 | 500 | 502 | 503 | 504)
    }

    /// Methods whose replay is safe because a duplicate delivery has the same
    /// effect as a single one. POST and PATCH are excluded: re-sending an
    /// accepted create would produce duplicate CloudConvert resources.
    #[cfg(feature = "retry")]
    fn is_idempotent_method(method: &Method) -> bool {
        matches!(
            *method,
            Method::GET
                | Method::HEAD
                | Method::OPTIONS
                | Method::TRACE
                | Method::PUT
                | Method::DELETE
        )
    }

    #[cfg(feature = "retry")]
    fn is_retryable_error(error: &reqwest::Error) -> bool {
        error.is_connect() || error.is_timeout()
    }

    async fn decode_response<T>(response: Response) -> Result<ApiResponse<T>>
    where
        T: DeserializeOwned,
    {
        let rate_limit = Self::rate_limit_from_headers(response.headers());
        let response = Self::ensure_success(response).await?;
        let envelope = response.json::<DataEnvelope<T>>().await?;
        Ok(ApiResponse {
            data: envelope.data,
            links: envelope.links,
            meta: envelope.meta,
            rate_limit,
        })
    }

    fn into_page<T>(response: ApiResponse<Vec<T>>) -> Page<T> {
        Page {
            data: response.data,
            links: response.links,
            meta: response.meta,
            rate_limit: response.rate_limit,
        }
    }

    async fn decode_redirect_location(response: Response) -> Result<Url> {
        let response_url = response.url().clone();
        if response.status().is_redirection() {
            let location = response
                .headers()
                .get(LOCATION)
                .ok_or(Error::MissingRedirectLocation)?;
            let location = location
                .to_str()
                .map_err(|_| Error::MissingRedirectLocation)?;
            return response_url.join(location).map_err(Error::Url);
        }

        let _ = Self::ensure_success(response).await?;
        Err(Error::MissingRedirectLocation)
    }

    async fn ensure_success(response: Response) -> Result<Response> {
        if response.status().is_success() {
            return Ok(response);
        }

        let status = response.status().as_u16();
        let rate_limit = Self::rate_limit_from_headers(response.headers());
        let body = response.text().await.unwrap_or_default();
        let parsed = serde_json::from_str::<ApiErrorBody>(&body).ok();
        let message = parsed
            .as_ref()
            .and_then(|body| body.message.clone())
            .filter(|message| !message.is_empty())
            .unwrap_or_else(|| "request failed".to_string());
        let code = parsed.as_ref().and_then(|body| body.code.clone());
        let errors = parsed.and_then(|body| body.errors);

        Err(Error::Api {
            status,
            message,
            code,
            errors: errors.map(Box::new),
            rate_limit: rate_limit.map(Box::new),
        })
    }

    fn rate_limit_from_headers(headers: &HeaderMap) -> Option<RateLimit> {
        let rate_limit = RateLimit {
            limit: header_u64(headers, "x-ratelimit-limit"),
            remaining: header_u64(headers, "x-ratelimit-remaining"),
            reset: header_u64(headers, "x-ratelimit-reset"),
            retry_after: header_u64(headers, "retry-after"),
        };

        if rate_limit.limit.is_some()
            || rate_limit.remaining.is_some()
            || rate_limit.reset.is_some()
            || rate_limit.retry_after.is_some()
        {
            Some(rate_limit)
        } else {
            None
        }
    }
}

fn header_u64(headers: &HeaderMap, name: &'static str) -> Option<u64> {
    headers.get(name)?.to_str().ok()?.parse().ok()
}

fn api_url(base: &Url, path: &str) -> Result<Url> {
    validate_api_path(path)?;
    base.join(path).map_err(Error::Url)
}

fn validate_api_path(path: &str) -> Result<()> {
    let valid = !path.is_empty()
        && path.split('/').all(|segment| {
            !segment.is_empty()
                && segment
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        });

    if valid {
        Ok(())
    } else {
        Err(Error::InvalidApiPath)
    }
}

fn resource_id(id: &str) -> Result<&str> {
    let valid = !id.is_empty()
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'));

    if valid {
        Ok(id)
    } else {
        Err(Error::InvalidResourceId)
    }
}

#[cfg(feature = "retry")]
fn next_retry_delay(current: Duration, policy: &crate::RetryPolicy) -> Duration {
    let next = current.mul_f64(policy.backoff_factor_value());
    next.min(policy.max_delay_value())
}

#[derive(Clone, Debug)]
pub struct JobsResource {
    client: CloudConvertClient,
}

impl JobsResource {
    pub async fn list(&self, query: &JobListQuery) -> Result<Vec<Job>> {
        Ok(self.list_page(query).await?.data)
    }

    pub async fn list_page(&self, query: &JobListQuery) -> Result<Page<Job>> {
        let response = self
            .client
            .get_with_query_response(&self.client.config.api_base_url, "jobs", query)
            .await?;
        Ok(CloudConvertClient::into_page(response))
    }

    pub async fn create(&self, request: impl Into<JobCreateRequest>) -> Result<Job> {
        Ok(self.create_response(request).await?.data)
    }

    pub async fn create_response(
        &self,
        request: impl Into<JobCreateRequest>,
    ) -> Result<ApiResponse<Job>> {
        self.client
            .post_response(&self.client.config.api_base_url, "jobs", &request.into())
            .await
    }

    pub async fn create_and_wait(&self, request: impl Into<JobCreateRequest>) -> Result<Job> {
        Ok(self.create_and_wait_response(request).await?.data)
    }

    #[cfg(feature = "socket")]
    pub async fn create_and_wait_socket(
        &self,
        request: impl Into<JobCreateRequest>,
    ) -> Result<Job> {
        let job = self.create(request).await?;
        if job.is_terminal() {
            return Ok(job);
        }

        self.wait_socket(&job.id).await
    }

    pub async fn create_and_wait_response(
        &self,
        request: impl Into<JobCreateRequest>,
    ) -> Result<ApiResponse<Job>> {
        self.client
            .post_response(&self.client.config.sync_base_url, "jobs", &request.into())
            .await
    }

    pub async fn create_and_wait_redirect_url(
        &self,
        request: impl Into<JobCreateRequest>,
    ) -> Result<Url> {
        let mut request = request.into();
        request.redirect = Some(true);
        self.client
            .post_redirect_location(&self.client.config.sync_base_url, "jobs", &request)
            .await
    }

    pub async fn get(&self, id: impl AsRef<str>) -> Result<Job> {
        Ok(self.get_response(id).await?.data)
    }

    pub async fn get_response(&self, id: impl AsRef<str>) -> Result<ApiResponse<Job>> {
        let id = resource_id(id.as_ref())?;
        self.client
            .get_response(&self.client.config.api_base_url, &format!("jobs/{id}"))
            .await
    }

    pub async fn get_with_query(&self, id: impl AsRef<str>, query: &JobGetQuery) -> Result<Job> {
        Ok(self.get_with_query_response(id, query).await?.data)
    }

    pub async fn get_with_query_response(
        &self,
        id: impl AsRef<str>,
        query: &JobGetQuery,
    ) -> Result<ApiResponse<Job>> {
        let id = resource_id(id.as_ref())?;
        self.client
            .get_with_query_response(
                &self.client.config.api_base_url,
                &format!("jobs/{id}"),
                query,
            )
            .await
    }

    pub async fn get_redirect_url(&self, id: impl AsRef<str>) -> Result<Url> {
        let id = resource_id(id.as_ref())?;
        let query = JobGetQuery::default().redirect(true);
        self.client
            .get_redirect_location(
                &self.client.config.api_base_url,
                &format!("jobs/{id}"),
                &query,
            )
            .await
    }

    pub async fn wait(&self, id: impl AsRef<str>) -> Result<Job> {
        Ok(self.wait_response(id).await?.data)
    }

    #[cfg(feature = "socket")]
    pub async fn wait_socket(&self, id: impl AsRef<str>) -> Result<Job> {
        let id = resource_id(id.as_ref())?.to_string();
        let mut socket = self.client.socket([SocketChannel::job(id.clone())]).await?;
        let current = self.get(&id).await?;
        if current.is_terminal() {
            let _ = socket.disconnect().await;
            return Ok(current);
        }

        loop {
            let event = socket
                .next_event()
                .await
                .ok_or_else(|| Error::Socket(format!("socket closed before job {id} completed")))?;

            if !event.is_job_event() || !event.is_terminal() {
                continue;
            }

            let job = match event.job()? {
                Some(job) if job.id == id => job,
                Some(_) => continue,
                None => self.get(&id).await?,
            };

            let _ = socket.disconnect().await;
            return Ok(job);
        }
    }

    #[cfg(feature = "socket")]
    pub async fn task_events_socket(&self, id: impl AsRef<str>) -> Result<CloudConvertSocket> {
        self.task_events_socket_with_buffer(id, 64).await
    }

    #[cfg(feature = "socket")]
    pub async fn task_events_socket_with_buffer(
        &self,
        id: impl AsRef<str>,
        buffer: usize,
    ) -> Result<CloudConvertSocket> {
        let id = resource_id(id.as_ref())?.to_string();
        self.client
            .socket_with_buffer([SocketChannel::job_tasks(id)], buffer)
            .await
    }

    pub async fn wait_response(&self, id: impl AsRef<str>) -> Result<ApiResponse<Job>> {
        let id = resource_id(id.as_ref())?;
        self.client
            .get_response(&self.client.config.sync_base_url, &format!("jobs/{id}"))
            .await
    }

    pub async fn wait_with_query(&self, id: impl AsRef<str>, query: &JobGetQuery) -> Result<Job> {
        Ok(self.wait_with_query_response(id, query).await?.data)
    }

    pub async fn wait_with_query_response(
        &self,
        id: impl AsRef<str>,
        query: &JobGetQuery,
    ) -> Result<ApiResponse<Job>> {
        let id = resource_id(id.as_ref())?;
        self.client
            .get_with_query_response(
                &self.client.config.sync_base_url,
                &format!("jobs/{id}"),
                query,
            )
            .await
    }

    pub async fn wait_redirect_url(&self, id: impl AsRef<str>) -> Result<Url> {
        let id = resource_id(id.as_ref())?;
        let query = JobGetQuery::default().redirect(true);
        self.client
            .get_redirect_location(
                &self.client.config.sync_base_url,
                &format!("jobs/{id}"),
                &query,
            )
            .await
    }

    pub async fn delete(&self, id: impl AsRef<str>) -> Result<()> {
        let id = resource_id(id.as_ref())?;
        self.client
            .delete(&self.client.config.api_base_url, &format!("jobs/{id}"))
            .await
    }
}

#[derive(Clone, Debug)]
pub struct TasksResource {
    client: CloudConvertClient,
}

impl TasksResource {
    pub async fn list(&self, query: &TaskListQuery) -> Result<Vec<Task>> {
        Ok(self.list_page(query).await?.data)
    }

    pub async fn list_page(&self, query: &TaskListQuery) -> Result<Page<Task>> {
        let response = self
            .client
            .get_with_query_response(&self.client.config.api_base_url, "tasks", query)
            .await?;
        Ok(CloudConvertClient::into_page(response))
    }

    pub async fn create(&self, request: impl Into<TaskRequest>) -> Result<Task> {
        Ok(self.create_response(request).await?.data)
    }

    pub async fn create_response(
        &self,
        request: impl Into<TaskRequest>,
    ) -> Result<ApiResponse<Task>> {
        let request = request.into();
        let path = request.operation().to_string();
        self.client
            .post_response(&self.client.config.api_base_url, &path, &request)
            .await
    }

    pub async fn get(&self, id: impl AsRef<str>) -> Result<Task> {
        Ok(self.get_response(id).await?.data)
    }

    pub async fn get_response(&self, id: impl AsRef<str>) -> Result<ApiResponse<Task>> {
        let id = resource_id(id.as_ref())?;
        self.client
            .get_response(&self.client.config.api_base_url, &format!("tasks/{id}"))
            .await
    }

    pub async fn get_with_query(&self, id: impl AsRef<str>, query: &TaskGetQuery) -> Result<Task> {
        Ok(self.get_with_query_response(id, query).await?.data)
    }

    pub async fn get_with_query_response(
        &self,
        id: impl AsRef<str>,
        query: &TaskGetQuery,
    ) -> Result<ApiResponse<Task>> {
        let id = resource_id(id.as_ref())?;
        self.client
            .get_with_query_response(
                &self.client.config.api_base_url,
                &format!("tasks/{id}"),
                query,
            )
            .await
    }

    pub async fn wait(&self, id: impl AsRef<str>) -> Result<Task> {
        Ok(self.wait_response(id).await?.data)
    }

    #[cfg(feature = "socket")]
    pub async fn wait_socket(&self, id: impl AsRef<str>) -> Result<Task> {
        let id = resource_id(id.as_ref())?.to_string();
        let mut socket = self
            .client
            .socket([SocketChannel::task(id.clone())])
            .await?;
        let current = self.get(&id).await?;
        if current.is_terminal() {
            let _ = socket.disconnect().await;
            return Ok(current);
        }

        loop {
            let event = socket.next_event().await.ok_or_else(|| {
                Error::Socket(format!("socket closed before task {id} completed"))
            })?;

            if !event.is_task_event() || !event.is_terminal() {
                continue;
            }

            let task = match event.task()? {
                Some(task) if task.id == id => task,
                Some(_) => continue,
                None => self.get(&id).await?,
            };

            let _ = socket.disconnect().await;
            return Ok(task);
        }
    }

    pub async fn wait_response(&self, id: impl AsRef<str>) -> Result<ApiResponse<Task>> {
        let id = resource_id(id.as_ref())?;
        self.client
            .get_response(&self.client.config.sync_base_url, &format!("tasks/{id}"))
            .await
    }

    pub async fn wait_with_query(&self, id: impl AsRef<str>, query: &TaskGetQuery) -> Result<Task> {
        Ok(self.wait_with_query_response(id, query).await?.data)
    }

    pub async fn wait_with_query_response(
        &self,
        id: impl AsRef<str>,
        query: &TaskGetQuery,
    ) -> Result<ApiResponse<Task>> {
        let id = resource_id(id.as_ref())?;
        self.client
            .get_with_query_response(
                &self.client.config.sync_base_url,
                &format!("tasks/{id}"),
                query,
            )
            .await
    }

    pub async fn cancel(&self, id: impl AsRef<str>) -> Result<Task> {
        Ok(self.cancel_response(id).await?.data)
    }

    pub async fn cancel_response(&self, id: impl AsRef<str>) -> Result<ApiResponse<Task>> {
        let id = resource_id(id.as_ref())?;
        let empty = serde_json::json!({});
        self.client
            .post_response(
                &self.client.config.api_base_url,
                &format!("tasks/{id}/cancel"),
                &empty,
            )
            .await
    }

    pub async fn retry(&self, id: impl AsRef<str>) -> Result<Task> {
        Ok(self.retry_response(id).await?.data)
    }

    pub async fn retry_response(&self, id: impl AsRef<str>) -> Result<ApiResponse<Task>> {
        let id = resource_id(id.as_ref())?;
        let empty = serde_json::json!({});
        self.client
            .post_response(
                &self.client.config.api_base_url,
                &format!("tasks/{id}/retry"),
                &empty,
            )
            .await
    }

    pub async fn delete(&self, id: impl AsRef<str>) -> Result<()> {
        let id = resource_id(id.as_ref())?;
        self.client
            .delete(&self.client.config.api_base_url, &format!("tasks/{id}"))
            .await
    }
}

#[derive(Clone, Debug)]
pub struct UsersResource {
    client: CloudConvertClient,
}

impl UsersResource {
    pub async fn me(&self) -> Result<User> {
        Ok(self.me_response().await?.data)
    }

    pub async fn me_response(&self) -> Result<ApiResponse<User>> {
        self.client
            .get_response(&self.client.config.api_base_url, "users/me")
            .await
    }

    #[cfg(feature = "socket")]
    pub async fn job_events_socket(&self) -> Result<CloudConvertSocket> {
        self.job_events_socket_with_buffer(64).await
    }

    #[cfg(feature = "socket")]
    pub async fn job_events_socket_with_buffer(&self, buffer: usize) -> Result<CloudConvertSocket> {
        let user = self.me().await?;
        self.client
            .socket_with_buffer([SocketChannel::user_jobs(user.id)], buffer)
            .await
    }

    #[cfg(feature = "socket")]
    pub async fn task_events_socket(&self) -> Result<CloudConvertSocket> {
        self.task_events_socket_with_buffer(64).await
    }

    #[cfg(feature = "socket")]
    pub async fn task_events_socket_with_buffer(
        &self,
        buffer: usize,
    ) -> Result<CloudConvertSocket> {
        let user = self.me().await?;
        self.client
            .socket_with_buffer([SocketChannel::user_tasks(user.id)], buffer)
            .await
    }

    #[cfg(feature = "socket")]
    pub async fn events_socket(&self) -> Result<CloudConvertSocket> {
        self.events_socket_with_buffer(64).await
    }

    #[cfg(feature = "socket")]
    pub async fn events_socket_with_buffer(&self, buffer: usize) -> Result<CloudConvertSocket> {
        let user = self.me().await?;
        self.client
            .socket_with_buffer(
                [
                    SocketChannel::user_jobs(user.id.clone()),
                    SocketChannel::user_tasks(user.id),
                ],
                buffer,
            )
            .await
    }
}

#[derive(Clone, Debug)]
pub struct WebhooksResource {
    client: CloudConvertClient,
}

impl WebhooksResource {
    pub async fn create(&self, request: &WebhookCreateRequest) -> Result<Webhook> {
        Ok(self.create_response(request).await?.data)
    }

    pub async fn create_response(
        &self,
        request: &WebhookCreateRequest,
    ) -> Result<ApiResponse<Webhook>> {
        self.client
            .post_response(&self.client.config.api_base_url, "webhooks", request)
            .await
    }

    pub async fn list(&self, query: &WebhookListQuery) -> Result<Vec<Webhook>> {
        Ok(self.list_page(query).await?.data)
    }

    pub async fn list_page(&self, query: &WebhookListQuery) -> Result<Page<Webhook>> {
        let response = self
            .client
            .get_with_query_response(&self.client.config.api_base_url, "users/me/webhooks", query)
            .await?;
        Ok(CloudConvertClient::into_page(response))
    }

    pub async fn delete(&self, id: impl AsRef<str>) -> Result<()> {
        let id = resource_id(id.as_ref())?;
        self.client
            .delete(&self.client.config.api_base_url, &format!("webhooks/{id}"))
            .await
    }
}

#[derive(Clone, Debug)]
pub struct OperationsResource {
    client: CloudConvertClient,
}

impl OperationsResource {
    pub async fn list(&self, query: &OperationListQuery) -> Result<Vec<Operation>> {
        Ok(self.list_page(query).await?.data)
    }

    pub async fn list_page(&self, query: &OperationListQuery) -> Result<Page<Operation>> {
        let response = self
            .client
            .get_with_query_response(&self.client.config.api_base_url, "operations", query)
            .await?;
        Ok(CloudConvertClient::into_page(response))
    }
}

fn form_value(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        Value::Number(value) => value.to_string(),
        Value::Bool(value) => value.to_string(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn temporary_download_path(destination: &Path) -> PathBuf {
    let mut name = OsString::from(".");
    name.push(
        destination
            .file_name()
            .unwrap_or_else(|| OsStr::new("download")),
    );
    name.push(format!(".cloudconvert-{}.tmp", unique_suffix()));

    match destination.parent() {
        Some(parent) => parent.join(name),
        None => PathBuf::from(name),
    }
}

struct TemporaryDownload {
    path: PathBuf,
    committed: bool,
}

impl TemporaryDownload {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            committed: false,
        }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn commit(&mut self) {
        self.committed = true;
    }
}

impl Drop for TemporaryDownload {
    fn drop(&mut self) {
        if !self.committed {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

fn unique_suffix() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();

    format!("{}-{nanos}", std::process::id())
}
