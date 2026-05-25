//! Runtime namespace mutation guard for S9.1 INV-012.

use std::sync::Arc;

use aios_fs::{AiosPath, FsError, NamespaceClass, NamespacePolicy, SubjectRef};

use crate::{RecoveryBoundary, RecoveryError};

/// Cross-crate guard that binds live recovery state to AIOS-FS namespace policy.
pub struct RecoveryGuard {
    boundary: Arc<dyn RecoveryBoundary>,
}

impl RecoveryGuard {
    /// Construct a recovery guard over a live recovery boundary.
    #[must_use]
    pub fn new(boundary: Arc<dyn RecoveryBoundary>) -> Self {
        Self { boundary }
    }

    /// Admit or reject a namespace mutation under the current recovery state.
    ///
    /// # Errors
    ///
    /// Returns [`RecoveryError::RecoveryOnlyPathMutationDenied`] when an
    /// S4.1 recovery-only namespace is mutated outside recovery mode, or
    /// [`RecoveryError::AiPathMutationDenied`] when an AI subject attempts to
    /// mutate an AI-locked namespace class. Other namespace-policy errors are
    /// preserved through [`RecoveryError::Internal`] until the recovery crate
    /// grows a wider FS error surface.
    pub async fn check_mutation(
        &self,
        path: &AiosPath,
        subject: &SubjectRef,
        is_ai: bool,
    ) -> Result<(), RecoveryError> {
        let recovery_mode = self.boundary.is_recovery_active().await;
        let namespace_class = path.namespace_class();

        NamespacePolicy::can_mutate(path, subject, recovery_mode, is_ai).map_err(|err| {
            map_namespace_policy_error(&err, path, namespace_class, recovery_mode, is_ai)
        })
    }
}

fn map_namespace_policy_error(
    err: &FsError,
    path: &AiosPath,
    namespace_class: Option<NamespaceClass>,
    recovery_mode: bool,
    is_ai: bool,
) -> RecoveryError {
    if namespace_class.is_some_and(|class| class.is_recovery_only_mutation()) && !recovery_mode {
        return RecoveryError::RecoveryOnlyPathMutationDenied {
            path: path.as_str().to_owned(),
            reason: "recovery mode required for this namespace class".to_owned(),
        };
    }

    if namespace_class.is_some_and(|class| class.is_read_only_for_ai()) && is_ai {
        return RecoveryError::AiPathMutationDenied {
            path: path.as_str().to_owned(),
        };
    }

    RecoveryError::Internal(err.to_string())
}
