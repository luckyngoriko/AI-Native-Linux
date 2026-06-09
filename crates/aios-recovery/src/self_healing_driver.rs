//! Self-healing driver trait and in-memory implementation.
//!
//! The driver is the autonomous decision engine: given a health observation,
//! the active recovery state, and the declared policy, it decides whether (and
//! how) to heal a component, then executes the action and emits evidence.

#![allow(
    clippy::module_name_repetitions,
    reason = "Public names mirror the self-healing vocabulary"
)]

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use tokio::sync::RwLock;

use crate::boundary::RecoveryBoundary;
use crate::evidence_emit::RecoveryEvidenceEmitter;
use crate::mode::RecoveryMode;
use crate::self_healing::{
    ComponentHealingTracker, ComponentHealthState, HealAction,
    HealActionKind, PanicContext, SelfHealingPolicy,
};
use crate::watchdog::{WatchdogPolicy, WatchdogTimer};
use crate::{RecoveryError, RecoveryMutableScope};

// ---------------------------------------------------------------------------
// Trait
// ---------------------------------------------------------------------------

/// Autonomous self-healing driver surface.
///
/// Implementors decide and execute healing actions for unhealthy components.
/// Every action MUST be emitted as immutable evidence; silent healing is
/// constitutionally forbidden by INV-005.
#[async_trait]
pub trait SelfHealingDriver: Send + Sync {
    /// Register or update a health observation for a component.
    async fn observe_health(
        &self,
        component_id: &str,
        state: ComponentHealthState,
    ) -> Result<(), RecoveryError>;

    /// Register a structured panic event for a component.
    ///
    /// Unlike [`SelfHealingDriver::observe_health`] which records state only,
    /// this method captures full panic context (severity, backtrace hash, core dump
    /// reference) and classifies the crash to decide whether auto-restart is safe
    /// or escalation is required.
    ///
    /// The panic is always recorded in the tracker (bumps consecutive failures)
    /// and evidence is emitted immediately when an emitter is attached — even before
    /// `evaluate()` / `heal_cycle()` runs.  This matches MINIX's behaviour where the
    /// process manager logs crashes at detection time, not at decision time.
    async fn observe_panic(&self, ctx: PanicContext) -> Result<String, RecoveryError>;

    /// Evaluate all observed components and produce a list of healing actions.
    ///
    /// Returns an empty vec when no intervention is needed or when the policy
    /// disables autonomous healing.
    async fn evaluate(&self) -> Result<Vec<HealAction>, RecoveryError>;

    /// Execute a single decided healing action within recovery scope.
    ///
    /// # Errors
    ///
    /// Returns [`RecoveryError`] when recovery is not active, the required
    /// scope grant is missing, or evidence emission fails.
    async fn execute_heal(&self, action: &HealAction) -> Result<HealExecutionResult, RecoveryError>;

    /// Run one full observe → evaluate → execute cycle for all components.
    ///
    /// This is the main loop primitive used by the background daemon.
    async fn heal_cycle(&self) -> Result<HealCycleResult, RecoveryError>;
}

/// Outcome of a single [`SelfHealingDriver::execute_heal`] call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HealExecutionResult {
    /// The component that was targeted.
    pub component_id: String,
    /// Action kind that was attempted / performed.
    pub action_kind: HealActionKind,
    /// `true` when the action was executed and evidence was emitted.
    pub success: bool,
    /// Evidence receipt id (when emission succeeded).
    pub receipt_id: Option<String>,
    /// Human-readable outcome detail.
    pub detail: String,
}

/// Outcome of a full heal cycle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HealCycleResult {
    /// Number of components evaluated.
    pub components_evaluated: u64,
    /// Healing actions decided.
    pub actions_decided: usize,
    /// Healing actions successfully executed.
    pub actions_executed: usize,
    /// Actions that failed (execution error or escalation).
    pub actions_failed: usize,
    /// UTC timestamp when the cycle started.
    pub started_at: chrono::DateTime<chrono::Utc>,
    /// UTC timestamp when the cycle completed.
    pub completed_at: chrono::DateTime<chrono::Utc>,
}

// ---------------------------------------------------------------------------
// In-memory implementation
// ---------------------------------------------------------------------------

/// In-process self-healing driver backed by `HashMap`s and an optional emitter.
pub struct InMemorySelfHealingDriver {
    policy: RwLock<SelfHealingPolicy>,
    health_registry: RwLock<HashMap<String, ComponentHealthState>>,
    trackers: RwLock<HashMap<String, ComponentHealingTracker>>,
    watchdog: WatchdogTimer,
    boundary: Arc<dyn RecoveryBoundary>,
    evidence_emitter: Option<Arc<RecoveryEvidenceEmitter>>,
    global_sequence: RwLock<u64>,
}

impl Default for InMemorySelfHealingDriver {
    fn default() -> Self {
        Self {
            policy: RwLock::new(SelfHealingPolicy::default()),
            health_registry: RwLock::new(HashMap::new()),
            trackers: RwLock::new(HashMap::new()),
            watchdog: WatchdogTimer::default(),
            boundary: Arc::new(crate::InMemoryRecoveryBoundary::new()),
            evidence_emitter: None,
            global_sequence: RwLock::new(0),
        }
    }
}

impl InMemorySelfHealingDriver {
    /// Construct a new disabled driver (policy `enabled = false`).
    #[must_use]
    pub fn new(boundary: Arc<dyn RecoveryBoundary>) -> Self {
        Self {
            boundary,
            ..Default::default()
        }
    }

    /// Set the declarative self-healing policy.
    ///
    /// # Errors
    ///
    /// Returns [`RecoveryError`] when the policy violates INV-012 (normal-mode
    /// mutations) or other structural invariants.
    pub async fn set_policy(&self, policy: SelfHealingPolicy) -> Result<(), RecoveryError> {
        policy.validate()?;
        {
            let mut guard = self.policy.write().await;
            *guard = policy;
        }
        Ok(())
    }

    /// Return a snapshot of the current policy.
    #[must_use]
    pub async fn policy(&self) -> SelfHealingPolicy {
        self.policy.read().await.clone()
    }

    /// Attach an evidence emitter so every heal action produces a receipt.
    #[must_use]
    pub fn with_evidence_emitter(mut self, emitter: Arc<RecoveryEvidenceEmitter>) -> Self {
        self.evidence_emitter = Some(emitter);
        self
    }

    /// Return the current health registry snapshot.
    #[must_use]
    pub async fn health_snapshot(&self) -> HashMap<String, ComponentHealthState> {
        self.health_registry.read().await.clone()
    }

    /// Return the tracker for a specific component (or default if untracked).
    #[must_use]
    pub async fn tracker_for(&self, component_id: &str) -> ComponentHealingTracker {
        self.trackers
            .read()
            .await
            .get(component_id)
            .cloned()
            .unwrap_or_default()
    }

    /// Set the watchdog policy and optionally replace the timer state.
    pub async fn set_watchdog_policy(&self, policy: WatchdogPolicy) {
        self.watchdog.set_policy(policy).await;
    }

    /// Return a snapshot of the current watchdog policy.
    #[must_use]
    pub async fn watchdog_policy(&self) -> WatchdogPolicy {
        self.watchdog.policy().await
    }

    /// Register a component for watchdog liveness monitoring.
    pub async fn register_watchdog(&self, component_id: &str) {
        self.watchdog.register(component_id).await;
    }

    /// Signal a liveness ping from a component, resetting its watchdog deadline.
    pub async fn ping_watchdog(&self, component_id: &str) {
        self.watchdog.ping(component_id).await;
    }

    /// Check all watchdog deadlines and auto-flag expired components as
    /// Degraded via [`SelfHealingDriver::observe_health`].
    ///
    /// # Errors
    ///
    /// Returns [`RecoveryError`] when `observe_health` fails on an expired
    /// component.
    pub async fn watchdog_check(&self) -> Result<(), RecoveryError> {
        let expired = self.watchdog.check_deadlines().await;
        for component_id in &expired {
            self.observe_health(component_id, ComponentHealthState::Degraded)
                .await?;
        }
        Ok(())
    }

    /// Builder: attach a pre-configured watchdog timer.
    #[must_use]
    pub fn with_watchdog_policy(mut self, policy: WatchdogPolicy) -> Self {
        self.watchdog = WatchdogTimer::new(policy);
        self
    }

    // --- internal decision logic ---

    /// Decide what (if any) to do for a single component given its state.
    async fn decide_for_component(
        &self,
        component_id: &str,
        state: ComponentHealthState,
        policy: &SelfHealingPolicy,
        tracker: &ComponentHealingTracker,
    ) -> Option<HealAction> {
        if !state.needs_intervention() {
            return None;
        }

        let restart_policy = policy.policy_for_component(component_id);
        let allowed_scopes = policy.scopes_for_component(component_id);

        // Pick the best available scope for this action
        let required_scope = allowed_scopes
            .first()
            .copied()
            .unwrap_or(RecoveryMutableScope::ProcessLifecycle);

        let attempt = tracker.consecutive_failures.saturating_add(1);
        let action_kind = if attempt > restart_policy.max_retries {
            HealActionKind::Escalate
        } else if state.is_terminal() && attempt > restart_policy.max_retries / 2 {
            // After half max retries with terminal failure, try failover first
            if allowed_scopes.contains(&RecoveryMutableScope::MeshRouting) {
                HealActionKind::Failover
            } else {
                HealActionKind::Isolate
            }
        } else {
            HealActionKind::Restart
        };

        let sequence = {
            let mut seq = self.global_sequence.write().await;
            *seq += 1;
            *seq
        };

        Some(HealAction {
            component_id: component_id.to_owned(),
            observed_state: state,
            action_kind,
            required_scope,
            reason: format!(
                "component={component_id} state={state:?} consecutive_failures={} attempt={}/{}",
                tracker.consecutive_failures,
                attempt,
                restart_policy.max_retries,
            ),
            decided_at: Utc::now(),
            sequence,
        })
    }
}

#[async_trait]
impl SelfHealingDriver for InMemorySelfHealingDriver {
    async fn observe_health(
        &self,
        component_id: &str,
        state: ComponentHealthState,
    ) -> Result<(), RecoveryError> {
        // Update health registry
        {
            let mut reg = self.health_registry.write().await;
            reg.insert(component_id.to_owned(), state);
        }
        // Update tracker
        {
            let mut trackers = self.trackers.write().await;
            trackers
                .entry(component_id.to_owned())
                .or_default()
                .record_observation(state);
        }
        Ok(())
    }

    async fn observe_panic(&self, ctx: PanicContext) -> Result<String, RecoveryError> {
        let component_id = &ctx.component_id;

        // 1. Update health registry to Failed (panic = failed state)
        {
            let mut reg = self.health_registry.write().await;
            reg.insert(component_id.to_owned(), ComponentHealthState::Failed);
        }

        // 2. Record in tracker using panic-specific logic
        {
            let mut trackers = self.trackers.write().await;
            trackers
                .entry(component_id.to_owned())
                .or_default()
                .record_panic();
        }

        // 3. Emit structured panic evidence immediately (MINIX-style:
        //    log at detection time, not decision time)
        if let Some(emitter) = &self.evidence_emitter {
            emit_panic_evidence(emitter, &ctx).await
        } else {
            Ok(format!(
                "panic-{}-{}",
                ctx.component_id,
                ctx.consecutive_panics
            ))
        }
    }

    async fn evaluate(&self) -> Result<Vec<HealAction>, RecoveryError> {
        // Clone policy data to minimise lock hold time
        let (enabled, minimum_mode) = {
            let policy = self.policy.read().await;
            (policy.enabled, policy.minimum_mode)
        };
        if !enabled {
            return Ok(Vec::new());
        }

        // Check minimum mode requirement via the boundary
        let current_state = self.boundary.current_state().await;
        let mode_satisfied = match minimum_mode {
            RecoveryMode::Normal => true,
            RecoveryMode::Recovery => current_state.mode == RecoveryMode::Recovery,
            RecoveryMode::Degraded => matches!(
                current_state.mode,
                RecoveryMode::Degraded | RecoveryMode::Recovery
            ),
            RecoveryMode::FirstBoot => current_state.mode == RecoveryMode::FirstBoot,
        };
        if !mode_satisfied {
            return Ok(Vec::new());
        }

        // Snapshot registries under separate read locks, then release
        let states: Vec<(String, ComponentHealthState)>;
        let trackers_snapshot: HashMap<String, ComponentHealingTracker>;
        {
            let health = self.health_registry.read().await;
            let trackers = self.trackers.read().await;
            states = health.iter().map(|(k, v)| (k.clone(), *v)).collect();
            trackers_snapshot = trackers.clone();
            drop(health);
            drop(trackers);
        }

        let policy = self.policy.read().await;
        let mut actions = Vec::new();
        for (component_id, state) in &states {
            let default_tracker = ComponentHealingTracker::default();
            let tracker = trackers_snapshot
                .get(component_id)
                .unwrap_or(&default_tracker);
            if let Some(action) = self
                .decide_for_component(component_id, *state, &policy, tracker)
                .await
            {
                actions.push(action);
            }
        }
        Ok(actions)
    }

    async fn execute_heal(&self, action: &HealAction) -> Result<HealExecutionResult, RecoveryError> {
        // INV-012 guard: must be in recovery (or policy's minimum_mode)
        if !self.boundary.is_recovery_active().await {
            let policy = self.policy.read().await;
            if policy.minimum_mode == RecoveryMode::Recovery {
                return Ok(HealExecutionResult {
                    component_id: action.component_id.clone(),
                    action_kind: action.action_kind,
                    success: false,
                    receipt_id: None,
                    detail: "recovery mode not active — healing denied (INV-012)".to_owned(),
                });
            }
        }

        // Update tracker
        {
            let mut trackers = self.trackers.write().await;
            if let Some(tracker) = trackers.get_mut(&action.component_id) {
                tracker.record_action(action.sequence);
            }
        }

        // Emit evidence (best-effort but we report failures)
        let receipt_id = if let Some(emitter) = &self.evidence_emitter {
            match emit_healing_action(emitter, action).await {
                Ok(id) => Some(id),
                Err(e) => {
                    // Evidence failure is logged but does not block the action result
                    // — the action itself was decided; only its audit trail is incomplete
                    return Ok(HealExecutionResult {
                        component_id: action.component_id.clone(),
                        action_kind: action.action_kind,
                        success: false,
                        receipt_id: None,
                        detail: format!("evidence emission failed: {e}"),
                    });
                }
            }
        } else {
            None
        };

        Ok(HealExecutionResult {
            component_id: action.component_id.clone(),
            action_kind: action.action_kind,
            success: true,
            receipt_id,
            detail: format!(
                "{:?} executed for {} (seq={})",
                action.action_kind, action.component_id, action.sequence
            ),
        })
    }

    async fn heal_cycle(&self) -> Result<HealCycleResult, RecoveryError> {
        let started_at = Utc::now();

        self.watchdog_check().await?;

        let actions = self.evaluate().await?;
        let actions_decided = actions.len();

        let mut actions_executed = 0_usize;
        let mut actions_failed = 0_usize;

        for action in &actions {
            match self.execute_heal(action).await {
                Ok(result) if result.success => {
                    actions_executed += 1;
                }
                Ok(_) | Err(_) => {
                    actions_failed += 1;
                }
            }
        }

        let components_evaluated = self.health_registry.read().await.len() as u64;

        Ok(HealCycleResult {
            components_evaluated,
            actions_decided,
            actions_executed,
            actions_failed,
            started_at,
            completed_at: Utc::now(),
        })
    }
}

// ---------------------------------------------------------------------------
// Evidence emission helpers
// ---------------------------------------------------------------------------

/// Emit a healing-action evidence record using the closest available S3.1 type.
///
/// Currently maps to `RECOVERY_OPERATION_PERFORMED`; a future reduced
/// vocabulary may fold into a generic `HEALING_EVENT`.
async fn emit_healing_action(
    emitter: &RecoveryEvidenceEmitter,
    action: &HealAction,
) -> Result<String, RecoveryError> {
    use crate::evidence_payloads::HealingAttemptedPayload;
    use aios_evidence::RecordType;

    let payload = HealingAttemptedPayload {
        component_id: action.component_id.clone(),
        observed_state: action.observed_state,
        action_kind: action.action_kind,
        required_scope: action.required_scope,
        reason: action.reason.clone(),
        decided_at: action.decided_at,
        sequence: action.sequence,
    };
    // Use RECOVERY_OPERATION_PERFORMED as the closest S3.1 record type for
    // autonomous healing actions.  A future vocabulary may add HEALING_ATTEMPTED.
    emitter.emit(RecordType::RecoveryOperationPerformed, &payload, None).await
    }

/// Emit a structured panic evidence record.
///
/// Uses `RECOVERYOperationPerformed` (same as healing actions) because panic
/// is an autonomous recovery operation. The severity and classification are
/// captured inside the payload for post-mortem filtering.
async fn emit_panic_evidence(
    emitter: &RecoveryEvidenceEmitter,
    ctx: &PanicContext,
) -> Result<String, RecoveryError> {
    use crate::evidence_payloads::ComponentPanicPayload;
    use aios_evidence::RecordType;

    let payload = ComponentPanicPayload {
        component_id: ctx.component_id.clone(),
        severity: ctx.severity,
        message: ctx.message.clone(),
        file: ctx.file.clone(),
        line: ctx.line,
        backtrace_hash: ctx.backtrace_hash.clone(),
        core_dump_ref: ctx.core_dump_ref.clone(),
        observed_at: ctx.observed_at,
        consecutive_panics: ctx.consecutive_panics,
        recoverable_by_restart: ctx.severity.is_recoverable_by_restart(),
        requires_escalation: ctx.severity.requires_escalation(),
    };

    emitter
        .emit(RecordType::RecoveryOperationPerformed, &payload, None)
        .await
}
