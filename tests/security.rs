use std::{env, time::Duration};

#[cfg(feature = "retry")]
use cloudconvert_sdk::RetryPolicy;
use cloudconvert_sdk::{
    ApiKey, CloudConvertClient, Error, ExportUploadTask, JobCreateRequest, OAuthAccessToken,
    OAuthClientSecret, OAuthRefreshToken, Region, S3ImportTask, SigningSecret, Task, TaskRequest,
    TransportConfig, User, Webhook, sign_job_url, sign_payload, verify_signature,
};
use serde_json::json;
use url::Url;

#[test]
fn client_config_debug_redacts_api_key() {
    let client = CloudConvertClient::builder(ApiKey::new("cc_test_fake_secret_key"))
        .build()
        .unwrap();

    let debug = format!("{:?}", client.config());

    assert!(debug.contains("REDACTED"));
    assert!(!debug.contains("cc_test_fake_secret_key"));
}

#[test]
fn client_config_debug_redacts_oauth_access_token() {
    let client =
        CloudConvertClient::builder_with_access_token(OAuthAccessToken::new("oauth_secret_token"))
            .build()
            .unwrap();

    let debug = format!("{:?}", client.config());

    assert!(debug.contains("REDACTED"));
    assert!(!debug.contains("oauth_secret_token"));
}

#[test]
fn builder_resolves_default_sandbox_region_and_custom_urls() {
    let sandbox = CloudConvertClient::builder(ApiKey::new("cc_test_fake_secret_key"))
        .sandbox(true)
        .build()
        .unwrap();
    assert_eq!(
        sandbox.config().api_base_url().as_str(),
        "https://api.sandbox.cloudconvert.com/v2/"
    );
    assert_eq!(
        sandbox.config().sync_base_url().as_str(),
        "https://sync.api.sandbox.cloudconvert.com/v2/"
    );
    assert!(sandbox.config().sandbox());

    let regional = CloudConvertClient::builder(ApiKey::new("cc_test_fake_secret_key"))
        .region(Region::UsEast)
        .build()
        .unwrap();
    assert_eq!(
        regional.config().api_base_url().as_str(),
        "https://us-east.api.cloudconvert.com/v2/"
    );
    assert_eq!(
        regional.config().sync_base_url().as_str(),
        "https://us-east.sync.api.cloudconvert.com/v2/"
    );
    assert_eq!(regional.config().region(), Some(&Region::UsEast));

    let custom_api = Url::parse("https://api.example.test/v2/").unwrap();
    let custom_sync = Url::parse("https://sync.example.test/v2/").unwrap();
    let custom = CloudConvertClient::builder(ApiKey::new("cc_test_fake_secret_key"))
        .with_base_urls(custom_api.clone(), custom_sync.clone())
        .build()
        .unwrap();
    assert_eq!(custom.config().api_base_url(), &custom_api);
    assert_eq!(custom.config().sync_base_url(), &custom_sync);

    let eu_central = CloudConvertClient::builder(ApiKey::new("cc_test_fake_secret_key"))
        .region(Region::EuCentral)
        .build()
        .unwrap();
    assert_eq!(
        eu_central.config().api_base_url().as_str(),
        "https://eu-central.api.cloudconvert.com/v2/"
    );

    let custom_region = CloudConvertClient::builder(ApiKey::new("cc_test_fake_secret_key"))
        .region(Region::Custom("ap-southeast".to_string()))
        .build()
        .unwrap();
    assert_eq!(
        custom_region.config().sync_base_url().as_str(),
        "https://ap-southeast.sync.api.cloudconvert.com/v2/"
    );

    let unslashed = CloudConvertClient::builder(ApiKey::new("cc_test_fake_secret_key"))
        .with_base_urls(
            Url::parse("https://api.example.test/v2").unwrap(),
            Url::parse("https://sync.example.test/v2").unwrap(),
        )
        .build()
        .unwrap();
    assert_eq!(
        unslashed.config().api_base_url().as_str(),
        "https://api.example.test/v2/"
    );
    assert_eq!(
        unslashed
            .config()
            .api_base_url()
            .join("jobs")
            .unwrap()
            .as_str(),
        "https://api.example.test/v2/jobs"
    );
    assert_eq!(
        unslashed.config().sync_base_url().as_str(),
        "https://sync.example.test/v2/"
    );
}

#[test]
fn custom_regions_reject_host_escape_values() {
    for region in [
        "",
        "attacker.example",
        "attacker/../api",
        "us-east:443",
        "-us-east",
        "us-east-",
    ] {
        let error = CloudConvertClient::builder(ApiKey::new("cc_test_fake_secret_key"))
            .region(Region::Custom(region.to_string()))
            .build()
            .unwrap_err();
        assert!(
            matches!(error, Error::InvalidRegion),
            "{region} returned {error:?}"
        );
    }
}

#[test]
fn transport_config_accessors_expose_builder_values() {
    let transport = TransportConfig::default()
        .request_timeout(Duration::from_secs(120))
        .connect_timeout(Duration::from_secs(10))
        .pool_idle_timeout(Duration::from_secs(30))
        .user_agent("cloudconvert-sdk-test-agent");

    assert_eq!(
        transport.request_timeout_value(),
        Some(Duration::from_secs(120))
    );
    assert_eq!(
        transport.connect_timeout_value(),
        Some(Duration::from_secs(10))
    );
    assert_eq!(
        transport.pool_idle_timeout_value(),
        Some(Duration::from_secs(30))
    );
    assert_eq!(
        transport.user_agent_value(),
        Some("cloudconvert-sdk-test-agent")
    );

    let client = CloudConvertClient::builder(ApiKey::new("cc_test_fake_secret_key"))
        .transport_config(transport)
        .build()
        .unwrap();
    assert_eq!(
        client.config().api_base_url().as_str(),
        "https://api.cloudconvert.com/v2/"
    );
}

#[test]
fn builder_accepts_custom_reqwest_clients() {
    let http = reqwest::Client::builder().build().unwrap();
    let client = CloudConvertClient::builder(ApiKey::new("cc_test_fake_secret_key"))
        .http_client(http)
        .build()
        .unwrap();
    assert_eq!(
        client.config().sync_base_url().as_str(),
        "https://sync.api.cloudconvert.com/v2/"
    );

    let http = reqwest::Client::builder().build().unwrap();
    let redirectless = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let client = CloudConvertClient::builder(ApiKey::new("cc_test_fake_secret_key"))
        .http_clients(http, redirectless)
        .build()
        .unwrap();
    assert_eq!(
        client.config().api_base_url().as_str(),
        "https://api.cloudconvert.com/v2/"
    );
}

#[cfg(feature = "retry")]
#[test]
fn retry_policy_accessors_expose_builder_values() {
    let policy = RetryPolicy::new(0)
        .max_attempts(4)
        .initial_delay(Duration::from_millis(25))
        .max_delay(Duration::from_secs(2))
        .backoff_factor(0.5)
        .respect_retry_after(false);

    assert_eq!(policy.max_attempts_value(), 4);
    assert_eq!(policy.initial_delay_value(), Duration::from_millis(25));
    assert_eq!(policy.max_delay_value(), Duration::from_secs(2));
    assert_eq!(policy.backoff_factor_value(), 1.0);
    assert!(!policy.respect_retry_after_value());
}

#[test]
fn secret_debug_output_is_redacted() {
    let api_key = ApiKey::new("cc_test_fake_secret_key");
    let access_token = OAuthAccessToken::new("oauth_test_fake_access_token");
    let refresh_token = OAuthRefreshToken::new("oauth_test_fake_refresh_token");
    let client_secret = OAuthClientSecret::new("oauth_test_fake_client_secret");
    let signing_secret = SigningSecret::new("whsec_test_fake_secret");

    assert_eq!(format!("{api_key:?}"), "ApiKey(REDACTED)");
    assert_eq!(format!("{access_token:?}"), "OAuthAccessToken(REDACTED)");
    assert_eq!(format!("{refresh_token:?}"), "OAuthRefreshToken(REDACTED)");
    assert_eq!(format!("{client_secret:?}"), "OAuthClientSecret(REDACTED)");
    assert_eq!(format!("{signing_secret:?}"), "SigningSecret(REDACTED)");
}

#[test]
fn credential_env_loaders_return_values_and_missing_errors() {
    // These SDK-specific variables are only used by this test in the local suite.
    unsafe {
        env::set_var("CLOUDCONVERT_API_KEY", "cc_env_key");
        env::set_var("CLOUDCONVERT_OAUTH_ACCESS_TOKEN", "oauth_env_access");
        env::set_var("CLOUDCONVERT_OAUTH_REFRESH_TOKEN", "oauth_env_refresh");
        env::set_var("CLOUDCONVERT_OAUTH_CLIENT_SECRET", "oauth_env_secret");
    }

    assert_eq!(
        format!("{:?}", ApiKey::from_env().unwrap()),
        "ApiKey(REDACTED)"
    );
    assert_eq!(
        format!("{:?}", OAuthAccessToken::from_env().unwrap()),
        "OAuthAccessToken(REDACTED)"
    );
    assert_eq!(
        format!("{:?}", OAuthRefreshToken::from_env().unwrap()),
        "OAuthRefreshToken(REDACTED)"
    );
    assert_eq!(
        format!("{:?}", OAuthClientSecret::from_env().unwrap()),
        "OAuthClientSecret(REDACTED)"
    );

    unsafe {
        env::remove_var("CLOUDCONVERT_API_KEY");
        env::remove_var("CLOUDCONVERT_OAUTH_ACCESS_TOKEN");
        env::remove_var("CLOUDCONVERT_OAUTH_REFRESH_TOKEN");
        env::remove_var("CLOUDCONVERT_OAUTH_CLIENT_SECRET");
    }

    assert!(matches!(
        ApiKey::from_env(),
        Err(Error::MissingEnv("CLOUDCONVERT_API_KEY"))
    ));
    assert!(matches!(
        OAuthAccessToken::from_env(),
        Err(Error::MissingEnv("CLOUDCONVERT_OAUTH_ACCESS_TOKEN"))
    ));
    assert!(matches!(
        OAuthRefreshToken::from_env(),
        Err(Error::MissingEnv("CLOUDCONVERT_OAUTH_REFRESH_TOKEN"))
    ));
    assert!(matches!(
        OAuthClientSecret::from_env(),
        Err(Error::MissingEnv("CLOUDCONVERT_OAUTH_CLIENT_SECRET"))
    ));
}

#[test]
fn response_model_debug_redacts_sensitive_fields() {
    let user: User = serde_json::from_value(json!({
        "id": "user_1",
        "username": "tester",
        "email": "tester@example.test",
        "credits": 1.0,
        "created_at": "2026-06-02T00:00:00+00:00"
    }))
    .unwrap();
    let user_debug = format!("{user:?}");
    assert!(user_debug.contains("REDACTED"));
    assert!(!user_debug.contains("tester@example.test"));

    let webhook: Webhook = serde_json::from_value(json!({
        "id": "webhook_1",
        "url": "https://example.test/hook",
        "events": ["job.finished"],
        "signing_secret": "whsec_test_fake_secret"
    }))
    .unwrap();
    let webhook_debug = format!("{webhook:?}");
    assert!(webhook_debug.contains("REDACTED"));
    assert!(!webhook_debug.contains("whsec_test_fake_secret"));

    let webhook_json = serde_json::to_string(&webhook).unwrap();
    assert!(!webhook_json.contains("signing_secret"));
    assert!(!webhook_json.contains("whsec_test_fake_secret"));
}

#[test]
fn task_and_job_debug_output_redacts_payload_secrets() {
    let s3_import = S3ImportTask::new(
        "input-bucket",
        "eu-central-1",
        "cc_test_access_key_id",
        "cc_test_secret_access_key",
    )
    .session_token("cc_test_session_token");
    let s3_debug = format!("{s3_import:?}");
    assert!(s3_debug.contains("REDACTED"));
    assert!(!s3_debug.contains("cc_test_access_key_id"));
    assert!(!s3_debug.contains("cc_test_secret_access_key"));
    assert!(!s3_debug.contains("cc_test_session_token"));

    let upload = ExportUploadTask::new("convert", "https://upload.example.test/output.pdf")
        .header("Authorization", "Bearer cc_test_upload_token");
    let upload_debug = format!("{upload:?}");
    assert!(upload_debug.contains("Authorization"));
    assert!(!upload_debug.contains("cc_test_upload_token"));

    let task_request = TaskRequest::from(s3_import);
    let task_debug = format!("{task_request:?}");
    assert!(task_debug.contains("REDACTED"));
    assert!(!task_debug.contains("cc_test_secret_access_key"));

    let custom = TaskRequest::custom("custom/op").field("token", "cc_test_custom_token");
    let custom_debug = format!("{custom:?}");
    assert!(!custom_debug.contains("cc_test_custom_token"));

    let job = JobCreateRequest::builder()
        .task("import-file", task_request)
        .option("secret_token", "cc_test_job_secret")
        .build();
    let job_debug = format!("{job:?}");
    assert!(job_debug.contains("REDACTED"));
    assert!(!job_debug.contains("cc_test_secret_access_key"));
    assert!(!job_debug.contains("cc_test_job_secret"));
}

#[test]
fn task_response_debug_output_redacts_payload_and_upload_parameters() {
    let task: Task = serde_json::from_value(json!({
        "id": "task_1",
        "job_id": "job_1",
        "operation": "import/upload",
        "status": "waiting",
        "result": {
            "form": {
                "url": "https://storage.example.test/upload",
                "parameters": {
                    "signature": "cc_test_upload_signature"
                }
            }
        },
        "payload": {
            "secret_access_key": "cc_test_payload_secret"
        }
    }))
    .unwrap();

    let debug = format!("{task:?}");

    assert!(debug.contains("REDACTED"));
    assert!(!debug.contains("cc_test_upload_signature"));
    assert!(!debug.contains("cc_test_payload_secret"));
}

#[test]
fn verifies_webhook_hmac_signature() {
    let secret = SigningSecret::new("whsec_test_fake_secret");
    let payload = br#"{"event":"job.finished","job":{"id":"job_1"}}"#;
    let signature = sign_payload(payload, &secret).unwrap();

    assert!(verify_signature(payload, &signature, &secret).unwrap());
    assert!(!verify_signature(payload, "00", &secret).unwrap());
    assert!(matches!(
        verify_signature(payload, "not hex", &secret),
        Err(Error::InvalidSignatureHex)
    ));
}

#[test]
fn signs_cloudconvert_signed_url_jobs() {
    let secret = SigningSecret::new("signed_url_test_secret");
    let job = JobCreateRequest::builder()
        .task(
            "import-file",
            TaskRequest::import_url("https://example.test/file.pdf"),
        )
        .task("export-file", TaskRequest::export_url("import-file"))
        .build();

    let signed = sign_job_url(
        "https://convert.example.test/signed",
        &secret,
        &job,
        Some("cache-key"),
    )
    .unwrap();
    let (unsigned, signature) = signed.rsplit_once("&s=").unwrap();

    assert!(unsigned.contains("?job="));
    assert!(unsigned.contains("&cache_key=cache-key"));
    assert_eq!(signature, sign_payload(unsigned, &secret).unwrap());
}
