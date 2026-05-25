//! T-076 first-boot provisioning driver tests for S9.2.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    reason = "Integration-test failures should point at the failing contract"
)]

use std::error::Error;
use std::sync::Arc;

use aios_recovery::first_boot::{FirstBootStageStatus, FIRST_BOOT_PROVISIONING_PHASES};
use aios_recovery::{
    BootPhase, EnterRecoveryRequest, FirstBootContext, FirstBootDriver, FirstBootPhase,
    FirstBootStatus, InMemoryRecoveryBoundary, RecoveryBoundary, RecoveryError,
};

type TestResult<T = ()> = Result<T, Box<dyn Error + Send + Sync>>;

fn driver() -> FirstBootDriver {
    FirstBootDriver::new(Arc::new(InMemoryRecoveryBoundary::new()))
}

fn fallback_request() -> EnterRecoveryRequest {
    EnterRecoveryRequest {
        reason: "BOOT_FAILURE_AUTO".to_owned(),
        operator_grant: None,
        expected_phases: vec![BootPhase::Recovery],
        bundle: None,
    }
}

#[tokio::test]
async fn new_driver_starts_with_not_started_context() -> TestResult {
    let driver = driver();

    let context = driver.current_context().await;

    assert_eq!(context.status, FirstBootStatus::NotStarted);
    assert!(context.performed_phases.is_empty());
    Ok(())
}

#[tokio::test]
async fn detect_returns_true_initially_and_false_after_run() -> TestResult {
    let driver = driver();

    assert!(driver.detect().await);
    let _context = driver.run_provisioning().await?;

    assert!(!driver.detect().await);
    Ok(())
}

#[tokio::test]
async fn run_provisioning_happy_path_records_phases_in_order() -> TestResult {
    let driver = driver();

    let context = driver.run_provisioning().await?;

    assert_eq!(context.status, FirstBootStatus::Completed);
    assert_eq!(context.performed_phases, FIRST_BOOT_PROVISIONING_PHASES);
    let records = driver.stage_records().await;
    assert_eq!(records.len(), FIRST_BOOT_PROVISIONING_PHASES.len());
    for record in records {
        assert_eq!(record.status, FirstBootStageStatus::Success);
        assert!(record.completed_at >= record.started_at);
    }
    Ok(())
}

#[tokio::test]
async fn run_provisioning_is_idempotent_after_completed() -> TestResult {
    let driver = driver();

    let first = driver.run_provisioning().await?;
    let second = driver.run_provisioning().await?;

    assert_eq!(second, first);
    assert_eq!(
        driver.stage_records().await.len(),
        FIRST_BOOT_PROVISIONING_PHASES.len()
    );
    Ok(())
}

#[tokio::test]
async fn skip_current_stage_then_run_proceeds_to_next_phase() -> TestResult {
    let driver = driver();

    driver
        .skip_stage(
            FirstBootPhase::StageInstallerMediaVerified,
            "fixture starts from verified media",
        )
        .await?;
    let context = driver.run_provisioning().await?;

    assert_eq!(context.status, FirstBootStatus::Completed);
    assert_eq!(
        context.performed_phases[0],
        FirstBootPhase::StageInstallerMediaVerified
    );
    assert_eq!(
        context.performed_phases[1],
        FirstBootPhase::StageDiskPartitioned
    );
    assert_eq!(
        driver.stage_records().await[0].status,
        FirstBootStageStatus::Skipped
    );
    Ok(())
}

#[tokio::test]
async fn skip_stage_records_reason() -> TestResult {
    let driver = driver();

    driver
        .skip_stage(
            FirstBootPhase::StageInstallerMediaVerified,
            "test harness preverified installer media",
        )
        .await?;

    let records = driver.stage_records().await;
    assert_eq!(
        records[0].reason.as_deref(),
        Some("test harness preverified installer media")
    );
    Ok(())
}

#[tokio::test]
async fn skip_future_stage_rejects_invalid_transition() -> TestResult {
    let driver = driver();

    let err = driver
        .skip_stage(
            FirstBootPhase::StageKernelInstalled,
            "cannot jump over disk partitioning",
        )
        .await
        .expect_err("out-of-order skip must reject");

    assert!(matches!(
        err,
        RecoveryError::InvalidPhaseTransition {
            from: FirstBootPhase::StageInstallerMediaVerified,
            to: FirstBootPhase::StageKernelInstalled
        }
    ));
    assert!(driver.current_context().await.performed_phases.is_empty());
    Ok(())
}

#[tokio::test]
async fn failed_stage_sets_failed_status_and_rerun_does_not_auto_recover() -> TestResult {
    let driver = driver();

    driver
        .fail_stage(
            FirstBootPhase::StageInstallerMediaVerified,
            "media signature mismatch",
        )
        .await?;
    let failed = driver.current_context().await;
    let rerun = driver.run_provisioning().await?;

    assert_eq!(failed.status, FirstBootStatus::Failed);
    assert_eq!(rerun, failed);
    assert_eq!(
        driver.stage_records().await[0].status,
        FirstBootStageStatus::Failed
    );
    Ok(())
}

#[tokio::test]
async fn mark_complete_on_not_started_rejects() -> TestResult {
    let driver = driver();

    let err = driver
        .mark_complete()
        .await
        .expect_err("mark_complete cannot skip the FSM");

    assert!(matches!(
        err,
        RecoveryError::InvalidPhaseTransition {
            from: FirstBootPhase::StageInstallerMediaVerified,
            to: FirstBootPhase::StageFirstBootComplete
        }
    ));
    Ok(())
}

#[tokio::test]
async fn mark_complete_at_last_phase_sets_completed() -> TestResult {
    let driver = driver();

    for phase in FIRST_BOOT_PROVISIONING_PHASES
        .iter()
        .copied()
        .take(FIRST_BOOT_PROVISIONING_PHASES.len() - 1)
    {
        driver.skip_stage(phase, "advance fixture").await?;
    }

    driver.mark_complete().await?;
    let context = driver.current_context().await;

    assert_eq!(context.status, FirstBootStatus::Completed);
    assert_eq!(context.performed_phases, FIRST_BOOT_PROVISIONING_PHASES);
    Ok(())
}

#[tokio::test]
async fn run_provisioning_rejects_when_recovery_is_active_per_spec() -> TestResult {
    let boundary = Arc::new(InMemoryRecoveryBoundary::new());
    let driver = FirstBootDriver::new(Arc::clone(&boundary) as Arc<dyn RecoveryBoundary>);
    let _state = boundary.enter_recovery(fallback_request()).await?;

    let err = driver
        .run_provisioning()
        .await
        .expect_err("S9.1 RECOVERY and S9.2 FIRST_BOOT are mutually exclusive");

    assert!(matches!(err, RecoveryError::AlreadyInRecovery));
    assert_eq!(
        driver.current_context().await.status,
        FirstBootStatus::NotStarted
    );
    Ok(())
}

#[tokio::test]
async fn run_provisioning_succeeds_when_recovery_is_inactive_per_spec() -> TestResult {
    let driver = driver();

    let context = driver.run_provisioning().await?;

    assert_eq!(context.status, FirstBootStatus::Completed);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_run_provisioning_is_idempotent() -> TestResult {
    let driver = Arc::new(driver());
    let mut handles = Vec::new();

    for _idx in 0..5 {
        let task_driver = Arc::clone(&driver);
        handles.push(tokio::spawn(
            async move { task_driver.run_provisioning().await },
        ));
    }

    let mut contexts: Vec<FirstBootContext> = Vec::new();
    for handle in handles {
        contexts.push(handle.await??);
    }

    assert_eq!(contexts.len(), 5);
    for context in &contexts {
        assert_eq!(context, &contexts[0]);
    }
    assert_eq!(
        driver.stage_records().await.len(),
        FIRST_BOOT_PROVISIONING_PHASES.len()
    );
    Ok(())
}

#[tokio::test]
async fn recovery_reset_flow_exits_recovery_before_first_boot_completes() -> TestResult {
    let boundary = Arc::new(InMemoryRecoveryBoundary::new());
    let driver = FirstBootDriver::new(Arc::clone(&boundary) as Arc<dyn RecoveryBoundary>);
    let _state = boundary.enter_recovery(fallback_request()).await?;
    let token = boundary
        .current_exit_token()
        .await
        .ok_or_else(|| RecoveryError::Internal("missing exit token".to_owned()))?;

    let err = driver
        .run_provisioning()
        .await
        .expect_err("first-boot cannot run inside RECOVERY mode");
    assert!(matches!(err, RecoveryError::AlreadyInRecovery));
    let _state = boundary.exit_recovery(&token).await?;
    let context = driver.run_provisioning().await?;

    assert_eq!(context.status, FirstBootStatus::Completed);
    Ok(())
}
