//! T-033 — tonic-build invocation that generates the gRPC `CapabilityRuntime`
//! server + client stubs from `proto/aios.runtime.v1alpha1.proto`.
//!
//! Mirrors the pattern established by `crates/aios-policy/build.rs` (T-023)
//! and `crates/aios-evidence/build.rs` (T-011). Read the spec at
//! `002.AI-OS.NET--SPECREV.2/L3_AIOS_SGR_Service_Graph_Runtime/`
//! `03_capability_runtime_grpc.md` §5 for the canonical IDL.
//!
//! ## Requirements
//!
//! - `protoc` must be on `PATH` (or `PROTOC` env var set). `tonic-build`
//!   shells out to it for proto parsing; this is standard for the tonic 0.12
//!   line. We unconditionally inject `protoc-bin-vendored` so the host does
//!   not need a system protobuf install.
//! - On rebuild, cargo re-runs this script only if `proto/` or `build.rs`
//!   itself changes (the `rerun-if-changed` directives below).

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Re-trigger codegen only when the proto file or this script changes —
    // keep clean builds fast and avoid spurious recompiles of the rest of
    // the crate.
    println!("cargo:rerun-if-changed=proto/aios.runtime.v1alpha1.proto");
    println!("cargo:rerun-if-changed=build.rs");

    // Point tonic-build at the vendored protoc binary so the build does not
    // depend on a system install of the protobuf compiler. `PROTOC` is the
    // canonical env var consumed by `prost-build`'s protoc lookup chain
    // (`std::env::var("PROTOC")` is the first probe; PATH is the fallback).
    let protoc = protoc_bin_vendored::protoc_bin_path()?;
    std::env::set_var("PROTOC", protoc);

    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        // `out_dir` defaults to OUT_DIR; `service/mod.rs` picks up the
        // generated file via `tonic::include_proto!("aios.runtime.v1alpha1")`.
        .compile_protos(&["proto/aios.runtime.v1alpha1.proto"], &["proto"])?;
    Ok(())
}
