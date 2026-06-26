use std::collections::BTreeMap;

use cloudconvert_sdk::{
    ArchiveTask, AzureBlobExportTask, AzureBlobImportTask, Base64ImportTask, CaptureWebsiteTask,
    CommandTask, ConvertTask, ExportUploadTask, ExportUrlTask, FileExtension,
    GoogleCloudStorageExportTask, GoogleCloudStorageImportTask, ImportUploadTask, ImportUrlTask,
    Input, InvalidBuilderStateKind, JobCreateRequest, JobGetQuery, JobGraphBuilder, JobListQuery,
    JobStatus, Layer, MergeTask, MetadataTask, MetadataWriteTask, OpenStackExportTask,
    OpenStackImportTask, OperationListQuery, OptimizeTask, PdfATask, PositionHorizontal,
    PositionVertical, RawImportTask, S3ExportTask, S3ImportTask, SftpExportTask, SftpImportTask,
    TaskGetQuery, TaskListQuery, TaskName, TaskRequest, TaskStatus, ThumbnailTask, WatermarkTask,
    WebhookCreateRequest, WebhookEvent, WebhookListQuery,
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
fn duplicate_explicit_task_names_are_suffixed_not_overwritten() {
    let job = JobCreateRequest::builder()
        .task(
            "import-url",
            ImportUrlTask::new("https://example.test/a.docx"),
        )
        .task(
            "import-url",
            ImportUrlTask::new("https://example.test/b.docx"),
        )
        .build();

    let payload = serde_json::to_value(&job).unwrap();
    let tasks = payload["tasks"].as_object().unwrap();
    assert_eq!(tasks.len(), 2);
    assert_eq!(tasks["import-url"]["url"], "https://example.test/a.docx");
    assert_eq!(tasks["import-url-2"]["url"], "https://example.test/b.docx");
}

#[test]
fn duplicate_named_task_handle_reflects_suffixed_name() {
    let mut builder = JobCreateRequest::builder();
    let first = builder.add_named_task("step", ImportUrlTask::new("https://example.test/a.docx"));
    let second = builder.add_named_task("step", ImportUrlTask::new("https://example.test/b.docx"));
    assert_eq!(first.as_str(), "step");
    assert_eq!(second.as_str(), "step-2");

    let payload = serde_json::to_value(builder.build()).unwrap();
    assert_eq!(payload["tasks"].as_object().unwrap().len(), 2);
}

#[test]
fn task_option_cannot_overwrite_canonical_fields() {
    // ConvertTask: option() must not replace input/output_format wiring.
    let convert = TaskRequest::from(
        ConvertTask::new("import-url", "pdf")
            .option("input", "wrong-task")
            .option("output_format", "exe")
            .option("ui_group", "extra"),
    );
    let payload = serde_json::to_value(&convert).unwrap();
    assert_eq!(payload["input"], "import-url");
    assert_eq!(payload["output_format"], "pdf");
    assert_eq!(payload["operation"], "convert");
    assert_eq!(payload["ui_group"], "extra");

    // PdfATask is generated by the pdf_task! macro; the same guard must hold.
    let pdf = TaskRequest::from(
        PdfATask::new("import-file")
            .engine("ghostscript")
            .option("input", "attacker-task")
            .option("engine", "evil"),
    );
    let pdf_payload = serde_json::to_value(&pdf).unwrap();
    assert_eq!(pdf_payload["input"], "import-file");
    assert_eq!(pdf_payload["engine"], "ghostscript");
    assert_eq!(pdf_payload["operation"], "pdf/a");
}

#[test]
fn job_option_cannot_overwrite_reserved_core_fields() {
    let job = JobCreateRequest::builder()
        .tag("real-tag")
        .task(
            "import-file",
            ImportUrlTask::new("https://example.test/input.docx"),
        )
        .task("convert-file", ConvertTask::new("import-file", "pdf"))
        .option(
            "tasks",
            json!({ "evil": { "operation": "import/url", "url": "https://attacker.test" } }),
        )
        .option("tag", "spoofed-tag")
        .option("webhook_url", "https://attacker.test/hook")
        .option("redirect", true)
        .build();

    let payload = serde_json::to_value(job).unwrap();
    // Reserved keys from option() must not replace the built task graph or tag.
    assert_eq!(payload["tasks"]["import-file"]["operation"], "import/url");
    assert_eq!(payload["tasks"]["convert-file"]["operation"], "convert");
    assert!(payload["tasks"].get("evil").is_none());
    assert_eq!(payload["tag"], "real-tag");
    assert!(payload.get("webhook_url").is_none());
    assert!(payload.get("redirect").is_none());
}

#[test]
fn graph_option_cannot_overwrite_reserved_core_fields() {
    let job = JobCreateRequest::graph(|job| {
        let import = job.add_task(ImportUrlTask::new("https://example.test/input.docx"));
        job.add_task(ConvertTask::new(&import, "pdf"));
        job.option(
            "tasks",
            json!({ "evil": { "operation": "import/url", "url": "https://attacker.test" } }),
        );
    })
    .build();

    let payload = serde_json::to_value(job).unwrap();
    assert!(payload["tasks"].get("evil").is_none());
    assert!(
        payload["tasks"]
            .as_object()
            .is_some_and(|tasks| tasks.values().any(|t| t["operation"] == "import/url"))
    );
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
fn job_builder_task_shorthands_chain_using_previous_task_input() -> cloudconvert_sdk::Result<()> {
    let job = JobCreateRequest::builder()
        .tag("shortcut-demo")
        .import_url("https://example.test/input.docx")
        .convert(FileExtension::Pdf)?
        .export_url()?
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
    Ok(())
}

#[test]
fn job_linear_shorthands_configure_tasks_without_manual_task_constructors()
-> cloudconvert_sdk::Result<()> {
    let job = JobCreateRequest::linear()
        .tag("configured-linear-demo")
        .import_url_with("https://example.test/input.docx", |task| {
            task.filename("input.docx")
        })
        .convert_with(FileExtension::Pdf, |task| {
            task.input_format(".DOCX")
                .engine("office")
                .filename("converted.pdf")
        })?
        .optimize_with(|task| {
            task.input_format(FileExtension::Pdf)
                .profile("print")
                .filename("converted-optimized.pdf")
        })?
        .export_url_with(|task| task.inline(false))?
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
    Ok(())
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
fn task_name_conversions_cover_owned_and_borrowed_inputs() {
    let mut builder = JobCreateRequest::builder();
    let first: TaskName =
        builder.add_task(TaskRequest::import_url("https://example.test/first.pdf"));
    let second: TaskName =
        builder.add_task(TaskRequest::import_url("https://example.test/second.pdf"));

    assert_eq!(first.as_ref(), "import-url");
    assert_eq!(String::from(first.clone()), "import-url");
    assert_eq!(String::from(&second), "import-url-2");
    assert_eq!(
        Input::from(first.clone()),
        Input::Task("import-url".to_string())
    );
    assert_eq!(
        Input::from(&second),
        Input::Task("import-url-2".to_string())
    );
    assert_eq!(
        Input::from(vec![first.clone(), second.clone()]),
        Input::Tasks(vec!["import-url".to_string(), "import-url-2".to_string()])
    );
    assert_eq!(
        Input::from(vec![&first, &second]),
        Input::Tasks(vec!["import-url".to_string(), "import-url-2".to_string()])
    );
    assert_eq!(
        Input::from([first.clone(), second.clone()]),
        Input::Tasks(vec!["import-url".to_string(), "import-url-2".to_string()])
    );
    assert_eq!(
        Input::from([&first, &second]),
        Input::Tasks(vec!["import-url".to_string(), "import-url-2".to_string()])
    );
}

#[test]
fn job_builder_covers_linear_shortcut_surface() -> cloudconvert_sdk::Result<()> {
    let job = JobCreateRequest::linear()
        .tag("all-linear-shortcuts")
        .webhook_url("https://example.test/hook")
        .redirect(false)
        .option("job_mode", "coverage")
        .import_url("https://example.test/input.docx")
        .import_upload()
        .import_base64("UERG", "input.pdf")
        .import_raw("%PDF-1.7", "raw.pdf")
        .import_s3("source-bucket", "eu-central-1", "access-id", "secret-key")
        .import_azure_blob("storage-account", "documents")
        .import_google_cloud_storage(
            "project-id",
            "source-bucket",
            "client@example.test",
            "private-key",
        )
        .import_openstack(
            "https://openstack.example.test/auth",
            "username",
            "password",
            "region-one",
            "documents",
        )
        .import_sftp("sftp.example.test", "username")
        .convert(FileExtension::Pdf)?
        .convert_with_input_format(FileExtension::Pdf, FileExtension::Png)?
        .optimize()?
        .watermark_text("Draft")?
        .watermark_image("logo-file")?
        .capture_website("https://example.test", FileExtension::Pdf)
        .thumbnail(FileExtension::Jpg)?
        .metadata()?
        .metadata_write()?
        .merge(FileExtension::Pdf)?
        .archive(FileExtension::Zip)?
        .command("imagemagick", "convert", "$input $output")?
        .pdf_a()?
        .pdf_x()?
        .pdf_ocr()?
        .pdf_encrypt()?
        .pdf_decrypt()?
        .pdf_split_pages()?
        .pdf_extract_pages()?
        .pdf_rotate_pages()?
        .export_url()?
        .export_s3("target-bucket", "eu-central-1", "access-id", "secret-key")?
        .export_azure_blob("storage-account", "documents")?
        .export_google_cloud_storage(
            "project-id",
            "target-bucket",
            "client@example.test",
            "private-key",
        )?
        .export_openstack(
            "https://openstack.example.test/auth",
            "username",
            "password",
            "region-one",
            "documents",
        )?
        .export_sftp("sftp.example.test", "username")?
        .export_upload("https://upload.example.test/report.pdf")?
        .build();

    let payload = serde_json::to_value(job).unwrap();
    let tasks = payload["tasks"].as_object().unwrap();

    assert_eq!(payload["tag"], "all-linear-shortcuts");
    assert_eq!(payload["webhook_url"], "https://example.test/hook");
    assert_eq!(payload["redirect"], false);
    assert_eq!(payload["job_mode"], "coverage");
    assert_eq!(tasks.len(), 36);
    assert_eq!(tasks["import-url"]["operation"], "import/url");
    assert_eq!(tasks["import-upload"]["operation"], "import/upload");
    assert_eq!(tasks["import-base64"]["operation"], "import/base64");
    assert_eq!(tasks["import-raw"]["operation"], "import/raw");
    assert_eq!(tasks["import-s3"]["operation"], "import/s3");
    assert_eq!(tasks["import-azure-blob"]["operation"], "import/azure/blob");
    assert_eq!(
        tasks["import-google-cloud-storage"]["operation"],
        "import/google-cloud-storage"
    );
    assert_eq!(tasks["import-openstack"]["operation"], "import/openstack");
    assert_eq!(tasks["import-sftp"]["operation"], "import/sftp");
    assert_eq!(tasks["convert"]["input"], "import-sftp");
    assert_eq!(tasks["convert-2"]["input_format"], "pdf");
    assert_eq!(tasks["optimize"]["input"], "convert-2");
    assert_eq!(tasks["watermark"]["input"], "optimize");
    assert_eq!(tasks["watermark-2"]["image"], "logo-file");
    assert_eq!(tasks["capture-website"]["operation"], "capture-website");
    assert_eq!(tasks["thumbnail"]["input"], "capture-website");
    assert_eq!(tasks["metadata"]["input"], "thumbnail");
    assert_eq!(tasks["metadata-write"]["input"], "metadata");
    assert_eq!(tasks["merge"]["input"], "metadata-write");
    assert_eq!(tasks["archive"]["input"], "merge");
    assert_eq!(tasks["command"]["input"], "archive");
    assert_eq!(tasks["pdf-a"]["input"], "command");
    assert_eq!(tasks["pdf-rotate-pages"]["input"], "pdf-extract-pages");
    assert_eq!(tasks["export-url"]["input"], "pdf-rotate-pages");
    assert_eq!(tasks["export-s3"]["input"], "export-url");
    assert_eq!(tasks["export-azure-blob"]["input"], "export-s3");
    assert_eq!(
        tasks["export-google-cloud-storage"]["input"],
        "export-azure-blob"
    );
    assert_eq!(
        tasks["export-openstack"]["input"],
        "export-google-cloud-storage"
    );
    assert_eq!(tasks["export-sftp"]["input"], "export-openstack");
    assert_eq!(tasks["export-upload"]["input"], "export-sftp");
    Ok(())
}

#[test]
fn job_builder_covers_configured_linear_shortcut_surface() -> cloudconvert_sdk::Result<()> {
    let job = JobCreateRequest::linear()
        .import_url_with("https://example.test/input.docx", |task| {
            task.filename("input.docx")
        })
        .import_upload_with(|task| task.redirect("https://example.test/upload-complete"))
        .import_s3_with(
            "source-bucket",
            "eu-central-1",
            "access-id",
            "secret-key",
            |task| task.key("incoming/report.pdf"),
        )
        .import_azure_blob_with("storage-account", "documents", |task| {
            task.blob("incoming/report.pdf")
        })
        .import_google_cloud_storage(
            "project-id",
            "source-bucket",
            "client@example.test",
            "private-key",
        )
        .import_google_cloud_storage_with(
            "project-id",
            "source-bucket",
            "client@example.test",
            "private-key",
            |task| task.file("incoming/report.pdf"),
        )
        .import_openstack_with(
            "https://openstack.example.test/auth",
            "username",
            "password",
            "region-one",
            "documents",
            |task| task.file("incoming/report.pdf"),
        )
        .import_sftp_with("sftp.example.test", "username", |task| {
            task.path("/incoming")
        })
        .convert_with(FileExtension::Pdf, |task| {
            task.input_format(FileExtension::Docx)
                .filename("converted.pdf")
        })?
        .optimize_with(|task| task.profile("print").quality(90))?
        .watermark_text_with("Draft", |task| task.opacity(40))?
        .watermark_image_with("logo-file", |task| task.image_width(200))?
        .capture_website_with("https://example.test", FileExtension::Pdf, |task| {
            task.filename("capture.pdf")
        })
        .thumbnail_with(FileExtension::Jpg, |task| task.dimensions(320, 180))?
        .metadata_with(|task| task.option("include_raw", true))?
        .metadata_write_with(|task| task.metadata("Author", "CloudConvert"))?
        .merge_with(FileExtension::Pdf, |task| task.filename("merged.pdf"))?
        .archive_with(FileExtension::Zip, |task| task.filename("archive.zip"))?
        .command_with("imagemagick", "convert", "$input $output", |task| {
            task.capture_output(true)
        })?
        .pdf_a_with(|task| task.option("level", "3b"))?
        .pdf_x_with(|task| task.option("standard", "PDF/X-4"))?
        .pdf_ocr_with(|task| task.option("language", "eng"))?
        .pdf_encrypt_with(|task| task.option("user_password", "secret"))?
        .pdf_decrypt_with(|task| task.option("password", "secret"))?
        .pdf_split_pages_with(|task| task.option("pages", "1-3"))?
        .pdf_extract_pages_with(|task| task.option("pages", "1"))?
        .pdf_rotate_pages_with(|task| task.option("angle", 90))?
        .export_url_with(|task| task.inline(false))?
        .export_s3_with(
            "target-bucket",
            "eu-central-1",
            "access-id",
            "secret-key",
            |task| task.key("exports/report.pdf"),
        )?
        .export_azure_blob_with("storage-account", "documents", |task| {
            task.blob("exports/report.pdf")
        })?
        .export_google_cloud_storage_with(
            "project-id",
            "target-bucket",
            "client@example.test",
            "private-key",
            |task| task.file("exports/report.pdf"),
        )?
        .export_openstack_with(
            "https://openstack.example.test/auth",
            "username",
            "password",
            "region-one",
            "documents",
            |task| task.file("exports/report.pdf"),
        )?
        .export_sftp_with("sftp.example.test", "username", |task| {
            task.path("/exports")
        })?
        .export_upload_with("https://upload.example.test/report.pdf", |task| {
            task.header("Authorization", "Bearer token")
        })?
        .build();

    let payload = serde_json::to_value(job).unwrap();
    let tasks = payload["tasks"].as_object().unwrap();

    assert_eq!(tasks["import-url"]["filename"], "input.docx");
    assert_eq!(
        tasks["import-upload"]["redirect"],
        "https://example.test/upload-complete"
    );
    assert_eq!(tasks["import-s3"]["key"], "incoming/report.pdf");
    assert_eq!(tasks["import-azure-blob"]["blob"], "incoming/report.pdf");
    assert_eq!(
        tasks["import-google-cloud-storage-2"]["file"],
        "incoming/report.pdf"
    );
    assert_eq!(tasks["import-openstack"]["file"], "incoming/report.pdf");
    assert_eq!(tasks["import-sftp"]["path"], "/incoming");
    assert_eq!(tasks["convert"]["input"], "import-sftp");
    assert_eq!(tasks["convert"]["filename"], "converted.pdf");
    assert_eq!(tasks["optimize"]["quality"], 90);
    assert_eq!(tasks["watermark"]["opacity"], 40);
    assert_eq!(tasks["watermark-2"]["image_width"], 200);
    assert_eq!(tasks["capture-website"]["filename"], "capture.pdf");
    assert_eq!(tasks["thumbnail"]["width"], 320);
    assert_eq!(tasks["metadata"]["include_raw"], true);
    assert_eq!(
        tasks["metadata-write"]["metadata"]["Author"],
        "CloudConvert"
    );
    assert_eq!(tasks["merge"]["filename"], "merged.pdf");
    assert_eq!(tasks["archive"]["filename"], "archive.zip");
    assert_eq!(tasks["command"]["capture_output"], true);
    assert_eq!(tasks["pdf-a"]["level"], "3b");
    assert_eq!(tasks["pdf-x"]["standard"], "PDF/X-4");
    assert_eq!(tasks["pdf-ocr"]["language"], "eng");
    assert_eq!(tasks["pdf-encrypt"]["user_password"], "secret");
    assert_eq!(tasks["pdf-decrypt"]["password"], "secret");
    assert_eq!(tasks["pdf-split-pages"]["pages"], "1-3");
    assert_eq!(tasks["pdf-extract-pages"]["pages"], "1");
    assert_eq!(tasks["pdf-rotate-pages"]["angle"], 90);
    assert_eq!(tasks["export-url"]["inline"], false);
    assert_eq!(tasks["export-s3"]["key"], "exports/report.pdf");
    assert_eq!(tasks["export-azure-blob"]["blob"], "exports/report.pdf");
    assert_eq!(
        tasks["export-google-cloud-storage"]["file"],
        "exports/report.pdf"
    );
    assert_eq!(tasks["export-openstack"]["file"], "exports/report.pdf");
    assert_eq!(tasks["export-sftp"]["path"], "/exports");
    assert_eq!(
        tasks["export-upload"]["headers"]["Authorization"],
        "Bearer token"
    );
    Ok(())
}

#[test]
fn job_graph_builder_covers_shortcut_surface() {
    let mut graph = JobGraphBuilder::new();
    graph
        .tag("graph-shortcuts")
        .webhook_url("https://example.test/hook")
        .redirect(true)
        .option("graph_mode", "coverage");

    let explicit = graph.add_named_task("explicit-source", TaskRequest::import_upload());
    let custom = graph.add_task(TaskRequest::custom("!!!").field("answer", 42));
    assert_eq!(custom.as_str(), "task");

    let import_url = graph.import_url("https://example.test/input.docx");
    let import_upload = graph.import_upload();
    let import_base64 = graph.import_base64("UERG", "input.pdf");
    let import_raw = graph.import_raw("%PDF-1.7", "raw.pdf");
    let import_s3 = graph.import_s3("source-bucket", "eu-central-1", "access-id", "secret-key");
    let import_azure = graph.import_azure_blob("storage-account", "documents");
    let import_gcs = graph.import_google_cloud_storage(
        "project-id",
        "source-bucket",
        "client@example.test",
        "private-key",
    );
    let import_openstack = graph.import_openstack(
        "https://openstack.example.test/auth",
        "username",
        "password",
        "region-one",
        "documents",
    );
    let import_sftp = graph.import_sftp("sftp.example.test", "username");
    let convert = graph.convert([&explicit, &import_url], FileExtension::Pdf);
    let convert_with = graph.convert_with(&convert, FileExtension::Png, |task| {
        task.input_format(FileExtension::Pdf)
    });
    let optimize = graph.optimize(&convert_with);
    let watermark_text = graph.watermark_text(&optimize, "Draft");
    let _watermark_image = graph.watermark_image(&watermark_text, "logo-file");
    let capture = graph.capture_website("https://example.test", FileExtension::Pdf);
    let thumbnail = graph.thumbnail(&capture, FileExtension::Jpg);
    let metadata = graph.metadata(&thumbnail);
    let metadata_write = graph.metadata_write(&metadata);
    let merge = graph.merge([&metadata_write, &import_upload], FileExtension::Pdf);
    let archive = graph.archive([&merge, &import_base64, &import_raw], FileExtension::Zip);
    let command = graph.command(&archive, "imagemagick", "convert", "$input $output");
    let pdf_a = graph.pdf_a(&command);
    let pdf_x = graph.pdf_x(&pdf_a);
    let pdf_ocr = graph.pdf_ocr(&pdf_x);
    let pdf_encrypt = graph.pdf_encrypt(&pdf_ocr);
    let pdf_decrypt = graph.pdf_decrypt(&pdf_encrypt);
    let pdf_split_pages = graph.pdf_split_pages(&pdf_decrypt);
    let pdf_extract_pages = graph.pdf_extract_pages(&pdf_split_pages);
    let pdf_rotate_pages = graph.pdf_rotate_pages([&pdf_extract_pages, &import_s3]);
    let export_url = graph.export_url([&pdf_rotate_pages, &import_azure]);
    let export_s3 = graph.export_s3(
        [&export_url, &import_gcs],
        "target-bucket",
        "eu-central-1",
        "access-id",
        "secret-key",
    );
    let export_azure = graph.export_azure_blob(
        [&export_s3, &import_openstack],
        "storage-account",
        "documents",
    );
    let export_gcs = graph.export_google_cloud_storage(
        [&export_azure, &import_sftp],
        "project-id",
        "target-bucket",
        "client@example.test",
        "private-key",
    );
    let export_openstack = graph.export_openstack(
        &export_gcs,
        "https://openstack.example.test/auth",
        "username",
        "password",
        "region-one",
        "documents",
    );
    let export_sftp = graph.export_sftp(&export_openstack, "sftp.example.test", "username");
    graph.export_upload(&export_sftp, "https://upload.example.test/report.pdf");

    let job = graph.build();
    let payload = serde_json::to_value(job).unwrap();
    let tasks = payload["tasks"].as_object().unwrap();

    assert_eq!(payload["tag"], "graph-shortcuts");
    assert_eq!(payload["webhook_url"], "https://example.test/hook");
    assert_eq!(payload["redirect"], true);
    assert_eq!(payload["graph_mode"], "coverage");
    assert_eq!(tasks["explicit-source"]["operation"], "import/upload");
    assert_eq!(tasks["task"]["answer"], 42);
    assert_eq!(
        tasks["convert"]["input"],
        json!(["explicit-source", "import-url"])
    );
    assert_eq!(tasks["convert-2"]["input"], "convert");
    assert_eq!(tasks["watermark-2"]["image"], "logo-file");
    assert_eq!(
        tasks["merge"]["input"],
        json!(["metadata-write", "import-upload"])
    );
    assert_eq!(
        tasks["archive"]["input"],
        json!(["merge", "import-base64", "import-raw"])
    );
    assert_eq!(
        tasks["pdf-rotate-pages"]["input"],
        json!(["pdf-extract-pages", "import-s3"])
    );
    assert_eq!(
        tasks["export-url"]["input"],
        json!(["pdf-rotate-pages", "import-azure-blob"])
    );
    assert_eq!(
        tasks["export-s3"]["input"],
        json!(["export-url", "import-google-cloud-storage"])
    );
    assert_eq!(
        tasks["export-azure-blob"]["input"],
        json!(["export-s3", "import-openstack"])
    );
    assert_eq!(
        tasks["export-google-cloud-storage"]["input"],
        json!(["export-azure-blob", "import-sftp"])
    );
    assert_eq!(tasks["export-upload"]["input"], "export-sftp");
}

#[test]
fn job_request_debug_redacts_extra_values() {
    let job = JobCreateRequest::builder()
        .option("access_token", "secret-value")
        .task(
            "import-file",
            ImportUrlTask::new("https://example.test/input.docx"),
        )
        .build();

    let debug = format!("{job:?}");
    assert!(debug.contains("access_token"));
    assert!(debug.contains("REDACTED"));
    assert!(!debug.contains("secret-value"));
}

#[test]
fn job_builder_shorthand_reports_typed_error_without_previous_task() {
    let error = JobCreateRequest::linear()
        .convert(FileExtension::Pdf)
        .unwrap_err();

    let cloudconvert_sdk::Error::InvalidBuilderState(state) = error else {
        panic!("expected invalid builder state error");
    };

    assert_eq!(state.kind, InvalidBuilderStateKind::MissingPreviousTask);
    assert_eq!(state.builder, "JobBuilder");
    assert_eq!(state.method, "convert");
}

#[test]
fn lifecycle_helpers_characterize_terminal_states_and_exports() {
    let job: cloudconvert_sdk::Job = serde_json::from_value(json!({
        "id": "job_1",
        "status": "finished",
        "tasks": [
            {
                "id": "task_import",
                "name": "import-file",
                "operation": "import/url",
                "status": "finished"
            },
            {
                "id": "task_export_waiting",
                "name": "export-waiting",
                "operation": "export/url",
                "status": "waiting",
                "result": {
                    "files": [
                        {
                            "filename": "not-ready.pdf",
                            "url": "https://storage.example.test/not-ready.pdf"
                        }
                    ]
                }
            },
            {
                "id": "task_export_finished",
                "name": "export-finished",
                "operation": "export/url",
                "status": "finished",
                "result": {
                    "files": [
                        {
                            "filename": "ready.pdf",
                            "url": "https://storage.example.test/ready.pdf"
                        }
                    ]
                }
            },
            {
                "id": "task_export_error",
                "name": "export-error",
                "operation": "export/url",
                "status": "error"
            }
        ]
    }))
    .unwrap();

    assert!(job.is_finished());
    assert!(job.is_terminal());
    assert_eq!(job.export_tasks().count(), 3);
    assert_eq!(job.finished_export_tasks().count(), 1);
    assert_eq!(job.export_urls()[0].filename, "ready.pdf");
    assert!(job.tasks[1].status.is_waiting());
    assert!(!job.tasks[1].is_terminal());
    assert!(job.tasks[2].is_finished());
    assert!(job.tasks[3].is_error());
    assert!(job.tasks[3].is_terminal());

    let unknown: cloudconvert_sdk::TaskStatus = serde_json::from_value(json!("paused")).unwrap();
    assert_eq!(unknown, cloudconvert_sdk::TaskStatus::Unknown);
    assert!(!unknown.is_terminal());
}

#[test]
fn status_helpers_and_response_debug_cover_non_terminal_states() {
    for (status, waiting, processing, finished, error, terminal) in [
        (JobStatus::Waiting, true, false, false, false, false),
        (JobStatus::Processing, false, true, false, false, false),
        (JobStatus::Finished, false, false, true, false, true),
        (JobStatus::Error, false, false, false, true, true),
        (JobStatus::Unknown, false, false, false, false, false),
    ] {
        assert_eq!(status.is_waiting(), waiting);
        assert_eq!(status.is_processing(), processing);
        assert_eq!(status.is_finished(), finished);
        assert_eq!(status.is_error(), error);
        assert_eq!(status.is_terminal(), terminal);
    }

    for (status, waiting, queued, processing, finished, error, terminal) in [
        (TaskStatus::Waiting, true, false, false, false, false, false),
        (TaskStatus::Queued, false, true, false, false, false, false),
        (
            TaskStatus::Processing,
            false,
            false,
            true,
            false,
            false,
            false,
        ),
        (TaskStatus::Finished, false, false, false, true, false, true),
        (TaskStatus::Error, false, false, false, false, true, true),
        (
            TaskStatus::Unknown,
            false,
            false,
            false,
            false,
            false,
            false,
        ),
    ] {
        assert_eq!(status.is_waiting(), waiting);
        assert_eq!(status.is_queued(), queued);
        assert_eq!(status.is_processing(), processing);
        assert_eq!(status.is_finished(), finished);
        assert_eq!(status.is_error(), error);
        assert_eq!(status.is_terminal(), terminal);
    }

    let job: cloudconvert_sdk::Job = serde_json::from_value(json!({
        "id": "job_error",
        "tag": "debug-demo",
        "status": "error",
        "created_at": "2026-06-02T00:00:00+00:00",
        "started_at": "2026-06-02T00:00:01+00:00",
        "ended_at": "2026-06-02T00:00:02+00:00",
        "links": {
            "self": "https://api.example.test/v2/jobs/job_error"
        },
        "secret_extra": "job-secret",
        "tasks": [
            {
                "id": "task_error",
                "name": "convert-file",
                "operation": "convert",
                "status": "error",
                "message": "conversion failed",
                "code": "CONVERT_FAILED",
                "credits": 1.5,
                "created_at": "2026-06-02T00:00:00+00:00",
                "started_at": "2026-06-02T00:00:01+00:00",
                "ended_at": "2026-06-02T00:00:02+00:00",
                "payload": {
                    "access_key": "task-payload-secret"
                },
                "secret_extra": "task-extra-secret"
            }
        ]
    }))
    .unwrap();

    assert!(job.is_error());
    assert!(job.is_terminal());
    let job_debug = format!("{job:?}");
    assert!(job_debug.contains("secret_extra"));
    assert!(job_debug.contains("REDACTED"));
    assert!(!job_debug.contains("job-secret"));

    let task = &job.tasks[0];
    assert!(task.is_error());
    assert!(task.is_terminal());
    assert!(!task.is_export_url());
    assert_eq!(task.files().count(), 0);
    let task_debug = format!("{task:?}");
    assert!(task_debug.contains("payload"));
    assert!(task_debug.contains("REDACTED"));
    assert!(!task_debug.contains("task-payload-secret"));
    assert!(!task_debug.contains("task-extra-secret"));

    let task: cloudconvert_sdk::Task = serde_json::from_value(json!({
        "id": "task_export",
        "job_id": "job_1",
        "operation": "export/url",
        "status": "finished",
        "message": "done",
        "code": null,
        "credits": 0.5,
        "created_at": "2026-06-02T00:00:00+00:00",
        "started_at": "2026-06-02T00:00:01+00:00",
        "ended_at": "2026-06-02T00:00:02+00:00",
        "depends_on_tasks": {
            "convert-file": "task_convert"
        },
        "result": {
            "files": [
                {
                    "filename": "output.pdf",
                    "url": "https://storage.example.test/output.pdf",
                    "size": 123
                }
            ]
        },
        "payload": {
            "access_key": "standalone-task-secret"
        },
        "secret_extra": "standalone-extra-secret"
    }))
    .unwrap();

    assert!(task.is_finished());
    assert!(task.is_terminal());
    assert_eq!(task.files().next().unwrap().filename, "output.pdf");
    let task_debug = format!("{task:?}");
    assert!(task_debug.contains("REDACTED"));
    assert!(!task_debug.contains("standalone-task-secret"));
    assert!(!task_debug.contains("standalone-extra-secret"));
}

#[test]
fn upload_readiness_helpers_require_import_upload_form() {
    let ready: cloudconvert_sdk::Task = serde_json::from_value(json!({
        "id": "task_upload",
        "operation": "import/upload",
        "status": "waiting",
        "result": {
            "form": {
                "url": "https://storage.example.test/upload",
                "parameters": {
                    "signature": "fake-signature"
                }
            }
        }
    }))
    .unwrap();

    let wrong_operation: cloudconvert_sdk::Task = serde_json::from_value(json!({
        "id": "task_import",
        "operation": "import/url",
        "status": "finished",
        "result": {
            "form": {
                "url": "https://storage.example.test/upload",
                "parameters": {}
            }
        }
    }))
    .unwrap();

    let missing_form: cloudconvert_sdk::Task = serde_json::from_value(json!({
        "id": "task_upload",
        "operation": "import/upload",
        "status": "waiting",
        "result": {}
    }))
    .unwrap();

    assert!(ready.is_import_upload());
    assert!(ready.is_upload_ready());
    assert_eq!(
        ready.upload_form().unwrap().url,
        "https://storage.example.test/upload"
    );
    assert!(!wrong_operation.is_import_upload());
    assert!(!wrong_operation.is_upload_ready());
    assert!(wrong_operation.upload_form().is_none());
    assert!(missing_form.is_import_upload());
    assert!(!missing_form.is_upload_ready());
}

#[test]
fn job_builder_task_shorthands_update_inputs_after_explicit_tasks() -> cloudconvert_sdk::Result<()>
{
    let job = JobCreateRequest::builder()
        .task(
            "uploaded-source",
            ImportUrlTask::new("https://example.test/input.txt"),
        )
        .convert_with_input_format(FileExtension::Txt, FileExtension::Pdf)?
        .pdf_a()?
        .export_url()?
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
    Ok(())
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
    assert_eq!(FileExtension::Png.as_ref(), "png");
    assert_eq!(FileExtension::TarGz.to_string(), "tar.gz");
    assert_eq!(String::from(FileExtension::Docx), "docx");
    assert_eq!(String::from(&FileExtension::Jpg), "jpg");
    assert_eq!(FileExtension::ALL.len(), 213);

    assert_eq!(".PDF".parse::<FileExtension>().unwrap(), FileExtension::Pdf);
    assert_eq!(
        FileExtension::parse(" .TAR.GZ ").unwrap(),
        FileExtension::TarGz
    );

    let error = FileExtension::parse("nope").unwrap_err();
    assert_eq!(error.extension(), "nope");
    assert_eq!(
        error.to_string(),
        "unsupported CloudConvert file extension: nope"
    );
    let error_trait: &dyn std::error::Error = &error;
    assert!(error_trait.source().is_none());

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
        serde_json::to_value(JobGetQuery::default().include("tasks").redirect(true)).unwrap(),
        json!({
            "include": "tasks",
            "redirect": true
        })
    );

    assert_eq!(
        serde_json::to_value(TaskGetQuery::default().include("job")).unwrap(),
        json!({
            "include": "job"
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
