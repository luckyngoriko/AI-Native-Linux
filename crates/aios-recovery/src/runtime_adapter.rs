//! Capability Runtime adapter for the S9.1 recovery boundary.

#![allow(
    clippy::module_name_repetitions,
    reason = "Public names mirror the cross-crate runtime integration vocabulary"
)]

use std::sync::Arc;

use async_trait::async_trait;

use aios_capability_runtime::runtime::RuntimeRecoveryHook;

use crate::RecoveryBoundary;

/// Bridges `aios-recovery` into `aios-capability-runtime` without adding a
/// reverse dependency from the runtime crate back to recovery.
#[derive(Clone)]
pub struct RecoveryRuntimeAdapter {
    boundary: Arc<dyn RecoveryBoundary>,
}

impl std::fmt::Debug for RecoveryRuntimeAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RecoveryRuntimeAdapter")
            .field("boundary", &"<dyn RecoveryBoundary>")
            .finish()
    }
}

impl RecoveryRuntimeAdapter {
    /// Construct an adapter from any shared recovery boundary.
    #[must_use]
    pub fn new<B>(boundary: Arc<B>) -> Self
    where
        B: RecoveryBoundary + 'static,
    {
        Self { boundary }
    }

    /// Construct an adapter from an already-erased recovery boundary.
    #[must_use]
    pub fn from_dyn(boundary: Arc<dyn RecoveryBoundary>) -> Self {
        Self { boundary }
    }
}

#[async_trait]
impl RuntimeRecoveryHook for RecoveryRuntimeAdapter {
    async fn current_recovery_mode(&self) -> bool {
        self.boundary.is_recovery_active().await
    }
}
