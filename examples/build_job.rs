use cloudconvert_sdk::{FileExtension, JobCreateRequest};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let request = JobCreateRequest::linear()
        .import_url("https://example.test/input.docx")
        .convert(FileExtension::Pdf)
        .export_url()
        .build();

    println!("{}", serde_json::to_string_pretty(&request)?);

    Ok(())
}
