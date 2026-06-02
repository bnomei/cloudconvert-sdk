use std::collections::BTreeMap;

use cloudconvert_sdk::{
    ArchiveTask, AzureBlobExportTask, AzureBlobImportTask, Base64ImportTask, CaptureWebsiteTask,
    CommandTask, ConvertTask, ExportUploadTask, ExportUrlTask, FileExtension,
    GoogleCloudStorageExportTask, GoogleCloudStorageImportTask, ImportUploadTask, ImportUrlTask,
    Input, JobCreateRequest, JobGraphBuilder, JobListQuery, JobStatus, Layer, MergeTask,
    MetadataTask, MetadataWriteTask, OpenStackExportTask, OpenStackImportTask, OperationListQuery,
    OptimizeTask, PdfATask, PositionHorizontal, PositionVertical, RawImportTask, S3ExportTask,
    S3ImportTask, SftpExportTask, SftpImportTask, TaskListQuery, TaskRequest, TaskStatus,
    ThumbnailTask, WatermarkTask, WebhookCreateRequest, WebhookEvent, WebhookListQuery,
};
use rstest::rstest;
use serde_json::{Value, json};

#[rstest]
#[case::text_watermark(
    TaskRequest::from(
        WatermarkTask::text("import-file", "Top Secret")
            .input_format("pdf")
            .layer(Layer::Above)
            .font_size(100)
            .font_color("#ff0400")
            .font_name("Helvetica Bold")
            .position(PositionVertical::Center, PositionHorizontal::Center)
            .opacity(50)
            .rotation(-45),
    ),
    json!({
        "operation": "watermark",
        "input": "import-file",
        "input_format": "pdf",
        "layer": "above",
        "text": "Top Secret",
        "font_size": 100,
        "font_color": "#ff0400",
        "font_name": "Helvetica Bold",
        "position_vertical": "center",
        "position_horizontal": "center",
        "opacity": 50,
        "rotation": -45
    })
)]
#[case::image_watermark(
    TaskRequest::from(
        WatermarkTask::image("import-file", "import-logo")
            .input_format("pdf")
            .layer(Layer::Above)
            .image_width(500)
            .position(PositionVertical::Bottom, PositionHorizontal::Right)
            .margins(25, 25)
            .opacity(75),
    ),
    json!({
        "operation": "watermark",
        "input": "import-file",
        "input_format": "pdf",
        "layer": "above",
        "image": "import-logo",
        "image_width": 500,
        "position_vertical": "bottom",
        "position_horizontal": "right",
        "margin_vertical": 25,
        "margin_horizontal": 25,
        "opacity": 75
    })
)]
fn serializes_watermark_tasks(#[case] task: TaskRequest, #[case] expected: Value) {
    assert_eq!(serde_json::to_value(task).unwrap(), expected);
}

#[test]
fn serializes_named_job_tasks() {
    let job = JobCreateRequest::builder()
        .tag("watermark-demo")
        .option("future_job_field", true)
        .task(
            "import-file",
            ImportUrlTask::new("https://example.test/input.docx"),
        )
        .task("convert-file", ConvertTask::new("import-file", "pdf"))
        .task(
            "add-watermark",
            WatermarkTask::text("convert-file", "Draft").opacity(50),
        )
        .task("export-file", ExportUrlTask::new("add-watermark"))
        .build();

    let payload = serde_json::to_value(job).unwrap();
    assert_eq!(payload["tag"], "watermark-demo");
    assert_eq!(payload["tasks"]["import-file"]["operation"], "import/url");
    assert_eq!(payload["tasks"]["convert-file"]["operation"], "convert");
    assert_eq!(payload["tasks"]["add-watermark"]["operation"], "watermark");
    assert_eq!(payload["tasks"]["export-file"]["operation"], "export/url");
    assert_eq!(payload["future_job_field"], true);
}

#[test]
fn job_builder_can_generate_task_names_as_dependency_handles() {
    let mut builder = JobCreateRequest::builder().tag("auto-name-demo");
    let import = builder.add_task(ImportUrlTask::new("https://example.test/input.docx"));
    let convert = builder.add_task(ConvertTask::new(&import, FileExtension::Pdf));
    let second_convert = builder.add_task(ConvertTask::new(&import, FileExtension::Png));
    let export = builder.add_task(ExportUrlTask::new(&convert));

    assert_eq!(import.as_str(), "import-url");
    assert_eq!(convert.as_str(), "convert");
    assert_eq!(second_convert.as_str(), "convert-2");
    assert_eq!(export.to_string(), "export-url");

    let payload = serde_json::to_value(builder.build()).unwrap();
    assert_eq!(payload["tag"], "auto-name-demo");
    assert_eq!(payload["tasks"]["import-url"]["operation"], "import/url");
    assert_eq!(payload["tasks"]["convert"]["operation"], "convert");
    assert_eq!(payload["tasks"]["convert"]["input"], "import-url");
    assert_eq!(payload["tasks"]["convert"]["output_format"], "pdf");
    assert_eq!(payload["tasks"]["convert-2"]["operation"], "convert");
    assert_eq!(payload["tasks"]["convert-2"]["input"], "import-url");
    assert_eq!(payload["tasks"]["convert-2"]["output_format"], "png");
    assert_eq!(payload["tasks"]["export-url"]["operation"], "export/url");
    assert_eq!(payload["tasks"]["export-url"]["input"], "convert");
}

#[test]
fn job_builder_task_shorthands_chain_using_previous_task_input() {
    let job = JobCreateRequest::builder()
        .tag("shortcut-demo")
        .import_url("https://example.test/input.docx")
        .convert(FileExtension::Pdf)
        .export_url()
        .build();

    let payload = serde_json::to_value(job).unwrap();
    assert_eq!(payload["tag"], "shortcut-demo");
    assert_eq!(payload["tasks"]["import-url"]["operation"], "import/url");
    assert_eq!(
        payload["tasks"]["import-url"]["url"],
        "https://example.test/input.docx"
    );
    assert_eq!(payload["tasks"]["convert"]["operation"], "convert");
    assert_eq!(payload["tasks"]["convert"]["input"], "import-url");
    assert_eq!(payload["tasks"]["convert"]["output_format"], "pdf");
    assert_eq!(payload["tasks"]["export-url"]["operation"], "export/url");
    assert_eq!(payload["tasks"]["export-url"]["input"], "convert");
}

#[test]
fn job_linear_shorthands_configure_tasks_without_manual_task_constructors() {
    let job = JobCreateRequest::linear()
        .tag("configured-linear-demo")
        .import_url_with("https://example.test/input.docx", |task| {
            task.filename("input.docx")
        })
        .convert_with(FileExtension::Pdf, |task| {
            task.input_format(".DOCX")
                .engine("office")
                .filename("converted.pdf")
        })
        .optimize_with(|task| {
            task.input_format(FileExtension::Pdf)
                .profile("print")
                .filename("converted-optimized.pdf")
        })
        .export_url_with(|task| task.inline(false))
        .build();

    let payload = serde_json::to_value(job).unwrap();
    assert_eq!(payload["tag"], "configured-linear-demo");
    assert_eq!(payload["tasks"]["import-url"]["filename"], "input.docx");
    assert_eq!(payload["tasks"]["convert"]["input"], "import-url");
    assert_eq!(payload["tasks"]["convert"]["input_format"], "docx");
    assert_eq!(payload["tasks"]["convert"]["output_format"], "pdf");
    assert_eq!(payload["tasks"]["convert"]["engine"], "office");
    assert_eq!(payload["tasks"]["convert"]["filename"], "converted.pdf");
    assert_eq!(payload["tasks"]["optimize"]["input"], "convert");
    assert_eq!(payload["tasks"]["optimize"]["input_format"], "pdf");
    assert_eq!(payload["tasks"]["optimize"]["profile"], "print");
    assert_eq!(
        payload["tasks"]["optimize"]["filename"],
        "converted-optimized.pdf"
    );
    assert_eq!(payload["tasks"]["export-url"]["input"], "optimize");
    assert_eq!(payload["tasks"]["export-url"]["inline"], false);
}

#[test]
fn job_graph_builder_branches_and_joins_with_generated_handles() {
    let job = JobCreateRequest::graph(|job| {
        let import = job.import_url("https://example.test/input.docx");
        let pdf = job.convert(&import, FileExtension::Pdf);
        let png = job.convert(&import, FileExtension::Png);
        job.export_url([&pdf, &png]);
    })
    .tag("branch-demo")
    .build();

    let payload = serde_json::to_value(job).unwrap();
    assert_eq!(payload["tag"], "branch-demo");
    assert_eq!(payload["tasks"]["import-url"]["operation"], "import/url");
    assert_eq!(payload["tasks"]["convert"]["input"], "import-url");
    assert_eq!(payload["tasks"]["convert"]["output_format"], "pdf");
    assert_eq!(payload["tasks"]["convert-2"]["input"], "import-url");
    assert_eq!(payload["tasks"]["convert-2"]["output_format"], "png");
    assert_eq!(
        payload["tasks"]["export-url"]["input"],
        json!(["convert", "convert-2"])
    );
}

#[test]
fn job_graph_builder_can_use_explicit_names_and_convert_into_request() {
    let mut graph = JobGraphBuilder::new();
    graph.tag("named-graph-demo");
    let upload = graph.add_named_task("uploaded-source", TaskRequest::import_upload());
    let convert = graph.convert_with(&upload, FileExtension::Pdf, |task| {
        task.input_format(FileExtension::Txt)
    });
    graph.export_url(&convert);

    let job: JobCreateRequest = graph.into();
    let payload = serde_json::to_value(job).unwrap();
    assert_eq!(payload["tag"], "named-graph-demo");
    assert_eq!(
        payload["tasks"]["uploaded-source"]["operation"],
        "import/upload"
    );
    assert_eq!(payload["tasks"]["convert"]["input"], "uploaded-source");
    assert_eq!(payload["tasks"]["convert"]["input_format"], "txt");
    assert_eq!(payload["tasks"]["export-url"]["input"], "convert");
}

#[test]
fn task_name_arrays_serialize_as_multi_input() {
    let mut builder = JobCreateRequest::builder();
    let import = builder.add_task(ImportUrlTask::new("https://example.test/input.docx"));
    let pdf = builder.add_task(ConvertTask::new(&import, FileExtension::Pdf));
    let png = builder.add_task(ConvertTask::new(&import, FileExtension::Png));

    let task = ExportUrlTask::new([&pdf, &png]);

    assert_eq!(
        serde_json::to_value(task).unwrap()["input"],
        json!(["convert", "convert-2"])
    );
}

#[test]
fn job_builder_task_shorthands_update_inputs_after_explicit_tasks() {
    let job = JobCreateRequest::builder()
        .task(
            "uploaded-source",
            ImportUrlTask::new("https://example.test/input.txt"),
        )
        .convert_with_input_format(FileExtension::Txt, FileExtension::Pdf)
        .pdf_a()
        .export_url()
        .build();

    let payload = serde_json::to_value(job).unwrap();
    assert_eq!(
        payload["tasks"]["uploaded-source"]["operation"],
        "import/url"
    );
    assert_eq!(payload["tasks"]["convert"]["input"], "uploaded-source");
    assert_eq!(payload["tasks"]["convert"]["input_format"], "txt");
    assert_eq!(payload["tasks"]["convert"]["output_format"], "pdf");
    assert_eq!(payload["tasks"]["pdf-a"]["operation"], "pdf/a");
    assert_eq!(payload["tasks"]["pdf-a"]["input"], "convert");
    assert_eq!(payload["tasks"]["export-url"]["input"], "pdf-a");
}

#[test]
fn job_request_accessors_expose_built_request_state() {
    let job = JobCreateRequest::builder()
        .tag("watermark-demo")
        .webhook_url("https://example.test/webhook")
        .redirect(true)
        .task(
            "import-file",
            ImportUrlTask::new("https://example.test/input.docx"),
        )
        .build();

    assert_eq!(job.tag(), Some("watermark-demo"));
    assert_eq!(job.webhook_url(), Some("https://example.test/webhook"));
    assert_eq!(job.redirect(), Some(true));
    assert_eq!(
        job.tasks().get("import-file").map(TaskRequest::operation),
        Some("import/url")
    );
    assert!(job.extra().is_empty());

    assert_eq!(
        JobCreateRequest::builder()
            .task("upload-file", TaskRequest::import_upload())
            .build()
            .tasks()
            .len(),
        1
    );
    assert_eq!(
        JobCreateRequest::builder()
            .task("upload-file", TaskRequest::import_upload())
            .build()
            .redirect(),
        None
    );
    assert_eq!(JobCreateRequest::builder().build().webhook_url(), None);
    assert_eq!(JobCreateRequest::builder().build().tag(), None);
    assert_eq!(
        cloudconvert_sdk::JobBuilder::new()
            .task("upload-file", TaskRequest::import_upload())
            .build()
            .tasks()
            .len(),
        1
    );
    let from_builder: JobCreateRequest = cloudconvert_sdk::JobBuilder::new()
        .task("upload-file", TaskRequest::import_upload())
        .into();
    assert_eq!(from_builder.tasks().len(), 1);

    let mut builder = JobCreateRequest::builder();
    let name = builder.add_named_task("upload-file", TaskRequest::import_upload());
    let job = builder.build();
    assert_eq!(name.as_str(), "upload-file");
    assert_eq!(
        job.tasks().get(name.as_str()).map(TaskRequest::operation),
        Some("import/upload")
    );
}

#[test]
fn fallible_task_payload_construction_preserves_payload_and_serialization() {
    let task = TaskRequest::try_from_payload(
        ImportUrlTask::new("https://example.test/input.docx").filename("input.docx"),
    )
    .unwrap();

    assert_eq!(task.operation(), "import/url");
    assert_eq!(task.payload()["url"], "https://example.test/input.docx");
    assert_eq!(
        serde_json::to_value(task).unwrap(),
        json!({
            "operation": "import/url",
            "url": "https://example.test/input.docx",
            "filename": "input.docx"
        })
    );
}

#[test]
fn generic_task_remains_escape_hatch_for_custom_operations() {
    let generic = TaskRequest::custom("custom/op")
        .field("input", "import-file")
        .field("arbitrary", json!({"nested": true}));

    assert_eq!(generic.operation(), "custom/op");
    assert_eq!(generic.data()["input"], "import-file");

    assert_eq!(
        serde_json::to_value(TaskRequest::from(generic)).unwrap(),
        json!({
            "operation": "custom/op",
            "input": "import-file",
            "arbitrary": {
                "nested": true
            }
        })
    );
}

#[test]
fn serializes_input_owned_value_conversions() {
    assert_eq!(
        serde_json::to_value(Input::from(String::from("import-file"))).unwrap(),
        json!("import-file")
    );
    assert_eq!(
        serde_json::to_value(Input::from(vec![
            String::from("first-file"),
            String::from("second-file"),
        ]))
        .unwrap(),
        json!(["first-file", "second-file"])
    );
}

#[test]
fn file_extensions_serialize_parse_and_normalize_known_formats() {
    assert_eq!(FileExtension::Pdf.as_str(), "pdf");
    assert_eq!(FileExtension::TarGz.to_string(), "tar.gz");
    assert_eq!(String::from(FileExtension::Docx), "docx");
    assert_eq!(FileExtension::ALL.len(), 213);

    assert_eq!(".PDF".parse::<FileExtension>().unwrap(), FileExtension::Pdf);
    assert_eq!(
        FileExtension::parse(" .TAR.GZ ").unwrap(),
        FileExtension::TarGz
    );

    let error = FileExtension::parse("nope").unwrap_err();
    assert_eq!(error.extension(), "nope");

    assert_eq!(
        serde_json::to_value(FileExtension::SevenZ).unwrap(),
        json!("7z")
    );
    assert_eq!(
        serde_json::from_value::<FileExtension>(json!(".JPG")).unwrap(),
        FileExtension::Jpg
    );
}

#[test]
fn query_builders_preserve_serialized_parameter_names() {
    assert_eq!(
        serde_json::to_value(
            JobListQuery::default()
                .status(JobStatus::Finished)
                .tag("release")
                .include("tasks")
                .per_page(25)
                .page(2)
        )
        .unwrap(),
        json!({
            "filter[status]": "finished",
            "filter[tag]": "release",
            "include": "tasks",
            "per_page": 25,
            "page": 2
        })
    );

    assert_eq!(
        serde_json::to_value(
            TaskListQuery::default()
                .job_id("job_1")
                .status(TaskStatus::Processing)
                .operation("convert")
                .include("job")
                .per_page(50)
                .page(3)
        )
        .unwrap(),
        json!({
            "filter[job_id]": "job_1",
            "filter[status]": "processing",
            "filter[operation]": "convert",
            "include": "job",
            "per_page": 50,
            "page": 3
        })
    );

    assert_eq!(
        serde_json::to_value(
            OperationListQuery::default()
                .operation("convert")
                .input_format(".DOCX")
                .output_format(FileExtension::Pdf)
                .engine("office")
                .engine_version("1.0")
                .include_options_and_engine_versions()
                .alternatives(true)
                .per_page(25)
                .page(2)
        )
        .unwrap(),
        json!({
            "filter[operation]": "convert",
            "filter[input_format]": "docx",
            "filter[output_format]": "pdf",
            "filter[engine]": "office",
            "filter[engine_version]": "1.0",
            "include": "options,engine_versions",
            "alternatives": true,
            "per_page": 25,
            "page": 2
        })
    );

    assert_eq!(
        serde_json::to_value(OperationListQuery::default().include_engine_versions()).unwrap(),
        json!({
            "include": "engine_versions"
        })
    );
    assert_eq!(
        serde_json::to_value(OperationListQuery::default().include_options()).unwrap(),
        json!({
            "include": "options"
        })
    );

    assert_eq!(
        serde_json::to_value(
            WebhookListQuery::default()
                .url("https://example.test/hook")
                .per_page(10)
                .page(2)
        )
        .unwrap(),
        json!({
            "filter[url]": "https://example.test/hook",
            "per_page": 10,
            "page": 2
        })
    );
}

#[test]
fn webhook_events_preserve_known_and_unknown_values() {
    let request = WebhookCreateRequest::new(
        "https://example.test/hook",
        vec![
            WebhookEvent::JobCreated,
            WebhookEvent::JobFinished,
            WebhookEvent::JobFailed,
            WebhookEvent::TaskCreated,
            WebhookEvent::TaskFinished,
            WebhookEvent::TaskFailed,
            WebhookEvent::Other("job.retried".to_string()),
        ],
    );

    assert_eq!(request.url(), "https://example.test/hook");
    assert_eq!(request.events().len(), 7);
    assert_eq!(
        serde_json::to_value(&request).unwrap(),
        json!({
            "url": "https://example.test/hook",
            "events": [
                "job.created",
                "job.finished",
                "job.failed",
                "task.created",
                "task.finished",
                "task.failed",
                "job.retried"
            ]
        })
    );
    assert_eq!(
        serde_json::from_value::<WebhookEvent>(json!("task.failed")).unwrap(),
        WebhookEvent::TaskFailed
    );
    assert_eq!(
        serde_json::from_value::<WebhookEvent>(json!("job.retried")).unwrap(),
        WebhookEvent::Other("job.retried".to_string())
    );
}

#[test]
fn task_request_convenience_constructors_delegate_to_typed_builders() {
    for (task, operation) in [
        (
            TaskRequest::import_raw("raw text", "input.txt"),
            "import/raw",
        ),
        (
            TaskRequest::import_s3("bucket", "region", "access-id", "secret-key"),
            "import/s3",
        ),
        (
            TaskRequest::import_azure_blob("storage-account", "container"),
            "import/azure/blob",
        ),
        (
            TaskRequest::import_google_cloud_storage(
                "project-id",
                "bucket",
                "client@example.test",
                "private-key",
            ),
            "import/google-cloud-storage",
        ),
        (
            TaskRequest::import_openstack(
                "https://openstack.example.test/auth",
                "username",
                "password",
                "region-one",
                "container",
            ),
            "import/openstack",
        ),
        (
            TaskRequest::import_sftp("sftp.example.test", "username"),
            "import/sftp",
        ),
        (TaskRequest::convert("import-file", "pdf"), "convert"),
        (TaskRequest::optimize("import-file"), "optimize"),
        (
            TaskRequest::capture_website("https://example.test", "pdf"),
            "capture-website",
        ),
        (TaskRequest::thumbnail("import-file", "png"), "thumbnail"),
        (TaskRequest::metadata("import-file"), "metadata"),
        (TaskRequest::metadata_write("import-file"), "metadata/write"),
        (TaskRequest::merge(vec!["first", "second"], "pdf"), "merge"),
        (
            TaskRequest::archive(vec!["first", "second"], "zip"),
            "archive",
        ),
        (
            TaskRequest::command("import-file", "engine", "command", "arguments"),
            "command",
        ),
        (
            TaskRequest::export_s3(
                "convert-file",
                "bucket",
                "region",
                "access-id",
                "secret-key",
            ),
            "export/s3",
        ),
        (
            TaskRequest::export_azure_blob("convert-file", "storage-account", "container"),
            "export/azure/blob",
        ),
        (
            TaskRequest::export_google_cloud_storage(
                "convert-file",
                "project-id",
                "bucket",
                "client@example.test",
                "private-key",
            ),
            "export/google-cloud-storage",
        ),
        (
            TaskRequest::export_openstack(
                "convert-file",
                "https://openstack.example.test/auth",
                "username",
                "password",
                "region-one",
                "container",
            ),
            "export/openstack",
        ),
        (
            TaskRequest::export_sftp("convert-file", "sftp.example.test", "username"),
            "export/sftp",
        ),
        (
            TaskRequest::export_upload("convert-file", "https://upload.example.test/report.pdf"),
            "export/upload",
        ),
    ] {
        assert_eq!(task.operation(), operation);
        assert_eq!(serde_json::to_value(task).unwrap()["operation"], operation);
    }
}

#[test]
fn serializes_simple_import_convenience_builders() {
    assert_eq!(
        serde_json::to_value(TaskRequest::import_upload()).unwrap(),
        json!({
            "operation": "import/upload"
        })
    );
    assert_eq!(
        serde_json::to_value(TaskRequest::from(
            ImportUploadTask::default().redirect("https://example.test/redirect")
        ))
        .unwrap(),
        json!({
            "operation": "import/upload",
            "redirect": "https://example.test/redirect"
        })
    );
    assert_eq!(
        serde_json::to_value(TaskRequest::import_base64("SGVsbG8=", "hello.txt")).unwrap(),
        json!({
            "operation": "import/base64",
            "file": "SGVsbG8=",
            "filename": "hello.txt"
        })
    );
    assert_eq!(
        serde_json::to_value(TaskRequest::from(RawImportTask::new(
            "raw text",
            "input.txt"
        )))
        .unwrap(),
        json!({
            "operation": "import/raw",
            "file": "raw text",
            "filename": "input.txt"
        })
    );
    assert_eq!(
        serde_json::to_value(TaskRequest::from(
            ImportUrlTask::new("https://example.test/input.pdf")
                .header("Authorization", "Bearer token")
        ))
        .unwrap(),
        json!({
            "operation": "import/url",
            "url": "https://example.test/input.pdf",
            "headers": {
                "Authorization": "Bearer token"
            }
        })
    );
    assert_eq!(
        serde_json::to_value(TaskRequest::from(Base64ImportTask::new(
            "ZmFrZQ==", "fake.bin"
        )))
        .unwrap(),
        json!({
            "operation": "import/base64",
            "file": "ZmFrZQ==",
            "filename": "fake.bin"
        })
    );
}

#[test]
fn serializes_import_export_provider_builders() {
    let import = TaskRequest::from(
        S3ImportTask::new("source-bucket", "eu-central-1", "access-id", "secret-key")
            .endpoint("https://s3.example.test")
            .key("incoming/report.pdf")
            .key_prefix("incoming/")
            .session_token("session-token")
            .filename("report.pdf"),
    );

    assert_eq!(
        serde_json::to_value(import).unwrap(),
        json!({
            "operation": "import/s3",
            "bucket": "source-bucket",
            "region": "eu-central-1",
            "endpoint": "https://s3.example.test",
            "key": "incoming/report.pdf",
            "key_prefix": "incoming/",
            "access_key_id": "access-id",
            "secret_access_key": "secret-key",
            "session_token": "session-token",
            "filename": "report.pdf"
        })
    );

    let export = TaskRequest::from(
        S3ExportTask::new(
            "convert-file",
            "target-bucket",
            "eu-central-1",
            "access-id",
            "secret-key",
        )
        .endpoint("https://s3.example.test")
        .key("exports/report.pdf")
        .key_prefix("exports/")
        .session_token("session-token")
        .acl("public-read")
        .cache_control("max-age=3600")
        .content_disposition("attachment")
        .content_type("application/pdf")
        .server_side_encryption("AES256")
        .option("metadata", "replace-me")
        .metadata("origin", "cloudconvert-sdk")
        .tag("env", "test"),
    );

    assert_eq!(
        serde_json::to_value(export).unwrap(),
        json!({
            "operation": "export/s3",
            "input": "convert-file",
            "bucket": "target-bucket",
            "region": "eu-central-1",
            "endpoint": "https://s3.example.test",
            "key": "exports/report.pdf",
            "key_prefix": "exports/",
            "access_key_id": "access-id",
            "secret_access_key": "secret-key",
            "session_token": "session-token",
            "acl": "public-read",
            "cache_control": "max-age=3600",
            "content_disposition": "attachment",
            "content_type": "application/pdf",
            "server_side_encryption": "AES256",
            "metadata": {
                "origin": "cloudconvert-sdk"
            },
            "tagging": {
                "env": "test"
            }
        })
    );
}

#[test]
fn serializes_non_s3_provider_import_builders() {
    assert_eq!(
        serde_json::to_value(TaskRequest::from(
            AzureBlobImportTask::new("storage-account", "documents")
                .storage_access_key("storage-key")
                .sas_token("sas-token")
                .blob("incoming/report.pdf")
                .blob_prefix("incoming/")
                .filename("report.pdf")
        ))
        .unwrap(),
        json!({
            "operation": "import/azure/blob",
            "storage_account": "storage-account",
            "storage_access_key": "storage-key",
            "sas_token": "sas-token",
            "container": "documents",
            "blob": "incoming/report.pdf",
            "blob_prefix": "incoming/",
            "filename": "report.pdf"
        })
    );

    assert_eq!(
        serde_json::to_value(TaskRequest::from(
            GoogleCloudStorageImportTask::new(
                "project-id",
                "source-bucket",
                "client@example.test",
                "private-key",
            )
            .file("incoming/report.pdf")
            .file_prefix("incoming/")
            .filename("report.pdf")
        ))
        .unwrap(),
        json!({
            "operation": "import/google-cloud-storage",
            "project_id": "project-id",
            "bucket": "source-bucket",
            "client_email": "client@example.test",
            "private_key": "private-key",
            "file": "incoming/report.pdf",
            "file_prefix": "incoming/",
            "filename": "report.pdf"
        })
    );

    assert_eq!(
        serde_json::to_value(TaskRequest::from(
            OpenStackImportTask::new(
                "https://openstack.example.test/auth",
                "username",
                "password",
                "region-one",
                "documents",
            )
            .file("incoming/report.pdf")
            .file_prefix("incoming/")
            .filename("report.pdf")
        ))
        .unwrap(),
        json!({
            "operation": "import/openstack",
            "auth_url": "https://openstack.example.test/auth",
            "username": "username",
            "password": "password",
            "region": "region-one",
            "container": "documents",
            "file": "incoming/report.pdf",
            "file_prefix": "incoming/",
            "filename": "report.pdf"
        })
    );

    assert_eq!(
        serde_json::to_value(TaskRequest::from(
            SftpImportTask::new("sftp.example.test", "username")
                .port(2222)
                .password("password")
                .private_key("private-key")
                .file("incoming/report.pdf")
                .path("/incoming")
                .filename("report.pdf")
        ))
        .unwrap(),
        json!({
            "operation": "import/sftp",
            "host": "sftp.example.test",
            "port": 2222,
            "username": "username",
            "password": "password",
            "private_key": "private-key",
            "file": "incoming/report.pdf",
            "path": "/incoming",
            "filename": "report.pdf"
        })
    );
}

#[test]
fn serializes_non_s3_provider_export_builders() {
    assert_eq!(
        serde_json::to_value(TaskRequest::from(
            AzureBlobExportTask::new("convert-file", "storage-account", "documents")
                .storage_access_key("storage-key")
                .sas_token("sas-token")
                .blob("exports/report.pdf")
                .blob_prefix("exports/")
                .metadata("origin", "cloudconvert-sdk")
                .option("content_type", "application/pdf")
        ))
        .unwrap(),
        json!({
            "operation": "export/azure/blob",
            "input": "convert-file",
            "storage_account": "storage-account",
            "storage_access_key": "storage-key",
            "sas_token": "sas-token",
            "container": "documents",
            "blob": "exports/report.pdf",
            "blob_prefix": "exports/",
            "metadata": {
                "origin": "cloudconvert-sdk"
            },
            "content_type": "application/pdf"
        })
    );

    assert_eq!(
        serde_json::to_value(TaskRequest::from(
            GoogleCloudStorageExportTask::new(
                "convert-file",
                "project-id",
                "target-bucket",
                "client@example.test",
                "private-key",
            )
            .file("exports/report.pdf")
            .file_prefix("exports/")
            .option("predefined_acl", "publicRead")
        ))
        .unwrap(),
        json!({
            "operation": "export/google-cloud-storage",
            "input": "convert-file",
            "project_id": "project-id",
            "bucket": "target-bucket",
            "client_email": "client@example.test",
            "private_key": "private-key",
            "file": "exports/report.pdf",
            "file_prefix": "exports/",
            "predefined_acl": "publicRead"
        })
    );

    assert_eq!(
        serde_json::to_value(TaskRequest::from(
            OpenStackExportTask::new(
                "convert-file",
                "https://openstack.example.test/auth",
                "username",
                "password",
                "region-one",
                "documents",
            )
            .file("exports/report.pdf")
            .file_prefix("exports/")
            .option("content_type", "application/pdf")
        ))
        .unwrap(),
        json!({
            "operation": "export/openstack",
            "input": "convert-file",
            "auth_url": "https://openstack.example.test/auth",
            "username": "username",
            "password": "password",
            "region": "region-one",
            "container": "documents",
            "file": "exports/report.pdf",
            "file_prefix": "exports/",
            "content_type": "application/pdf"
        })
    );

    assert_eq!(
        serde_json::to_value(TaskRequest::from(
            SftpExportTask::new("convert-file", "sftp.example.test", "username")
                .port(2222)
                .password("password")
                .private_key("private-key")
                .file("exports/report.pdf")
                .path("/exports")
                .option("create_dirs", true)
        ))
        .unwrap(),
        json!({
            "operation": "export/sftp",
            "input": "convert-file",
            "host": "sftp.example.test",
            "port": 2222,
            "username": "username",
            "password": "password",
            "private_key": "private-key",
            "file": "exports/report.pdf",
            "path": "/exports",
            "create_dirs": true
        })
    );
}

#[test]
fn serializes_processing_builders_with_extra_options() {
    let convert = TaskRequest::from(
        ConvertTask::new("import-file", FileExtension::Pdf)
            .input_format(".DOCX")
            .engine("office")
            .engine_version("1.0")
            .filename("converted.pdf")
            .timeout(120)
            .option("pdf_a", true),
    );

    assert_eq!(
        serde_json::to_value(convert).unwrap(),
        json!({
            "operation": "convert",
            "input": "import-file",
            "input_format": "docx",
            "output_format": "pdf",
            "engine": "office",
            "engine_version": "1.0",
            "filename": "converted.pdf",
            "timeout": 120,
            "pdf_a": true
        })
    );

    let optimize = TaskRequest::from(
        OptimizeTask::new("import-file")
            .input_format(FileExtension::Pdf)
            .profile("web")
            .quality(80)
            .engine("qpdf")
            .engine_version("1.0")
            .filename("optimized.pdf")
            .timeout(90)
            .option("linearize", true),
    );

    assert_eq!(
        serde_json::to_value(optimize).unwrap(),
        json!({
            "operation": "optimize",
            "input": "import-file",
            "input_format": "pdf",
            "profile": "web",
            "quality": 80,
            "engine": "qpdf",
            "engine_version": "1.0",
            "filename": "optimized.pdf",
            "timeout": 90,
            "linearize": true
        })
    );

    let capture = TaskRequest::from(
        CaptureWebsiteTask::new("https://example.test", ".PDF")
            .engine("chrome")
            .engine_version("120")
            .filename("capture.pdf")
            .timeout(60)
            .option("display_header_footer", true)
            .option("margin_top", 20),
    );

    assert_eq!(
        serde_json::to_value(capture).unwrap(),
        json!({
            "operation": "capture-website",
            "url": "https://example.test",
            "output_format": "pdf",
            "engine": "chrome",
            "engine_version": "120",
            "filename": "capture.pdf",
            "timeout": 60,
            "display_header_footer": true,
            "margin_top": 20
        })
    );
}

#[test]
fn serializes_thumbnail_metadata_merge_archive_and_command_builders() {
    assert_eq!(
        serde_json::to_value(TaskRequest::from(
            ThumbnailTask::new("video-file", FileExtension::Jpg)
                .input_format(FileExtension::Mp4)
                .engine("ffmpeg")
                .engine_version("6")
                .width(320)
                .height(180)
                .dimensions(320, 180)
                .fit("crop")
                .count(3)
                .timestamp("00:00:05")
                .filename("thumb.jpg")
                .timeout(30)
                .option("alpha", true)
        ))
        .unwrap(),
        json!({
            "operation": "thumbnail",
            "input": "video-file",
            "input_format": "mp4",
            "output_format": "jpg",
            "engine": "ffmpeg",
            "engine_version": "6",
            "width": 320,
            "height": 180,
            "fit": "crop",
            "count": 3,
            "timestamp": "00:00:05",
            "filename": "thumb.jpg",
            "timeout": 30,
            "alpha": true
        })
    );

    assert_eq!(
        serde_json::to_value(TaskRequest::from(
            MetadataTask::new("import-file")
                .input_format(".PDF")
                .engine("exiftool")
                .engine_version("12")
                .timeout(30)
                .option("include_raw", true)
        ))
        .unwrap(),
        json!({
            "operation": "metadata",
            "input": "import-file",
            "input_format": "pdf",
            "engine": "exiftool",
            "engine_version": "12",
            "timeout": 30,
            "include_raw": true
        })
    );

    assert_eq!(
        serde_json::to_value(TaskRequest::from(
            MergeTask::new(vec!["first-file", "second-file"], FileExtension::Pdf)
                .engine("qpdf")
                .engine_version("11")
                .filename("merged.pdf")
                .timeout(60)
                .option("bookmarks", true)
        ))
        .unwrap(),
        json!({
            "operation": "merge",
            "input": ["first-file", "second-file"],
            "output_format": "pdf",
            "engine": "qpdf",
            "engine_version": "11",
            "filename": "merged.pdf",
            "timeout": 60,
            "bookmarks": true
        })
    );

    assert_eq!(
        serde_json::to_value(TaskRequest::from(
            ArchiveTask::new(vec!["first-file", "second-file"], ".ZIP")
                .engine("zip")
                .engine_version("3")
                .filename("archive.zip")
                .timeout(60)
                .option("compression_level", 9)
        ))
        .unwrap(),
        json!({
            "operation": "archive",
            "input": ["first-file", "second-file"],
            "output_format": "zip",
            "engine": "zip",
            "engine_version": "3",
            "filename": "archive.zip",
            "timeout": 60,
            "compression_level": 9
        })
    );

    assert_eq!(
        serde_json::to_value(TaskRequest::from(
            CommandTask::new("import-file", "imagemagick", "convert", "$input $output")
                .engine_version("7")
                .capture_output(true)
                .timeout(60)
                .option("output_format", "png")
        ))
        .unwrap(),
        json!({
            "operation": "command",
            "input": "import-file",
            "engine": "imagemagick",
            "command": "convert",
            "arguments": "$input $output",
            "engine_version": "7",
            "capture_output": true,
            "timeout": 60,
            "output_format": "png"
        })
    );
}

#[test]
fn serializes_metadata_write_builder() {
    let mut metadata = BTreeMap::new();
    metadata.insert("Author".to_string(), json!("Richard Hendricks"));
    metadata.insert("Producer".to_string(), json!("CloudConvert"));

    let task = TaskRequest::from(
        MetadataWriteTask::new("import-file")
            .input_format(FileExtension::Pdf)
            .engine("exiftool")
            .engine_version("12")
            .metadata("Ignored", "overwritten")
            .metadata_map(metadata)
            .filename("with-metadata.pdf")
            .timeout(30)
            .option("overwrite", true),
    );

    assert_eq!(
        serde_json::to_value(task).unwrap(),
        json!({
            "operation": "metadata/write",
            "input": "import-file",
            "input_format": "pdf",
            "engine": "exiftool",
            "engine_version": "12",
            "metadata": {
                "Author": "Richard Hendricks",
                "Producer": "CloudConvert"
            },
            "filename": "with-metadata.pdf",
            "timeout": 30,
            "overwrite": true
        })
    );
}

#[test]
fn serializes_pdf_and_export_upload_convenience_builders() {
    for (task, operation) in [
        (TaskRequest::pdf_a("import-file"), "pdf/a"),
        (TaskRequest::pdf_x("import-file"), "pdf/x"),
        (TaskRequest::pdf_encrypt("import-file"), "pdf/encrypt"),
        (TaskRequest::pdf_decrypt("import-file"), "pdf/decrypt"),
        (
            TaskRequest::pdf_split_pages("import-file"),
            "pdf/split-pages",
        ),
        (
            TaskRequest::pdf_extract_pages("import-file"),
            "pdf/extract-pages",
        ),
    ] {
        assert_eq!(
            serde_json::to_value(task).unwrap(),
            json!({
                "operation": operation,
                "input": "import-file"
            })
        );
    }

    assert_eq!(
        serde_json::to_value(TaskRequest::pdf_ocr("import-file")).unwrap(),
        json!({
            "operation": "pdf/ocr",
            "input": "import-file"
        })
    );

    assert_eq!(
        serde_json::to_value(TaskRequest::pdf_rotate_pages(vec!["pdf-a", "pdf-b"])).unwrap(),
        json!({
            "operation": "pdf/rotate-pages",
            "input": ["pdf-a", "pdf-b"]
        })
    );

    assert_eq!(
        serde_json::to_value(TaskRequest::from(
            PdfATask::new("import-file")
                .engine("qpdf")
                .engine_version("11")
                .filename("pdf-a.pdf")
                .timeout(60)
                .option("level", "3b")
        ))
        .unwrap(),
        json!({
            "operation": "pdf/a",
            "input": "import-file",
            "engine": "qpdf",
            "engine_version": "11",
            "filename": "pdf-a.pdf",
            "timeout": 60,
            "level": "3b"
        })
    );

    assert_eq!(
        serde_json::to_value(TaskRequest::from(
            ExportUrlTask::new(vec!["first-file", "second-file"])
                .inline(false)
                .archive_multiple_files(true)
        ))
        .unwrap(),
        json!({
            "operation": "export/url",
            "input": ["first-file", "second-file"],
            "inline": false,
            "archive_multiple_files": true
        })
    );

    assert_eq!(
        serde_json::to_value(TaskRequest::from(
            ExportUploadTask::new("convert-file", "https://upload.example.test/report.pdf")
                .header("Authorization", "Bearer token")
                .option("method", "PUT")
        ))
        .unwrap(),
        json!({
            "operation": "export/upload",
            "input": "convert-file",
            "url": "https://upload.example.test/report.pdf",
            "headers": {
                "Authorization": "Bearer token"
            },
            "method": "PUT"
        })
    );
}

#[test]
fn serializes_remaining_watermark_options() {
    assert_eq!(
        serde_json::to_value(TaskRequest::watermark(
            WatermarkTask::text("import-file", "Draft")
                .pages("1-3")
                .font_width_percent(80)
                .font_align_left()
                .font_align_center()
                .font_align_right()
                .filename("watermarked.pdf")
                .engine("poppler")
                .engine_version("24")
                .timeout(120)
                .option("custom", true)
        ))
        .unwrap(),
        json!({
            "operation": "watermark",
            "input": "import-file",
            "pages": "1-3",
            "text": "Draft",
            "font_width_percent": 80,
            "font_align": "right",
            "filename": "watermarked.pdf",
            "engine": "poppler",
            "engine_version": "24",
            "timeout": 120,
            "custom": true
        })
    );

    assert_eq!(
        serde_json::to_value(TaskRequest::from(
            WatermarkTask::image("import-file", "logo-file")
                .image_height(120)
                .image_width_percent(50)
        ))
        .unwrap(),
        json!({
            "operation": "watermark",
            "input": "import-file",
            "image": "logo-file",
            "image_height": 120,
            "image_width_percent": 50
        })
    );
}
