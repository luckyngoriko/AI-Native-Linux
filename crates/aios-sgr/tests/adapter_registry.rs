//! Integration tests for the T-088 SGR-side adapter registry.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use std::sync::Arc;

use chrono::{TimeZone, Utc};
use ed25519_dalek::{Signer, SigningKey};
use serde::Serialize;

use aios_sgr::{
    AdapterActionDeclaration, AdapterCapability, AdapterCapabilityClass, AdapterDeclaration,
    AdapterDispatchKind, AdapterFailureMode, AdapterIOMode, AdapterManifest,
    AdapterRegistrationState, AdapterRollbackStrategy, AdapterStability, DependencyKind,
    DesiredState, GpuBudget, HealthCheckKind, HealthCheckSpec, RegisteredAdapter, ResourceBudget,
    RestartBudget, RestartPolicy, RollbackPointer, RollbackTrigger, SgrAdapterRegistry, SgrError,
    UnitDependency, UnitId, UnitKind, UnitManifest, VerificationIntentRef,
};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

const TRUSTED_AUTHORITY: &str = "key_aiosroot_2026q2";
const UNKNOWN_AUTHORITY: &str = "key_unknown";

fn signing_key(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
}

#[derive(Debug, Serialize)]
struct SignedCapabilityBody<'a> {
    capability_id: &'a str,
    provides: &'a [String],
    requires: &'a [String],
    risk_template: &'a str,
}

impl<'a> From<&'a AdapterCapability> for SignedCapabilityBody<'a> {
    fn from(capability: &'a AdapterCapability) -> Self {
        Self {
            capability_id: &capability.capability_id,
            provides: &capability.provides,
            requires: &capability.requires,
            risk_template: &capability.risk_template,
        }
    }
}

fn sign_capability(capability: &mut AdapterCapability, sk: &SigningKey) -> TestResult {
    let body = SignedCapabilityBody::from(&*capability);
    let bytes = serde_json::to_vec(&body)?;
    capability.manifest_signature_ed25519 = sk.sign(&bytes).to_bytes().to_vec();
    Ok(())
}

fn capability(capability_id: &str, provides: &[&str]) -> AdapterCapability {
    AdapterCapability {
        capability_id: capability_id.to_owned(),
        provides: provides.iter().map(ToString::to_string).collect(),
        requires: vec!["SERVICE_LIFECYCLE".to_owned()],
        risk_template: "REQUIRE_APPROVAL".to_owned(),
        manifest_signature_ed25519: Vec::new(),
    }
}

fn manifest_declaration(signing_key_id: &str) -> AdapterDeclaration {
    AdapterDeclaration::Manifest(Box::new(AdapterManifest {
        adapter_id: "adapter:aiosroot:systemd:1.0.0".to_owned(),
        vendor: "aiosroot".to_owned(),
        name: "systemd".to_owned(),
        adapter_version: "1.0.0".to_owned(),
        spec_version: "v1alpha1".to_owned(),
        declared_actions: vec![AdapterActionDeclaration {
            action_kind: "unit.start".to_owned(),
            target_schema: serde_json::json!({ "type": "object" }),
            response_schema: serde_json::json!({ "type": "object" }),
            rollback_strategy: AdapterRollbackStrategy::IdempotentReverse,
            timeout_seconds: 30,
            template_string: None,
            template_substitution_variables: Vec::new(),
            per_action_capabilities: vec![AdapterCapabilityClass::ServiceLifecycle],
        }],
        declared_capabilities: vec![AdapterCapabilityClass::ServiceLifecycle],
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
        manifest_signature: Vec::new(),
        signing_key_id: signing_key_id.to_owned(),
        manifest_created_at: Utc
            .with_ymd_and_hms(2026, 5, 9, 0, 0, 0)
            .single()
            .expect("valid datetime"),
        manifest_expires_at: Utc
            .with_ymd_and_hms(2026, 8, 9, 0, 0, 0)
            .single()
            .expect("valid datetime"),
    }))
}

fn registry(sk: &SigningKey) -> SgrAdapterRegistry {
    SgrAdapterRegistry::with_trusted_authority(TRUSTED_AUTHORITY.to_owned(), sk.verifying_key())
}

fn unit_id(name: &str) -> UnitId {
    UnitId::from_parts("aios", name, None).expect("valid unit id")
}

fn unit_manifest(name: &str, requires: &[&str]) -> UnitManifest {
    UnitManifest {
        schema_version: "aios.unit.v1alpha1".to_owned(),
        unit_id: unit_id(name),
        unit_kind: UnitKind::Service,
        display_name: name.to_owned(),
        description: "test unit".to_owned(),
        issued_at: Utc
            .with_ymd_and_hms(2026, 5, 9, 0, 0, 0)
            .single()
            .expect("valid datetime"),
        publisher_id: "pub_01HXY9ROOTAIOS01KEY".to_owned(),
        publisher_root_id: "aios-root".to_owned(),
        publisher_signature: Vec::new(),
        canonical_hash: "a3f1c9e2a3f1c9e2a3f1c9e2a3f1c9e2".to_owned(),
        dependencies: vec![UnitDependency {
            unit_id: unit_id("evidence-log"),
            kind: DependencyKind::RequiresHealthy,
        }],
        sandbox_profile_ref: "prof_aios_runtime_floor_001".to_owned(),
        verification_intent: vec![VerificationIntentRef {
            type_: "service.active".to_owned(),
            args: serde_json::json!({ "service": name }),
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
        adapter_target: serde_json::json!({ "requires": requires }),
        labels: None,
        correlation_id: None,
        desired_state: DesiredState::Running,
        provides: Vec::new(),
        adapter_id: None,
    }
}

async fn register_signed(
    registry: &SgrAdapterRegistry,
    sk: &SigningKey,
    capability_id: &str,
    provides: &[&str],
) -> TestResult<RegisteredAdapter> {
    let mut cap = capability(capability_id, provides);
    sign_capability(&mut cap, sk)?;
    Ok(registry
        .register_adapter(cap, manifest_declaration(TRUSTED_AUTHORITY))
        .await?)
}

#[tokio::test]
async fn register_adapter_with_valid_signature_and_trusted_authority_becomes_active() -> TestResult
{
    let sk = signing_key(31);
    let registry = registry(&sk);
    let registered =
        register_signed(&registry, &sk, "cap_service_lifecycle", &["unit.start"]).await?;

    assert_eq!(registered.state, AdapterRegistrationState::Active);
    assert_eq!(registered.capability.capability_id, "cap_service_lifecycle");
    Ok(())
}

#[tokio::test]
async fn register_adapter_with_bad_signature_rejects() -> TestResult {
    let sk = signing_key(32);
    let registry = registry(&sk);
    let mut cap = capability("cap_service_lifecycle", &["unit.start"]);
    sign_capability(&mut cap, &sk)?;
    cap.manifest_signature_ed25519[0] ^= 0x01;

    let err = registry
        .register_adapter(cap, manifest_declaration(TRUSTED_AUTHORITY))
        .await
        .expect_err("bad signature must reject");
    assert!(matches!(err, SgrError::ManifestSignatureInvalid));
    assert!(registry.list_adapters().await.is_empty());
    Ok(())
}

#[tokio::test]
async fn register_adapter_from_unknown_authority_rejects() -> TestResult {
    let sk = signing_key(33);
    let registry = registry(&sk);
    let mut cap = capability("cap_service_lifecycle", &["unit.start"]);
    sign_capability(&mut cap, &sk)?;

    let err = registry
        .register_adapter(cap, manifest_declaration(UNKNOWN_AUTHORITY))
        .await
        .expect_err("unknown authority must reject");
    assert!(matches!(err, SgrError::ManifestUnknownAuthority(ref s) if s == UNKNOWN_AUTHORITY));
    Ok(())
}

#[tokio::test]
async fn duplicate_capability_id_with_same_payload_is_idempotent() -> TestResult {
    let sk = signing_key(34);
    let registry = registry(&sk);
    let mut cap = capability("cap_service_lifecycle", &["unit.start"]);
    sign_capability(&mut cap, &sk)?;
    let declaration = manifest_declaration(TRUSTED_AUTHORITY);

    let first = registry
        .register_adapter(cap.clone(), declaration.clone())
        .await?;
    let second = registry.register_adapter(cap, declaration).await?;

    assert_eq!(first.registered_at, second.registered_at);
    assert_eq!(registry.list_adapters().await.len(), 1);
    Ok(())
}

#[tokio::test]
async fn lookup_adapter_returns_registered_adapter() -> TestResult {
    let sk = signing_key(35);
    let registry = registry(&sk);
    register_signed(&registry, &sk, "cap_service_lifecycle", &["unit.start"]).await?;

    let found = registry.lookup_adapter("cap_service_lifecycle").await?;
    assert_eq!(found.capability.provides, vec!["unit.start"]);
    Ok(())
}

#[tokio::test]
async fn lookup_adapter_unknown_returns_internal_error() {
    let sk = signing_key(36);
    let registry = registry(&sk);

    let err = registry
        .lookup_adapter("cap_missing")
        .await
        .expect_err("missing adapter must error");
    assert!(matches!(err, SgrError::Internal(ref s) if s.contains("cap_missing")));
}

#[tokio::test]
async fn list_adapters_returns_all_registered_adapters() -> TestResult {
    let sk = signing_key(37);
    let registry = registry(&sk);
    register_signed(&registry, &sk, "cap_service_lifecycle", &["unit.start"]).await?;
    register_signed(
        &registry,
        &sk,
        "cap_network",
        &["network.vpn.establish_tunnel"],
    )
    .await?;

    let listed = registry.list_adapters().await;
    assert_eq!(listed.len(), 2);
    Ok(())
}

#[tokio::test]
async fn find_adapter_for_unit_with_matching_adapter_returns_some() -> TestResult {
    let sk = signing_key(38);
    let registry = registry(&sk);
    register_signed(
        &registry,
        &sk,
        "cap_service_lifecycle",
        &["unit.start", "unit.stop"],
    )
    .await?;
    let manifest = unit_manifest("capability-runtime", &["unit.start"]);

    let found = registry.find_adapter_for_unit(&manifest).await?;
    assert_eq!(
        found.map(|adapter| adapter.capability.capability_id),
        Some("cap_service_lifecycle".to_owned())
    );
    Ok(())
}

#[tokio::test]
async fn find_adapter_for_unit_with_no_requires_and_no_adapters_returns_none() -> TestResult {
    let sk = signing_key(39);
    let registry = registry(&sk);
    let manifest = unit_manifest("capability-runtime", &[]);

    assert!(registry.find_adapter_for_unit(&manifest).await?.is_none());
    Ok(())
}

#[tokio::test]
async fn find_adapter_for_unit_with_requires_but_no_provider_returns_none() -> TestResult {
    let sk = signing_key(40);
    let registry = registry(&sk);
    let manifest = unit_manifest("capability-runtime", &["unit.start"]);

    assert!(registry.find_adapter_for_unit(&manifest).await?.is_none());
    Ok(())
}

#[tokio::test]
async fn find_adapter_for_unit_with_partial_match_returns_none() -> TestResult {
    let sk = signing_key(41);
    let registry = registry(&sk);
    register_signed(&registry, &sk, "cap_service_lifecycle", &["unit.start"]).await?;
    let manifest = unit_manifest("capability-runtime", &["unit.start", "unit.stop"]);

    assert!(registry.find_adapter_for_unit(&manifest).await?.is_none());
    Ok(())
}

#[tokio::test]
async fn find_adapter_for_unit_picks_first_active_adapter_by_registration_time() -> TestResult {
    let sk = signing_key(42);
    let registry = registry(&sk);
    register_signed(&registry, &sk, "cap_first", &["unit.start"]).await?;
    register_signed(&registry, &sk, "cap_second", &["unit.start"]).await?;
    let manifest = unit_manifest("capability-runtime", &["unit.start"]);

    let found = registry
        .find_adapter_for_unit(&manifest)
        .await?
        .expect("matching adapter");
    assert_eq!(found.capability.capability_id, "cap_first");
    Ok(())
}

#[tokio::test]
async fn suspend_adapter_moves_active_to_suspended_and_lookup_still_returns_it() -> TestResult {
    let sk = signing_key(43);
    let registry = registry(&sk);
    register_signed(&registry, &sk, "cap_service_lifecycle", &["unit.start"]).await?;

    registry
        .suspend_adapter("cap_service_lifecycle", "maintenance")
        .await?;

    let found = registry.lookup_adapter("cap_service_lifecycle").await?;
    assert_eq!(found.state, AdapterRegistrationState::Suspended);
    Ok(())
}

#[tokio::test]
async fn find_adapter_for_unit_skips_suspended_adapters() -> TestResult {
    let sk = signing_key(44);
    let registry = registry(&sk);
    register_signed(&registry, &sk, "cap_service_lifecycle", &["unit.start"]).await?;
    registry
        .suspend_adapter("cap_service_lifecycle", "maintenance")
        .await?;
    let manifest = unit_manifest("capability-runtime", &["unit.start"]);

    assert!(registry.find_adapter_for_unit(&manifest).await?.is_none());
    Ok(())
}

#[tokio::test]
async fn retire_adapter_moves_suspended_to_retired() -> TestResult {
    let sk = signing_key(45);
    let registry = registry(&sk);
    register_signed(&registry, &sk, "cap_service_lifecycle", &["unit.start"]).await?;
    registry
        .suspend_adapter("cap_service_lifecycle", "maintenance")
        .await?;

    registry.retire_adapter("cap_service_lifecycle").await?;

    let found = registry.lookup_adapter("cap_service_lifecycle").await?;
    assert_eq!(found.state, AdapterRegistrationState::Retired);
    Ok(())
}

#[tokio::test]
async fn registry_is_usable_through_arc() -> TestResult {
    let sk = signing_key(46);
    let registry = Arc::new(registry(&sk));
    register_signed(&registry, &sk, "cap_service_lifecycle", &["unit.start"]).await?;
    let manifest = unit_manifest("capability-runtime", &["unit.start"]);

    let found = registry.find_adapter_for_unit(&manifest).await?;
    assert!(found.is_some());
    Ok(())
}
