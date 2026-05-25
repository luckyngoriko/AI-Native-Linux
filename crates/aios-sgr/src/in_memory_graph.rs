//! In-memory S15.1 service-graph implementation.

use std::collections::HashMap;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::Serialize;
use tokio::sync::RwLock;

use crate::graph::ServiceGraph;
use crate::{
    DependencyEdge, DependencyKind, DesiredState, GraphState, HealthCheckSpec, ResourceBudget,
    RestartBudget, RestartPolicy, RollbackPointer, ServiceUnit, SgrError, UnitDependency, UnitId,
    UnitKind, UnitManifest, UnitState, VerificationIntentRef,
};

/// HashMap-backed [`ServiceGraph`] used by tests and future SGR service shells.
#[derive(Debug, Default)]
pub struct InMemoryServiceGraph {
    units: RwLock<HashMap<UnitId, ServiceUnit>>,
    dependencies: RwLock<HashMap<UnitId, Vec<DependencyEdge>>>,
    trusted_authorities: HashMap<String, VerifyingKey>,
}

impl InMemoryServiceGraph {
    /// Construct an empty graph with no trusted authorities.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Construct an empty graph trusting one unit-manifest signing authority.
    #[must_use]
    pub fn with_trusted_authority(name: impl Into<String>, key: VerifyingKey) -> Self {
        let mut graph = Self::new();
        graph.trusted_authorities.insert(name.into(), key);
        graph
    }

    fn verify_manifest(&self, manifest: &UnitManifest) -> Result<(), SgrError> {
        let verifying_key = self
            .trusted_authorities
            .get(&manifest.publisher_root_id)
            .ok_or_else(|| {
                SgrError::ManifestUnknownAuthority(manifest.publisher_root_id.clone())
            })?;
        let signature_bytes: [u8; 64] = manifest
            .publisher_signature
            .as_slice()
            .try_into()
            .map_err(|_| SgrError::ManifestSignatureInvalid)?;
        let signature = Signature::from_bytes(&signature_bytes);
        let digest = canonical_manifest_digest(manifest)?;
        let digest_hex = digest.to_hex().to_string();
        if manifest.canonical_hash != digest_hex[..32] {
            return Err(SgrError::ManifestSignatureInvalid);
        }
        verifying_key
            .verify(digest.as_bytes(), &signature)
            .map_err(|_| SgrError::ManifestSignatureInvalid)
    }
}

#[async_trait]
impl ServiceGraph for InMemoryServiceGraph {
    async fn register_unit(&self, manifest: UnitManifest) -> Result<ServiceUnit, SgrError> {
        self.verify_manifest(&manifest)?;
        let unit_id = manifest.unit_id.clone();
        let declared_edges = manifest
            .dependencies
            .iter()
            .map(|dependency| DependencyEdge {
                from_unit_id: unit_id.clone(),
                to_unit_id: dependency.unit_id.clone(),
                kind: dependency.kind,
            })
            .collect::<Vec<_>>();

        let unit = ServiceUnit {
            unit_id: unit_id.clone(),
            manifest,
            state: UnitState::Queued,
            last_transition_at: Utc::now(),
            evidence_chain: Vec::new(),
        };

        let mut units = self.units.write().await;
        if units.contains_key(&unit_id) {
            return Err(SgrError::UnitAlreadyRegistered(unit_id));
        }
        for edge in &declared_edges {
            if edge.to_unit_id != unit_id && !units.contains_key(&edge.to_unit_id) {
                return Err(SgrError::DependencyTargetNotRegistered(
                    edge.to_unit_id.clone(),
                ));
            }
        }
        units.insert(unit_id.clone(), unit.clone());
        drop(units);

        if !declared_edges.is_empty() {
            let mut dependencies = self.dependencies.write().await;
            dependencies
                .entry(unit_id)
                .or_default()
                .extend(declared_edges);
        }

        Ok(unit)
    }

    async fn get_unit(&self, unit_id: &UnitId) -> Result<ServiceUnit, SgrError> {
        let units = self.units.read().await;
        units
            .get(unit_id)
            .cloned()
            .ok_or_else(|| SgrError::UnitNotFound(unit_id.clone()))
    }

    async fn list_units(&self) -> Result<Vec<ServiceUnit>, SgrError> {
        let units = self.units.read().await;
        Ok(units.values().cloned().collect())
    }

    async fn declare_dependency(
        &self,
        from: &UnitId,
        to: &UnitId,
        kind: DependencyKind,
    ) -> Result<DependencyEdge, SgrError> {
        let units = self.units.read().await;
        if !units.contains_key(from) {
            return Err(SgrError::UnitNotFound(from.clone()));
        }
        if !units.contains_key(to) {
            return Err(SgrError::DependencyTargetNotRegistered(to.clone()));
        }
        drop(units);

        let edge = DependencyEdge {
            from_unit_id: from.clone(),
            to_unit_id: to.clone(),
            kind,
        };
        let mut dependencies = self.dependencies.write().await;
        dependencies
            .entry(from.clone())
            .or_default()
            .push(edge.clone());
        drop(dependencies);
        Ok(edge)
    }

    async fn list_dependencies(&self, unit_id: &UnitId) -> Result<Vec<DependencyEdge>, SgrError> {
        let units = self.units.read().await;
        if !units.contains_key(unit_id) {
            return Err(SgrError::UnitNotFound(unit_id.clone()));
        }
        drop(units);

        let dependencies = self.dependencies.read().await;
        Ok(dependencies.get(unit_id).cloned().unwrap_or_default())
    }

    async fn graph_state(&self) -> Result<GraphState, SgrError> {
        let units = self.units.read().await;
        let state = if units.is_empty() {
            GraphState::Empty
        } else if units.values().any(|unit| unit.state == UnitState::Failed) {
            let failed_count = units
                .values()
                .filter(|unit| unit.state == UnitState::Failed)
                .count();
            let critical_failed = units
                .values()
                .any(|unit| unit.state == UnitState::Failed && is_critical_unit(unit));
            if critical_failed || failed_count == units.len() {
                GraphState::Failed
            } else {
                GraphState::Degraded
            }
        } else if units.values().any(|unit| {
            matches!(
                unit.state,
                UnitState::Degraded | UnitState::Unhealthy | UnitState::Retired
            )
        }) {
            GraphState::Degraded
        } else if units.values().any(|unit| {
            matches!(
                unit.state,
                UnitState::Draft | UnitState::Queued | UnitState::Starting | UnitState::Stopping
            )
        }) {
            GraphState::Converging
        } else {
            GraphState::Converged
        };
        drop(units);
        Ok(state)
    }

    async fn set_unit_state(
        &self,
        unit_id: &UnitId,
        new_state: UnitState,
    ) -> Result<ServiceUnit, SgrError> {
        let mut units = self.units.write().await;
        let unit = units
            .get_mut(unit_id)
            .ok_or_else(|| SgrError::UnitNotFound(unit_id.clone()))?;
        let from = unit.state;
        if !is_valid_transition(from, new_state) {
            return Err(SgrError::InvalidStateTransition {
                from,
                to: new_state,
            });
        }
        unit.state = new_state;
        unit.last_transition_at = Utc::now();
        let updated = unit.clone();
        drop(units);
        Ok(updated)
    }
}

#[derive(Debug, Serialize)]
struct SignedUnitManifestBody<'a> {
    schema_version: &'a str,
    unit_id: &'a UnitId,
    unit_kind: &'a UnitKind,
    display_name: &'a str,
    description: &'a str,
    issued_at: &'a DateTime<Utc>,
    publisher_id: &'a str,
    publisher_root_id: &'a str,
    dependencies: &'a [UnitDependency],
    sandbox_profile_ref: &'a str,
    verification_intent: &'a [VerificationIntentRef],
    rollback_pointer: &'a RollbackPointer,
    resource_budget: &'a ResourceBudget,
    restart_policy: &'a RestartPolicy,
    restart_budget: &'a RestartBudget,
    health_check: &'a HealthCheckSpec,
    startup_deadline_seconds: u32,
    stop_deadline_seconds: u32,
    adapter_target: &'a serde_json::Value,
    labels: &'a Option<serde_json::Value>,
    correlation_id: &'a Option<String>,
    desired_state: &'a DesiredState,
    provides: &'a [String],
    adapter_id: &'a Option<String>,
}

impl<'a> From<&'a UnitManifest> for SignedUnitManifestBody<'a> {
    fn from(manifest: &'a UnitManifest) -> Self {
        Self {
            schema_version: &manifest.schema_version,
            unit_id: &manifest.unit_id,
            unit_kind: &manifest.unit_kind,
            display_name: &manifest.display_name,
            description: &manifest.description,
            issued_at: &manifest.issued_at,
            publisher_id: &manifest.publisher_id,
            publisher_root_id: &manifest.publisher_root_id,
            dependencies: &manifest.dependencies,
            sandbox_profile_ref: &manifest.sandbox_profile_ref,
            verification_intent: &manifest.verification_intent,
            rollback_pointer: &manifest.rollback_pointer,
            resource_budget: &manifest.resource_budget,
            restart_policy: &manifest.restart_policy,
            restart_budget: &manifest.restart_budget,
            health_check: &manifest.health_check,
            startup_deadline_seconds: manifest.startup_deadline_seconds,
            stop_deadline_seconds: manifest.stop_deadline_seconds,
            adapter_target: &manifest.adapter_target,
            labels: &manifest.labels,
            correlation_id: &manifest.correlation_id,
            desired_state: &manifest.desired_state,
            provides: &manifest.provides,
            adapter_id: &manifest.adapter_id,
        }
    }
}

fn canonical_manifest_digest(manifest: &UnitManifest) -> Result<blake3::Hash, SgrError> {
    let body = SignedUnitManifestBody::from(manifest);
    let bytes = serde_json::to_vec(&body)
        .map_err(|err| SgrError::Internal(format!("unit manifest serialise: {err}")))?;
    Ok(blake3::hash(&bytes))
}

const fn is_valid_transition(from: UnitState, to: UnitState) -> bool {
    matches!(
        (from, to),
        (
            UnitState::Draft | UnitState::Stopped,
            UnitState::Queued | UnitState::Retired,
        ) | (
            UnitState::Queued | UnitState::Failed,
            UnitState::Starting | UnitState::Retired,
        ) | (UnitState::Starting, UnitState::Running | UnitState::Failed)
            | (
                UnitState::Running,
                UnitState::Healthy | UnitState::Stopped | UnitState::Failed
            )
            | (
                UnitState::Healthy,
                UnitState::Degraded | UnitState::Unhealthy | UnitState::Stopping,
            )
            | (
                UnitState::Degraded,
                UnitState::Healthy | UnitState::Unhealthy | UnitState::Stopping,
            )
            | (
                UnitState::Unhealthy,
                UnitState::Healthy | UnitState::Starting | UnitState::Failed | UnitState::Stopping,
            )
            | (UnitState::Stopping, UnitState::Stopped | UnitState::Failed)
    )
}

fn is_critical_unit(unit: &ServiceUnit) -> bool {
    unit.manifest
        .labels
        .as_ref()
        .and_then(|labels| labels.get("criticality"))
        .and_then(serde_json::Value::as_str)
        .is_some_and(|value| value.eq_ignore_ascii_case("critical"))
}
