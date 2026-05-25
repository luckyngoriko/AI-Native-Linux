//! T-078 `RecoveryGuard` cross-crate INV-012 runtime enforcement tests.

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "Integration-test failures should point at the failing contract"
)]

use std::error::Error;
use std::sync::Arc;

use aios_fs::{AiosPath, FsError, NamespacePolicy, SubjectRef};
use aios_recovery::{
    BootPhase, EnterRecoveryRequest, InMemoryRecoveryBoundary, RecoveryBoundary, RecoveryError,
    RecoveryGuard,
};

type TestResult<T = ()> = Result<T, Box<dyn Error + Send + Sync>>;

fn subject(id: &str) -> SubjectRef {
    SubjectRef(id.to_owned())
}

fn operator_request() -> EnterRecoveryRequest {
    EnterRecoveryRequest {
        reason: "OPERATOR_INITIATED".to_owned(),
        operator_grant: Some("ovr_01HXY8K2JPQ7N3M4R5S6T7V8W9".to_owned()),
        expected_phases: vec![BootPhase::Recovery],
        bundle: None,
    }
}

fn boundary_and_guard() -> (Arc<InMemoryRecoveryBoundary>, RecoveryGuard) {
    let boundary = Arc::new(InMemoryRecoveryBoundary::new());
    let guard = RecoveryGuard::new(boundary.clone());
    (boundary, guard)
}

async fn enter_recovery(boundary: &InMemoryRecoveryBoundary) -> TestResult {
    boundary.enter_recovery(operator_request()).await?;
    Ok(())
}

async fn exit_recovery(boundary: &InMemoryRecoveryBoundary) -> TestResult {
    let token = boundary
        .current_exit_token()
        .await
        .ok_or_else(|| RecoveryError::Internal("missing exit token".to_owned()))?;
    boundary.exit_recovery(&token).await?;
    Ok(())
}

#[tokio::test]
async fn normal_mode_denies_recovery_only_path() {
    let (_boundary, guard) = boundary_and_guard();
    let path = AiosPath::new("/aios/system/policy/active.bundle");

    let err = guard
        .check_mutation(&path, &subject("family:alice"), false)
        .await
        .expect_err("normal-mode recovery-only mutation must deny");

    assert!(matches!(
        err,
        RecoveryError::RecoveryOnlyPathMutationDenied { path, reason }
            if path == "/aios/system/policy/active.bundle"
                && reason.contains("recovery mode required")
    ));
}

#[tokio::test]
async fn recovery_mode_allows_recovery_only_path() -> TestResult {
    let (boundary, guard) = boundary_and_guard();
    enter_recovery(&boundary).await?;
    let path = AiosPath::new("/aios/system/policy/active.bundle");

    assert_eq!(
        guard
            .check_mutation(&path, &subject("_system:recovery:operator"), false)
            .await,
        Ok(())
    );
    Ok(())
}

#[tokio::test]
async fn normal_mode_allows_user_space_path() {
    let (_boundary, guard) = boundary_and_guard();
    let path = AiosPath::new("/aios/groups/family/users/alice/home/notes.md");

    assert_eq!(
        guard
            .check_mutation(&path, &subject("family:alice"), false)
            .await,
        Ok(())
    );
}

#[tokio::test]
async fn ai_subject_denied_ai_locked_path() {
    let (_boundary, guard) = boundary_and_guard();
    let path = AiosPath::new("/aios/system/apps/evidence-viewer");

    let err = guard
        .check_mutation(&path, &subject("agent:coder"), true)
        .await
        .expect_err("AI mutation of AI-locked namespace must deny");

    assert!(matches!(
        err,
        RecoveryError::AiPathMutationDenied { path }
            if path == "/aios/system/apps/evidence-viewer"
    ));
}

#[tokio::test]
async fn ai_subject_allowed_ai_allowed_path() {
    let (_boundary, guard) = boundary_and_guard();
    let path = AiosPath::new("/aios/groups/family/users/alice/home/notes.md");

    assert_eq!(
        guard
            .check_mutation(&path, &subject("family:alice"), true)
            .await,
        Ok(())
    );
}

#[tokio::test]
async fn human_subject_allowed_ai_locked_path() {
    let (_boundary, guard) = boundary_and_guard();
    let path = AiosPath::new("/aios/system/apps/evidence-viewer");

    assert_eq!(
        guard
            .check_mutation(&path, &subject("family:alice"), false)
            .await,
        Ok(())
    );
}

#[tokio::test]
async fn live_boundary_enter_then_exit_changes_recovery_only_result() -> TestResult {
    let (boundary, guard) = boundary_and_guard();
    let path = AiosPath::new("/aios/system/policy/active.bundle");
    let operator = subject("_system:recovery:operator");

    enter_recovery(&boundary).await?;
    assert_eq!(guard.check_mutation(&path, &operator, false).await, Ok(()));

    exit_recovery(&boundary).await?;
    let err = guard
        .check_mutation(&path, &operator, false)
        .await
        .expect_err("recovery-only mutation must deny after recovery exit");
    assert!(matches!(
        err,
        RecoveryError::RecoveryOnlyPathMutationDenied { path, .. }
            if path == "/aios/system/policy/active.bundle"
    ));
    Ok(())
}

#[tokio::test]
async fn concurrent_checks_read_consistent_boundary_state() -> TestResult {
    let boundary = Arc::new(InMemoryRecoveryBoundary::new());
    let guard = Arc::new(RecoveryGuard::new(boundary));
    let path = AiosPath::new("/aios/system/policy/active.bundle");
    let operator = subject("_system:recovery:operator");
    let expected = RecoveryError::RecoveryOnlyPathMutationDenied {
        path: "/aios/system/policy/active.bundle".to_owned(),
        reason: "recovery mode required for this namespace class".to_owned(),
    };

    let mut handles = Vec::new();
    for _ in 0..5 {
        let guard = Arc::clone(&guard);
        let path = path.clone();
        let operator = operator.clone();
        handles.push(tokio::spawn(async move {
            guard.check_mutation(&path, &operator, false).await
        }));
    }

    for handle in handles {
        assert_eq!(handle.await?, Err(expected.clone()));
    }
    Ok(())
}

#[tokio::test]
async fn guard_ok_matches_namespace_policy_ok_for_user_space() {
    let (_boundary, guard) = boundary_and_guard();
    let path = AiosPath::new("/aios/groups/family/users/alice/home/notes.md");
    let actor = subject("family:alice");

    assert_eq!(
        NamespacePolicy::can_mutate(&path, &actor, false, false),
        Ok(())
    );
    assert_eq!(guard.check_mutation(&path, &actor, false).await, Ok(()));
}

#[tokio::test]
async fn guard_recovery_denial_preserves_namespace_policy_path_and_reason() {
    let (_boundary, guard) = boundary_and_guard();
    let path = AiosPath::new("/aios/system/policy/active.bundle");
    let actor = subject("family:alice");

    let FsError::NamespaceMutationDenied {
        path: policy_path,
        reason: policy_reason,
    } = NamespacePolicy::can_mutate(&path, &actor, false, false)
        .expect_err("namespace policy must deny recovery-only mutation")
    else {
        panic!("expected namespace mutation denial");
    };

    let err = guard
        .check_mutation(&path, &actor, false)
        .await
        .expect_err("guard must map recovery-only policy denial");
    assert!(matches!(
        err,
        RecoveryError::RecoveryOnlyPathMutationDenied { path, reason }
            if path == policy_path && reason == policy_reason
    ));
}

#[tokio::test]
async fn guard_ai_denial_maps_namespace_policy_ai_denial() {
    let (_boundary, guard) = boundary_and_guard();
    let path = AiosPath::new("/aios/system/apps/evidence-viewer");
    let actor = subject("agent:coder");

    let FsError::NamespaceMutationDenied {
        path: policy_path, ..
    } = NamespacePolicy::can_mutate(&path, &actor, false, true)
        .expect_err("namespace policy must deny AI-locked mutation")
    else {
        panic!("expected namespace mutation denial");
    };

    let err = guard
        .check_mutation(&path, &actor, true)
        .await
        .expect_err("guard must map AI policy denial");
    assert!(matches!(
        err,
        RecoveryError::AiPathMutationDenied { path } if path == policy_path
    ));
}

#[tokio::test]
async fn foreign_namespace_returns_policy_invalid_path_as_internal_error() {
    let (_boundary, guard) = boundary_and_guard();
    let path = AiosPath::new("/etc/passwd");
    let actor = subject("family:alice");

    let policy_err = NamespacePolicy::can_mutate(&path, &actor, false, false)
        .expect_err("namespace policy must reject foreign paths");
    assert!(matches!(policy_err, FsError::InvalidPath(path) if path == "/etc/passwd"));

    let err = guard
        .check_mutation(&path, &actor, false)
        .await
        .expect_err("guard must surface foreign namespace rejection");
    assert!(matches!(
        err,
        RecoveryError::Internal(message)
            if message.contains("invalid AIOS path") && message.contains("/etc/passwd")
    ));
}
