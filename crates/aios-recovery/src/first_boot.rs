//! S9.2 first-boot provisioning FSM driver shell.
//!
//! T-076 intentionally implements only the coordinator shell. Every real
//! per-stage side effect (vault root provisioning, policy bundle install,
//! identity bootstrap, runtime startup, and marker write) is deferred to later
//! spec-driven drivers; this module enforces ordering, idempotency, and stage
//! accounting at the FSM boundary.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::{
    BootId, FirstBootContext, FirstBootPhase, FirstBootStatus, RecoveryBoundary, RecoveryError,
    RecoveryEvidenceEmitter,
};

/// The S9.2 happy-path provisioning stages in strict forward order.
///
/// `STAGE_FAILED_REQUIRES_RECOVERY` is the fifteenth enum value in the closed
/// S9.2 vocabulary, but it is a non-linear terminal failure state rather than a
/// happy-path provisioning stage.
pub const FIRST_BOOT_PROVISIONING_PHASES: [FirstBootPhase; 14] = [
    FirstBootPhase::StageInstallerMediaVerified,
    FirstBootPhase::StageDiskPartitioned,
    FirstBootPhase::StageKernelInstalled,
    FirstBootPhase::StageAiosFsInitialized,
    FirstBootPhase::StageVaultRootGenerated,
    FirstBootPhase::StageInvariantBundleLoaded,
    FirstBootPhase::StagePolicyBundleLoaded,
    FirstBootPhase::StageIdentityBundleLoaded,
    FirstBootPhase::StageRecoveryOperatorRegistration,
    FirstBootPhase::StageAiProviderConfiguration,
    FirstBootPhase::StageFirstGroupRegistration,
    FirstBootPhase::StageFirstUserRegistration,
    FirstBootPhase::StageRuntimeServicesStarted,
    FirstBootPhase::StageFirstBootComplete,
];

/// Per-stage execution status recorded by the first-boot driver shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FirstBootStageStatus {
    /// Stage completed through the deterministic T-076 success stub.
    Success,
    /// Stage was intentionally skipped by a test or non-applicable fixture.
    Skipped,
    /// Stage failed and moved the first-boot context to failed.
    Failed,
}

/// Stage accounting record captured by the T-076 driver shell.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FirstBootStageRecord {
    /// S9.2 phase covered by this record.
    pub phase: FirstBootPhase,
    /// UTC timestamp when the phase attempt started.
    pub started_at: DateTime<Utc>,
    /// UTC timestamp when the phase attempt completed.
    pub completed_at: DateTime<Utc>,
    /// Result status for this phase attempt.
    pub status: FirstBootStageStatus,
    /// Optional skip or failure reason.
    pub reason: Option<String>,
}

/// S9.2 first-boot coordinator shell.
pub struct FirstBootDriver {
    state: RwLock<FirstBootContext>,
    recovery_boundary: Arc<dyn RecoveryBoundary>,
    stage_records: RwLock<Vec<FirstBootStageRecord>>,
    evidence_emitter: Option<Arc<RecoveryEvidenceEmitter>>,
}

impl FirstBootDriver {
    /// Construct a first-boot driver with a fresh `NotStarted` context.
    #[must_use]
    pub fn new(boundary: Arc<dyn RecoveryBoundary>) -> Self {
        Self {
            state: RwLock::new(FirstBootContext {
                boot_id: BootId::new(),
                started_at: Utc::now(),
                completed_at: None,
                status: FirstBootStatus::NotStarted,
                performed_phases: Vec::new(),
            }),
            recovery_boundary: boundary,
            stage_records: RwLock::new(Vec::new()),
            evidence_emitter: None,
        }
    }

    /// Construct a first-boot driver with evidence emission enabled.
    #[must_use]
    pub fn with_evidence_emitter(
        boundary: Arc<dyn RecoveryBoundary>,
        evidence_emitter: Arc<RecoveryEvidenceEmitter>,
    ) -> Self {
        let mut driver = Self::new(boundary);
        driver.evidence_emitter = Some(evidence_emitter);
        driver
    }

    /// Return a snapshot of the current first-boot context.
    pub async fn current_context(&self) -> FirstBootContext {
        self.state.read().await.clone()
    }

    /// Return a snapshot of recorded stage attempts.
    pub async fn stage_records(&self) -> Vec<FirstBootStageRecord> {
        self.stage_records.read().await.clone()
    }

    /// Detect whether first-boot should run.
    ///
    /// The T-076 shell treats a `NotStarted` context as detectable first boot.
    /// Once any provisioning action mutates the context, detection becomes
    /// false and remains false for completed or failed sessions.
    pub async fn detect(&self) -> bool {
        self.state.read().await.status == FirstBootStatus::NotStarted
    }

    /// Drive the S9.2 provisioning FSM through the deterministic T-076 stubs.
    ///
    /// # Errors
    ///
    /// Returns [`RecoveryError::AlreadyInRecovery`] when S9.1 recovery mode is
    /// active, because `RECOVERY` and `FIRST_BOOT` are mutually exclusive. Also
    /// returns [`RecoveryError::InvalidPhaseTransition`] if the context has been
    /// manually advanced out of sequence.
    pub async fn run_provisioning(&self) -> Result<FirstBootContext, RecoveryError> {
        if let Some(context) = self.terminal_context().await {
            return Ok(context);
        }
        if self.recovery_boundary.is_recovery_active().await {
            return Err(RecoveryError::AlreadyInRecovery);
        }

        let mut context = self.state.write().await;
        if matches!(
            context.status,
            FirstBootStatus::Completed | FirstBootStatus::Failed
        ) {
            let terminal_context = context.clone();
            drop(context);
            return Ok(terminal_context);
        }
        context.status = FirstBootStatus::InProgress;
        if let Some(emitter) = &self.evidence_emitter {
            emitter.emit_first_boot_started(&context, None).await?;
        }

        while let Some(phase) = next_phase(&context) {
            if phase == FirstBootPhase::StageFirstBootComplete {
                let completed_at = self.record_success(&mut context, phase).await;
                if let Some(emitter) = &self.evidence_emitter {
                    emitter
                        .emit_first_boot_phase_completed(phase, &context, None)
                        .await?;
                }
                context.status = FirstBootStatus::Completed;
                context.completed_at = Some(completed_at);
                let completed_context = context.clone();
                let skipped_phases = self.skipped_phases().await;
                drop(context);
                if let Some(emitter) = &self.evidence_emitter {
                    emitter
                        .emit_first_boot_completed_with_skipped(
                            &completed_context,
                            skipped_phases,
                            None,
                        )
                        .await?;
                }
                return Ok(completed_context);
            }
            self.record_success(&mut context, phase).await;
            if let Some(emitter) = &self.evidence_emitter {
                emitter
                    .emit_first_boot_phase_completed(phase, &context, None)
                    .await?;
            }
        }

        drop(context);
        Err(RecoveryError::InvalidPhaseTransition {
            from: FirstBootPhase::StageFirstBootComplete,
            to: FirstBootPhase::StageFirstBootComplete,
        })
    }

    /// Apply the terminal first-boot completion transition.
    ///
    /// # Errors
    ///
    /// Returns [`RecoveryError::InvalidPhaseTransition`] if any prior
    /// provisioning stage has not been performed in order.
    pub async fn mark_complete(&self) -> Result<(), RecoveryError> {
        if self.recovery_boundary.is_recovery_active().await {
            return Err(RecoveryError::AlreadyInRecovery);
        }

        let mut context = self.state.write().await;
        if context.status == FirstBootStatus::Completed {
            drop(context);
            return Ok(());
        }
        if let Err(err) = validate_next_phase(&context, FirstBootPhase::StageFirstBootComplete) {
            drop(context);
            return Err(err);
        }
        let completed_at = self
            .record_success(&mut context, FirstBootPhase::StageFirstBootComplete)
            .await;
        if let Some(emitter) = &self.evidence_emitter {
            emitter
                .emit_first_boot_phase_completed(
                    FirstBootPhase::StageFirstBootComplete,
                    &context,
                    None,
                )
                .await?;
        }
        context.status = FirstBootStatus::Completed;
        context.completed_at = Some(completed_at);
        let completed_context = context.clone();
        let skipped_phases = self.skipped_phases().await;
        drop(context);
        if let Some(emitter) = &self.evidence_emitter {
            emitter
                .emit_first_boot_completed_with_skipped(&completed_context, skipped_phases, None)
                .await?;
        }
        Ok(())
    }

    /// Skip the current stage and record the reason.
    ///
    /// # Errors
    ///
    /// Returns [`RecoveryError::InvalidPhaseTransition`] when `phase` is not the
    /// next expected S9.2 stage.
    pub async fn skip_stage(
        &self,
        phase: FirstBootPhase,
        reason: &str,
    ) -> Result<(), RecoveryError> {
        if matches!(
            phase,
            FirstBootPhase::StageFirstBootComplete | FirstBootPhase::StageFailedRequiresRecovery
        ) {
            let context = self.state.read().await;
            return Err(RecoveryError::InvalidPhaseTransition {
                from: next_phase(&context).unwrap_or(FirstBootPhase::StageFirstBootComplete),
                to: phase,
            });
        }

        let mut context = self.state.write().await;
        if let Err(err) = validate_next_phase(&context, phase) {
            drop(context);
            return Err(err);
        }
        context.status = FirstBootStatus::InProgress;
        self.record_stage(
            &mut context,
            phase,
            FirstBootStageStatus::Skipped,
            Some(reason),
        )
        .await;
        if let Some(emitter) = &self.evidence_emitter {
            emitter
                .emit_first_boot_phase_completed(phase, &context, None)
                .await?;
        }
        drop(context);
        Ok(())
    }

    /// Mark the current stage as failed and move to the S9.2 failure terminal.
    ///
    /// This is a T-076 harness hook for validating the failed-state semantics;
    /// real failure reasons and rollback drivers are owned by later tasks.
    ///
    /// # Errors
    ///
    /// Returns [`RecoveryError::InvalidPhaseTransition`] when `phase` is not the
    /// next expected S9.2 stage.
    pub async fn fail_stage(
        &self,
        phase: FirstBootPhase,
        reason: &str,
    ) -> Result<(), RecoveryError> {
        let mut context = self.state.write().await;
        if let Err(err) = validate_next_phase(&context, phase) {
            drop(context);
            return Err(err);
        }
        context.status = FirstBootStatus::InProgress;
        self.record_stage(
            &mut context,
            phase,
            FirstBootStageStatus::Failed,
            Some(reason),
        )
        .await;
        let completed_at = self
            .record_stage(
                &mut context,
                FirstBootPhase::StageFailedRequiresRecovery,
                FirstBootStageStatus::Failed,
                Some(reason),
            )
            .await;
        context.status = FirstBootStatus::Failed;
        context.completed_at = Some(completed_at);
        drop(context);
        Ok(())
    }

    async fn terminal_context(&self) -> Option<FirstBootContext> {
        let context = self.state.read().await;
        if matches!(
            context.status,
            FirstBootStatus::Completed | FirstBootStatus::Failed
        ) {
            return Some(context.clone());
        }
        None
    }

    async fn record_success(
        &self,
        context: &mut FirstBootContext,
        phase: FirstBootPhase,
    ) -> DateTime<Utc> {
        self.record_stage(context, phase, FirstBootStageStatus::Success, None)
            .await
    }

    async fn record_stage(
        &self,
        context: &mut FirstBootContext,
        phase: FirstBootPhase,
        status: FirstBootStageStatus,
        reason: Option<&str>,
    ) -> DateTime<Utc> {
        let started_at = Utc::now();
        let completed_at = Utc::now();
        context.performed_phases.push(phase);
        self.stage_records.write().await.push(FirstBootStageRecord {
            phase,
            started_at,
            completed_at,
            status,
            reason: reason.map(str::to_owned),
        });
        completed_at
    }

    async fn skipped_phases(&self) -> Vec<FirstBootPhase> {
        self.stage_records
            .read()
            .await
            .iter()
            .filter_map(|record| {
                (record.status == FirstBootStageStatus::Skipped).then_some(record.phase)
            })
            .collect()
    }
}

fn validate_next_phase(
    context: &FirstBootContext,
    to: FirstBootPhase,
) -> Result<(), RecoveryError> {
    let Some(from) = next_phase(context) else {
        return Err(RecoveryError::InvalidPhaseTransition {
            from: FirstBootPhase::StageFirstBootComplete,
            to,
        });
    };
    if from == to {
        return Ok(());
    }
    Err(RecoveryError::InvalidPhaseTransition { from, to })
}

fn next_phase(context: &FirstBootContext) -> Option<FirstBootPhase> {
    if context.status == FirstBootStatus::Failed {
        return Some(FirstBootPhase::StageFailedRequiresRecovery);
    }
    FIRST_BOOT_PROVISIONING_PHASES
        .get(context.performed_phases.len())
        .copied()
}
