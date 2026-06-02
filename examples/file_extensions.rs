use cloudconvert_sdk::FileExtension;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let parsed = ".PDF".parse::<FileExtension>()?;

    println!("parsed extension: {parsed}");
    println!("known extension count: {}", FileExtension::ALL.len());

    Ok(())
}
