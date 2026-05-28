//! T-160 — tonic-build invocation that generates gRPC `NetworkPolicyService`
//! + `DnsVpnService` server/client stubs from `proto/aios_network.proto`.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=proto/aios_network.proto");
    println!("cargo:rerun-if-changed=build.rs");

    let protoc = protoc_bin_vendored::protoc_bin_path()?;
    std::env::set_var("PROTOC", protoc);

    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(&["proto/aios_network.proto"], &["proto"])?;

    Ok(())
}
