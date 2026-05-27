//! T-134 — tonic-build invocation that generates the gRPC `KdeRendererService`
//! server + client stubs from `proto/aios_renderer_kde.proto`.
//!
//! Mirrors the pattern established by `crates/aios-apps/build.rs` (T-122)
//! and `crates/aios-capability-runtime/build.rs` (T-033).

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=proto/aios_renderer_kde.proto");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/qt_bridge/");
    println!("cargo:rerun-if-changed=qml/");

    let protoc = protoc_bin_vendored::protoc_bin_path()?;
    std::env::set_var("PROTOC", protoc);

    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(&["proto/aios_renderer_kde.proto"], &["proto"])?;

    // T-136: cxx-qt build generates C++ that needs libstdc++ for __cxa_guard_* etc.
    // qt-build-utils does not emit this directive automatically.
    if std::env::var("CARGO_FEATURE_QT_BRIDGE").is_ok() {
        println!("cargo:rustc-link-lib=stdc++");
    }

    // T-136: cxx-qt build — only when feature "qt-bridge" is enabled
    if std::env::var("CARGO_FEATURE_QT_BRIDGE").is_ok() {
        cxx_qt_build::CxxQtBuilder::new()
            .qml_module(cxx_qt_build::QmlModule {
                uri: "AiosPrimitives",
                version_major: 1,
                version_minor: 0,
                rust_files: &["src/qt_bridge/aios_window.rs"],
                qml_files: &[
                    "qml/AIOSWindow.qml",
                    "qml/AIOSApprovalDialog.qml",
                    "qml/AIOSSecurityIndicator.qml",
                ],
                qrc_files: &[],
            })
            .cc_builder(|cc| {
                cc.flag("-include").flag("cxx-qt-lib/qstring.h");
            })
            .build();
    }

    Ok(())
}
