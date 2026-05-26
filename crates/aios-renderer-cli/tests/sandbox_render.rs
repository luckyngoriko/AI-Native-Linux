//! Tests for sandbox renderable implementations and CLI wiring (T-113).

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use aios_renderer_cli::{
    AiosCli, InProcessBackend, OutputFormat, RenderContext, Renderable, SandboxProfileListView,
};
use aios_sandbox::{
    GpuCapabilityBinding, GpuCapabilityClass, GpuPolicy, IommuStatus, IsolationKind,
    NetworkPosture, ProfileId, ResourceLimits, SandboxProfile,
};
use chrono::{TimeZone, Utc};
use clap::Parser;
use strum::IntoEnumIterator;

fn ctx(color: bool) -> RenderContext {
    RenderContext {
        color,
        width: Some(220),
        redact_secrets: true,
        verbose: false,
        locale: "en_US.UTF-8".to_owned(),
    }
}

fn fixed_time() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 5, 26, 10, 0, 0)
        .single()
        .expect("timestamp")
}

const fn policy_perm() -> GpuPolicy {
    GpuPolicy {
        gpu_capability_class: GpuCapabilityClass::GpuFull3d,
        vk_device_required: true,
        dmabuf_passthrough_allowed: true,
        per_group_partitioning: true,
        iommu_required: true,
        expires_at: None,
    }
}

const fn limits_default() -> ResourceLimits {
    ResourceLimits::default_strict()
}

fn profile() -> SandboxProfile {
    SandboxProfile {
        profile_id: ProfileId("sbx_01JQ6EXAMPLE".into()),
        name: "browser-sandbox".into(),
        description: "Per-origin browser isolation profile".into(),
        isolation_kind: IsolationKind::BrowserOriginIsolated,
        resource_limits: limits_default(),
        gpu_policy: policy_perm(),
        network_posture: NetworkPosture::ExplicitAllowlist,
        syscall_allowlist: Some(vec!["browser-default".into(), "seccomp-bpf".into()]),
        signing_authority: "aios-root".into(),
        signature_ed25519: vec![0xAB; 64],
    }
}

fn binding() -> GpuCapabilityBinding {
    GpuCapabilityBinding {
        binding_id: "gcb_01JQ6EXAMPLE".into(),
        gpu_capability_class: GpuCapabilityClass::GpuFull3d,
        group_id: "grp-vfx-render".into(),
        subject: aios_sandbox::SubjectRef("agent:blender:cli".into()),
        vk_device_required: true,
        dmabuf_passthrough_allowed: true,
        iommu_required: false,
        degraded_isolation: false,
        issued_at: fixed_time(),
        expires_at: None,
    }
}

// ── IsolationKind renders in all 4 formats ──

#[test]
fn isolation_kind_all_variants_render_text() {
    for variant in IsolationKind::iter() {
        let output = variant
            .render(OutputFormat::Text, &ctx(true))
            .expect("render");
        assert!(!output.is_empty(), "empty render for {variant:?}");
    }
}

#[test]
fn isolation_kind_json_roundtrip() {
    for variant in IsolationKind::iter() {
        let output = variant
            .render(OutputFormat::Json, &ctx(false))
            .expect("render");
        assert!(output.contains('"'));
    }
}

// ── GpuCapabilityClass renders in all 4 formats ──

#[test]
fn gpu_capability_class_text_includes_label() {
    let output = GpuCapabilityClass::GpuBasic2d
        .render(OutputFormat::Text, &ctx(false))
        .expect("render");
    assert!(output.contains("GPU_BASIC_2D"));
}

#[test]
fn gpu_capability_class_color_on() {
    let output = GpuCapabilityClass::GpuComputeHeavy
        .render(OutputFormat::Text, &ctx(true))
        .expect("render");
    assert!(output.contains("\u{1b}["));
}

#[test]
fn gpu_capability_class_color_off() {
    let output = GpuCapabilityClass::GpuComputeHeavy
        .render(OutputFormat::Text, &ctx(false))
        .expect("render");
    assert!(!output.contains("\u{1b}["));
}

// ── NetworkPosture renders ──

#[test]
fn network_posture_deny_all_renders_text() {
    let output = NetworkPosture::DenyAll
        .render(OutputFormat::Text, &ctx(false))
        .expect("render");
    assert!(output.contains("DENY_ALL"));
}

#[test]
fn network_posture_color_coding_varies() {
    let deny = NetworkPosture::DenyAll
        .render(OutputFormat::Text, &ctx(true))
        .expect("render");
    let full = NetworkPosture::Full
        .render(OutputFormat::Text, &ctx(true))
        .expect("render");
    assert_ne!(deny, full);
}

// ── IommuStatus renders ──

#[test]
fn iommu_status_available_renders_text() {
    let output = IommuStatus::Available
        .render(OutputFormat::Text, &ctx(false))
        .expect("render");
    assert!(output.contains("AVAILABLE"));
}

#[test]
fn iommu_status_all_variants_roundtrip_json() {
    for status in &[
        IommuStatus::Available,
        IommuStatus::Unavailable,
        IommuStatus::Unknown,
    ] {
        let output = status
            .render(OutputFormat::Json, &ctx(false))
            .expect("render");
        assert!(output.contains('"'));
    }
}

// ── SandboxProfile renders ──

#[test]
fn sandbox_profile_text_contains_key_fields() {
    let output = profile()
        .render(OutputFormat::Text, &ctx(false))
        .expect("render");
    assert!(output.contains("browser-sandbox"));
    assert!(output.contains("BROWSER_ORIGIN_ISOLATED"));
    assert!(output.contains("EXPLICIT_ALLOWLIST"));
    assert!(output.contains("aios-root"));
}

#[test]
fn sandbox_profile_json_serializes() {
    let output = profile()
        .render(OutputFormat::Json, &ctx(false))
        .expect("render");
    assert!(output.starts_with('{'));
    assert!(output.contains("browser-sandbox"));
}

#[test]
fn sandbox_profile_tree_contains_nested_fields() {
    let output = profile()
        .render(OutputFormat::Tree, &ctx(false))
        .expect("render");
    assert!(output.contains("isolation_kind"));
    assert!(output.contains("network_posture"));
    assert!(output.contains("syscall_allowlist"));
}

#[test]
fn sandbox_profile_table_has_expected_columns() {
    let output = profile()
        .render(OutputFormat::Table, &ctx(false))
        .expect("render");
    assert!(output.contains("profile_id"));
    assert!(output.contains("isolation_kind"));
    assert!(output.contains("network_posture"));
}

// ── ResourceLimits renders ──

#[test]
fn resource_limits_text_contains_all_fields() {
    let limits = limits_default();
    let output = limits
        .render(OutputFormat::Text, &ctx(false))
        .expect("render");
    assert!(output.contains("cpu_quota_percent"));
    assert!(output.contains("memory_max_bytes"));
}

#[test]
fn resource_limits_json_serializes() {
    let output = limits_default()
        .render(OutputFormat::Json, &ctx(false))
        .expect("render");
    assert!(output.starts_with('{'));
}

// ── GpuPolicy renders ──

#[test]
fn gpu_policy_text_contains_capability_class() {
    let output = policy_perm()
        .render(OutputFormat::Text, &ctx(false))
        .expect("render");
    assert!(output.contains("GPU_FULL_3D"));
    assert!(output.contains("iommu_required"));
}

#[test]
fn gpu_policy_table_has_six_columns() {
    let output = policy_perm()
        .render(OutputFormat::Table, &ctx(false))
        .expect("render");
    assert!(output.contains("gpu_capability_class"));
    assert!(output.contains("vk_device_required"));
    assert!(output.contains("expires_at"));
}

// ── GpuCapabilityBinding renders ──

#[test]
fn gpu_binding_text_contains_subject() {
    let output = binding()
        .render(OutputFormat::Text, &ctx(false))
        .expect("render");
    assert!(output.contains("gcb_01JQ6EXAMPLE"));
    assert!(output.contains("agent:blender:cli"));
}

#[test]
fn gpu_binding_json_serializes() {
    let output = binding()
        .render(OutputFormat::Json, &ctx(false))
        .expect("render");
    assert!(output.starts_with('{'));
    assert!(output.contains("gcb_01JQ6EXAMPLE"));
}

// ── SandboxProfileListView renders ──

#[test]
fn profile_list_view_text_shows_count() {
    let profiles = vec![profile(), {
        let mut p = profile();
        p.profile_id = ProfileId("sbx_01JQ6OTHER".into());
        p.name = "vfx-render-sandbox".into();
        p
    }];
    let view = SandboxProfileListView(profiles);
    let output = view
        .render(OutputFormat::Text, &ctx(false))
        .expect("render");
    assert!(output.contains("profiles"));
    assert!(output.contains('2'));
    assert!(output.contains("browser-sandbox"));
    assert!(output.contains("vfx-render-sandbox"));
}

#[test]
fn profile_list_view_table_has_columns() {
    let view = SandboxProfileListView(vec![profile()]);
    let output = view
        .render(OutputFormat::Table, &ctx(false))
        .expect("render");
    assert!(output.contains("profile_id"));
    assert!(output.contains("isolation_kind"));
    assert!(output.contains("gpu_class"));
}

// ── In-process backend includes sandbox service ──

#[tokio::test]
async fn in_process_backend_spawns_sandbox_service() {
    let (_client, shutdown) = InProcessBackend::spawn_and_connect()
        .await
        .expect("spawn backend");

    assert_eq!(shutdown.service_count(), 9);

    shutdown.shutdown().await.expect("shutdown");
}

// ── Sandbox gRPC roundtrip through AiosClient ──

#[tokio::test]
async fn compose_sandbox_returns_profile() {
    let (mut client, shutdown) = InProcessBackend::spawn_and_connect()
        .await
        .expect("spawn backend");

    let profile = client
        .compose_sandbox("subject:cli:test", "app.launch", None, false, false)
        .await
        .expect("compose sandbox");

    assert!(!profile.profile_id.to_string().is_empty());
    assert!(!profile.name.is_empty());

    shutdown.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn list_sandbox_profiles_returns_vec() {
    let (mut client, shutdown) = InProcessBackend::spawn_and_connect()
        .await
        .expect("spawn backend");

    let profiles = client.list_sandbox_profiles().await.expect("list profiles");

    // InMemorySandboxComposer::with_fixtures() seeds several profiles
    assert!(
        !profiles.is_empty(),
        "fixtures should seed at least one profile"
    );

    shutdown.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn get_sandbox_profile_by_id() {
    let (mut client, shutdown) = InProcessBackend::spawn_and_connect()
        .await
        .expect("spawn backend");

    let profiles = client.list_sandbox_profiles().await.expect("list profiles");
    let first_id = profiles[0].profile_id.to_string();

    let profile = client
        .get_sandbox_profile(&first_id)
        .await
        .expect("get profile");

    assert_eq!(profile.profile_id.to_string(), first_id);
    assert_eq!(profile.name, profiles[0].name);

    shutdown.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn compute_gpu_binding_returns_binding() {
    let (mut client, shutdown) = InProcessBackend::spawn_and_connect()
        .await
        .expect("spawn backend");

    let policy = policy_perm();
    let binding = client
        .compute_gpu_binding(
            &policy,
            "grp-vfx-render",
            "agent:blender:cli",
            IommuStatus::Available,
        )
        .await
        .expect("compute gpu binding");

    assert!(!binding.binding_id.is_empty());
    assert_eq!(binding.gpu_capability_class, GpuCapabilityClass::GpuFull3d);
    assert_eq!(binding.group_id, "grp-vfx-render");

    shutdown.shutdown().await.expect("shutdown");
}

// ── CLI: `aios sandbox list` renders ──

#[tokio::test]
async fn cli_sandbox_list_renders_json() {
    let cli = AiosCli::parse_from(["aios", "--format", "json", "sandbox", "list"]);
    let (mut client, shutdown) = InProcessBackend::spawn_and_connect()
        .await
        .expect("spawn backend");

    let output = cli.execute(&mut client).await.expect("execute");
    assert!(output.contains('['), "JSON list output failed: {output}");

    shutdown.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn cli_sandbox_get_renders_text() {
    let (mut client, shutdown) = InProcessBackend::spawn_and_connect()
        .await
        .expect("spawn backend");
    let profiles = client.list_sandbox_profiles().await.expect("list profiles");
    let first_id = profiles[0].profile_id.to_string();

    let cli = AiosCli::parse_from(["aios", "sandbox", "get", &first_id]);
    let output = cli.execute(&mut client).await.expect("execute");
    assert!(!output.is_empty());
    assert!(output.contains(&first_id));

    shutdown.shutdown().await.expect("shutdown");
}
