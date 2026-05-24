//! T-052 — tonic-build invocation that generates the gRPC `VaultBroker`
//! server + client stubs from `proto/aios.vault.v1alpha1.proto`.
//!
//! Mirrors the pattern established by `crates/aios-policy/build.rs` (T-023),
//! `crates/aios-capability-runtime/build.rs` (T-033), and
//! `crates/aios-fs/build.rs` (T-043).

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=proto/aios.vault.v1alpha1.proto");
    println!("cargo:rerun-if-changed=build.rs");

    let protoc = protoc_bin_vendored::protoc_bin_path()?;
    std::env::set_var("PROTOC", protoc);

    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(&["proto/aios.vault.v1alpha1.proto"], &["proto"])?;
    Ok(())
}
