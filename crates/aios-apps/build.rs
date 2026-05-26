//! T-122 — tonic-build invocation that generates the gRPC `AppsService`
//! server + client stubs from `proto/aios_apps.proto`.
//!
//! Mirrors the pattern established by `crates/aios-capability-runtime/build.rs`
//! (T-033): `protoc-bin-vendored` supplies the protobuf compiler so the host
//! does not need a system install.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=proto/aios_apps.proto");
    println!("cargo:rerun-if-changed=build.rs");

    let protoc = protoc_bin_vendored::protoc_bin_path()?;
    std::env::set_var("PROTOC", protoc);

    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(&["proto/aios_apps.proto"], &["proto"])?;
    Ok(())
}
