//! Namespace mutation admission helpers.

use crate::{AiosPath, FsError, SubjectRef};

/// S4.1 namespace policy admission gate.
pub struct NamespacePolicy;

impl NamespacePolicy {
    /// Admit or reject a namespace mutation request.
    ///
    /// # Errors
    ///
    /// Returns [`FsError::InvalidPath`] when the path is outside the AIOS
    /// namespace, or [`FsError::NamespaceMutationDenied`] when the S4.1
    /// recovery/AI mutation boundary rejects the request.
    pub fn can_mutate(
        path: &AiosPath,
        _subject: &SubjectRef,
        recovery_mode: bool,
        is_ai: bool,
    ) -> Result<(), FsError> {
        let class = path
            .namespace_class()
            .ok_or_else(|| FsError::InvalidPath(path.as_str().to_owned()))?;

        if is_ai && class.is_read_only_for_ai() {
            return Err(FsError::NamespaceMutationDenied {
                path: path.as_str().to_owned(),
                reason: "AI subjects cannot mutate this namespace class".to_owned(),
            });
        }

        if class.is_recovery_only_mutation() && !recovery_mode {
            return Err(FsError::NamespaceMutationDenied {
                path: path.as_str().to_owned(),
                reason: "recovery mode required for this namespace class".to_owned(),
            });
        }

        Ok(())
    }
}
