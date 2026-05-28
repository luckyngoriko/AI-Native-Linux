//! T-183 — tonic-build invocation that generates gRPC `IntegrationService`
//! server/client stubs from `proto/aios_integration.proto`.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=proto/aios_integration.proto");
    println!("cargo:rerun-if-changed=build.rs");

    let protoc = protoc_bin_vendored::protoc_bin_path()?;
    std::env::set_var("PROTOC", protoc);

    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(&["proto/aios_integration.proto"], &["proto"])?;

    Ok(())
}
