//! T-148 — Rust-side smoke test asserting the web-app scaffold landed on disk.
//! Does NOT require Node at test time — purely file-existence assertions.

use std::path::Path;

macro_rules! web_app_path {
    ($rel:expr) => {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("web-app")
            .join($rel)
    };
}

#[test]
fn web_app_package_json_exists() {
    let p = web_app_path!("package.json");
    assert!(p.exists(), "expected {} to exist", p.display());
}

#[test]
fn web_app_page_tsx_exists() {
    let p = web_app_path!("app/page.tsx");
    assert!(p.exists(), "expected {} to exist", p.display());
}

#[test]
fn web_app_layout_tsx_exists() {
    let p = web_app_path!("app/layout.tsx");
    assert!(p.exists(), "expected {} to exist", p.display());
}

#[test]
fn web_app_recovery_page_exists() {
    let p = web_app_path!("app/recovery/page.tsx");
    assert!(p.exists(), "expected {} to exist", p.display());
}

#[test]
fn web_app_tsconfig_exists() {
    let p = web_app_path!("tsconfig.json");
    assert!(p.exists(), "expected {} to exist", p.display());
}

#[test]
fn web_app_next_config_exists() {
    let p = web_app_path!("next.config.mjs");
    assert!(p.exists(), "expected {} to exist", p.display());
}

#[test]
fn node_kind_registry_ts_exists() {
    let p = web_app_path!("lib/nodeKindRegistry.ts");
    assert!(p.exists(), "expected {} to exist", p.display());
}

#[test]
fn shadow_root_host_component_exists() {
    let p = web_app_path!("components/chrome/ShadowRootHost.tsx");
    assert!(p.exists(), "expected {} to exist", p.display());
}

#[test]
fn all_nineteen_components_exist() {
    let components = [
        "Container.tsx",
        "Divider.tsx",
        "Spacer.tsx",
        "Text.tsx",
        "Heading.tsx",
        "InlineCode.tsx",
        "CodeBlock.tsx",
        "Card.tsx",
        "List.tsx",
        "Table.tsx",
        "Form.tsx",
        "ActionButton.tsx",
        "Visualization.tsx",
        "Stream.tsx",
        "SurfaceEmbed.tsx",
        "SecurityIndicator.tsx",
        "ApprovalPrompt.tsx",
        "EvidenceLink.tsx",
        "AgentMessage.tsx",
    ];
    for name in &components {
        let p = web_app_path!("components/nodes/").join(name);
        assert!(p.exists(), "expected component {} to exist", p.display());
    }
}

#[test]
fn components_index_ts_exists() {
    let p = web_app_path!("components/nodes/index.ts");
    assert!(p.exists(), "expected {} to exist", p.display());
}

#[test]
fn registry_test_exists() {
    let p = web_app_path!("__tests__/nodeKindRegistry.test.ts");
    assert!(p.exists(), "expected {} to exist", p.display());
}
