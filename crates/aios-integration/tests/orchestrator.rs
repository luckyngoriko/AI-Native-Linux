#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_lines,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    missing_docs,
    reason = "test code"
)]

use aios_integration::composition::{ComposedService, ServiceComposition, ServiceDependency};
use aios_integration::composition_engine::default_aios_composition;
use aios_integration::ids::ComposedSystemId;
use aios_integration::orchestrator::{Orchestrator, ServiceScaffoldStatus};
use aios_integration::IntegrationError;

fn orchestrator() -> Orchestrator {
    Orchestrator::from_default_composition().unwrap()
}

#[test]
fn from_default_composition_succeeds() {
    let orch = Orchestrator::from_default_composition();
    assert!(orch.is_ok());
}

#[tokio::test]
async fn boot_order_has_17_entries() {
    let orch = orchestrator();
    let order = orch.boot_order().await;
    assert_eq!(order.len(), 17);
}

#[tokio::test]
async fn boot_order_first_is_aios_action() {
    let orch = orchestrator();
    let order = orch.boot_order().await;
    assert_eq!(order[0], "aios-action");
}

#[tokio::test]
async fn boot_order_last_is_aios_hardware() {
    let orch = orchestrator();
    let order = orch.boot_order().await;
    assert_eq!(order[16], "aios-hardware");
}

#[tokio::test]
async fn boot_order_evidence_before_policy() {
    let orch = orchestrator();
    let order = orch.boot_order().await;
    let evidence_pos = order.iter().position(|s| s == "aios-evidence").unwrap();
    let policy_pos = order.iter().position(|s| s == "aios-policy").unwrap();
    assert!(evidence_pos < policy_pos);
}

#[tokio::test]
async fn boot_order_policy_before_capability_runtime() {
    let orch = orchestrator();
    let order = orch.boot_order().await;
    let policy_pos = order.iter().position(|s| s == "aios-policy").unwrap();
    let cr_pos = order
        .iter()
        .position(|s| s == "aios-capability-runtime")
        .unwrap();
    assert!(policy_pos < cr_pos);
}

#[tokio::test]
async fn boot_order_renderer_cli_before_renderer_kde() {
    let orch = orchestrator();
    let order = orch.boot_order().await;
    let cli_pos = order.iter().position(|s| s == "aios-renderer-cli").unwrap();
    let kde_pos = order.iter().position(|s| s == "aios-renderer-kde").unwrap();
    assert!(cli_pos < kde_pos);
}

#[tokio::test]
async fn health_summary_returns_17_entries() {
    let orch = orchestrator();
    let summaries = orch.health_summary().await;
    assert_eq!(summaries.len(), 17);
}

#[tokio::test]
async fn health_summary_all_status_scaffold_ready_by_default() {
    let orch = orchestrator();
    let summaries = orch.health_summary().await;
    for s in &summaries {
        assert_eq!(s.status, ServiceScaffoldStatus::ScaffoldReady);
    }
}

#[tokio::test]
async fn health_summary_topological_index_matches_position() {
    let orch = orchestrator();
    let order = orch.boot_order().await;
    let summaries = orch.health_summary().await;

    for summary in &summaries {
        let pos = order
            .iter()
            .position(|id| id == &summary.service_id)
            .unwrap();
        assert_eq!(summary.topological_index, pos);
    }
}

#[tokio::test]
async fn validate_external_composition_with_valid_succeeds() {
    let orch = orchestrator();
    let comp = ServiceComposition {
        composition_id: ComposedSystemId("ext-valid".into()),
        services: vec![
            ComposedService {
                service_id: "a".into(),
                crate_name: "a".into(),
                binding_endpoint: "unix:/run/a.sock".into(),
                depends_on: vec![],
            },
            ComposedService {
                service_id: "b".into(),
                crate_name: "b".into(),
                binding_endpoint: "unix:/run/b.sock".into(),
                depends_on: vec!["a".into()],
            },
        ],
        dependencies: vec![ServiceDependency {
            from_service: "b".into(),
            to_service: "a".into(),
            required: true,
        }],
        topological_order: vec![],
    };
    let order = orch.validate_external_composition(&comp).await.unwrap();
    assert_eq!(order, vec!["a", "b"]);
}

#[tokio::test]
async fn validate_external_composition_with_cycle_returns_composition_cycle_detected() {
    let orch = orchestrator();
    let comp = ServiceComposition {
        composition_id: ComposedSystemId("ext-cycle".into()),
        services: vec![
            ComposedService {
                service_id: "x".into(),
                crate_name: "x".into(),
                binding_endpoint: "unix:/run/x.sock".into(),
                depends_on: vec!["y".into()],
            },
            ComposedService {
                service_id: "y".into(),
                crate_name: "y".into(),
                binding_endpoint: "unix:/run/y.sock".into(),
                depends_on: vec!["x".into()],
            },
        ],
        dependencies: vec![
            ServiceDependency {
                from_service: "x".into(),
                to_service: "y".into(),
                required: true,
            },
            ServiceDependency {
                from_service: "y".into(),
                to_service: "x".into(),
                required: true,
            },
        ],
        topological_order: vec![],
    };
    let err = orch.validate_external_composition(&comp).await.unwrap_err();
    assert!(matches!(
        err,
        IntegrationError::CompositionCycleDetected { .. }
    ));
}

#[tokio::test]
async fn validate_external_composition_with_missing_dep_returns_composed_service_missing() {
    let orch = orchestrator();
    let comp = ServiceComposition {
        composition_id: ComposedSystemId("ext-missing".into()),
        services: vec![ComposedService {
            service_id: "only".into(),
            crate_name: "only".into(),
            binding_endpoint: "unix:/run/only.sock".into(),
            depends_on: vec![],
        }],
        dependencies: vec![ServiceDependency {
            from_service: "only".into(),
            to_service: "missing".into(),
            required: true,
        }],
        topological_order: vec![],
    };
    let err = orch.validate_external_composition(&comp).await.unwrap_err();
    assert!(matches!(
        err,
        IntegrationError::ComposedServiceMissing { .. }
    ));
}

#[test]
fn default_composition_available_independently() {
    let comp = default_aios_composition();
    assert_eq!(comp.services.len(), 17);
    assert!(!comp.topological_order.is_empty());
}
