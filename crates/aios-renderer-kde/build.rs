//! T-134 — tonic-build invocation that generates the gRPC `KdeRendererService`
//! server + client stubs from `proto/aios_renderer_kde.proto`.
//!
//! Mirrors the pattern established by `crates/aios-apps/build.rs` (T-122)
//! and `crates/aios-capability-runtime/build.rs` (T-033).

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=proto/aios_renderer_kde.proto");
    println!("cargo:rerun-if-changed=build.rs");

    let protoc = protoc_bin_vendored::protoc_bin_path()?;
    std::env::set_var("PROTOC", protoc);

    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(&["proto/aios_renderer_kde.proto"], &["proto"])?;
    Ok(())
}
