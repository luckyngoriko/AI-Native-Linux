//! Integration tests for the T-084 `aios-sgr` typed core skeleton.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::no_effect_underscore_binding,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use chrono::{TimeZone, Utc};
use serde::{de::DeserializeOwned, Serialize};
use strum::{EnumCount, IntoEnumIterator};

use aios_sgr::{
    ABPromotionState, AdapterCapability, AdapterDeclaration, AdapterDispatchKind,
    AdapterFailureMode, AdapterIOMode, AdapterManifest, AdapterManifestRegistrationState,
    AdapterRegistrationState, AdapterRollbackStrategy, AdapterStability, DependencyEdge,
    DependencyKind, DependencySolveResult, DesiredState, GpuBudget, GraphEvaluationResult,
    GraphState, HealthCheckKind, HealthCheckSpec, ResourceBudget, RestartBudget, RestartPolicy,
    RollbackPointer, RollbackTrigger, ServiceUnit, SgrError, TransitionKind, UnitDependency,
    UnitId, UnitKind, UnitManifest, UnitState, VerificationIntentRef,
};

fn round_trip<T>(value: &T) -> T
where
    T: Serialize + DeserializeOwned,
{
    let json = serde_json::to_string(value).expect("serialize");
    serde_json::from_str(&json).expect("deserialize")
}

fn sample_unit_id() -> UnitId {
    UnitId::parse("unit:aios:capability_runtime").expect("valid unit id")
}

fn sample_manifest() -> UnitManifest {
    UnitManifest {
        schema_version: "aios.unit.v1alpha1".to_owned(),
        unit_id: sample_unit_id(),
        unit_kind: UnitKind::Service,
        display_name: "AIOS Capability Runtime".to_owned(),
        description: "Dispatches typed actions to adapters.".to_owned(),
        issued_at: Utc
            .with_ymd_and_hms(2026, 5, 9, 0, 0, 0)
            .single()
            .expect("valid datetime"),
        publisher_id: "pub_01HXY9ROOTAIOS01KEY".to_owned(),
        publisher_root_id: "aios-root".to_owned(),
        publisher_signature: vec![1, 2, 3, 4],
        canonical_hash: "a3f1c9e2a3f1c9e2a3f1c9e2a3f1c9e2".to_owned(),
        dependencies: vec![UnitDependency {
            unit_id: UnitId::parse("unit:aios:evidence_log").expect("valid dependency"),
            kind: DependencyKind::RequiresHealthy,
        }],
        sandbox_profile_ref: "prof_aios_runtime_floor_001".to_owned(),
        verification_intent: vec![VerificationIntentRef {
            type_: "service.active".to_owned(),
            args: serde_json::json!({ "service": "aios-capability-runtime" }),
        }],
        rollback_pointer: RollbackPointer {
            aiosfs_pointer_id: "ptr_system_capability_runtime_release".to_owned(),
            expected_current_version_id: "ver_01HXY8K2".to_owned(),
            trigger: RollbackTrigger::OnHealthFailure,
        },
        resource_budget: ResourceBudget {
            memory_bytes_max: 2_147_483_648,
            cpu_quota_cores: 2.0,
            disk_bytes_max: 4_294_967_296,
            file_descriptors_max: 16_384,
            process_count_max: 256,
            queue_depth_max: 4_096,
            gpu: Some(GpuBudget {
                requires_compute: false,
                vram_bytes_max: 0,
            }),
        },
        restart_policy: RestartPolicy::Always,
        restart_budget: RestartBudget {
            max_attempts: 5,
            reset_window_seconds: 300,
            backoff_initial_seconds: 2,
            backoff_max_seconds: 60,
        },
        health_check: HealthCheckSpec {
            kind: HealthCheckKind::HttpOk,
            probe_interval_seconds: 10,
            probe_timeout_seconds: 3,
            startup_grace_seconds: 30,
            unhealthy_threshold: 3,
            healthy_threshold: 2,
            args: serde_json::json!({ "path": "/healthz", "port": 7421 }),
        },
        startup_deadline_seconds: 60,
        stop_deadline_seconds: 30,
        adapter_target: serde_json::json!({
            "binary_pointer": "ptr_system_capability_runtime_bin",
            "args": ["--config", "/aios/system/runtime/config.toml"]
        }),
        labels: Some(serde_json::json!({ "layer": "L3", "criticality": "critical" })),
        correlation_id: Some("corr_aios_bootstrap_001".to_owned()),
        desired_state: DesiredState::Running,
        provides: vec!["runtime.capability".to_owned()],
        adapter_id: Some("adapter:aiosroot:systemd:1.0.0".to_owned()),
    }
}

fn sample_adapter_manifest() -> AdapterManifest {
    AdapterManifest {
        adapter_id: "adapter:aiosroot:systemd:1.0.0".to_owned(),
        vendor: "aiosroot".to_owned(),
        name: "systemd".to_owned(),
        adapter_version: "1.0.0".to_owned(),
        spec_version: "v1alpha1".to_owned(),
        declared_actions: vec![aios_sgr::AdapterActionDeclaration {
            action_kind: "unit.start".to_owned(),
            target_schema: serde_json::json!({ "type": "object" }),
            response_schema: serde_json::json!({ "type": "object" }),
            rollback_strategy: AdapterRollbackStrategy::IdempotentReverse,
            timeout_seconds: 30,
            template_string: None,
            template_substitution_variables: Vec::new(),
            per_action_capabilities: vec![aios_sgr::AdapterCapabilityClass::ServiceLifecycle],
        }],
        declared_capabilities: vec![aios_sgr::AdapterCapabilityClass::ServiceLifecycle],
        declared_invariants_supported: vec!["INV-013".to_owned(), "INV-017".to_owned()],
        io_mode: AdapterIOMode::TypedParametersOnly,
        preferred_dispatch_kind: AdapterDispatchKind::SubprocessFork,
        declared_stability: AdapterStability::Registered,
        sandbox_profile_minimum: serde_json::json!({ "network": "none" }),
        declared_failure_modes: vec![AdapterFailureMode::AdapterTimeout],
        default_adapter_timeout_seconds: 30,
        default_rollback_timeout_seconds: 30,
        network_outbound_hosts: Vec::new(),
        external_api_hosts: Vec::new(),
        declared_evidence_record_types: Vec::new(),
        source_package_id: "pkg:aiosroot:adapter-systemd:1.0.0".to_owned(),
        publisher_root_id: "pubcat_aiosroot".to_owned(),
        manifest_signature: vec![9, 8, 7],
        signing_key_id: "key_aiosroot_2026q2".to_owned(),
        manifest_created_at: Utc
            .with_ymd_and_hms(2026, 5, 9, 0, 0, 0)
            .single()
            .expect("valid datetime"),
        manifest_expires_at: Utc
            .with_ymd_and_hms(2026, 8, 9, 0, 0, 0)
            .single()
            .expect("valid datetime"),
    }
}

#[test]
fn unit_state_count_matches_s15_1() {
    assert_eq!(UnitState::COUNT, 11);
    assert_eq!(UnitState::iter().count(), 11);
}

#[test]
fn unit_kind_count_matches_s15_1() {
    assert_eq!(UnitKind::COUNT, 10);
}

#[test]
fn desired_state_count_matches_t084_contract() {
    assert_eq!(DesiredState::COUNT, 4);
}

#[test]
fn dependency_kind_count_matches_s15_1() {
    assert_eq!(DependencyKind::COUNT, 3);
}

#[test]
fn graph_state_count_matches_t084_contract() {
    assert_eq!(GraphState::COUNT, 6);
}

#[test]
fn graph_evaluation_enums_match_s15_2() {
    assert_eq!(GraphEvaluationResult::COUNT, 5);
    assert_eq!(TransitionKind::COUNT, 10);
    assert_eq!(DependencySolveResult::COUNT, 4);
    assert_eq!(ABPromotionState::COUNT, 5);
}

#[test]
fn adapter_model_enums_match_s15_3() {
    assert_eq!(AdapterManifestRegistrationState::COUNT, 6);
    assert_eq!(AdapterRegistrationState::COUNT, 4);
    assert_eq!(aios_sgr::AdapterCapabilityClass::COUNT, 10);
    assert_eq!(AdapterIOMode::COUNT, 2);
    assert_eq!(AdapterDispatchKind::COUNT, 4);
    assert_eq!(AdapterStability::COUNT, 5);
    assert_eq!(AdapterFailureMode::COUNT, 10);
    assert_eq!(AdapterRollbackStrategy::COUNT, 5);
}

#[test]
fn unit_state_terminal_classification_matches_s15_1() {
    let terminals = [UnitState::Stopped, UnitState::Failed, UnitState::Retired];
    for state in UnitState::iter() {
        assert_eq!(
            state.is_terminal(),
            terminals.contains(&state),
            "{state:?}.is_terminal() mismatch",
        );
    }
}

#[test]
fn unit_manifest_round_trips_through_json() {
    let manifest = sample_manifest();
    assert_eq!(round_trip::<UnitManifest>(&manifest), manifest);
}

#[test]
fn service_unit_round_trips_through_json() {
    let service = ServiceUnit {
        unit_id: sample_unit_id(),
        manifest: sample_manifest(),
        state: UnitState::Queued,
        last_transition_at: Utc
            .with_ymd_and_hms(2026, 5, 9, 0, 1, 0)
            .single()
            .expect("valid datetime"),
        evidence_chain: vec!["evr_01HXY8K2JPQ7N3M4R5S6T7V8W9".to_owned()],
    };

    assert_eq!(round_trip::<ServiceUnit>(&service), service);
}

#[test]
fn unit_dependency_round_trips_through_json() {
    let dependency = UnitDependency {
        unit_id: UnitId::parse("unit:aios:policy_kernel").expect("valid dependency"),
        kind: DependencyKind::RequiresRunning,
    };

    assert_eq!(round_trip::<UnitDependency>(&dependency), dependency);
}

#[test]
fn dependency_edge_round_trips_through_json() {
    let edge = DependencyEdge {
        from_unit_id: sample_unit_id(),
        to_unit_id: UnitId::parse("unit:aios:evidence_log").expect("valid target"),
        kind: DependencyKind::OrdersAfter,
    };

    assert_eq!(round_trip::<DependencyEdge>(&edge), edge);
}

#[test]
fn adapter_capability_round_trips_through_json() {
    let capability = AdapterCapability {
        capability_id: "cap_service_lifecycle".to_owned(),
        provides: vec!["unit.start".to_owned(), "unit.stop".to_owned()],
        requires: vec!["SERVICE_LIFECYCLE".to_owned()],
        risk_template: "REQUIRE_APPROVAL".to_owned(),
        manifest_signature_ed25519: vec![1, 2, 3],
    };

    assert_eq!(round_trip::<AdapterCapability>(&capability), capability);
}

#[test]
fn adapter_declaration_round_trips_through_json() {
    let declaration = AdapterDeclaration::Manifest(Box::new(sample_adapter_manifest()));
    assert_eq!(round_trip::<AdapterDeclaration>(&declaration), declaration);
}

#[test]
fn sgr_error_display_strings_are_non_empty() {
    let unit_id = sample_unit_id();
    let errors = vec![
        SgrError::UnitNotFound(unit_id.clone()),
        SgrError::DependencyCycleDetected(vec![unit_id.clone()]),
        SgrError::InvalidStateTransition {
            from: UnitState::Draft,
            to: UnitState::Healthy,
        },
        SgrError::ManifestSignatureInvalid,
        SgrError::ManifestUnknownAuthority("pubcat_unknown".to_owned()),
        SgrError::DependencyTargetNotRegistered(unit_id),
        SgrError::Internal("boom".to_owned()),
    ];

    for error in errors {
        assert!(!error.to_string().is_empty());
    }
}
