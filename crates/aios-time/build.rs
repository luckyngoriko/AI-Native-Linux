fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=build.rs");
    let _protoc = protoc_bin_vendored::protoc_bin_path()?;
    Ok(())
}
