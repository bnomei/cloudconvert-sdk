use cloudconvert_sdk::{FileExtension, JobCreateRequest};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let request = JobCreateRequest::linear()
        .import_url_with("https://example.test/input.docx", |task| {
            task.filename("input.docx")
        })
        .convert_with(FileExtension::Pdf, |task| {
            task.input_format(FileExtension::Docx)
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

    println!("{}", serde_json::to_string_pretty(&request)?);

    Ok(())
}
