//! T-110 — tonic-build invocation that generates the gRPC `SandboxService`
//! server + client stubs from `proto/aios.sandbox.v1alpha1.proto`.
//!
//! Mirrors the pattern established by `crates/aios-cognitive/build.rs` (T-101).

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=proto/aios.sandbox.v1alpha1.proto");
    println!("cargo:rerun-if-changed=build.rs");

    let protoc = protoc_bin_vendored::protoc_bin_path()?;
    std::env::set_var("PROTOC", protoc);

    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(&["proto/aios.sandbox.v1alpha1.proto"], &["proto"])?;
    Ok(())
}
