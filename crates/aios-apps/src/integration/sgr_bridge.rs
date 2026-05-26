//! SGR bridge — when an app declares `runtime_class = "service"`,
//! register/start a corresponding aios-sgr `ServiceNode`.

use std::sync::Arc;

use aios_sgr::{
    DesiredState, HealthCheckKind, HealthCheckSpec, ResourceBudget, RestartBudget, RestartPolicy,
    RollbackPointer, RollbackTrigger, ServiceGraph, UnitId, UnitKind, UnitManifest, UnitState,
};

use crate::app_profile::AppProfile;
use crate::error::AppsError;
use crate::package::PackageId;

/// Bridge that wraps a [`ServiceGraph`] and exposes apps-specific service
/// lifecycle methods.
pub struct SgrBridge {
    graph: Arc<dyn ServiceGraph + Send + Sync>,
}

impl SgrBridge {
    /// Create a new bridge over the supplied service graph.
    #[must_use]
    pub fn new(graph: Arc<dyn ServiceGraph + Send + Sync>) -> Self {
        Self { graph }
    }

    /// Register a service unit derived from the given package and profile.
    ///
    /// Only valid when `runtime_class` is `"service"`. Builds a minimal
    /// [`UnitManifest`] and calls [`ServiceGraph::register_unit`].
    ///
    /// # Errors
    ///
    /// Returns [`AppsError::InvalidRuntimeClass`] when `runtime_class` is
    /// not `"service"`. Propagates [`aios_sgr::SgrError`] wrapped as
    /// [`AppsError::RuntimeReject`].
    pub async fn register_service(
        &self,
        package_id: &PackageId,
        app_profile: &AppProfile,
        runtime_class: &str,
    ) -> Result<UnitId, AppsError> {
        if runtime_class != "service" {
            return Err(AppsError::InvalidRuntimeClass(format!(
                "expected runtime_class=\"service\", got \"{runtime_class}\""
            )));
        }

        let unit_id = UnitId::from_parts("aios", &format!("svc_{pid}", pid = &package_id.0), None)
            .map_err(|e| AppsError::RuntimeReject(format!("invalid unit id: {e}")))?;

        let manifest = UnitManifest {
            schema_version: "aios.unit.v1alpha1".into(),
            unit_id: unit_id.clone(),
            unit_kind: UnitKind::Service,
            display_name: format!("app-{pid}", pid = &package_id.0),
            description: format!(
                "Service unit for {app_id} ({runtime})",
                app_id = app_profile.app_id,
                runtime = app_profile.ecosystem_runtime,
            ),
            issued_at: chrono::Utc::now(),
            publisher_id: "aios-apps".into(),
            publisher_root_id: "aios-apps".into(),
            publisher_signature: Vec::new(),
            canonical_hash: String::new(),
            dependencies: Vec::new(),
            sandbox_profile_ref: String::new(),
            verification_intent: Vec::new(),
            rollback_pointer: RollbackPointer {
                aiosfs_pointer_id: String::new(),
                expected_current_version_id: String::new(),
                trigger: RollbackTrigger::Never,
            },
            resource_budget: ResourceBudget {
                memory_bytes_max: 512 * 1024 * 1024,
                cpu_quota_cores: 1.0,
                disk_bytes_max: 1024 * 1024 * 1024,
                file_descriptors_max: 1024,
                process_count_max: 64,
                queue_depth_max: 32,
                gpu: None,
            },
            restart_policy: RestartPolicy::Always,
            restart_budget: RestartBudget {
                max_attempts: 3,
                reset_window_seconds: 300,
                backoff_initial_seconds: 1,
                backoff_max_seconds: 30,
            },
            health_check: HealthCheckSpec {
                kind: HealthCheckKind::TcpPort,
                probe_interval_seconds: 10,
                probe_timeout_seconds: 3,
                startup_grace_seconds: 30,
                unhealthy_threshold: 3,
                healthy_threshold: 2,
                args: serde_json::Value::Null,
            },
            startup_deadline_seconds: 30,
            stop_deadline_seconds: 10,
            adapter_target: serde_json::json!({
                "package_id": package_id.0,
            }),
            labels: None,
            correlation_id: None,
            desired_state: DesiredState::Running,
            provides: Vec::new(),
            adapter_id: None,
        };

        let unit = self
            .graph
            .register_unit(manifest)
            .await
            .map_err(|e| AppsError::RuntimeReject(format!("sgr register failed: {e}")))?;

        Ok(unit.unit_id)
    }

    /// Start a previously registered service node by transitioning it to
    /// [`UnitState::Running`] via [`ServiceGraph::set_unit_state`].
    ///
    /// # Errors
    ///
    /// Returns [`AppsError::NotFound`] when the node is absent from the
    /// graph. Propagates [`aios_sgr::SgrError`] wrapped as
    /// [`AppsError::RuntimeReject`].
    pub async fn start_service(&self, node_id: &UnitId) -> Result<(), AppsError> {
        // Verify the unit exists.
        let _existing = self
            .graph
            .get_unit(node_id)
            .await
            .map_err(|e| AppsError::NotFound(format!("service node not registered: {e}")))?;

        self.graph
            .set_unit_state(node_id, UnitState::Running)
            .await
            .map_err(|e| AppsError::RuntimeReject(format!("sgr start failed: {e}")))?;

        Ok(())
    }
}
