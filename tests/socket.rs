use cloudconvert_sdk::{
    ApiKey, CloudConvertClient, JobSocketEvent, SocketChannel, TaskSocketEvent, socket_base_url,
};
use serde_json::json;

#[test]
fn socket_channels_and_events_match_cloudconvert_names() {
    assert_eq!(SocketChannel::job("job_1").name(), "private-job.job_1");
    assert_eq!(
        SocketChannel::job_tasks("job_1").name(),
        "private-job.job_1.tasks"
    );
    assert_eq!(SocketChannel::task("task_1").name(), "private-task.task_1");
    assert_eq!(
        SocketChannel::user_jobs("user_1").name(),
        "private-user.user_1.jobs"
    );
    assert_eq!(
        SocketChannel::user_tasks("user_1").name(),
        "private-user.user_1.tasks"
    );
    assert_eq!(
        SocketChannel::Custom("private-custom.channel".to_string()).name(),
        "private-custom.channel"
    );
    assert_eq!(JobSocketEvent::Created.name(), "job.created");
    assert_eq!(JobSocketEvent::Updated.name(), "job.updated");
    assert_eq!(JobSocketEvent::Finished.name(), "job.finished");
    assert_eq!(JobSocketEvent::Failed.name(), "job.failed");
    assert_eq!(TaskSocketEvent::Created.name(), "task.created");
    assert_eq!(TaskSocketEvent::Updated.name(), "task.updated");
    assert_eq!(TaskSocketEvent::Finished.name(), "task.finished");
    assert_eq!(TaskSocketEvent::Failed.name(), "task.failed");
}

#[test]
fn socket_subscription_payload_serializes_auth_but_redacts_debug() {
    let client = CloudConvertClient::builder(ApiKey::new("cc_test_fake_key"))
        .sandbox(true)
        .build()
        .unwrap();

    assert_eq!(
        client.socket_base_url(),
        "https://socketio.sandbox.cloudconvert.com"
    );
    assert_eq!(socket_base_url(false), "https://socketio.cloudconvert.com");

    let subscription = client.socket_subscription(SocketChannel::job("job_1"));
    assert_eq!(subscription.channel(), "private-job.job_1");
    assert_eq!(
        serde_json::to_value(&subscription).unwrap(),
        json!({
            "channel": "private-job.job_1",
            "auth": {
                "headers": {
                    "Authorization": "Bearer cc_test_fake_key"
                }
            }
        })
    );

    let debug = format!("{subscription:?}");
    assert!(debug.contains("private-job.job_1"));
    assert!(!debug.contains("cc_test_fake_key"));
    assert!(!debug.contains("Bearer"));
}
