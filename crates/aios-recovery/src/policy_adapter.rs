//! Policy hydrator adapter for the S9.1 recovery boundary.

#![allow(
    clippy::module_name_repetitions,
    reason = "Public names mirror the cross-crate policy integration vocabulary"
)]

use std::sync::Arc;

use async_trait::async_trait;

use aios_policy::{HydratedSubject, PolicyError, SubjectHydrator};

use crate::RecoveryBoundary;

/// Wraps a policy subject hydrator and replaces `subject.recovery_mode` with
/// the live S9.1 recovery-boundary state.
#[derive(Clone)]
pub struct RecoveryPolicyHydratorEnhancer {
    inner: Arc<dyn SubjectHydrator>,
    boundary: Arc<dyn RecoveryBoundary>,
}

impl std::fmt::Debug for RecoveryPolicyHydratorEnhancer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RecoveryPolicyHydratorEnhancer")
            .field("inner", &"<dyn SubjectHydrator>")
            .field("boundary", &"<dyn RecoveryBoundary>")
            .finish()
    }
}

impl RecoveryPolicyHydratorEnhancer {
    /// Construct an enhancer from a base policy hydrator and recovery boundary.
    #[must_use]
    pub fn new<B>(inner: Arc<dyn SubjectHydrator>, boundary: Arc<B>) -> Self
    where
        B: RecoveryBoundary + 'static,
    {
        Self { inner, boundary }
    }

    /// Construct an enhancer from already-erased trait objects.
    #[must_use]
    pub fn from_dyn(inner: Arc<dyn SubjectHydrator>, boundary: Arc<dyn RecoveryBoundary>) -> Self {
        Self { inner, boundary }
    }
}

#[async_trait]
impl SubjectHydrator for RecoveryPolicyHydratorEnhancer {
    async fn hydrate(&self, provisional: &str) -> Result<HydratedSubject, PolicyError> {
        let mut subject = self.inner.hydrate(provisional).await?;
        subject.recovery_mode = self.boundary.is_recovery_active().await;
        Ok(subject)
    }
}
