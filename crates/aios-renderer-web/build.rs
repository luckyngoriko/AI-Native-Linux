//! T-147 — tonic-build invocation that generates the gRPC `WebRendererService`
//! server + client stubs from `proto/aios_renderer_web.proto`.
//!
//! Mirrors the pattern established by `crates/aios-renderer-kde/build.rs` (T-134).

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=proto/aios_renderer_web.proto");
    println!("cargo:rerun-if-changed=build.rs");

    let protoc = protoc_bin_vendored::protoc_bin_path()?;
    std::env::set_var("PROTOC", protoc);

    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(&["proto/aios_renderer_web.proto"], &["proto"])?;

    Ok(())
}
