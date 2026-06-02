use std::{borrow::Cow, collections::BTreeMap, fmt};

use serde::Serialize;

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
