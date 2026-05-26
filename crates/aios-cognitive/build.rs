//! T-101 — tonic-build invocation that generates the gRPC `CognitiveCore`
//! server + client stubs from `proto/aios.cognitive.v1alpha1.proto`.
//!
//! Mirrors the pattern established by `crates/aios-sgr/build.rs` (T-089).

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=proto/aios.cognitive.v1alpha1.proto");
    println!("cargo:rerun-if-changed=build.rs");

    let protoc = protoc_bin_vendored::protoc_bin_path()?;
    std::env::set_var("PROTOC", protoc);

    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(&["proto/aios.cognitive.v1alpha1.proto"], &["proto"])?;
    Ok(())
}
