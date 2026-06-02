//! Builds a multi-file graph with merge, watermark, and archive export tasks.

use cloudconvert_sdk::{
    FileExtension, JobCreateRequest, Layer, PositionHorizontal, PositionVertical,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let request = JobCreateRequest::graph(|job| {
        let cover_docx = job.import_url_with("https://example.test/report-cover.docx", |task| {
            task.filename("cover.docx")
        });
        let body_docx = job.import_url_with("https://example.test/report-body.docx", |task| {
            task.filename("body.docx")
        });
        let logo_png = job.import_url_with("https://example.test/logo.png", |task| {
            task.filename("logo.png")
        });

        let cover_pdf = job.convert_with(&cover_docx, FileExtension::Pdf, |task| {
            task.input_format(FileExtension::Docx).filename("cover.pdf")
        });
        let body_pdf = job.convert_with(&body_docx, FileExtension::Pdf, |task| {
            task.input_format(FileExtension::Docx).filename("body.pdf")
        });

        let merged = job.merge_with([&cover_pdf, &body_pdf], FileExtension::Pdf, |task| {
            task.filename("report.pdf")
        });

        let watermarked = job.watermark_image_with(&merged, &logo_png, |task| {
            task.input_format(FileExtension::Pdf)
                .layer(Layer::Above)
                .image_width(180)
                .position(PositionVertical::Bottom, PositionHorizontal::Right)
                .margins(24, 24)
                .opacity(80)
                .filename("report-watermarked.pdf")
        });

        job.export_url_with([&cover_pdf, &body_pdf, &watermarked], |task| {
            task.archive_multiple_files(true)
        });
    })
    .tag("report-package")
    .build();

    println!("{}", serde_json::to_string_pretty(&request)?);

    Ok(())
}
