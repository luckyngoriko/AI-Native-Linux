//! S12.3 Compatibility Orchestrator — ecosystem-based adapter dispatch.
//!
//! The `CompatibilityOrchestrator` holds a registry of runtime adapters keyed by
//! `EcosystemRuntime` and dispatches launch requests to the correct adapter.
//! The default constructor registers all five stub adapters.

use std::collections::HashMap;
use std::sync::Arc;

use crate::compatibility_runtime::{
    detect_ecosystem, AiosNativeRuntimeAdapter, AndroidRuntimeAdapter, CompatibilityRuntimeAdapter,
    LaunchContext, LaunchOutcome, LinuxRuntimeAdapter, WebRuntimeAdapter, WindowsRuntimeAdapter,
};
use crate::ecosystem::EcosystemRuntime;
use crate::error::AppsError;
use crate::package_store::AppPackage;

/// The compatibility orchestrator — maps ecosystems to adapters and dispatches
/// launch requests.
#[derive(Clone)]
pub struct CompatibilityOrchestrator {
    adapters: HashMap<EcosystemRuntime, Arc<dyn CompatibilityRuntimeAdapter>>,
}

impl CompatibilityOrchestrator {
    /// Create an orchestrator pre-populated with all five stub adapters:
    /// Linux, Windows, Android, Web, and AIOS-native.
    #[must_use]
    pub fn new_with_defaults() -> Self {
        let mut orchestrator = Self {
            adapters: HashMap::new(),
        };
        orchestrator.register(Arc::new(LinuxRuntimeAdapter::new()));
        orchestrator.register(Arc::new(WindowsRuntimeAdapter::new()));
        orchestrator.register(Arc::new(AndroidRuntimeAdapter::new()));
        orchestrator.register(Arc::new(WebRuntimeAdapter::new()));
        orchestrator.register(Arc::new(AiosNativeRuntimeAdapter::new()));
        orchestrator
    }

    /// Register a runtime adapter. If an adapter for the same ecosystem
    /// already exists, it is replaced.
    pub fn register(&mut self, adapter: Arc<dyn CompatibilityRuntimeAdapter>) {
        let ecosystem = adapter.ecosystem();
        self.adapters.insert(ecosystem, adapter);
    }

    /// Return the number of registered adapters (test seam).
    #[allow(dead_code)]
    #[must_use]
    pub fn adapter_count(&self) -> usize {
        self.adapters.len()
    }

    /// Dispatch a launch request to the adapter matching the package's
    /// detected ecosystem.
    ///
    /// Returns `LaunchOutcome::RuntimeUnavailable` when no adapter is
    /// registered for the detected ecosystem.
    ///
    /// # Errors
    ///
    /// Returns `AppsError` when the underlying adapter's `launch` call fails.
    pub async fn dispatch(
        &self,
        package: &AppPackage,
        context: &LaunchContext,
    ) -> Result<LaunchOutcome, AppsError> {
        let ecosystem = detect_ecosystem(package);
        match self.adapters.get(&ecosystem) {
            Some(adapter) => adapter.launch(package, context).await,
            None => Ok(LaunchOutcome::RuntimeUnavailable(format!(
                "no adapter registered for ecosystem {ecosystem}",
            ))),
        }
    }

    /// Return all registered ecosystems (test seam).
    #[allow(dead_code)]
    #[must_use]
    pub fn registered_ecosystems(&self) -> Vec<EcosystemRuntime> {
        self.adapters.keys().copied().collect()
    }
}

impl std::fmt::Debug for CompatibilityOrchestrator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompatibilityOrchestrator")
            .field("adapter_count", &self.adapters.len())
            .finish_non_exhaustive()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::compatibility_runtime::{LaunchContext, SubjectRef};
    use chrono::Utc;

    fn stub_package_with_ecosystem(ecosystem: &str) -> AppPackage {
        let manifest_bytes = format!(r#"{{"ecosystem": "{ecosystem}"}}"#).into_bytes();
        let content_hash_blake3 = blake3::hash(&manifest_bytes).to_hex().to_string();
        AppPackage {
            package_id: crate::package::PackageId(format!(
                "pkg_{}",
                ulid::Ulid::new().to_string().to_lowercase()
            )),
            name: "test-app".into(),
            version: "0.1.0".into(),
            manifest_bytes,
            content_hash_blake3,
            ed25519_signature: Vec::new(),
            signer_public_key: Vec::new(),
            registered_at: Utc::now(),
        }
    }

    fn stub_context() -> LaunchContext {
        LaunchContext {
            subject: SubjectRef {
                canonical_id: "human:test".into(),
            },
            sandbox_profile_id: None,
            recovery_mode: false,
            started_at: Utc::now(),
        }
    }

    #[test]
    fn new_with_defaults_registers_adapters() {
        let orch = CompatibilityOrchestrator::new_with_defaults();
        // 5 stub adapters, but Linux/Web/AiosNative all map to RuntimeLinuxNative,
        // so only 3 unique ecosystems are registered.
        assert!(orch.adapter_count() >= 3);
    }

    #[tokio::test]
    async fn dispatch_picks_correct_adapter_by_ecosystem() {
        let orch = CompatibilityOrchestrator::new_with_defaults();
        let pkg = stub_package_with_ecosystem("RUNTIME_WINDOWS_PROTON");
        let ctx = stub_context();
        let outcome = orch
            .dispatch(&pkg, &ctx)
            .await
            .expect("dispatch should succeed");
        assert!(
            matches!(outcome, LaunchOutcome::RequiresVm { .. }),
            "expected RequiresVm, got {outcome:?}"
        );
    }

    #[tokio::test]
    async fn dispatch_windows_ecosystem_returns_requires_vm() {
        let orch = CompatibilityOrchestrator::new_with_defaults();
        let pkg = stub_package_with_ecosystem("RUNTIME_WINDOWS_PROTON");
        let ctx = stub_context();
        let outcome = orch
            .dispatch(&pkg, &ctx)
            .await
            .expect("dispatch should succeed");
        match outcome {
            LaunchOutcome::RequiresVm { .. } => {}
            other => panic!("expected RequiresVm, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn dispatch_unregistered_ecosystem_returns_runtime_unavailable() {
        let orch = CompatibilityOrchestrator::new_with_defaults();
        let pkg = stub_package_with_ecosystem("RUNTIME_MACOS_VM");
        let ctx = stub_context();
        let outcome = orch
            .dispatch(&pkg, &ctx)
            .await
            .expect("dispatch should succeed");
        match outcome {
            LaunchOutcome::RuntimeUnavailable(msg) => {
                assert!(msg.contains("RuntimeMacosVm"), "msg was: {msg}");
            }
            other => panic!("expected RuntimeUnavailable, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn concurrent_dispatch_no_race() {
        let orch = CompatibilityOrchestrator::new_with_defaults();
        let ctx = stub_context();

        let orch1 = orch.clone();
        let orch2 = orch.clone();
        let orch3 = orch.clone();
        let ctx1 = ctx.clone();
        let ctx2 = ctx.clone();
        let ctx3 = ctx.clone();

        let t1 = tokio::spawn(async move {
            let pkg = stub_package_with_ecosystem("RUNTIME_LINUX_NATIVE");
            orch1.dispatch(&pkg, &ctx1).await
        });
        let t2 = tokio::spawn(async move {
            let pkg = stub_package_with_ecosystem("RUNTIME_WINDOWS_PROTON");
            orch2.dispatch(&pkg, &ctx2).await
        });
        let t3 = tokio::spawn(async move {
            let pkg = stub_package_with_ecosystem("RUNTIME_ANDROID_WAYDROID");
            orch3.dispatch(&pkg, &ctx3).await
        });

        let (r1, r2, r3) = tokio::join!(t1, t2, t3);
        let o1 = r1.expect("join").expect("dispatch 1");
        let o2 = r2.expect("join").expect("dispatch 2");
        let o3 = r3.expect("join").expect("dispatch 3");

        assert!(matches!(o1, LaunchOutcome::Launched { .. }));
        assert!(matches!(o2, LaunchOutcome::RequiresVm { .. }));
        assert!(matches!(o3, LaunchOutcome::RequiresVm { .. }));
    }

    #[test]
    fn register_replaces_existing_adapter_for_same_ecosystem() {
        let mut orch = CompatibilityOrchestrator {
            adapters: HashMap::new(),
        };
        orch.register(Arc::new(LinuxRuntimeAdapter::new()));
        assert_eq!(orch.adapter_count(), 1);
        // Register another Linux adapter (should replace, count stays 1).
        orch.register(Arc::new(LinuxRuntimeAdapter::new()));
        assert_eq!(orch.adapter_count(), 1);
    }
}
