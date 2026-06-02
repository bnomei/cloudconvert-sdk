use cloudconvert_sdk::{FileExtension, JobCreateRequest};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let request = JobCreateRequest::graph(|job| {
        let import = job.import_url("https://example.test/input.docx");
        let pdf = job.convert(&import, FileExtension::Pdf);
        let png = job.convert(&import, FileExtension::Png);
        job.export_url([&pdf, &png]);
    })
    .tag("branch-demo")
    .build();

    println!("{}", serde_json::to_string_pretty(&request)?);

    Ok(())
}
