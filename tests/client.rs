#[cfg(feature = "retry")]
use std::time::Duration;
use std::{collections::BTreeMap, sync::Arc, time::SystemTime};

use axum::{
    Json, Router,
    body::Bytes,
    extract::{OriginalUri, Query, State},
    http::{
        HeaderMap, Method, StatusCode,
        header::{AUTHORIZATION, LOCATION},
    },
    response::{IntoResponse, Response},
    routing::{delete, get, post},
};
#[cfg(feature = "retry")]
use cloudconvert_sdk::RetryPolicy;
use cloudconvert_sdk::{
    ApiKey, CloudConvertClient, ConvertTask, Error, ImportUrlTask, JobCreateRequest, JobListQuery,
    OAuthAccessToken, OAuthClient, OAuthClientSecret, OAuthScope, Operation, OperationListQuery,
    OperationOption, OperationOptionKind, OperationValidationErrorKind, OperationValidationMode,
    SocketChannel, Task, TaskRequest, TransportConfig, WatermarkTask,
};
use futures_util::stream;
use serde_json::{Value, json};
use tokio::{net::TcpListener, sync::Mutex, task::JoinHandle};
use url::Url;

#[derive(Clone, Debug, Eq, PartialEq)]
struct RecordedRequest {
    method: Method,
    path_and_query: String,
    authorization: Option<String>,
}

#[derive(Clone, Default)]
struct Observed {
    api_requests: Arc<Mutex<Vec<RecordedRequest>>>,
    job_payloads: Arc<Mutex<Vec<Value>>>,
    api_auth_headers: Arc<Mutex<Vec<Option<String>>>>,
    api_user_agents: Arc<Mutex<Vec<Option<String>>>>,
    download_auth_headers: Arc<Mutex<Vec<Option<String>>>>,
    upload_auth_headers: Arc<Mutex<Vec<Option<String>>>>,
    upload_bodies: Arc<Mutex<Vec<Vec<u8>>>>,
    oauth_token_bodies: Arc<Mutex<Vec<Vec<u8>>>>,
    flaky_job_attempts: Arc<Mutex<u32>>,
    flaky_create_attempts: Arc<Mutex<u32>>,
}

struct MockApi {
    base_url: Url,
    observed: Observed,
    task: JoinHandle<()>,
}

impl MockApi {
    async fn spawn() -> Self {
        let observed = Observed::default();
        let app = Router::new()
            .route("/v2/jobs", get(list_jobs).post(create_job))
            .route("/v2/jobs/job_1", get(show_job).delete(delete_job))
            .route("/v2/jobs/flaky", get(show_flaky_job))
            .route(
                "/v2/jobs/large-retry-after",
                get(show_large_retry_after_job),
            )
            .route("/v2/jobs/missing-location", get(missing_redirect_location))
            .route("/v2/jobs/no-redirect", get(show_job_without_redirect))
            .route("/v2/jobs/non-json-error", get(non_json_error))
            .route("/v2/jobs/missing-data", get(missing_data))
            .route("/v2/jobs/malformed-data", get(malformed_data))
            .route("/v2/jobs/delete-fail", delete(delete_job_failure))
            .route("/v2/operations", get(list_operations))
            .route("/v2/tasks", get(list_tasks))
            .route("/v2/tasks/task_1", get(show_task).delete(delete_task))
            .route("/v2/tasks/task_1/cancel", post(cancel_task))
            .route("/v2/tasks/task_1/retry", post(retry_task))
            .route("/v2/users/me", get(show_user))
            .route("/v2/users/me/webhooks", get(list_webhooks))
            .route("/v2/webhooks", post(create_webhook))
            .route("/v2/webhooks/webhook_1", delete(delete_webhook))
            .route("/v2/import/url", post(create_task))
            .route("/oauth/token", post(oauth_token))
            .route("/oauth/token-error", post(oauth_token_error))
            .route("/download", get(download))
            .route("/download-fail", get(download_failure))
            .route("/upload", post(upload))
            .route("/upload-fail", post(upload_failure))
            .with_state(observed.clone());
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let base_url = Url::parse(&format!("http://{addr}/v2/")).unwrap();

        Self {
            base_url,
            observed,
            task,
        }
    }
}

impl Drop for MockApi {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[tokio::test]
async fn creates_and_waits_for_job_against_axum_mock() {
    let api = MockApi::spawn().await;
    let client = mock_client(&api);
    let job_request = JobCreateRequest::builder()
        .tag("test-job")
        .task(
            "import-file",
            ImportUrlTask::new("https://example.test/input.pdf"),
        )
        .task(
            "add-watermark",
            WatermarkTask::text("import-file", "Draft").opacity(50),
        )
        .task(
            "export-file",
            cloudconvert_sdk::TaskRequest::export_url("add-watermark"),
        )
        .build();

    let created = client.jobs().create(job_request).await.unwrap();
    let finished = client.jobs().wait(&created.id).await.unwrap();
    client.jobs().delete(&created.id).await.unwrap();

    assert_eq!(created.id, "job_1");
    assert_eq!(finished.export_urls()[0].filename, "output.pdf");

    let payloads = api.observed.job_payloads.lock().await;
    assert_eq!(
        payloads[0]["tasks"]["add-watermark"]["operation"],
        "watermark"
    );
    assert_eq!(payloads[0]["tasks"]["add-watermark"]["text"], "Draft");

    let auth_headers = api.observed.api_auth_headers.lock().await;
    assert_eq!(auth_headers[0].as_deref(), Some("Bearer cc_test_fake_key"));
}

#[tokio::test]
async fn list_page_preserves_pagination_and_rate_limit_headers() {
    let api = MockApi::spawn().await;
    let client = mock_client(&api);

    let page = client
        .jobs()
        .list_page(&JobListQuery::default().per_page(1))
        .await
        .unwrap();

    assert_eq!(page.data[0].id, "job_1");
    assert_eq!(
        page.links.next.as_deref(),
        Some("https://api.example.test/jobs?page=2")
    );
    assert_eq!(page.meta.current_page, Some(1));
    let rate_limit = page.rate_limit.expect("rate-limit headers should parse");
    assert_eq!(rate_limit.limit, Some(120));
    assert_eq!(rate_limit.remaining, Some(119));
    assert_eq!(rate_limit.reset, Some(1_802_000_000));
}

#[tokio::test]
async fn operation_discovery_returns_typed_metadata() {
    let api = MockApi::spawn().await;
    let client = mock_client(&api);

    let operations = client
        .operations()
        .list(
            &OperationListQuery::default()
                .operation("convert")
                .input_format("pdf")
                .output_format("png")
                .include_options_and_engine_versions()
                .alternatives(true),
        )
        .await
        .unwrap();

    assert_eq!(operations[0].operation, "convert");
    assert_eq!(operations[0].input_format.as_deref(), Some("pdf"));
    assert_eq!(operations[0].output_format.as_deref(), Some("png"));
    assert_eq!(
        operations[0].engine_version_values().collect::<Vec<_>>(),
        vec!["1.0", "1.1"]
    );
    assert_eq!(
        operations[0].default_engine_version().unwrap().version,
        "1.0"
    );
    assert_eq!(
        operations[0].latest_engine_version().unwrap().version,
        "1.1"
    );

    let width = operations[0].option("width").unwrap();
    assert_eq!(width.kind(), Some(&OperationOptionKind::Integer));
    assert!(width.is_required());
    assert_eq!(width.default.as_ref(), Some(&json!(800)));
    assert_eq!(width.extra["ui_group"], "size");
    let fit = operations[0].option("fit").unwrap();
    assert_eq!(fit.kind(), Some(&OperationOptionKind::Enum));
    assert_eq!(fit.possible_values(), &[json!("max"), json!("crop")]);
    assert_eq!(
        operations[0].alternatives[0].engine.as_deref(),
        Some("imagemagick")
    );
}

#[tokio::test]
async fn operation_metadata_validates_task_requests_when_requested() {
    let api = MockApi::spawn().await;
    let client = mock_client(&api);
    let operation = client
        .operations()
        .list(&OperationListQuery::default().include_options())
        .await
        .unwrap()
        .remove(0);

    let valid = TaskRequest::from(
        ConvertTask::new("import-file", "png")
            .option("width", 800)
            .option("fit", "max"),
    );
    operation.validate_task(&valid).unwrap();
    operation.validate_task_strict(&valid).unwrap();

    let missing_required = TaskRequest::from(ConvertTask::new("import-file", "png"));
    let error = operation.validate_task(&missing_required).unwrap_err();
    assert_eq!(
        error.kind,
        OperationValidationErrorKind::MissingRequiredOption
    );
    assert_eq!(error.option.as_deref(), Some("width"));

    let wrong_type =
        TaskRequest::from(ConvertTask::new("import-file", "png").option("width", "800"));
    let error = operation.validate_task(&wrong_type).unwrap_err();
    assert_eq!(error.kind, OperationValidationErrorKind::InvalidOptionType);

    let disallowed = TaskRequest::from(
        ConvertTask::new("import-file", "png")
            .option("width", 800)
            .option("fit", "stretch"),
    );
    let error = operation.validate_task(&disallowed).unwrap_err();
    assert_eq!(error.kind, OperationValidationErrorKind::InvalidOptionValue);

    let unknown = TaskRequest::from(
        ConvertTask::new("import-file", "png")
            .option("width", 800)
            .option("new_cloudconvert_option", true),
    );
    operation.validate_task(&unknown).unwrap();
    let error = operation.validate_task_strict(&unknown).unwrap_err();
    assert_eq!(error.kind, OperationValidationErrorKind::UnknownOption);

    let wrong_format =
        TaskRequest::from(ConvertTask::new("import-file", "jpg").option("width", 800));
    let error = operation.validate_task(&wrong_format).unwrap_err();
    assert_eq!(error.kind, OperationValidationErrorKind::FormatMismatch);
    assert_eq!(error.option.as_deref(), Some("output_format"));
    assert_eq!(error.expected.as_deref(), Some("png"));
    assert_eq!(error.actual.as_deref(), Some("jpg"));
}

#[test]
fn strict_validation_accepts_structural_operation_fields() {
    let operation: Operation = serde_json::from_value(json!({
        "operation": "import/url",
        "options": {
            "filename": { "type": "string" }
        }
    }))
    .unwrap();

    let task = TaskRequest::from(
        ImportUrlTask::new("https://example.test/in.pdf")
            .header("Authorization", "Bearer x")
            .filename("in.pdf"),
    );

    operation.validate_task_strict(&task).unwrap();

    let bogus: TaskRequest = TaskRequest::custom("import/url")
        .field("url", "https://example.test/in.pdf")
        .field("not_a_real_field", true)
        .into();
    let error = operation.validate_task_strict(&bogus).unwrap_err();
    assert_eq!(error.kind, OperationValidationErrorKind::UnknownOption);
}

#[test]
fn strict_validation_keeps_structural_fields_operation_specific() {
    let operation: Operation = serde_json::from_value(json!({
        "operation": "convert",
        "options": {
            "width": { "type": "integer" }
        }
    }))
    .unwrap();

    for unknown_field in ["url", "bucket"] {
        let task: TaskRequest = TaskRequest::custom("convert")
            .field("input", "import-file")
            .field("output_format", "pdf")
            .field("width", 800)
            .field(unknown_field, "import-only-value")
            .into();

        let error = operation.validate_task_strict(&task).unwrap_err();
        assert_eq!(error.kind, OperationValidationErrorKind::UnknownOption);
        assert_eq!(error.option.as_deref(), Some(unknown_field));
    }
}

#[test]
fn operation_metadata_covers_wire_variants_and_validation_helpers() {
    let operation: Operation = serde_json::from_value(json!({
        "operation": "convert",
        "options": {
            "width": {
                "type": "integer",
                "required": true
            },
            "mode": {
                "type": "enum",
                "values": ["fast", "best"]
            }
        },
        "engine_versions": [
            "1.0",
            {
                "version": "1.1",
                "latest": true,
                "deprecated": false
            }
        ],
        "undocumented": true
    }))
    .unwrap();

    assert_eq!(
        operation
            .options()
            .map(|(name, _)| name)
            .collect::<Vec<_>>(),
        vec!["mode", "width"]
    );
    assert_eq!(operation.option("width").unwrap().name(), Some("width"));
    assert_eq!(
        operation.engine_version_values().collect::<Vec<_>>(),
        vec!["1.0", "1.1"]
    );
    assert!(operation.default_engine_version().is_none());
    assert_eq!(operation.latest_engine_version().unwrap().version, "1.1");
    assert_eq!(operation.extra["undocumented"], true);

    let strict_common_fields = TaskRequest::custom("convert")
        .field("input", "import-file")
        .field("ignore_error", true)
        .field("input_format", "pdf")
        .field("output_format", "png")
        .field("engine", "poppler")
        .field("engine_version", "1.0")
        .field("filename", "output.png")
        .field("timeout", 60)
        .field("width", 800)
        .field("mode", "fast");
    let strict_common_fields = TaskRequest::from(strict_common_fields);
    operation
        .validate_task_with_mode(&strict_common_fields, OperationValidationMode::Strict)
        .unwrap();

    let mismatch = operation
        .validate_task(&TaskRequest::metadata("import-file"))
        .unwrap_err();
    assert_eq!(
        mismatch.kind,
        OperationValidationErrorKind::OperationMismatch
    );
    assert!(
        mismatch
            .to_string()
            .contains("operation convert failed validation")
    );
    let source: &dyn std::error::Error = &mismatch;
    assert!(source.source().is_none());

    let kinds = [
        (
            OperationOptionKind::String,
            "string",
            json!("text"),
            json!(true),
        ),
        (
            OperationOptionKind::Boolean,
            "boolean",
            json!(true),
            json!("true"),
        ),
        (
            OperationOptionKind::Integer,
            "integer",
            json!(42),
            json!(42.5),
        ),
        (
            OperationOptionKind::Float,
            "float",
            json!(42.5),
            json!("42.5"),
        ),
        (OperationOptionKind::Enum, "enum", json!("fast"), json!(1)),
        (
            OperationOptionKind::Dictionary,
            "dictionary",
            json!({"key": "value"}),
            json!(["key", "value"]),
        ),
        (
            OperationOptionKind::Array,
            "array",
            json!(["value"]),
            json!({"key": "value"}),
        ),
        (
            OperationOptionKind::Other("new-kind".to_string()),
            "new-kind",
            json!(null),
            json!(null),
        ),
    ];

    for (kind, name, valid, invalid) in kinds {
        assert_eq!(kind.as_str(), name);
        assert_eq!(
            serde_json::from_value::<OperationOptionKind>(json!(name)).unwrap(),
            kind
        );
        assert_eq!(serde_json::to_value(&kind).unwrap(), json!(name));

        let option: OperationOption = serde_json::from_value(json!({
            "name": name,
            "type": name
        }))
        .unwrap();
        option.validate_value(&valid).unwrap();
        if !matches!(kind, OperationOptionKind::Other(_)) {
            let error = option.validate_value(&invalid).unwrap_err();
            assert_eq!(error.kind, OperationValidationErrorKind::InvalidOptionType);
            assert!(error.to_string().contains(name));
        }
    }
}

#[tokio::test]
async fn oauth_client_builds_urls_exchanges_tokens_and_authorizes_api_calls() {
    let api = MockApi::spawn().await;
    let authorize_url = api.base_url.join("../oauth/authorize").unwrap();
    let token_url = api.base_url.join("../oauth/token").unwrap();
    let oauth = OAuthClient::new("client_1", OAuthClientSecret::new("client_secret_1"))
        .unwrap()
        .with_endpoints(authorize_url, token_url);

    let authorize = oauth
        .authorization_code_url_with_state(
            "https://app.example.test/callback",
            [OAuthScope::TaskRead, OAuthScope::TaskWrite],
            "state_1",
        )
        .unwrap();
    assert_eq!(authorize.path(), "/oauth/authorize");
    assert!(authorize.as_str().contains("response_type=code"));
    assert!(authorize.as_str().contains("client_id=client_1"));
    assert!(authorize.as_str().contains("scope=task.read+task.write"));
    assert!(authorize.as_str().contains("state=state_1"));

    let implicit = oauth
        .implicit_url("https://app.example.test/callback", [OAuthScope::UserRead])
        .unwrap();
    assert!(implicit.as_str().contains("response_type=token"));

    let token = oauth
        .exchange_code("authorization_code_1", "https://app.example.test/callback")
        .await
        .unwrap();
    assert_eq!(
        format!("{:?}", token.access_token()),
        "OAuthAccessToken(REDACTED)"
    );
    assert_eq!(token.token_type.as_deref(), Some("Bearer"));
    assert_eq!(token.expires_in, Some(3600));
    assert!(format!("{token:?}").contains("REDACTED"));
    assert!(!format!("{token:?}").contains("oauth_access_token_1"));

    let refreshed = oauth
        .refresh_access_token(token.refresh_token().unwrap())
        .await
        .unwrap();
    assert_eq!(refreshed.scope.as_deref(), Some("task.read task.write"));

    let client = token
        .client_builder()
        .with_base_urls(api.base_url.clone(), api.base_url.clone())
        .build()
        .unwrap();
    let user = client.users().me().await.unwrap();
    assert_eq!(user.id, "user_1");

    let auth_headers = api.observed.api_auth_headers.lock().await;
    assert_eq!(
        auth_headers.last().and_then(|value| value.as_deref()),
        Some("Bearer oauth_access_token_1")
    );

    let token_for_builder = oauth
        .exchange_code("authorization_code_2", "https://app.example.test/callback")
        .await
        .unwrap();
    let client = token_for_builder
        .into_client_builder()
        .with_base_urls(api.base_url.clone(), api.base_url.clone())
        .build()
        .unwrap();
    assert_eq!(
        serde_json::to_value(client.socket_subscription(SocketChannel::custom("private-test")))
            .unwrap(),
        json!({
            "channel": "private-test",
            "auth": {
                "headers": {
                    "Authorization": "Bearer oauth_access_token_1"
                }
            }
        })
    );

    let access_token = oauth
        .exchange_code("authorization_code_3", "https://app.example.test/callback")
        .await
        .unwrap()
        .into_access_token();
    assert_eq!(format!("{access_token:?}"), "OAuthAccessToken(REDACTED)");

    let token_bodies = api.observed.oauth_token_bodies.lock().await;
    let joined = token_bodies
        .iter()
        .map(|body| String::from_utf8_lossy(body).into_owned())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(joined.contains("grant_type=authorization_code"));
    assert!(joined.contains("grant_type=refresh_token"));
    assert!(joined.contains("client_secret=client_secret_1"));
}

#[test]
fn oauth_accessors_and_scope_conversions_cover_public_surface() {
    let oauth = OAuthClient::new("client_1", OAuthClientSecret::new("client_secret_1")).unwrap();
    assert_eq!(oauth.client_id(), "client_1");
    assert_eq!(
        oauth.authorize_url().as_str(),
        "https://cloudconvert.com/oauth/authorize"
    );
    assert_eq!(
        oauth.token_url().as_str(),
        "https://cloudconvert.com/oauth/token"
    );
    assert!(format!("{oauth:?}").contains("OAuthClient"));
    assert!(!format!("{oauth:?}").contains("client_secret_1"));

    let authorization_code = oauth
        .authorization_code_url(
            "https://app.example.test/callback",
            [
                OAuthScope::UserRead,
                OAuthScope::UserWrite,
                OAuthScope::WebhookRead,
                OAuthScope::WebhookWrite,
                OAuthScope::from(String::from("custom.scope")),
            ],
        )
        .unwrap();
    assert!(authorization_code.as_str().contains("response_type=code"));
    assert!(
        authorization_code
            .as_str()
            .contains("scope=user.read+user.write+webhook.read+webhook.write+custom.scope")
    );

    let implicit = oauth
        .implicit_url_with_state(
            "https://app.example.test/callback",
            Vec::<OAuthScope>::new(),
            "state_2",
        )
        .unwrap();
    assert!(implicit.as_str().contains("response_type=token"));
    assert!(implicit.as_str().contains("state=state_2"));
    assert!(!implicit.as_str().contains("scope="));

    assert_eq!(
        serde_json::to_value(OAuthScope::TaskRead).unwrap(),
        json!("task.read")
    );
    assert_eq!(OAuthScope::from("task.read").as_str(), "task.read");
}

#[tokio::test]
async fn oauth_token_errors_decode_as_api_errors() {
    let api = MockApi::spawn().await;
    let oauth = OAuthClient::new("client_1", OAuthClientSecret::new("client_secret_1"))
        .unwrap()
        .with_endpoints(
            api.base_url.join("../oauth/authorize").unwrap(),
            api.base_url.join("../oauth/token-error").unwrap(),
        );

    let error = oauth
        .exchange_code("expired_code", "https://app.example.test/callback")
        .await
        .unwrap_err();
    let api_error = error.api_error().unwrap();
    assert_eq!(api_error.status, 400);
    assert_eq!(api_error.message, "authorization code expired");
    assert_eq!(api_error.code, Some("invalid_grant"));
}

#[tokio::test]
async fn oauth_access_token_builder_authorizes_api_but_not_storage_urls() {
    let api = MockApi::spawn().await;
    let client =
        CloudConvertClient::builder_with_access_token(OAuthAccessToken::new("oauth_direct_token"))
            .with_base_urls(api.base_url.clone(), api.base_url.clone())
            .build()
            .unwrap();

    let _ = client.jobs().get_redirect_url("job_1").await.unwrap();
    let _ = client
        .download(api.base_url.join("../download").unwrap())
        .await
        .unwrap();
    let task = upload_task(api.base_url.join("../upload").unwrap().to_string());
    client
        .upload_bytes(&task, "input.txt", bytes::Bytes::from_static(b"test"))
        .await
        .unwrap();

    let api_auth_headers = api.observed.api_auth_headers.lock().await;
    assert_eq!(
        api_auth_headers.last().and_then(|value| value.as_deref()),
        Some("Bearer oauth_direct_token")
    );
    let download_auth_headers = api.observed.download_auth_headers.lock().await;
    assert_eq!(download_auth_headers[0], None);
    let upload_auth_headers = api.observed.upload_auth_headers.lock().await;
    assert_eq!(upload_auth_headers[0], None);
}

#[tokio::test]
async fn resource_methods_emit_expected_http_contracts() {
    let api = MockApi::spawn().await;
    let client = mock_client(&api);

    let _ = client
        .tasks()
        .list(&cloudconvert_sdk::TaskListQuery::default().include("payload"))
        .await
        .unwrap();
    let _ = client
        .tasks()
        .get_with_query(
            "task_1",
            &cloudconvert_sdk::TaskGetQuery::default().include("job"),
        )
        .await
        .unwrap();
    let _ = client.tasks().cancel("task_1").await.unwrap();
    let _ = client.tasks().retry("task_1").await.unwrap();
    client.tasks().delete("task_1").await.unwrap();
    let _ = client.users().me().await.unwrap();
    let _ = client
        .webhooks()
        .list(&cloudconvert_sdk::WebhookListQuery::default())
        .await
        .unwrap();
    let _ = client
        .webhooks()
        .create(&cloudconvert_sdk::WebhookCreateRequest::new(
            "https://example.test/hook",
            vec![cloudconvert_sdk::WebhookEvent::JobFinished],
        ))
        .await
        .unwrap();
    client.webhooks().delete("webhook_1").await.unwrap();

    let requests = api.observed.api_requests.lock().await;
    assert!(requests.contains(&recorded(Method::GET, "/v2/tasks?include=payload")));
    assert!(requests.contains(&recorded(Method::GET, "/v2/tasks/task_1?include=job")));
    assert!(requests.contains(&recorded(Method::POST, "/v2/tasks/task_1/cancel")));
    assert!(requests.contains(&recorded(Method::POST, "/v2/tasks/task_1/retry")));
    assert!(requests.contains(&recorded(Method::DELETE, "/v2/tasks/task_1")));
    assert!(requests.contains(&recorded(Method::GET, "/v2/users/me")));
    assert!(requests.contains(&recorded(Method::GET, "/v2/users/me/webhooks")));
    assert!(requests.contains(&recorded(Method::POST, "/v2/webhooks")));
    assert!(requests.contains(&recorded(Method::DELETE, "/v2/webhooks/webhook_1")));
}

#[tokio::test]
async fn rejects_custom_task_operations_that_are_not_api_paths() {
    let api = MockApi::spawn().await;
    let client = mock_client(&api);

    for operation in [
        "https://attacker.example/collect",
        "//attacker.example/collect",
        "/users/me",
        "../users/me",
        "import/url?include=payload",
        r"import\url",
    ] {
        let error = client
            .tasks()
            .create(TaskRequest::custom(operation))
            .await
            .unwrap_err();
        assert!(
            matches!(error, Error::InvalidApiPath),
            "{operation} returned {error:?}"
        );
    }

    assert!(api.observed.api_requests.lock().await.is_empty());
}

#[tokio::test]
async fn rejects_resource_ids_that_would_change_api_paths() {
    let api = MockApi::spawn().await;
    let client = mock_client(&api);

    assert_invalid_resource_id(client.jobs().get("../tasks/task_1").await);
    assert_invalid_resource_id(client.jobs().get("job_1/tasks/task_1").await);
    assert_invalid_resource_id(client.tasks().cancel("task_1/../../users/me").await);
    assert_invalid_resource_id(client.tasks().retry("task_1?redirect=true").await);
    assert_invalid_resource_id(client.webhooks().delete("../users/me").await);

    assert!(api.observed.api_requests.lock().await.is_empty());
}

#[tokio::test]
async fn convenience_wrappers_return_unwrapped_data() {
    let api = MockApi::spawn().await;
    let client = mock_client(&api);

    assert_eq!(
        client.jobs().list(&JobListQuery::default()).await.unwrap()[0].id,
        "job_1"
    );
    assert_eq!(
        client
            .jobs()
            .create_and_wait(
                JobCreateRequest::builder()
                    .task(
                        "import-file",
                        ImportUrlTask::new("https://example.test/input.pdf"),
                    )
                    .build(),
            )
            .await
            .unwrap()
            .id,
        "job_1"
    );
    assert_eq!(
        client
            .jobs()
            .get_with_query(
                "job_1",
                &cloudconvert_sdk::JobGetQuery::default().include("tasks")
            )
            .await
            .unwrap()
            .id,
        "job_1"
    );
    assert_eq!(
        client
            .jobs()
            .wait_with_query(
                "job_1",
                &cloudconvert_sdk::JobGetQuery::default().include("tasks")
            )
            .await
            .unwrap()
            .id,
        "job_1"
    );
    assert_eq!(
        client
            .jobs()
            .get_redirect_url("job_1")
            .await
            .unwrap()
            .as_str(),
        "https://storage.example.test/final.pdf"
    );

    assert_eq!(
        client
            .tasks()
            .create(cloudconvert_sdk::TaskRequest::import_url(
                "https://example.test/input.pdf",
            ))
            .await
            .unwrap()
            .id,
        "task_1"
    );
    assert_eq!(client.tasks().get("task_1").await.unwrap().id, "task_1");
    assert_eq!(client.tasks().wait("task_1").await.unwrap().id, "task_1");
    assert_eq!(
        client
            .tasks()
            .wait_with_query(
                "task_1",
                &cloudconvert_sdk::TaskGetQuery::default().include("job"),
            )
            .await
            .unwrap()
            .id,
        "task_1"
    );
}

#[tokio::test]
async fn redirect_methods_return_location_without_following_it() {
    let api = MockApi::spawn().await;
    let client = mock_client(&api);

    let waited_url = client.jobs().wait_redirect_url("job_1").await.unwrap();
    assert_eq!(
        waited_url.as_str(),
        "https://storage.example.test/final.pdf"
    );

    let request = JobCreateRequest::builder()
        .task(
            "import-file",
            ImportUrlTask::new("https://example.test/input.pdf"),
        )
        .task(
            "export-file",
            cloudconvert_sdk::TaskRequest::export_url("import-file"),
        )
        .build();
    let created_url = client
        .jobs()
        .create_and_wait_redirect_url(request)
        .await
        .unwrap();
    assert_eq!(
        created_url.as_str(),
        "https://storage.example.test/created.pdf"
    );

    let payloads = api.observed.job_payloads.lock().await;
    assert_eq!(payloads.last().unwrap()["redirect"], true);
}

#[tokio::test]
async fn transport_config_applies_to_redirectless_client() {
    let api = MockApi::spawn().await;
    let client = CloudConvertClient::builder(ApiKey::new("cc_test_fake_key"))
        .with_base_urls(api.base_url.clone(), api.base_url.clone())
        .transport_config(TransportConfig::default().user_agent("cloudconvert-sdk-test-agent"))
        .build()
        .unwrap();

    let _ = client.jobs().wait_redirect_url("job_1").await.unwrap();

    let user_agents = api.observed.api_user_agents.lock().await;
    assert_eq!(
        user_agents.last().and_then(|value| value.as_deref()),
        Some("cloudconvert-sdk-test-agent")
    );
}

#[cfg(feature = "retry")]
#[tokio::test]
async fn retry_feature_retries_transient_api_statuses() {
    let api = MockApi::spawn().await;
    let client = CloudConvertClient::builder(ApiKey::new("cc_test_fake_key"))
        .with_base_urls(api.base_url.clone(), api.base_url.clone())
        .retry_policy(
            RetryPolicy::new(3)
                .initial_delay(Duration::from_millis(1))
                .max_delay(Duration::from_millis(2))
                .respect_retry_after(false),
        )
        .build()
        .unwrap();

    let job = client.jobs().get("flaky").await.unwrap();

    assert_eq!(job.id, "flaky");
    assert_eq!(*api.observed.flaky_job_attempts.lock().await, 3);
}

#[cfg(feature = "retry")]
#[tokio::test]
async fn retry_feature_does_not_replay_post_creates() {
    let api = MockApi::spawn().await;
    let client = CloudConvertClient::builder(ApiKey::new("cc_test_fake_key"))
        .with_base_urls(api.base_url.clone(), api.base_url.clone())
        .retry_policy(
            RetryPolicy::new(3)
                .initial_delay(Duration::from_millis(1))
                .max_delay(Duration::from_millis(2))
                .respect_retry_after(false),
        )
        .build()
        .unwrap();

    let request = JobCreateRequest::builder()
        .tag("flaky-create")
        .task(
            "import-file",
            ImportUrlTask::new("https://example.test/input.pdf"),
        )
        .build();

    let result = client.jobs().create(request).await;
    assert!(result.is_err());
    assert_eq!(*api.observed.flaky_create_attempts.lock().await, 1);
}

#[cfg(feature = "retry")]
#[tokio::test]
async fn retry_feature_respects_retry_after_headers() {
    let api = MockApi::spawn().await;
    let client = CloudConvertClient::builder(ApiKey::new("cc_test_fake_key"))
        .with_base_urls(api.base_url.clone(), api.base_url.clone())
        .retry_policy(
            RetryPolicy::new(3)
                .initial_delay(Duration::from_secs(1))
                .max_delay(Duration::from_secs(2)),
        )
        .build()
        .unwrap();

    let job = client.jobs().get("flaky").await.unwrap();

    assert_eq!(job.id, "flaky");
    assert_eq!(*api.observed.flaky_job_attempts.lock().await, 3);
}

#[cfg(feature = "retry")]
#[tokio::test(start_paused = true)]
async fn retry_after_headers_are_capped_by_max_delay() {
    let api = MockApi::spawn().await;
    let client = CloudConvertClient::builder(ApiKey::new("cc_test_fake_key"))
        .with_base_urls(api.base_url.clone(), api.base_url.clone())
        .retry_policy(
            RetryPolicy::new(2)
                .initial_delay(Duration::from_millis(10))
                .max_delay(Duration::from_millis(50)),
        )
        .build()
        .unwrap();

    let handle = tokio::spawn(async move { client.jobs().get("large-retry-after").await });
    for _ in 0..10 {
        if *api.observed.flaky_job_attempts.lock().await == 1 {
            break;
        }
        tokio::task::yield_now().await;
    }
    assert_eq!(*api.observed.flaky_job_attempts.lock().await, 1);

    tokio::time::advance(Duration::from_millis(49)).await;
    tokio::task::yield_now().await;
    assert!(!handle.is_finished());

    tokio::time::advance(Duration::from_millis(1)).await;
    let job = handle.await.unwrap().unwrap();

    assert_eq!(job.id, "large-retry-after");
    assert_eq!(*api.observed.flaky_job_attempts.lock().await, 2);
}

#[tokio::test]
async fn uploads_to_presigned_form_without_bearer_auth() {
    let api = MockApi::spawn().await;
    let client = mock_client(&api);
    let task = upload_task(api.base_url.join("../upload").unwrap().to_string());

    client
        .upload_bytes(&task, "input.pdf", Bytes::from_static(b"%PDF-1.7"))
        .await
        .unwrap();

    let upload_auth_headers = api.observed.upload_auth_headers.lock().await;
    assert_eq!(upload_auth_headers[0], None);

    let upload_bodies = api.observed.upload_bodies.lock().await;
    let body = String::from_utf8_lossy(&upload_bodies[0]);
    assert!(body.contains("fake-upload-signature"));
    assert!(body.contains("input.pdf"));
    assert!(body.contains("%PDF-1.7"));
    assert!(body.contains("name=\"signature\""));
    assert!(body.contains("name=\"enabled\""));
    assert!(!body.contains("name=\"empty\""));
}

#[tokio::test]
async fn uploads_path_to_presigned_form_without_bearer_auth() {
    let api = MockApi::spawn().await;
    let client = mock_client(&api);
    let task = upload_task(api.base_url.join("../upload").unwrap().to_string());
    let path = test_path("upload-path-input.pdf");
    tokio::fs::write(&path, b"%PDF-1.7 from file")
        .await
        .unwrap();

    client.upload_path(&task, &path).await.unwrap();

    let upload_auth_headers = api.observed.upload_auth_headers.lock().await;
    assert_eq!(upload_auth_headers[0], None);

    let upload_bodies = api.observed.upload_bodies.lock().await;
    let body = String::from_utf8_lossy(&upload_bodies[0]);
    assert!(body.contains("fake-upload-signature"));
    assert!(body.contains("upload-path-input.pdf"));
    assert!(body.contains("%PDF-1.7 from file"));

    let _ = tokio::fs::remove_file(path).await;
}

#[tokio::test]
async fn uploads_stream_to_presigned_form_without_bearer_auth() {
    let api = MockApi::spawn().await;
    let client = mock_client(&api);
    let task = upload_task(api.base_url.join("../upload").unwrap().to_string());
    let chunks = stream::iter([
        Ok::<_, std::io::Error>(Bytes::from_static(b"%PDF-1.7 ")),
        Ok::<_, std::io::Error>(Bytes::from_static(b"from stream")),
    ]);

    client
        .upload_stream(&task, "streamed.pdf", chunks)
        .await
        .unwrap();

    let upload_auth_headers = api.observed.upload_auth_headers.lock().await;
    assert_eq!(upload_auth_headers[0], None);

    let upload_bodies = api.observed.upload_bodies.lock().await;
    let body = String::from_utf8_lossy(&upload_bodies[0]);
    assert!(body.contains("fake-upload-signature"));
    assert!(body.contains("streamed.pdf"));
    assert!(body.contains("%PDF-1.7 from stream"));
}

#[tokio::test]
async fn downloads_to_path_without_bearer_auth() {
    let api = MockApi::spawn().await;
    let client = mock_client(&api);
    let destination = test_path("download-output.pdf");
    let _ = tokio::fs::remove_file(&destination).await;

    client
        .download_to_path(
            api.base_url.join("../download").unwrap().as_str(),
            &destination,
        )
        .await
        .unwrap();

    assert_eq!(
        tokio::fs::read(&destination).await.unwrap(),
        b"%PDF-1.7 downloaded in chunks"
    );

    let download_auth_headers = api.observed.download_auth_headers.lock().await;
    assert_eq!(download_auth_headers[0], None);

    let _ = tokio::fs::remove_file(destination).await;
}

#[tokio::test]
async fn downloads_to_relative_path_without_parent_directory() {
    let api = MockApi::spawn().await;
    let client = mock_client(&api);
    let destination = relative_test_path("download-output.pdf");
    let _ = tokio::fs::remove_file(&destination).await;

    client
        .download_to_path(
            api.base_url.join("../download").unwrap().as_str(),
            &destination,
        )
        .await
        .unwrap();

    assert_eq!(
        tokio::fs::read(&destination).await.unwrap(),
        b"%PDF-1.7 downloaded in chunks"
    );

    let _ = tokio::fs::remove_file(destination).await;
}

#[tokio::test]
async fn downloads_bytes_without_bearer_auth() {
    let api = MockApi::spawn().await;
    let client = mock_client(&api);

    let bytes = client
        .download(api.base_url.join("../download").unwrap().as_str())
        .await
        .unwrap();

    assert_eq!(bytes, Bytes::from_static(b"%PDF-1.7 downloaded in chunks"));

    let download_auth_headers = api.observed.download_auth_headers.lock().await;
    assert_eq!(download_auth_headers[0], None);
}

#[tokio::test]
async fn failed_download_to_path_does_not_create_destination_or_leak_body() {
    let api = MockApi::spawn().await;
    let client = mock_client(&api);
    let destination = test_path("download-failure-output.pdf");
    let _ = tokio::fs::remove_file(&destination).await;

    let error = client
        .download_to_path(
            api.base_url.join("../download-fail").unwrap().as_str(),
            &destination,
        )
        .await
        .unwrap_err();
    let api_error = error.api_error().expect("expected API error");

    assert_eq!(api_error.status, 500);
    assert_eq!(api_error.message, "request failed");
    assert!(!error.to_string().contains("download_secret_from_body"));
    assert!(tokio::fs::metadata(&destination).await.is_err());
}

#[tokio::test]
async fn redirect_without_location_reports_typed_error() {
    let api = MockApi::spawn().await;
    let client = mock_client(&api);

    let error = client
        .jobs()
        .get_redirect_url("missing-location")
        .await
        .unwrap_err();

    assert!(matches!(error, Error::MissingRedirectLocation));
}

#[tokio::test]
async fn successful_non_redirect_response_reports_missing_location_for_redirect_helpers() {
    let api = MockApi::spawn().await;
    let client = mock_client(&api);

    let error = client
        .jobs()
        .get_redirect_url("no-redirect")
        .await
        .unwrap_err();

    assert!(matches!(error, Error::MissingRedirectLocation));
}

#[tokio::test]
async fn upload_helpers_reject_tasks_without_ready_import_upload_form() {
    let api = MockApi::spawn().await;
    let client = mock_client(&api);
    let task = task_without_upload_form();

    let error = client
        .upload_bytes(&task, "input.pdf", Bytes::from_static(b"%PDF-1.7"))
        .await
        .unwrap_err();

    assert!(matches!(error, Error::UploadTaskNotReady));
    assert!(error.api_error().is_none());
}

#[tokio::test]
async fn non_json_error_body_uses_generic_message_without_leaking_body() {
    let api = MockApi::spawn().await;
    let client = mock_client(&api);

    let error = client.jobs().get("non-json-error").await.unwrap_err();
    let api_error = error.api_error().expect("expected API error");

    assert_eq!(api_error.status, 502);
    assert_eq!(api_error.message, "request failed");
    assert_eq!(api_error.code, None);
    assert_eq!(api_error.errors, None);
    assert!(!error.to_string().contains("cc_live_secret_from_body"));
}

#[tokio::test]
async fn json_error_envelope_is_exposed_on_delete_failure() {
    let api = MockApi::spawn().await;
    let client = mock_client(&api);

    let error = client.jobs().delete("delete-fail").await.unwrap_err();
    let api_error = error.api_error().expect("expected API error");

    assert_eq!(api_error.status, 404);
    assert_eq!(api_error.message, "Job not found");
    assert_eq!(api_error.code, Some("NOT_FOUND"));
    assert_eq!(
        api_error.errors,
        Some(&json!({
            "id": ["delete-fail does not exist"]
        }))
    );
    assert_eq!(api_error.retry_after, Some(60));
    assert_eq!(
        api_error
            .rate_limit
            .and_then(|rate_limit| rate_limit.retry_after),
        Some(60)
    );
}

#[tokio::test]
async fn successful_responses_must_have_valid_data_envelope() {
    let api = MockApi::spawn().await;
    let client = mock_client(&api);

    for id in ["missing-data", "malformed-data"] {
        let error = client.jobs().get(id).await.unwrap_err();
        match error {
            Error::Http(error) => assert!(error.is_decode(), "{id} should fail JSON decoding"),
            other => panic!("{id} returned unexpected error: {other:?}"),
        }
    }
}

#[tokio::test]
async fn failed_upload_reports_api_error_without_bearer_auth() {
    let api = MockApi::spawn().await;
    let client = mock_client(&api);
    let task = upload_task(api.base_url.join("../upload-fail").unwrap().to_string());

    let error = client
        .upload_bytes(&task, "input.pdf", Bytes::from_static(b"%PDF-1.7"))
        .await
        .unwrap_err();
    let api_error = error.api_error().expect("expected API error");

    assert_eq!(api_error.status, 403);
    assert_eq!(api_error.message, "request failed");
    assert_eq!(api_error.code, None);
    assert_eq!(api_error.errors, None);
    assert!(!error.to_string().contains("upload_secret_from_body"));

    let upload_auth_headers = api.observed.upload_auth_headers.lock().await;
    assert_eq!(upload_auth_headers[0], None);
}

fn mock_client(api: &MockApi) -> CloudConvertClient {
    CloudConvertClient::builder(ApiKey::new("cc_test_fake_key"))
        .with_base_urls(api.base_url.clone(), api.base_url.clone())
        .build()
        .unwrap()
}

fn upload_task(url: String) -> Task {
    serde_json::from_value(json!({
        "id": "task_1",
        "job_id": "job_1",
        "operation": "import/upload",
        "status": "waiting",
        "result": {
            "form": {
                "url": url,
                "parameters": {
                    "signature": "fake-upload-signature",
                    "enabled": true,
                    "empty": null,
                    "part_count": 2,
                    "metadata": {
                        "source": "test"
                    }
                }
            }
        }
    }))
    .expect("upload task JSON should deserialize")
}

fn task_without_upload_form() -> Task {
    serde_json::from_value(json!({
        "id": "task_1",
        "job_id": "job_1",
        "operation": "import/url",
        "status": "finished",
        "result": {}
    }))
    .expect("task JSON should deserialize")
}

fn test_path(name: &str) -> std::path::PathBuf {
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("system clock should be after UNIX_EPOCH")
        .as_nanos();
    std::env::temp_dir().join(format!("cloudconvert-sdk-{nanos}-{name}"))
}

fn relative_test_path(name: &str) -> std::path::PathBuf {
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("system clock should be after UNIX_EPOCH")
        .as_nanos();
    std::path::PathBuf::from(format!("cloudconvert-sdk-{nanos}-{name}"))
}

fn recorded(method: Method, path_and_query: &str) -> RecordedRequest {
    RecordedRequest {
        method,
        path_and_query: path_and_query.to_string(),
        authorization: Some("Bearer cc_test_fake_key".to_string()),
    }
}

fn assert_invalid_resource_id<T>(result: std::result::Result<T, Error>) {
    assert!(matches!(result, Err(Error::InvalidResourceId)));
}

async fn create_job(
    State(observed): State<Observed>,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    record_api_auth(&observed, &headers).await;
    if payload["tag"] == json!("flaky-create") {
        *observed.flaky_create_attempts.lock().await += 1;
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            [("retry-after", "0")],
            Json(json!({ "message": "temporary CloudConvert outage", "code": "TEMPORARY" })),
        )
            .into_response();
    }
    let redirect = payload["redirect"].as_bool().unwrap_or(false);
    observed.job_payloads.lock().await.push(payload);
    if redirect {
        return (
            StatusCode::FOUND,
            [(LOCATION, "https://storage.example.test/created.pdf")],
        )
            .into_response();
    }

    Json(json!({
        "data": {
            "id": "job_1",
            "tag": "test-job",
            "status": "processing",
            "created_at": "2026-06-02T00:00:00+00:00",
            "tasks": []
        }
    }))
    .into_response()
}

async fn list_jobs(State(observed): State<Observed>, headers: HeaderMap) -> impl IntoResponse {
    record_api_auth(&observed, &headers).await;
    (
        [
            ("x-ratelimit-limit", "120"),
            ("x-ratelimit-remaining", "119"),
            ("x-ratelimit-reset", "1802000000"),
        ],
        Json(json!({
            "data": [
                {
                    "id": "job_1",
                    "tag": "test-job",
                    "status": "finished",
                    "created_at": "2026-06-02T00:00:00+00:00",
                    "tasks": []
                }
            ],
            "links": {
                "next": "https://api.example.test/jobs?page=2"
            },
            "meta": {
                "current_page": 1,
                "per_page": 1,
                "total": 2
            }
        })),
    )
}

async fn show_job(
    State(observed): State<Observed>,
    headers: HeaderMap,
    Query(query): Query<BTreeMap<String, String>>,
) -> Response {
    record_api_auth(&observed, &headers).await;
    if query
        .get("redirect")
        .map(|value| value == "true")
        .unwrap_or(false)
    {
        return (
            StatusCode::FOUND,
            [(LOCATION, "https://storage.example.test/final.pdf")],
        )
            .into_response();
    }

    Json(json!({
        "data": {
            "id": "job_1",
            "tag": "test-job",
            "status": "finished",
            "created_at": "2026-06-02T00:00:00+00:00",
            "tasks": [
                {
                    "id": "task_export",
                    "name": "export-file",
                    "operation": "export/url",
                    "status": "finished",
                    "result": {
                        "files": [
                            {
                                "filename": "output.pdf",
                                "url": "https://storage.example.test/output.pdf",
                                "size": 123
                            }
                        ]
                    }
                }
            ]
        }
    }))
    .into_response()
}

async fn show_flaky_job(State(observed): State<Observed>, headers: HeaderMap) -> impl IntoResponse {
    record_api_auth(&observed, &headers).await;
    let mut attempts = observed.flaky_job_attempts.lock().await;
    *attempts += 1;

    if *attempts < 3 {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            [("retry-after", "0")],
            Json(json!({
                "message": "temporary CloudConvert outage",
                "code": "TEMPORARY"
            })),
        )
            .into_response();
    }

    Json(json!({
        "data": {
            "id": "flaky",
            "tag": null,
            "status": "finished",
            "created_at": "2026-06-02T00:00:00+00:00",
            "tasks": []
        }
    }))
    .into_response()
}

async fn show_large_retry_after_job(
    State(observed): State<Observed>,
    headers: HeaderMap,
) -> impl IntoResponse {
    record_api_auth(&observed, &headers).await;
    let mut attempts = observed.flaky_job_attempts.lock().await;
    *attempts += 1;

    if *attempts == 1 {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            [("retry-after", "3600")],
            Json(json!({
                "message": "temporary CloudConvert outage",
                "code": "TEMPORARY"
            })),
        )
            .into_response();
    }

    Json(json!({
        "data": {
            "id": "large-retry-after",
            "tag": null,
            "status": "finished",
            "created_at": "2026-06-02T00:00:00+00:00",
            "tasks": []
        }
    }))
    .into_response()
}

async fn missing_redirect_location(
    State(observed): State<Observed>,
    headers: HeaderMap,
) -> impl IntoResponse {
    record_api_auth(&observed, &headers).await;
    StatusCode::FOUND
}

async fn show_job_without_redirect(
    State(observed): State<Observed>,
    headers: HeaderMap,
) -> impl IntoResponse {
    record_api_auth(&observed, &headers).await;
    Json(json!({
        "data": {
            "id": "no-redirect",
            "tag": null,
            "status": "finished",
            "created_at": "2026-06-02T00:00:00+00:00",
            "tasks": []
        }
    }))
}

async fn list_operations(
    State(observed): State<Observed>,
    headers: HeaderMap,
    Query(_query): Query<BTreeMap<String, String>>,
) -> impl IntoResponse {
    record_api_auth(&observed, &headers).await;
    Json(json!({
        "data": [
            {
                "operation": "convert",
                "input_format": "pdf",
                "output_format": "png",
                "engine": "poppler",
                "engine_version": "1.0",
                "engine_versions": [
                    {
                        "version": "1.0",
                        "default": true,
                        "latest": false
                    },
                    {
                        "version": "1.1",
                        "default": false,
                        "latest": true,
                        "experimental": true
                    }
                ],
                "options": [
                    {
                        "name": "width",
                        "type": "integer",
                        "label": "Width",
                        "required": true,
                        "default": 800,
                        "ui_group": "size"
                    },
                    {
                        "name": "fit",
                        "type": "enum",
                        "label": "Fit",
                        "required": false,
                        "default": "max",
                        "possible_values": ["max", "crop"]
                    }
                ],
                "alternatives": [
                    {
                        "operation": "convert",
                        "input_format": "pdf",
                        "output_format": "png",
                        "engine": "imagemagick"
                    }
                ],
                "deprecated": false,
                "experimental": false,
                "meta": {
                    "category": "image"
                }
            }
        ],
        "links": {},
        "meta": {}
    }))
}

async fn list_tasks(
    State(observed): State<Observed>,
    headers: HeaderMap,
    OriginalUri(uri): OriginalUri,
) -> impl IntoResponse {
    record_api_request(&observed, Method::GET, uri, &headers).await;
    Json(json!({
        "data": [task_json()],
        "links": {},
        "meta": {}
    }))
}

async fn create_task(
    State(observed): State<Observed>,
    headers: HeaderMap,
    OriginalUri(uri): OriginalUri,
    Json(_payload): Json<Value>,
) -> impl IntoResponse {
    record_api_request(&observed, Method::POST, uri, &headers).await;
    Json(json!({
        "data": task_json()
    }))
}

async fn show_task(
    State(observed): State<Observed>,
    headers: HeaderMap,
    OriginalUri(uri): OriginalUri,
) -> impl IntoResponse {
    record_api_request(&observed, Method::GET, uri, &headers).await;
    Json(json!({
        "data": task_json()
    }))
}

async fn cancel_task(
    State(observed): State<Observed>,
    headers: HeaderMap,
    OriginalUri(uri): OriginalUri,
) -> impl IntoResponse {
    record_api_request(&observed, Method::POST, uri, &headers).await;
    Json(json!({
        "data": task_json()
    }))
}

async fn retry_task(
    State(observed): State<Observed>,
    headers: HeaderMap,
    OriginalUri(uri): OriginalUri,
) -> impl IntoResponse {
    record_api_request(&observed, Method::POST, uri, &headers).await;
    Json(json!({
        "data": task_json()
    }))
}

async fn delete_task(
    State(observed): State<Observed>,
    headers: HeaderMap,
    OriginalUri(uri): OriginalUri,
) -> impl IntoResponse {
    record_api_request(&observed, Method::DELETE, uri, &headers).await;
    StatusCode::NO_CONTENT
}

async fn show_user(
    State(observed): State<Observed>,
    headers: HeaderMap,
    OriginalUri(uri): OriginalUri,
) -> impl IntoResponse {
    record_api_request(&observed, Method::GET, uri, &headers).await;
    Json(json!({
        "data": {
            "id": "user_1",
            "username": "tester",
            "email": "tester@example.test",
            "credits": 1.0
        }
    }))
}

async fn list_webhooks(
    State(observed): State<Observed>,
    headers: HeaderMap,
    OriginalUri(uri): OriginalUri,
) -> impl IntoResponse {
    record_api_request(&observed, Method::GET, uri, &headers).await;
    Json(json!({
        "data": [webhook_json()],
        "links": {},
        "meta": {}
    }))
}

async fn create_webhook(
    State(observed): State<Observed>,
    headers: HeaderMap,
    OriginalUri(uri): OriginalUri,
    Json(_payload): Json<Value>,
) -> impl IntoResponse {
    record_api_request(&observed, Method::POST, uri, &headers).await;
    Json(json!({
        "data": webhook_json()
    }))
}

async fn delete_webhook(
    State(observed): State<Observed>,
    headers: HeaderMap,
    OriginalUri(uri): OriginalUri,
) -> impl IntoResponse {
    record_api_request(&observed, Method::DELETE, uri, &headers).await;
    StatusCode::NO_CONTENT
}

async fn oauth_token(State(observed): State<Observed>, body: Bytes) -> impl IntoResponse {
    observed.oauth_token_bodies.lock().await.push(body.to_vec());
    Json(json!({
        "access_token": "oauth_access_token_1",
        "refresh_token": "oauth_refresh_token_1",
        "token_type": "Bearer",
        "expires_in": 3600,
        "scope": "task.read task.write"
    }))
}

async fn oauth_token_error(body: Bytes) -> impl IntoResponse {
    let _ = body;
    (
        StatusCode::BAD_REQUEST,
        Json(json!({
            "error": "invalid_grant",
            "error_description": "authorization code expired"
        })),
    )
}

async fn delete_job(State(observed): State<Observed>, headers: HeaderMap) -> impl IntoResponse {
    record_api_auth(&observed, &headers).await;
    StatusCode::NO_CONTENT
}

async fn download(State(observed): State<Observed>, headers: HeaderMap) -> impl IntoResponse {
    observed
        .download_auth_headers
        .lock()
        .await
        .push(header_value(&headers, AUTHORIZATION.as_str()));
    Bytes::from_static(b"%PDF-1.7 downloaded in chunks")
}

async fn download_failure(
    State(observed): State<Observed>,
    headers: HeaderMap,
) -> impl IntoResponse {
    observed
        .download_auth_headers
        .lock()
        .await
        .push(header_value(&headers, AUTHORIZATION.as_str()));
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        "download failed with download_secret_from_body",
    )
}

async fn non_json_error(State(observed): State<Observed>, headers: HeaderMap) -> impl IntoResponse {
    record_api_auth(&observed, &headers).await;
    (
        StatusCode::BAD_GATEWAY,
        "upstream returned non-json body with cc_live_secret_from_body",
    )
}

async fn missing_data(State(observed): State<Observed>, headers: HeaderMap) -> impl IntoResponse {
    record_api_auth(&observed, &headers).await;
    Json(json!({
        "meta": {
            "status": "ok"
        }
    }))
}

async fn malformed_data(State(observed): State<Observed>, headers: HeaderMap) -> impl IntoResponse {
    record_api_auth(&observed, &headers).await;
    Json(json!({
        "data": {
            "id": "job_1",
            "status": "finished",
            "tasks": "not a task list"
        }
    }))
}

async fn delete_job_failure(
    State(observed): State<Observed>,
    headers: HeaderMap,
) -> impl IntoResponse {
    record_api_auth(&observed, &headers).await;
    (
        StatusCode::NOT_FOUND,
        [("retry-after", "60")],
        Json(json!({
            "message": "Job not found",
            "code": "NOT_FOUND",
            "errors": {
                "id": ["delete-fail does not exist"]
            }
        })),
    )
}

async fn upload(
    State(observed): State<Observed>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    observed
        .upload_auth_headers
        .lock()
        .await
        .push(header_value(&headers, AUTHORIZATION.as_str()));
    observed.upload_bodies.lock().await.push(body.to_vec());
    StatusCode::NO_CONTENT
}

async fn upload_failure(
    State(observed): State<Observed>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    observed
        .upload_auth_headers
        .lock()
        .await
        .push(header_value(&headers, AUTHORIZATION.as_str()));
    observed.upload_bodies.lock().await.push(body.to_vec());
    (
        StatusCode::FORBIDDEN,
        "upload rejected with upload_secret_from_body",
    )
}

fn task_json() -> Value {
    json!({
        "id": "task_1",
        "job_id": "job_1",
        "name": "convert-file",
        "operation": "convert",
        "status": "finished",
        "result": {
            "files": [
                {
                    "filename": "output.pdf",
                    "url": "https://storage.example.test/output.pdf",
                    "size": 123
                }
            ]
        }
    })
}

fn webhook_json() -> Value {
    json!({
        "id": "webhook_1",
        "url": "https://example.test/hook",
        "events": ["job.finished"],
        "disabled": false,
        "failing": false
    })
}

async fn record_api_request(
    observed: &Observed,
    method: Method,
    uri: axum::http::Uri,
    headers: &HeaderMap,
) {
    record_api_auth(observed, headers).await;
    observed.api_requests.lock().await.push(RecordedRequest {
        method,
        path_and_query: uri
            .path_and_query()
            .map(ToString::to_string)
            .unwrap_or_else(|| uri.path().to_string()),
        authorization: header_value(headers, AUTHORIZATION.as_str()),
    });
}

async fn record_api_auth(observed: &Observed, headers: &HeaderMap) {
    observed
        .api_auth_headers
        .lock()
        .await
        .push(header_value(headers, AUTHORIZATION.as_str()));
    observed
        .api_user_agents
        .lock()
        .await
        .push(header_value(headers, "user-agent"));
}

fn header_value(headers: &HeaderMap, key: &str) -> Option<String> {
    headers
        .get(key)
        .and_then(|value| value.to_str().ok())
        .map(ToString::to_string)
}
