use std::{borrow::Cow, collections::BTreeMap, fmt};

#[cfg(feature = "socket")]
use futures_util::{FutureExt, future::BoxFuture};
#[cfg(feature = "socket")]
use rust_socketio::{
    Event, Payload,
    asynchronous::{Client as SocketIoClient, ClientBuilder as SocketIoClientBuilder},
};
use serde::Serialize;
#[cfg(feature = "socket")]
use serde::de::DeserializeOwned;
#[cfg(feature = "socket")]
use serde_json::Value;
#[cfg(feature = "socket")]
use tokio::sync::mpsc;

#[cfg(feature = "socket")]
use crate::{
    Error, Result,
    jobs::{Job, Task},
};

#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum SocketChannel {
    Job { job_id: String },
    JobTasks { job_id: String },
    Task { task_id: String },
    UserJobs { user_id: String },
    UserTasks { user_id: String },
    Custom(String),
}

impl SocketChannel {
    pub fn custom(channel: impl Into<String>) -> Self {
        Self::Custom(channel.into())
    }

    pub fn job(job_id: impl Into<String>) -> Self {
        Self::Job {
            job_id: job_id.into(),
        }
    }

    pub fn job_tasks(job_id: impl Into<String>) -> Self {
        Self::JobTasks {
            job_id: job_id.into(),
        }
    }

    pub fn task(task_id: impl Into<String>) -> Self {
        Self::Task {
            task_id: task_id.into(),
        }
    }

    pub fn user_jobs(user_id: impl Into<String>) -> Self {
        Self::UserJobs {
            user_id: user_id.into(),
        }
    }

    pub fn user_tasks(user_id: impl Into<String>) -> Self {
        Self::UserTasks {
            user_id: user_id.into(),
        }
    }

    pub fn name(&self) -> Cow<'_, str> {
        match self {
            Self::Job { job_id } => Cow::Owned(format!("private-job.{job_id}")),
            Self::JobTasks { job_id } => Cow::Owned(format!("private-job.{job_id}.tasks")),
            Self::Task { task_id } => Cow::Owned(format!("private-task.{task_id}")),
            Self::UserJobs { user_id } => Cow::Owned(format!("private-user.{user_id}.jobs")),
            Self::UserTasks { user_id } => Cow::Owned(format!("private-user.{user_id}.tasks")),
            Self::Custom(channel) => Cow::Borrowed(channel.as_str()),
        }
    }

    pub fn is_job(&self) -> bool {
        matches!(self, Self::Job { .. })
    }

    pub fn is_job_tasks(&self) -> bool {
        matches!(self, Self::JobTasks { .. })
    }

    pub fn is_task(&self) -> bool {
        matches!(self, Self::Task { .. })
    }

    pub fn is_user_jobs(&self) -> bool {
        matches!(self, Self::UserJobs { .. })
    }

    pub fn is_user_tasks(&self) -> bool {
        matches!(self, Self::UserTasks { .. })
    }

    pub fn job_id(&self) -> Option<&str> {
        match self {
            Self::Job { job_id } | Self::JobTasks { job_id } => Some(job_id),
            _ => None,
        }
    }

    pub fn task_id(&self) -> Option<&str> {
        match self {
            Self::Task { task_id } => Some(task_id),
            _ => None,
        }
    }

    pub fn user_id(&self) -> Option<&str> {
        match self {
            Self::UserJobs { user_id } | Self::UserTasks { user_id } => Some(user_id),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum JobSocketEvent {
    Created,
    Updated,
    Finished,
    Failed,
}

impl JobSocketEvent {
    pub fn name(self) -> &'static str {
        match self {
            Self::Created => "job.created",
            Self::Updated => "job.updated",
            Self::Finished => "job.finished",
            Self::Failed => "job.failed",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "job.created" => Self::Created,
            "job.updated" => Self::Updated,
            "job.finished" => Self::Finished,
            "job.failed" => Self::Failed,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum TaskSocketEvent {
    Created,
    Updated,
    Finished,
    Failed,
}

impl TaskSocketEvent {
    pub fn name(self) -> &'static str {
        match self {
            Self::Created => "task.created",
            Self::Updated => "task.updated",
            Self::Finished => "task.finished",
            Self::Failed => "task.failed",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "task.created" => Self::Created,
            "task.updated" => Self::Updated,
            "task.finished" => Self::Finished,
            "task.failed" => Self::Failed,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum SocketEventKind {
    Job(JobSocketEvent),
    Task(TaskSocketEvent),
    Other(String),
}

impl SocketEventKind {
    pub fn from_name(name: impl Into<String>) -> Self {
        let name = name.into();
        if let Some(event) = JobSocketEvent::from_name(&name) {
            return Self::Job(event);
        }
        if let Some(event) = TaskSocketEvent::from_name(&name) {
            return Self::Task(event);
        }
        Self::Other(name)
    }

    pub fn name(&self) -> &str {
        match self {
            Self::Job(event) => event.name(),
            Self::Task(event) => event.name(),
            Self::Other(event) => event.as_str(),
        }
    }

    pub fn is_job(&self) -> bool {
        matches!(self, Self::Job(_))
    }

    pub fn is_task(&self) -> bool {
        matches!(self, Self::Task(_))
    }
}

#[derive(Clone, Serialize)]
pub struct SocketSubscription {
    channel: String,
    auth: SocketAuth,
}

impl SocketSubscription {
    pub(crate) fn new(channel: impl Into<String>, api_key: &str) -> Self {
        Self {
            channel: channel.into(),
            auth: SocketAuth::bearer(api_key),
        }
    }

    pub fn channel(&self) -> &str {
        self.channel.as_str()
    }
}

impl fmt::Debug for SocketSubscription {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SocketSubscription")
            .field("channel", &self.channel)
            .field("auth", &"REDACTED")
            .finish()
    }
}

#[derive(Clone, Serialize)]
pub struct SocketAuth {
    headers: BTreeMap<String, String>,
}

impl SocketAuth {
    fn bearer(api_key: &str) -> Self {
        let mut headers = BTreeMap::new();
        headers.insert("Authorization".to_string(), format!("Bearer {api_key}"));
        Self { headers }
    }
}

impl fmt::Debug for SocketAuth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SocketAuth(REDACTED)")
    }
}

pub fn socket_base_url(sandbox: bool) -> &'static str {
    if sandbox {
        "https://socketio.sandbox.cloudconvert.com"
    } else {
        "https://socketio.cloudconvert.com"
    }
}

#[cfg(feature = "socket")]
#[derive(Clone, Debug)]
pub struct SocketEvent {
    event: String,
    channel: Option<String>,
    data: Value,
}

#[cfg(feature = "socket")]
impl SocketEvent {
    pub fn event(&self) -> &str {
        self.event.as_str()
    }

    pub fn channel(&self) -> Option<&str> {
        self.channel.as_deref()
    }

    pub fn data(&self) -> &Value {
        &self.data
    }

    pub fn kind(&self) -> SocketEventKind {
        SocketEventKind::from_name(self.event.clone())
    }

    pub fn job_event(&self) -> Option<JobSocketEvent> {
        JobSocketEvent::from_name(&self.event)
    }

    pub fn task_event(&self) -> Option<TaskSocketEvent> {
        TaskSocketEvent::from_name(&self.event)
    }

    pub fn is_job_event(&self) -> bool {
        self.event.starts_with("job.")
    }

    pub fn is_task_event(&self) -> bool {
        self.event.starts_with("task.")
    }

    pub fn is_finished(&self) -> bool {
        self.event.ends_with(".finished")
    }

    pub fn is_created(&self) -> bool {
        self.event.ends_with(".created")
    }

    pub fn is_updated(&self) -> bool {
        self.event.ends_with(".updated")
    }

    pub fn is_failed(&self) -> bool {
        self.event.ends_with(".failed")
    }

    pub fn is_terminal(&self) -> bool {
        self.is_finished() || self.is_failed()
    }

    pub fn job(&self) -> Result<Option<Job>> {
        decode_socket_data_field(&self.data, "job")
    }

    pub fn task(&self) -> Result<Option<Task>> {
        decode_socket_data_field(&self.data, "task")
    }

    #[allow(deprecated)]
    fn from_payload(event: impl Into<String>, payload: Payload) -> Self {
        let (channel, data) = match payload {
            Payload::Text(values) => split_socket_payload_values(values),
            Payload::String(value) => {
                let data = serde_json::from_str(&value).unwrap_or(Value::String(value));
                (None, data)
            }
            Payload::Binary(bytes) => (
                None,
                Value::Array(
                    bytes
                        .into_iter()
                        .map(|byte| Value::Number(byte.into()))
                        .collect(),
                ),
            ),
        };

        Self {
            event: event.into(),
            channel,
            data,
        }
    }
}

#[cfg(feature = "socket")]
pub struct CloudConvertSocket {
    client: SocketIoClient,
    receiver: mpsc::Receiver<SocketEvent>,
}

#[cfg(feature = "socket")]
impl fmt::Debug for CloudConvertSocket {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CloudConvertSocket")
            .field("client", &"Socket.IO client")
            .field("receiver", &"event receiver")
            .finish()
    }
}

#[cfg(feature = "socket")]
impl CloudConvertSocket {
    pub async fn connect(
        base_url: impl Into<String>,
        subscriptions: impl IntoIterator<Item = SocketSubscription>,
    ) -> Result<Self> {
        Self::connect_with_buffer(base_url, subscriptions, 64).await
    }

    pub async fn connect_with_buffer(
        base_url: impl Into<String>,
        subscriptions: impl IntoIterator<Item = SocketSubscription>,
        buffer: usize,
    ) -> Result<Self> {
        let (sender, receiver) = mpsc::channel(buffer.max(1));
        let subscriptions: Vec<SocketSubscription> = subscriptions.into_iter().collect();
        // Pre-serialize the subscribe payloads so a serialization failure surfaces
        // here (before connecting) and so the reconnect handler can re-emit them.
        let payloads = subscriptions
            .iter()
            .map(serde_json::to_value)
            .collect::<std::result::Result<Vec<Value>, _>>()?;
        let client = socket_client_builder(base_url.into(), sender, payloads)
            .connect()
            .await
            .map_err(socket_error)?;
        let socket = Self { client, receiver };

        for subscription in subscriptions {
            socket.subscribe(subscription).await?;
        }

        Ok(socket)
    }

    pub async fn subscribe(&self, subscription: SocketSubscription) -> Result<()> {
        let payload = serde_json::to_value(subscription)?;
        self.client
            .emit("subscribe", payload)
            .await
            .map_err(socket_error)
    }

    pub async fn next_event(&mut self) -> Option<SocketEvent> {
        self.receiver.recv().await
    }

    pub async fn disconnect(&self) -> Result<()> {
        self.client.disconnect().await.map_err(socket_error)
    }
}

#[cfg(feature = "socket")]
fn socket_client_builder(
    base_url: String,
    sender: mpsc::Sender<SocketEvent>,
    subscribe_payloads: Vec<Value>,
) -> SocketIoClientBuilder {
    let mut builder = SocketIoClientBuilder::new(base_url)
        .reconnect(true)
        .reconnect_on_disconnect(true);

    for event in [
        JobSocketEvent::Created.name(),
        JobSocketEvent::Updated.name(),
        JobSocketEvent::Finished.name(),
        JobSocketEvent::Failed.name(),
        TaskSocketEvent::Created.name(),
        TaskSocketEvent::Updated.name(),
        TaskSocketEvent::Finished.name(),
        TaskSocketEvent::Failed.name(),
    ] {
        builder = builder.on(event, socket_event_callback(event, sender.clone()));
    }

    // rust_socketio fires `Event::Connect` on every (re)connection. Re-emit the
    // subscribe payloads each time so private channels are restored after a
    // transport drop; otherwise managed waits stop receiving terminal events.
    builder.on(Event::Connect, resubscribe_callback(subscribe_payloads))
}

#[cfg(feature = "socket")]
fn resubscribe_callback(
    subscribe_payloads: Vec<Value>,
) -> impl FnMut(Payload, SocketIoClient) -> BoxFuture<'static, ()> + Send + Sync + 'static {
    move |_payload, client| {
        let subscribe_payloads = subscribe_payloads.clone();
        async move {
            for payload in subscribe_payloads {
                let _ = client.emit("subscribe", payload).await;
            }
        }
        .boxed()
    }
}

#[cfg(feature = "socket")]
fn socket_event_callback(
    event: &'static str,
    sender: mpsc::Sender<SocketEvent>,
) -> impl FnMut(Payload, SocketIoClient) -> BoxFuture<'static, ()> + Send + Sync + 'static {
    move |payload, _client| {
        let sender = sender.clone();
        async move {
            let _ = sender.send(SocketEvent::from_payload(event, payload)).await;
        }
        .boxed()
    }
}

#[cfg(feature = "socket")]
fn split_socket_payload_values(mut values: Vec<Value>) -> (Option<String>, Value) {
    match values.as_mut_slice() {
        [Value::String(channel), data] => (Some(channel.clone()), data.clone()),
        [_] => (None, values.remove(0)),
        _ => (None, Value::Array(values)),
    }
}

#[cfg(feature = "socket")]
fn decode_socket_data_field<T>(data: &Value, field: &'static str) -> Result<Option<T>>
where
    T: DeserializeOwned,
{
    let Some(value) = data.get(field) else {
        return Ok(None);
    };

    serde_json::from_value(value.clone())
        .map(Some)
        .map_err(Error::Json)
}

#[cfg(feature = "socket")]
fn socket_error(error: impl fmt::Display) -> Error {
    Error::Socket(error.to_string())
}

#[cfg(all(test, feature = "socket"))]
mod managed_socket_tests {
    use super::*;
    use std::time::Duration;

    use serde_json::json;

    #[test]
    fn socket_event_decodes_channel_and_job_payload() {
        let event = SocketEvent::from_payload(
            "job.finished",
            Payload::Text(vec![
                json!("private-job.job_1"),
                json!({
                    "job": {
                        "id": "job_1",
                        "status": "finished",
                        "tasks": []
                    }
                }),
            ]),
        );

        assert_eq!(event.event(), "job.finished");
        assert_eq!(event.channel(), Some("private-job.job_1"));
        assert_eq!(event.kind(), SocketEventKind::Job(JobSocketEvent::Finished));
        assert_eq!(event.job_event(), Some(JobSocketEvent::Finished));
        assert!(event.is_job_event());
        assert!(event.is_finished());
        assert!(event.is_terminal());
        assert_eq!(event.job().unwrap().unwrap().id, "job_1");
    }

    #[test]
    fn socket_event_preserves_unknown_payload_shapes() {
        let event = SocketEvent::from_payload(
            "task.updated",
            Payload::Text(vec![json!({"unexpected": true})]),
        );

        assert_eq!(event.channel(), None);
        assert_eq!(event.data()["unexpected"], true);
        assert_eq!(event.task_event(), Some(TaskSocketEvent::Updated));
        assert!(event.is_updated());
        assert!(event.task().unwrap().is_none());
    }

    #[test]
    fn socket_event_decodes_task_payload_and_status_helpers() {
        let event = SocketEvent::from_payload(
            "task.failed",
            Payload::from(
                json!({
                    "task": {
                        "id": "task_1",
                        "job_id": "job_1",
                        "operation": "convert",
                        "status": "error"
                    }
                })
                .to_string(),
            ),
        );

        assert_eq!(event.event(), "task.failed");
        assert_eq!(event.channel(), None);
        assert_eq!(event.kind(), SocketEventKind::Task(TaskSocketEvent::Failed));
        assert_eq!(event.task_event(), Some(TaskSocketEvent::Failed));
        assert_eq!(event.job_event(), None);
        assert!(event.is_task_event());
        assert!(!event.is_job_event());
        assert!(event.is_failed());
        assert!(event.is_terminal());
        assert_eq!(event.task().unwrap().unwrap().id, "task_1");
        assert!(event.job().unwrap().is_none());
    }

    #[test]
    fn socket_event_handles_created_updated_binary_and_unknown_payloads() {
        let created = SocketEvent::from_payload(
            "task.created",
            Payload::Text(vec![
                json!("private-task.task_1"),
                json!({
                    "task": {
                        "id": "task_1",
                        "job_id": "job_1",
                        "operation": "convert",
                        "status": "waiting"
                    }
                }),
            ]),
        );
        assert_eq!(created.channel(), Some("private-task.task_1"));
        assert!(created.is_created());
        assert!(!created.is_terminal());

        let binary = SocketEvent::from_payload(
            "job.updated",
            Payload::Binary(bytes::Bytes::from_static(&[1, 2, 3])),
        );
        assert_eq!(binary.data(), &json!([1, 2, 3]));
        assert!(binary.is_updated());

        let string = SocketEvent::from_payload("job.created", Payload::from("raw".to_string()));
        assert_eq!(string.data(), &json!("raw"));
        assert!(string.is_created());

        let unknown = SocketEvent::from_payload(
            "custom.event",
            Payload::Text(vec![json!("one"), json!("two"), json!("three")]),
        );
        assert_eq!(
            unknown.kind(),
            SocketEventKind::Other("custom.event".to_string())
        );
        assert_eq!(unknown.kind().name(), "custom.event");
        assert!(!unknown.kind().is_job());
        assert!(!unknown.kind().is_task());
        assert!(!unknown.is_job_event());
        assert!(!unknown.is_task_event());
        assert_eq!(unknown.data(), &json!(["one", "two", "three"]));
    }

    #[test]
    fn socket_event_reports_json_decode_errors_for_bad_payload_fields() {
        let event = SocketEvent::from_payload(
            "job.finished",
            Payload::Text(vec![json!({
                "job": "not a job object"
            })]),
        );

        assert!(matches!(event.job().unwrap_err(), Error::Json(_)));
    }

    #[tokio::test]
    async fn socket_connect_reports_socket_errors_for_unavailable_local_endpoint() {
        let result = tokio::time::timeout(
            Duration::from_secs(2),
            CloudConvertSocket::connect("http://127.0.0.1:1", Vec::<SocketSubscription>::new()),
        )
        .await
        .expect("local refused socket connection should finish quickly");

        assert!(matches!(result, Err(Error::Socket(_))));
    }
}
