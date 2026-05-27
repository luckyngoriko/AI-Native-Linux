//! T-147 — tonic-build invocation that generates the gRPC `WebRendererService`
//! server + client stubs from `proto/aios_renderer_web.proto`.
//!
//! T-148 — when `AIOS_BUILD_WEB_APP=1`, also invokes `pnpm install --frozen-lockfile`
//! && `pnpm build` inside `web-app/` so the Next.js output is available at
//! Cargo build time. Default OFF — consistent with the cxx-qt feature-flag pattern
//! from T-136, so cargo gates don't depend on Node toolchain availability.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=proto/aios_renderer_web.proto");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=AIOS_BUILD_WEB_APP");

    let protoc = protoc_bin_vendored::protoc_bin_path()?;
    std::env::set_var("PROTOC", protoc);

    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(&["proto/aios_renderer_web.proto"], &["proto"])?;

    // T-148: optional Next.js web-app build, gated by AIOS_BUILD_WEB_APP env var
    if std::env::var("AIOS_BUILD_WEB_APP").as_deref() == Ok("1") {
        let web_app_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("web-app");
        println!("cargo:rerun-if-changed=web-app/package.json");
        println!("cargo:rerun-if-changed=web-app/next.config.mjs");
        println!("cargo:rerun-if-changed=web-app/tsconfig.json");

        let install = std::process::Command::new("pnpm")
            .args(["install", "--frozen-lockfile"])
            .current_dir(&web_app_dir)
            .status()?;
        if !install.success() {
            return Err("pnpm install --frozen-lockfile failed".into());
        }

        let build = std::process::Command::new("pnpm")
            .args(["build"])
            .current_dir(&web_app_dir)
            .status()?;
        if !build.success() {
            return Err("pnpm build failed".into());
        }
    }

    Ok(())
}
