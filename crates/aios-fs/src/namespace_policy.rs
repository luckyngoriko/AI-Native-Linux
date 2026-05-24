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
        subject: &SubjectRef,
        recovery_mode: bool,
        is_ai: bool,
    ) -> Result<(), FsError> {
        let class = path
            .namespace_class()
            .ok_or_else(|| FsError::InvalidPath(path.as_str().to_owned()))?;

        reject_reserved_ids(path)?;

        if class_is_virtual_view(class) {
            return Err(FsError::NamespaceMutationDenied {
                path: path.as_str().to_owned(),
                reason: "virtual view paths are read-only".to_owned(),
            });
        }

        if let Some(target_group_id) = group_id_from_path(path) {
            if !subject_may_access_group(subject, target_group_id) {
                return Err(FsError::NamespaceMutationDenied {
                    path: path.as_str().to_owned(),
                    reason: "cross-group access forbidden".to_owned(),
                });
            }
        }

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

const fn class_is_virtual_view(class: crate::NamespaceClass) -> bool {
    matches!(
        class,
        crate::NamespaceClass::GroupInbox
            | crate::NamespaceClass::UserInbox
            | crate::NamespaceClass::UserOutbox
    )
}

fn group_id_from_path(path: &AiosPath) -> Option<&str> {
    let rest = path.as_str().trim_end_matches('/').strip_prefix("/aios/")?;
    let mut segments = rest.split('/');
    match (segments.next(), segments.next()) {
        (Some("groups"), Some(group_id)) => Some(group_id),
        _ => None,
    }
}

fn subject_may_access_group(subject: &SubjectRef, target_group_id: &str) -> bool {
    if subject.0.starts_with("_system:") {
        return true;
    }

    subject
        .0
        .split_once(':')
        .is_some_and(|(subject_group_id, _)| subject_group_id == target_group_id)
}

fn reject_reserved_ids(path: &AiosPath) -> Result<(), FsError> {
    let rest = path
        .as_str()
        .trim_end_matches('/')
        .strip_prefix("/aios/")
        .ok_or_else(|| FsError::InvalidPath(path.as_str().to_owned()))?;
    let segments = rest.split('/').collect::<Vec<_>>();

    if let Some(group_id) = segments
        .first()
        .filter(|segment| **segment == "groups")
        .and_then(|_| segments.get(1))
    {
        reject_reserved_id(path, group_id)?;
    }

    if let Some(user_id) = segments
        .get(2)
        .filter(|segment| **segment == "users")
        .and_then(|_| segments.get(3))
    {
        reject_reserved_id(path, user_id)?;
    }

    Ok(())
}

fn reject_reserved_id(path: &AiosPath, id: &str) -> Result<(), FsError> {
    if id.starts_with('_') {
        return Err(FsError::InvalidPath(path.as_str().to_owned()));
    }

    Ok(())
}
