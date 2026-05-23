//! T-011 — tonic-build invocation that generates the gRPC `EvidenceLog` server
//! + client stubs from `proto/aios.evidence.v1alpha1.proto`.
//!
//! This is the **first build script** in the AIOS workspace; the pattern
//! established here is the template for every future AIOS service crate
//! (capability runtime, policy kernel, vault broker, etc.). Read the spec at
//! `002.AI-OS.NET--SPECREV.2/L9_Observability_Admin_Operations/01_evidence_log.md`
//! Appendix A for the canonical IDL.
//!
//! ## Requirements
//!
//! - `protoc` must be on `PATH` (or `PROTOC` env var set). `tonic-build` shells
//!   out to it for the actual proto parsing pass; this is standard for the
//!   tonic 0.12 line.
//! - On rebuild, cargo re-runs this script only if `proto/` changes (the
//!   `rerun-if-changed` directive below); the generated Rust code is cached
//!   under `target/<profile>/build/aios-evidence-*/out/`.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Re-trigger codegen only when the proto file changes — keep clean builds
    // fast and avoid spurious recompiles of the rest of the crate.
    println!("cargo:rerun-if-changed=proto/aios.evidence.v1alpha1.proto");
    println!("cargo:rerun-if-changed=build.rs");

    // Point tonic-build at the vendored protoc binary so the build does not
    // depend on a system install of the protobuf compiler. `PROTOC` is the
    // canonical env var consumed by `prost-build`'s protoc lookup chain
    // (`std::env::var("PROTOC")` is the first probe; PATH is the fallback).
    // We unconditionally override it to keep builds reproducible across
    // dev hosts that may have an old system protoc.
    let protoc = protoc_bin_vendored::protoc_bin_path()?;
    std::env::set_var("PROTOC", protoc);

    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        // `out_dir` defaults to OUT_DIR; service/mod.rs picks up the generated
        // file via `tonic::include_proto!("aios.evidence.v1alpha1")`.
        .compile_protos(&["proto/aios.evidence.v1alpha1.proto"], &["proto"])?;
    Ok(())
}
