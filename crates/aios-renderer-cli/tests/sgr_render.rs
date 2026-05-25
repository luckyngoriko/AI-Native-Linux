//! Tests for SGR renderable implementations and CLI wiring.

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use aios_renderer_cli::{
    AiosCli, AiosCommand, InProcessBackend, OutputFormat, RenderContext, Renderable, SgrGraphView,
    SgrSubcommand, SgrUnitListView,
};
use aios_sgr::{
    AdapterCapability, AdapterDeclaration, AdapterRegistrationState, DependencyKind, DesiredState,
    GraphState, HealthCheckKind, HealthCheckSpec, RegisteredAdapter, ResourceBudget, RestartBudget,
    RestartPolicy, RollbackPointer, RollbackTrigger, ServiceUnit, UnitId, UnitKind, UnitManifest,
    UnitState,
};
use chrono::{TimeZone, Utc};
use clap::Parser;
use serde_json::json;
use strum::IntoEnumIterator;

fn ctx(color: bool) -> RenderContext {
    RenderContext {
        color,
        width: Some(240),
        redact_secrets: true,
        verbose: false,
        locale: "en_US.UTF-8".to_owned(),
    }
}

fn fixed_time() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 5, 25, 10, 0, 0)
        .single()
        .expect("timestamp")
}

fn unit_id(name: &str) -> UnitId {
    UnitId::from_parts("aiosroot", name, None).expect("valid unit id")
}

fn manifest(kind: UnitKind) -> UnitManifest {
    UnitManifest {
        schema_version: "aios.unit.v1alpha1".to_owned(),
        unit_id: unit_id("nginx"),
        unit_kind: kind,
        display_name: "nginx".to_owned(),
        description: "Edge HTTP service".to_owned(),
        issued_at: fixed_time(),
        publisher_id: "publisher:aiosroot".to_owned(),
        publisher_root_id: "aiosroot".to_owned(),
        publisher_signature: vec![7; 64],
        canonical_hash: "0123456789abcdef0123456789abcdef".to_owned(),
        dependencies: Vec::new(),
        sandbox_profile_ref: "sandbox:service".to_owned(),
        verification_intent: Vec::new(),
        rollback_pointer: RollbackPointer {
            aiosfs_pointer_id: "ptr_nginx".to_owned(),
            expected_current_version_id: "ver_01".to_owned(),
            trigger: RollbackTrigger::OnStartupFailure,
        },
        resource_budget: ResourceBudget {
            memory_bytes_max: 536_870_912,
            cpu_quota_cores: 0.5,
            disk_bytes_max: 1_073_741_824,
            file_descriptors_max: 128,
            process_count_max: 16,
            queue_depth_max: 64,
            gpu: None,
        },
        restart_policy: RestartPolicy::OnFailure,
        restart_budget: RestartBudget {
            max_attempts: 3,
            reset_window_seconds: 60,
            backoff_initial_seconds: 1,
            backoff_max_seconds: 30,
        },
        health_check: HealthCheckSpec {
            kind: HealthCheckKind::HttpOk,
            probe_interval_seconds: 10,
            probe_timeout_seconds: 2,
            startup_grace_seconds: 5,
            unhealthy_threshold: 3,
            healthy_threshold: 2,
            args: json!({ "url": "http://127.0.0.1/health" }),
        },
        startup_deadline_seconds: 30,
        stop_deadline_seconds: 10,
        adapter_target: json!({ "systemd_unit": "nginx.service" }),
        labels: Some(json!({ "critical": true })),
        correlation_id: Some("corr_t092".to_owned()),
        desired_state: DesiredState::Running,
        provides: vec!["service.nginx".to_owned()],
        adapter_id: Some("adapter:systemd".to_owned()),
    }
}

fn service_unit(state: UnitState) -> ServiceUnit {
    ServiceUnit {
        unit_id: unit_id("nginx"),
        manifest: manifest(UnitKind::Service),
        state,
        last_transition_at: fixed_time(),
        evidence_chain: vec!["evr_01HEAD".to_owned(), "evr_01TAIL".to_owned()],
    }
}

fn registered_adapter(state: AdapterRegistrationState) -> RegisteredAdapter {
    let capability = AdapterCapability {
        capability_id: "cap_systemd_service".to_owned(),
        provides: vec!["service.lifecycle".to_owned()],
        requires: vec!["vault.keysign".to_owned()],
        risk_template: "system.low".to_owned(),
        manifest_signature_ed25519: vec![9; 64],
    };

    RegisteredAdapter {
        capability: capability.clone(),
        declaration: AdapterDeclaration::Capability(capability),
        registered_at: fixed_time(),
        state,
    }
}

#[test]
fn service_unit_renders_all_output_formats() {
    let unit = service_unit(UnitState::Running);

    let text = unit
        .render(OutputFormat::Text, &ctx(false))
        .expect("render text");
    let json = unit
        .render(OutputFormat::Json, &ctx(false))
        .expect("render json");
    let tree = unit
        .render(OutputFormat::Tree, &ctx(false))
        .expect("render tree");
    let table = unit
        .render(OutputFormat::Table, &ctx(false))
        .expect("render table");

    assert!(text.contains("unit_id: unit:aiosroot:nginx"), "{text}");
    assert!(text.contains("kind: SERVICE"), "{text}");
    assert!(text.contains("name: nginx"), "{text}");
    assert!(text.contains("evidence_head: evr_01HEAD"), "{text}");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&json).expect("json")["unit_id"],
        "unit:aiosroot:nginx"
    );
    assert!(
        tree.starts_with("ServiceUnit unit:aiosroot:nginx"),
        "{tree}"
    );
    assert!(table.contains("unit_id"), "{table}");
    assert!(table.contains("RUNNING"), "{table}");
}

#[test]
fn unit_state_each_variant_renders_expected_color() {
    let cases = [
        (UnitState::Draft, "34", "DRAFT"),
        (UnitState::Queued, "34", "QUEUED"),
        (UnitState::Starting, "33", "STARTING"),
        (UnitState::Running, "32", "RUNNING"),
        (UnitState::Healthy, "32", "HEALTHY"),
        (UnitState::Degraded, "33", "DEGRADED"),
        (UnitState::Unhealthy, "33", "UNHEALTHY"),
        (UnitState::Stopping, "33", "STOPPING"),
        (UnitState::Stopped, "90", "STOPPED"),
        (UnitState::Failed, "31", "FAILED"),
        (UnitState::Retired, "90", "RETIRED"),
    ];

    for (state, color, label) in cases {
        let rendered = state
            .render(OutputFormat::Text, &ctx(true))
            .expect("render unit state");
        let expected = format!("\u{1b}[{color}m{label}\u{1b}[0m");
        assert!(rendered.contains(&expected), "{rendered}");
    }
}

#[test]
fn unit_kind_all_ten_variants_render_wire_names() {
    let rendered = UnitKind::iter()
        .map(|kind| kind.render(OutputFormat::Text, &ctx(false)))
        .collect::<Result<Vec<_>, _>>()
        .expect("render kinds");

    assert_eq!(rendered.len(), 10);
    for expected in [
        "SERVICE",
        "ONE_SHOT_JOB",
        "TIMER",
        "MOUNT",
        "DEVICE",
        "APP_SESSION",
        "AGENT_WORKER",
        "MODEL_SERVER",
        "RECOVERY_TASK",
        "OBSERVER",
    ] {
        assert!(rendered.iter().any(|value| value.contains(expected)));
    }
}

#[test]
fn dependency_kind_all_variants_render_wire_names() {
    let rendered = DependencyKind::iter()
        .map(|kind| kind.render(OutputFormat::Text, &ctx(false)))
        .collect::<Result<Vec<_>, _>>()
        .expect("render dependency kinds");

    assert_eq!(rendered.len(), 3);
    assert!(rendered
        .iter()
        .any(|value| value.contains("REQUIRES_HEALTHY")));
    assert!(rendered
        .iter()
        .any(|value| value.contains("REQUIRES_RUNNING")));
    assert!(rendered.iter().any(|value| value.contains("ORDERS_AFTER")));
}

#[test]
fn graph_state_variants_are_color_coded() {
    let cases = [
        (GraphState::Empty, "90", "EMPTY"),
        (GraphState::Resolving, "33", "RESOLVING"),
        (GraphState::Converging, "33", "CONVERGING"),
        (GraphState::Converged, "32", "CONVERGED"),
        (GraphState::Degraded, "33", "DEGRADED"),
        (GraphState::Failed, "31", "FAILED"),
    ];

    for (state, color, label) in cases {
        let rendered = state
            .render(OutputFormat::Text, &ctx(true))
            .expect("render graph state");
        let expected = format!("\u{1b}[{color}m{label}\u{1b}[0m");
        assert!(rendered.contains(&expected), "{rendered}");
    }
}

#[test]
fn registered_adapter_renders_all_output_formats() {
    let adapter = registered_adapter(AdapterRegistrationState::Active);

    for format in [
        OutputFormat::Text,
        OutputFormat::Json,
        OutputFormat::Tree,
        OutputFormat::Table,
    ] {
        let rendered = adapter.render(format, &ctx(false)).expect("render adapter");
        assert!(rendered.contains("cap_systemd_service"), "{rendered}");
        assert!(rendered.contains("ACTIVE"), "{rendered}");
    }
}

#[test]
fn adapter_registration_state_variants_are_color_coded() {
    let cases = [
        (AdapterRegistrationState::Pending, "34", "PENDING"),
        (AdapterRegistrationState::Active, "32", "ACTIVE"),
        (AdapterRegistrationState::Suspended, "33", "SUSPENDED"),
        (AdapterRegistrationState::Retired, "90", "RETIRED"),
    ];

    for (state, color, label) in cases {
        let rendered = state
            .render(OutputFormat::Text, &ctx(true))
            .expect("render adapter state");
        let expected = format!("\u{1b}[{color}m{label}\u{1b}[0m");
        assert!(rendered.contains(&expected), "{rendered}");
    }
}

#[test]
fn parser_sgr_list_accepts_command() {
    let cli = AiosCli::try_parse_from(["aios", "sgr", "list"]).expect("parse sgr list");

    match cli.command {
        AiosCommand::Sgr {
            subcommand: SgrSubcommand::List,
        } => {}
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parser_sgr_get_accepts_unit_id() {
    let cli = AiosCli::try_parse_from(["aios", "sgr", "get", "unit_01XXX"]).expect("parse sgr get");

    match cli.command {
        AiosCommand::Sgr {
            subcommand: SgrSubcommand::Get { unit_id },
        } => assert_eq!(unit_id, "unit_01XXX"),
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parser_sgr_graph_accepts_command() {
    let cli = AiosCli::try_parse_from(["aios", "sgr", "graph"]).expect("parse sgr graph");

    match cli.command {
        AiosCommand::Sgr {
            subcommand: SgrSubcommand::Graph,
        } => {}
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parser_sgr_evaluate_accepts_command() {
    let cli = AiosCli::try_parse_from(["aios", "sgr", "evaluate"]).expect("parse sgr evaluate");

    match cli.command {
        AiosCommand::Sgr {
            subcommand: SgrSubcommand::Evaluate,
        } => {}
        other => panic!("unexpected command: {other:?}"),
    }
}

#[tokio::test]
async fn in_process_backend_service_count_is_seven() {
    let (_client, shutdown) = InProcessBackend::spawn_and_connect()
        .await
        .expect("spawn backend");

    assert_eq!(shutdown.service_count(), 7);

    shutdown.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn in_process_sgr_list_units_is_empty_initially_and_renders() {
    let (mut client, shutdown) = InProcessBackend::spawn_and_connect()
        .await
        .expect("spawn backend");

    let units = client.list_units().await.expect("list units");
    let rendered = SgrUnitListView::new(units.clone())
        .render(OutputFormat::Text, &ctx(false))
        .expect("render units");

    assert!(units.is_empty());
    assert!(rendered.contains("SgrUnits"), "{rendered}");
    assert!(rendered.contains("units: 0"), "{rendered}");

    shutdown.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn in_process_sgr_traverse_graph_returns_ordered_vec_and_renders() {
    let (mut client, shutdown) = InProcessBackend::spawn_and_connect()
        .await
        .expect("spawn backend");

    let ordered_unit_ids = client.traverse_graph().await.expect("traverse graph");
    let graph_state = client.evaluate_graph().await.expect("evaluate graph");
    let rendered = SgrGraphView::new(ordered_unit_ids.clone(), graph_state)
        .render(OutputFormat::Tree, &ctx(false))
        .expect("render graph");

    assert!(ordered_unit_ids.is_empty());
    assert_eq!(graph_state, GraphState::Empty);
    assert!(rendered.contains("sgr_graph state=EMPTY"), "{rendered}");
    assert!(rendered.contains("ordered_unit_ids: count=0"), "{rendered}");

    shutdown.shutdown().await.expect("shutdown");
}
