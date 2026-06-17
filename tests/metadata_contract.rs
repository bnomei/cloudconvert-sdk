use cloudconvert_sdk::{
    MetadataWriteTask, Operation, OperationOptionKind, OperationValidationErrorKind,
    PaginationLinks, PaginationMeta, TaskRequest,
};
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize)]
struct PageEnvelope<T> {
    data: Vec<T>,
    links: PaginationLinks,
    meta: PaginationMeta,
}

#[test]
fn recorded_convert_operation_fixture_preserves_contract_shape() {
    let envelope: PageEnvelope<Operation> = serde_json::from_str(include_str!(
        "fixtures/cloudconvert/operations-convert-docx-pdf.json"
    ))
    .unwrap();

    assert_eq!(envelope.data.len(), 1);
    assert_eq!(envelope.meta.current_page, Some(1));
    assert_eq!(envelope.meta.total, Some(1));
    assert_eq!(
        envelope.links.first.as_deref(),
        Some("https://api.cloudconvert.com/v2/operations?page=1")
    );

    let operation = &envelope.data[0];
    assert_eq!(operation.operation, "convert");
    assert_eq!(operation.input_format.as_deref(), Some("docx"));
    assert_eq!(operation.output_format.as_deref(), Some("pdf"));
    assert_eq!(operation.engine.as_deref(), Some("office"));

    let input_format = operation.option("input_format").unwrap();
    assert_eq!(input_format.name(), Some("input_format"));
    assert_eq!(input_format.kind(), Some(&OperationOptionKind::String));
    assert_eq!(input_format.default.as_ref(), Some(&json!("docx")));

    let pdf_a = operation.option("pdf_a").unwrap();
    assert_eq!(pdf_a.kind(), Some(&OperationOptionKind::Boolean));
    assert_eq!(pdf_a.default.as_ref(), Some(&json!(false)));

    assert_eq!(
        operation.engine_version_values().collect::<Vec<_>>(),
        vec!["2016", "2019"]
    );
    let latest = operation.latest_engine_version().unwrap();
    assert_eq!(latest.version, "2019");

    let alternative = &operation.alternatives[0];
    assert_eq!(alternative.engine.as_deref(), Some("libreoffice"));
    assert_eq!(
        alternative.engine_version_values().collect::<Vec<_>>(),
        vec!["7.6"]
    );
}

#[test]
fn recorded_metadata_operation_fixtures_cover_drift_sensitive_options() {
    let metadata: PageEnvelope<Operation> = serde_json::from_str(include_str!(
        "fixtures/cloudconvert/operations-metadata.json"
    ))
    .unwrap();
    let operation = &metadata.data[0];

    assert_eq!(operation.operation, "metadata");
    assert_eq!(
        operation.option("include_raw").unwrap().default,
        Some(json!(false))
    );
    assert_eq!(
        operation.option("format").unwrap().possible_values(),
        &[json!("json")]
    );
    assert_eq!(
        operation.meta.as_ref().unwrap()["category"],
        json!("metadata")
    );

    let metadata_write: PageEnvelope<Operation> = serde_json::from_str(include_str!(
        "fixtures/cloudconvert/operations-metadata-write.json"
    ))
    .unwrap();
    let operation = &metadata_write.data[0];

    assert_eq!(operation.operation, "metadata/write");
    let metadata_option = operation.option("metadata").unwrap();
    assert_eq!(
        metadata_option.kind(),
        Some(&OperationOptionKind::Dictionary)
    );
    assert!(metadata_option.is_required());
    assert_eq!(
        operation.option("remove").unwrap().possible_values(),
        &[
            json!("Author"),
            json!("Title"),
            json!("Subject"),
            json!("Keywords")
        ]
    );

    let valid = TaskRequest::from(MetadataWriteTask::new("import-file").metadata("Author", "SDK"));
    operation.validate_task_strict(&valid).unwrap();

    let invalid: TaskRequest = TaskRequest::custom("metadata/write")
        .field("input", "import-file")
        .field("metadata", "not-an-object")
        .into();
    let error = operation.validate_task(&invalid).unwrap_err();
    assert_eq!(error.kind, OperationValidationErrorKind::InvalidOptionType);
    assert_eq!(error.option.as_deref(), Some("metadata"));
}
