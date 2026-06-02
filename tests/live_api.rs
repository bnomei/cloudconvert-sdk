use std::{env, fs, time::SystemTime};

use bytes::Bytes;
use cloudconvert_sdk::{
    ApiKey, CloudConvertClient, ConvertTask, ImportUrlTask, JobCreateRequest, Layer,
    PositionHorizontal, PositionVertical, TaskRequest, WatermarkTask,
};

const LIVE_TEST_REASON: &str =
    "requires CLOUDCONVERT_API_KEY in .env or the environment and performs live API calls";

#[tokio::test]
#[ignore = "requires CLOUDCONVERT_API_KEY in .env or the environment and performs live API calls"]
async fn live_api_accepts_watermark_job_shape() -> cloudconvert_sdk::Result<()> {
    if live_tests_are_disabled_in_ci() {
        return Ok(());
    }

    let client = CloudConvertClient::builder(live_api_key()).build()?;
    let tag = format!("rust-sdk-live-watermark-{}", timestamp());
    let request = JobCreateRequest::builder()
        .tag(tag.clone())
        .task(
            "import-file",
            ImportUrlTask::new("http://invalid.url").filename("input.pdf"),
        )
        .task(
            "add-watermark",
            WatermarkTask::text("import-file", "Rust SDK Live Test")
                .input_format("pdf")
                .layer(Layer::Above)
                .position(PositionVertical::Center, PositionHorizontal::Center)
                .opacity(35)
                .rotation(-20),
        )
        .task("export-file", TaskRequest::export_url("add-watermark"))
        .build();

    let job = client.jobs().create(request).await?;
    let job_id = job.id.clone();
    let job_tag = job.tag.clone();
    let task_operations = job
        .tasks
        .iter()
        .map(|task| task.operation.as_str())
        .collect::<Vec<_>>();

    client.jobs().delete(job_id).await?;

    assert_eq!(job_tag.as_deref(), Some(tag.as_str()));
    assert!(task_operations.contains(&"import/url"));
    assert!(task_operations.contains(&"watermark"));
    assert!(task_operations.contains(&"export/url"));

    Ok(())
}

#[tokio::test]
#[ignore = "requires CLOUDCONVERT_API_KEY in .env or the environment and performs live API calls"]
async fn live_api_creates_and_deletes_import_url_task() -> cloudconvert_sdk::Result<()> {
    if live_tests_are_disabled_in_ci() {
        return Ok(());
    }

    let client = CloudConvertClient::builder(live_api_key()).build()?;
    let task = client
        .tasks()
        .create(TaskRequest::from(
            ImportUrlTask::new("http://invalid.url").filename("input.pdf"),
        ))
        .await?;
    let task_id = task.id.clone();
    let operation = task.operation.clone();

    client.tasks().delete(task_id).await?;

    assert_eq!(operation, "import/url");

    Ok(())
}

#[tokio::test]
#[ignore = "requires CLOUDCONVERT_API_KEY in .env or the environment and performs live API calls"]
async fn live_api_uploads_converts_exports_and_downloads() -> cloudconvert_sdk::Result<()> {
    if live_tests_are_disabled_in_ci() {
        return Ok(());
    }

    let client = CloudConvertClient::builder(live_api_key()).build()?;
    let tag = format!("rust-sdk-live-upload-convert-{}", timestamp());
    let request = JobCreateRequest::builder()
        .tag(tag)
        .task("upload-file", TaskRequest::import_upload())
        .task(
            "convert-file",
            ConvertTask::new("upload-file", "pdf")
                .input_format("txt")
                .filename("cloudconvert-sdk-live-test.pdf"),
        )
        .task(
            "export-file",
            cloudconvert_sdk::ExportUrlTask::new("convert-file").inline(false),
        )
        .build();

    let job = client.jobs().create(request).await?;
    let job_id = job.id.clone();
    let result = run_uploaded_conversion(&client, &job_id, &job).await;
    let cleanup = client.jobs().delete(&job_id).await;

    result?;
    cleanup?;
    Ok(())
}

#[cfg(feature = "socket")]
#[tokio::test]
#[ignore = "requires CLOUDCONVERT_API_KEY in .env or the environment and performs live API calls"]
async fn live_api_waits_for_terminal_job_over_socket() -> cloudconvert_sdk::Result<()> {
    if live_tests_are_disabled_in_ci() {
        return Ok(());
    }

    let client = CloudConvertClient::builder(live_api_key()).build()?;
    let tag = format!("rust-sdk-live-socket-{}", timestamp());
    let request = JobCreateRequest::builder()
        .tag(tag)
        .task(
            "import-file",
            ImportUrlTask::new("http://invalid.url").filename("input.pdf"),
        )
        .task("export-file", TaskRequest::export_url("import-file"))
        .build();

    let job = client.jobs().create(request).await?;
    let job_id = job.id.clone();
    let result = client.jobs().wait_socket(&job_id).await;
    let cleanup = client.jobs().delete(&job_id).await;

    let finished = result?;
    cleanup?;
    assert!(finished.is_terminal());

    Ok(())
}

async fn run_uploaded_conversion(
    client: &CloudConvertClient,
    job_id: &str,
    job: &cloudconvert_sdk::Job,
) -> cloudconvert_sdk::Result<()> {
    let upload_task_id = job
        .tasks
        .iter()
        .find(|task| task.operation == "import/upload")
        .and_then(|task| task.id.as_deref())
        .ok_or_else(|| std::io::Error::other("import/upload task should have an id"))?;
    let upload_task = client.tasks().get(upload_task_id).await?;
    client
        .upload_bytes(
            &upload_task,
            "cloudconvert-sdk-live-test.txt",
            Bytes::from_static(b"CloudConvert Rust SDK live upload test\n"),
        )
        .await?;

    let finished = client.jobs().wait(job_id).await?;
    let export = finished
        .export_urls()
        .into_iter()
        .find_map(|file| file.url.as_deref())
        .ok_or_else(|| std::io::Error::other("finished job should include an export/url result"))?;
    let bytes = client.download(export).await?;
    assert!(!bytes.is_empty());

    Ok(())
}

fn live_api_key() -> ApiKey {
    if let Ok(value) = env::var("CLOUDCONVERT_API_KEY") {
        let value = value.trim();
        if !value.is_empty() {
            return ApiKey::new(value);
        }
    }

    let env_file =
        fs::read_to_string(".env").unwrap_or_else(|_| panic!("missing {LIVE_TEST_REASON}"));
    for line in env_file.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if let Some((key, value)) = line.split_once('=')
            && key.trim() == "CLOUDCONVERT_API_KEY"
        {
            let value = unquote(value.trim());
            if !value.is_empty() {
                return ApiKey::new(value);
            }
        }
    }

    panic!("missing {LIVE_TEST_REASON}");
}

fn live_tests_are_disabled_in_ci() -> bool {
    env::var_os("CI").is_some()
}

fn unquote(value: &str) -> String {
    value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .or_else(|| {
            value
                .strip_prefix('\'')
                .and_then(|value| value.strip_suffix('\''))
        })
        .unwrap_or(value)
        .to_string()
}

fn timestamp() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("system clock is before UNIX_EPOCH")
        .as_secs()
}
