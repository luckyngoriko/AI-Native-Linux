//! T-079 — tonic-build invocation that generates the gRPC `RecoveryService`
//! server + client stubs from `proto/aios.recovery.v1alpha1.proto`.
//!
//! Mirrors the pattern established by `crates/aios-policy/build.rs` (T-023),
//! `crates/aios-capability-runtime/build.rs` (T-033),
//! `crates/aios-fs/build.rs` (T-043), `crates/aios-vault/build.rs` (T-052),
//! and `crates/aios-verification/build.rs` (T-069).

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=proto/aios.recovery.v1alpha1.proto");
    println!("cargo:rerun-if-changed=build.rs");

    let protoc = protoc_bin_vendored::protoc_bin_path()?;
    std::env::set_var("PROTOC", protoc);

    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(&["proto/aios.recovery.v1alpha1.proto"], &["proto"])?;
    Ok(())
}
