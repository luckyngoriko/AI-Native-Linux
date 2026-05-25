//! T-089 — tonic-build invocation that generates the gRPC `SgrService`
//! server + client stubs from `proto/aios.sgr.v1alpha1.proto`.
//!
//! Mirrors the pattern established by `crates/aios-recovery/build.rs`.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=proto/aios.sgr.v1alpha1.proto");
    println!("cargo:rerun-if-changed=build.rs");

    let protoc = protoc_bin_vendored::protoc_bin_path()?;
    std::env::set_var("PROTOC", protoc);

    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(&["proto/aios.sgr.v1alpha1.proto"], &["proto"])?;
    Ok(())
}
