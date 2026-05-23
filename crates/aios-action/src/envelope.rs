//! Top-level `ActionEnvelope` — the four-section partition from S0.1 §2.
//!
//! ```text
//! ActionEnvelope
//! ├── schema_version : "aios.action.v1alpha1"
//! ├── identity       (caller-owned, immutable)
//! ├── request        (caller-owned, immutable)
//! ├── execution      (runtime-owned, mutates over lifecycle)
//! └── trace          (transport-owned, set once)
//! ```

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::canonical::{blake3_hash, jcs_canonicalize, CanonicalError};
use crate::error::TransitionError;
use crate::execution::{Condition, ConditionStatus, ConditionType, Execution};
use crate::phase::ActionPhase;
use crate::{identity::Identity, request::Request, trace::Trace};

/// Canonical proto package name for this envelope version (S0.1 §2 / §8.1).
///
/// Promotion to `v1beta1` / `v1` is a deliberate, evidenced step per S0.1 §8.1; this
/// crate ships the alpha version and the constant is the single source of truth that
/// every constructed envelope stamps onto the `schema_version` field.
pub const SCHEMA_VERSION: &str = "aios.action.v1alpha1";

/// The four-section envelope per S0.1 §2.
///
/// Invariants the type system enforces today:
/// - `identity` and `request` are public fields but documented as immutable post-creation
///   (S0.1 §2.2 invariant 1). Wire-level enforcement (hash drift detection in Capability
///   Runtime) lands in T-002 / T-006.
/// - `execution` starts as [`Execution::pending`] on every fresh envelope (S0.1 §6.1 T1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionEnvelope {
    /// Canonical proto package name — see [`SCHEMA_VERSION`].
    pub schema_version: String,
    /// Caller identity; immutable after creation (S0.1 §2.1).
    pub identity: Identity,
    /// Caller request; immutable after creation (S0.1 §2.1).
    pub request: Request,
    /// Runtime-observed execution state; mutates over the lifecycle (S0.1 §2.1).
    pub execution: Execution,
    /// W3C trace context; set once (S0.1 §9.1).
    pub trace: Trace,
}

impl ActionEnvelope {
    /// Construct a fresh envelope in [`crate::ActionPhase::Pending`] with the supplied
    /// caller intent and trace context.
    ///
    /// This is the in-process constructor used by callers (cognitive core, CLI, tests).
    /// The wire-level entry point — `SubmitAction` (S0.1 §10) — performs additional
    /// validation (schema, idempotency, subject-cert binding) before accepting the
    /// envelope into the Capability Runtime.
    #[must_use]
    pub fn new(identity: Identity, request: Request, trace: Trace) -> Self {
        Self {
            schema_version: SCHEMA_VERSION.to_owned(),
            identity,
            request,
            execution: Execution::pending(),
            trace,
        }
    }

    /// Compute the idempotency hash per S0.1 §3.3 / §8.5.
    ///
    /// Returns:
    /// - `Ok(None)` when no `idempotency_key` is set on the request — idempotency is
    ///   opt-in, and absence is a documented signal that the caller does not request
    ///   dedup (S0.1 §3.3).
    /// - `Ok(Some(hex))` with a 64-character lowercase hex BLAKE3-256 digest of the
    ///   canonical `{"idempotency_key": ..., "request": ...}` envelope.
    ///
    /// This is what the Capability Runtime stores in its dedup table; same hash within
    /// the configured TTL means "safe retry, return the existing envelope" (S0.1 §3.3
    /// rule 1).
    ///
    /// # Errors
    ///
    /// Propagates [`CanonicalError`] from the underlying JCS canonicalizer.
    pub fn idempotency_hash(&self) -> Result<Option<String>, CanonicalError> {
        let Some(key) = self.request.idempotency_key.as_ref() else {
            return Ok(None);
        };

        // Bind the key to the request content so that the same key with a different
        // request produces a different hash — that's what makes IdempotencyConflict
        // (S0.1 §3.3 rule 2) detectable.
        //
        // We construct the canonical tuple as a `serde_json::Value` rather than an
        // anonymous struct so that the field names are explicit, sorted, and easy to
        // cross-implement in Python/TypeScript later.
        let bundle = serde_json::json!({
            "idempotency_key": key,
            "request":         self.request,
        });

        let canonical = jcs_canonicalize(&bundle)?;
        Ok(Some(blake3_hash(canonical.as_bytes())))
    }

    /// Transition the envelope to `next` per the S0.1 §6.2 FSM.
    ///
    /// Enforces:
    /// - Only the six allowed transitions (delegates to [`ActionPhase::can_transition_to`]).
    /// - Terminality (S0.1 §6.3): once in `Succeeded` / `Failed` / `RolledBack`, this
    ///   method always returns [`TransitionError::TerminalPhase`].
    /// - `phase_changed_at` monotonicity (S0.1 §6.7): the new timestamp is `Utc::now()`,
    ///   which is guaranteed `>=` the previous one on any sane clock; we additionally
    ///   pin it to `max(prev, now)` to be robust against clock skew.
    ///
    /// On `Pending -> Running` (T6 in S0.1 §6.2), `started_at` is set; on a transition
    /// into any terminal phase, `ended_at` is set. These two stamps are set exactly once.
    ///
    /// # Errors
    ///
    /// - [`TransitionError::TerminalPhase`] if the envelope is already terminal.
    /// - [`TransitionError::IllegalTransition`] if `(self.phase, next)` is not one of the
    ///   six S0.1 §6.2 edges.
    pub fn transition_to(&mut self, next: ActionPhase) -> Result<(), TransitionError> {
        if self.execution.phase.is_terminal() {
            return Err(TransitionError::TerminalPhase);
        }
        if !self.execution.phase.can_transition_to(next) {
            return Err(TransitionError::IllegalTransition {
                from: self.execution.phase,
                to: next,
            });
        }

        // S0.1 §6.7 phase_changed_at monotonicity: defend against clock skew by pinning
        // the new timestamp to max(now, prev). Equal timestamps within a single tick are
        // permitted by the "non-decreasing" rule.
        let now = Utc::now();
        let new_stamp = if now < self.execution.phase_changed_at {
            self.execution.phase_changed_at
        } else {
            now
        };

        // T6: Pending -> Running stamps started_at exactly once.
        if matches!(self.execution.phase, ActionPhase::Pending)
            && matches!(next, ActionPhase::Running)
            && self.execution.started_at.is_none()
        {
            self.execution.started_at = Some(new_stamp);
        }

        // T2-T5, T7-T10: any transition into a terminal phase stamps ended_at exactly once.
        if next.is_terminal() && self.execution.ended_at.is_none() {
            self.execution.ended_at = Some(new_stamp);
        }

        self.execution.phase = next;
        self.execution.phase_changed_at = new_stamp;
        Ok(())
    }

    /// Append a condition to the envelope's condition list, enforcing the
    /// monotonicity invariants from S0.1 §6.7.
    ///
    /// Invariants enforced:
    /// 1. `observed_at` must be `>=` the most recent condition's `observed_at` (no
    ///    timestamp regression).
    /// 2. If a condition of the same `condition_type` already exists with status
    ///    `True`, the new condition's status must also be `True` — a `True -> False`
    ///    flip on the same type is forbidden ("conditions are added, not retracted").
    ///
    /// # Errors
    ///
    /// Returns [`TransitionError::MonotonicityViolation`] when either invariant is
    /// violated. The error message identifies which invariant was broken.
    pub fn add_condition(&mut self, condition: Condition) -> Result<(), TransitionError> {
        // Invariant 1: observed_at non-decreasing.
        if let Some(last) = self.execution.conditions.last() {
            if condition.observed_at < last.observed_at {
                return Err(TransitionError::MonotonicityViolation(format!(
                    "condition observed_at {prev} regresses past last observed_at {last}",
                    prev = condition.observed_at,
                    last = last.observed_at,
                )));
            }
        }

        // Invariant 2: no True -> False flip on same condition_type.
        if condition.status != ConditionStatus::True {
            for existing in &self.execution.conditions {
                if existing.condition_type == condition.condition_type
                    && existing.status == ConditionStatus::True
                {
                    return Err(TransitionError::MonotonicityViolation(format!(
                        "cannot flip condition {ct:?} from True to {new:?} (S0.1 §6.7)",
                        ct = condition.condition_type,
                        new = condition.status,
                    )));
                }
            }
        }

        self.execution.conditions.push(condition);
        Ok(())
    }

    /// Closed set of conditions that MUST be observed `True` for the envelope's
    /// current phase, per S0.1 §6.6 phase ↔ conditions consistency.
    ///
    /// The mapping is:
    /// - `Pending` — empty set (no observations are required to enter Pending).
    /// - `Running` — `PolicyEvaluated`, `Sandboxed` (S0.1 §6.6: a Running envelope has
    ///   passed policy evaluation and a sandbox is bound).
    /// - `Succeeded` — `PolicyEvaluated`, `Sandboxed`, `Executed`, `Verified`
    ///   (the four conditions that together canonicalise the `Succeeded` phase per §6.6).
    /// - `Failed` — `PolicyEvaluated` (a Failed envelope has, at minimum, been evaluated
    ///   by policy; other conditions may be set depending on the failure stage).
    /// - `RolledBack` — `PolicyEvaluated`, `Sandboxed`, `Executed`, `RolledBack`
    ///   (`RolledBack` implies execution ran and then was reversed).
    ///
    /// Use [`Self::validate_phase_conditions`] to verify the actual condition list
    /// against this canonical set.
    #[must_use]
    pub fn canonical_conditions_for_phase(&self) -> Vec<ConditionType> {
        match self.execution.phase {
            ActionPhase::Pending => Vec::new(),
            ActionPhase::Running => vec![ConditionType::PolicyEvaluated, ConditionType::Sandboxed],
            ActionPhase::Succeeded => vec![
                ConditionType::PolicyEvaluated,
                ConditionType::Sandboxed,
                ConditionType::Executed,
                ConditionType::Verified,
            ],
            ActionPhase::Failed => vec![ConditionType::PolicyEvaluated],
            ActionPhase::RolledBack => vec![
                ConditionType::PolicyEvaluated,
                ConditionType::Sandboxed,
                ConditionType::Executed,
                ConditionType::RolledBack,
            ],
        }
    }

    /// Validate that all canonical conditions required by the current phase
    /// (per [`Self::canonical_conditions_for_phase`]) are observed `True`.
    ///
    /// This is the S0.1 §6.6 consistency check: `phase` is a denormalisation of
    /// `conditions[]`, and the two must agree. Capability Runtime calls this before
    /// committing a terminal phase to the evidence log.
    ///
    /// # Errors
    ///
    /// Returns [`TransitionError::PhaseConditionMismatch`] listing the conditions that
    /// are required but not observed `True`.
    pub fn validate_phase_conditions(&self) -> Result<(), TransitionError> {
        let required = self.canonical_conditions_for_phase();
        let mut missing = Vec::new();
        for ct in required {
            let observed_true = self
                .execution
                .conditions
                .iter()
                .any(|c| c.condition_type == ct && c.status == ConditionStatus::True);
            if !observed_true {
                missing.push(ct);
            }
        }
        if missing.is_empty() {
            Ok(())
        } else {
            Err(TransitionError::PhaseConditionMismatch {
                phase: self.execution.phase,
                missing,
            })
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "panic-on-failure is the idiomatic test signal"
)]
mod tests {
    use super::{ActionEnvelope, SCHEMA_VERSION};
    use crate::error::TransitionError;
    use crate::execution::{Condition, ConditionStatus, ConditionType};
    use crate::{identity::Identity, phase::ActionPhase, request::Request, trace::Trace};
    use chrono::{Duration, Utc};

    /// Test helper: a fresh `ActionEnvelope` in `Pending` with no conditions.
    fn make_envelope() -> ActionEnvelope {
        ActionEnvelope::new(
            Identity::new("agent:dev", true),
            Request::new("service.restart", serde_json::json!({"service": "nginx"})),
            Trace::new("0af7651916cd43dd8448eb211c80319c", "b7ad6b7169203331", None),
        )
    }

    /// Test helper: append a `True` condition observed `now + offset_secs`. Returns the
    /// observed timestamp so the next condition can reuse or exceed it.
    fn push_true(
        env: &mut ActionEnvelope,
        ct: ConditionType,
        offset_secs: i64,
    ) -> chrono::DateTime<chrono::Utc> {
        let observed_at = Utc::now() + Duration::seconds(offset_secs);
        env.add_condition(Condition {
            condition_type: ct,
            status: ConditionStatus::True,
            observed_at,
            message: format!("{ct:?} observed in test"),
        })
        .expect("add_condition must succeed in test setup");
        observed_at
    }

    #[test]
    fn new_envelope_starts_in_pending_with_canonical_schema_version() {
        let env = ActionEnvelope::new(
            Identity::new("agent:dev", true),
            Request::new("service.restart", serde_json::json!({"service": "nginx"})),
            Trace::new("0af7651916cd43dd8448eb211c80319c", "b7ad6b7169203331", None),
        );

        assert_eq!(env.schema_version, SCHEMA_VERSION);
        assert_eq!(env.execution.phase, ActionPhase::Pending);
        assert!(env.execution.started_at.is_none());
        assert!(env.execution.ended_at.is_none());
        assert!(env.execution.conditions.is_empty());
    }

    #[test]
    fn idempotency_hash_is_none_when_no_key_is_set() {
        // S0.1 §3.3: idempotency is opt-in; an absent key means "no dedup".
        let env = ActionEnvelope::new(
            Identity::new("agent:dev", true),
            Request::new("service.restart", serde_json::json!({"service": "nginx"})),
            Trace::new("0af7651916cd43dd8448eb211c80319c", "b7ad6b7169203331", None),
        );
        assert!(env.request.idempotency_key.is_none());
        let h = env
            .idempotency_hash()
            .expect("idempotency_hash must succeed");
        assert!(
            h.is_none(),
            "idempotency_hash must be None when no key is set, got {h:?}"
        );
    }

    #[test]
    fn idempotency_hash_is_stable_for_same_key_same_request() {
        // Two envelopes built independently with the same key and the same logical
        // request content must produce the same idempotency hash — that's what makes
        // the safe-retry rule (S0.1 §3.3 rule 1) work.
        let make = || {
            let mut req = Request::new(
                "service.restart",
                serde_json::json!({"service": "nginx", "force": true}),
            );
            req.idempotency_key = Some("retry-token-42".to_owned());
            ActionEnvelope::new(
                Identity::new("agent:dev", true),
                req,
                Trace::new("0af7651916cd43dd8448eb211c80319c", "b7ad6b7169203331", None),
            )
        };

        let h1 = make()
            .idempotency_hash()
            .expect("hash 1 must succeed")
            .expect("key is set so hash must be Some");
        let h2 = make()
            .idempotency_hash()
            .expect("hash 2 must succeed")
            .expect("key is set so hash must be Some");

        assert_eq!(h1, h2, "same key + same request must hash identically");
        assert_eq!(h1.len(), 64, "idempotency hash must be 64 hex chars");
    }

    #[test]
    fn idempotency_hash_differs_when_key_differs() {
        // Different idempotency_key, same request → different hash. This is what makes
        // S0.1 §3.3 rule 3 ("different key + same content = distinct actions") work.
        let make = |key: &str| {
            let mut req = Request::new("service.restart", serde_json::json!({"service": "nginx"}));
            req.idempotency_key = Some(key.to_owned());
            ActionEnvelope::new(
                Identity::new("agent:dev", true),
                req,
                Trace::new("0af7651916cd43dd8448eb211c80319c", "b7ad6b7169203331", None),
            )
        };

        let h_a = make("token-A")
            .idempotency_hash()
            .expect("hash a must succeed")
            .expect("key set");
        let h_b = make("token-B")
            .idempotency_hash()
            .expect("hash b must succeed")
            .expect("key set");

        assert_ne!(h_a, h_b, "different keys must produce different hashes");
    }

    #[test]
    fn idempotency_hash_differs_when_request_differs_for_same_key() {
        // S0.1 §3.3 rule 2: same key + different request → IdempotencyConflict. That
        // conflict is detectable only because the hash changes when the request changes.
        let make = |service: &str| {
            let mut req = Request::new("service.restart", serde_json::json!({"service": service}));
            req.idempotency_key = Some("shared-token".to_owned());
            ActionEnvelope::new(
                Identity::new("agent:dev", true),
                req,
                Trace::new("0af7651916cd43dd8448eb211c80319c", "b7ad6b7169203331", None),
            )
        };

        let h_nginx = make("nginx")
            .idempotency_hash()
            .expect("hash nginx must succeed")
            .expect("key set");
        let h_apache = make("apache")
            .idempotency_hash()
            .expect("hash apache must succeed")
            .expect("key set");

        assert_ne!(
            h_nginx, h_apache,
            "same key + different request must produce different hashes (S0.1 §3.3 rule 2)"
        );
    }

    #[test]
    fn envelope_serde_round_trips_via_json() {
        let original = ActionEnvelope::new(
            Identity::new("human:lucky", false),
            Request::new(
                "aiosfs.pointer.promote",
                serde_json::json!({"object_id": "obj_42"}),
            ),
            Trace::new(
                "4bf92f3577b34da6a3ce929d0e0e4736",
                "00f067aa0ba902b7",
                Some("aaaaaaaaaaaaaaaa".to_owned()),
            ),
        );

        let json = serde_json::to_string(&original).expect("serialize must succeed");
        let reparsed: ActionEnvelope =
            serde_json::from_str(&json).expect("deserialize must succeed");

        assert_eq!(original, reparsed, "serde JSON round-trip must be lossless");
    }

    // ============================================================
    // T-004: lifecycle FSM transitions + monotonicity + canonical conditions
    // ============================================================

    #[test]
    fn transition_pending_to_running_sets_started_at_and_advances_phase() {
        // T6 (S0.1 §6.2): Pending -> Running.
        let mut env = make_envelope();
        let before = env.execution.phase_changed_at;
        env.transition_to(ActionPhase::Running)
            .expect("T6 Pending -> Running must succeed");
        assert_eq!(env.execution.phase, ActionPhase::Running);
        assert!(
            env.execution.started_at.is_some(),
            "started_at must be set on T6 transition"
        );
        assert!(
            env.execution.phase_changed_at >= before,
            "phase_changed_at must be monotonically non-decreasing"
        );
        assert!(env.execution.ended_at.is_none());
    }

    #[test]
    fn transition_pending_to_failed_is_valid_and_terminal() {
        // T2-T5 (S0.1 §6.2): Pending -> Failed.
        let mut env = make_envelope();
        env.transition_to(ActionPhase::Failed)
            .expect("T2-T5 Pending -> Failed must succeed");
        assert_eq!(env.execution.phase, ActionPhase::Failed);
        assert!(
            env.execution.ended_at.is_some(),
            "ended_at must be set on transition into terminal phase"
        );
        assert!(
            env.execution.started_at.is_none(),
            "started_at must remain None — execution never began"
        );
    }

    #[test]
    fn transition_running_to_succeeded_is_valid() {
        // T7 (S0.1 §6.2): Running -> Succeeded.
        let mut env = make_envelope();
        env.transition_to(ActionPhase::Running)
            .expect("Pending -> Running must succeed");
        env.transition_to(ActionPhase::Succeeded)
            .expect("T7 Running -> Succeeded must succeed");
        assert_eq!(env.execution.phase, ActionPhase::Succeeded);
        assert!(env.execution.ended_at.is_some());
    }

    #[test]
    fn transition_running_to_failed_is_valid() {
        // T8-T9 (S0.1 §6.2): Running -> Failed.
        let mut env = make_envelope();
        env.transition_to(ActionPhase::Running).expect("setup");
        env.transition_to(ActionPhase::Failed)
            .expect("T8-T9 Running -> Failed must succeed");
        assert_eq!(env.execution.phase, ActionPhase::Failed);
    }

    #[test]
    fn transition_running_to_rolled_back_is_valid() {
        // T10 (S0.1 §6.2): Running -> RolledBack.
        let mut env = make_envelope();
        env.transition_to(ActionPhase::Running).expect("setup");
        env.transition_to(ActionPhase::RolledBack)
            .expect("T10 Running -> RolledBack must succeed");
        assert_eq!(env.execution.phase, ActionPhase::RolledBack);
        assert!(env.execution.ended_at.is_some());
    }

    #[test]
    fn transition_pending_to_succeeded_is_rejected() {
        // S0.1 §6.2: Pending may not skip Running; Succeeded requires verification.
        let mut env = make_envelope();
        let err = env
            .transition_to(ActionPhase::Succeeded)
            .expect_err("Pending -> Succeeded must be rejected");
        assert!(matches!(
            err,
            TransitionError::IllegalTransition {
                from: ActionPhase::Pending,
                to: ActionPhase::Succeeded
            }
        ));
        assert_eq!(env.execution.phase, ActionPhase::Pending);
    }

    #[test]
    fn transition_pending_to_rolled_back_is_rejected() {
        let mut env = make_envelope();
        let err = env
            .transition_to(ActionPhase::RolledBack)
            .expect_err("Pending -> RolledBack must be rejected");
        assert!(matches!(err, TransitionError::IllegalTransition { .. }));
    }

    #[test]
    fn transition_pending_to_pending_self_loop_is_rejected() {
        let mut env = make_envelope();
        let err = env
            .transition_to(ActionPhase::Pending)
            .expect_err("self-loop Pending -> Pending must be rejected");
        assert!(matches!(err, TransitionError::IllegalTransition { .. }));
    }

    #[test]
    fn transition_running_to_running_self_loop_is_rejected() {
        let mut env = make_envelope();
        env.transition_to(ActionPhase::Running).expect("setup");
        let err = env
            .transition_to(ActionPhase::Running)
            .expect_err("self-loop Running -> Running must be rejected");
        assert!(matches!(err, TransitionError::IllegalTransition { .. }));
    }

    #[test]
    fn transition_succeeded_to_rolled_back_is_rejected_as_terminal() {
        // The classic mistake S0.1 §6.2 forbids: post-hoc rollback is a NEW envelope.
        let mut env = make_envelope();
        env.transition_to(ActionPhase::Running).expect("setup");
        env.transition_to(ActionPhase::Succeeded).expect("setup");

        // Once terminal, the FIRST barrier is TerminalPhase (not IllegalTransition).
        let err = env
            .transition_to(ActionPhase::RolledBack)
            .expect_err("Succeeded -> RolledBack must be rejected as terminal");
        assert!(matches!(err, TransitionError::TerminalPhase));
    }

    #[test]
    fn no_transitions_allowed_from_failed_terminal() {
        let mut env = make_envelope();
        env.transition_to(ActionPhase::Failed).expect("setup");
        for target in [
            ActionPhase::Pending,
            ActionPhase::Running,
            ActionPhase::Succeeded,
            ActionPhase::RolledBack,
            ActionPhase::Failed,
        ] {
            let err = env
                .transition_to(target)
                .expect_err("terminal phase must reject all further transitions");
            assert!(
                matches!(err, TransitionError::TerminalPhase),
                "expected TerminalPhase, got {err:?} for target {target:?}",
            );
        }
    }

    #[test]
    fn no_transitions_allowed_from_rolled_back_terminal() {
        let mut env = make_envelope();
        env.transition_to(ActionPhase::Running).expect("setup");
        env.transition_to(ActionPhase::RolledBack).expect("setup");
        let err = env
            .transition_to(ActionPhase::Failed)
            .expect_err("RolledBack is terminal");
        assert!(matches!(err, TransitionError::TerminalPhase));
    }

    #[test]
    fn add_condition_happy_path_appends_in_order() {
        let mut env = make_envelope();
        push_true(&mut env, ConditionType::PolicyEvaluated, 0);
        push_true(&mut env, ConditionType::Sandboxed, 1);
        push_true(&mut env, ConditionType::Executed, 2);
        assert_eq!(env.execution.conditions.len(), 3);
        assert_eq!(
            env.execution.conditions[0].condition_type,
            ConditionType::PolicyEvaluated
        );
        assert_eq!(
            env.execution.conditions[2].condition_type,
            ConditionType::Executed
        );
    }

    #[test]
    fn add_condition_rejects_timestamp_regression() {
        // S0.1 §6.7: observed_at is monotonically non-decreasing across conditions.
        let mut env = make_envelope();
        let last_at = push_true(&mut env, ConditionType::PolicyEvaluated, 10);
        let regressed = Condition {
            condition_type: ConditionType::Sandboxed,
            status: ConditionStatus::True,
            observed_at: last_at - Duration::seconds(5),
            message: "back in time".to_owned(),
        };
        let err = env
            .add_condition(regressed)
            .expect_err("timestamp regression must be rejected");
        assert!(matches!(err, TransitionError::MonotonicityViolation(_)));
        assert_eq!(
            env.execution.conditions.len(),
            1,
            "rejected condition must not be appended"
        );
    }

    #[test]
    fn add_condition_rejects_true_to_false_flip() {
        // S0.1 §6.7: once observed True, a condition_type cannot be set False later
        // ("conditions are added, not retracted").
        let mut env = make_envelope();
        push_true(&mut env, ConditionType::PolicyEvaluated, 0);
        let flip = Condition {
            condition_type: ConditionType::PolicyEvaluated,
            status: ConditionStatus::False,
            observed_at: Utc::now() + Duration::seconds(5),
            message: "attempt to retract".to_owned(),
        };
        let err = env
            .add_condition(flip)
            .expect_err("True -> False flip must be rejected");
        assert!(matches!(err, TransitionError::MonotonicityViolation(_)));
    }

    #[test]
    fn add_condition_allows_unknown_to_true_transition_through_two_entries() {
        // The append-only model allows multiple Condition entries for the same type;
        // an earlier Unknown followed by a later True is the canonical observation
        // pattern (the runtime learns the fact). The forbidden direction is True -> False.
        let mut env = make_envelope();
        env.add_condition(Condition {
            condition_type: ConditionType::Sandboxed,
            status: ConditionStatus::Unknown,
            observed_at: Utc::now(),
            message: "pending sandbox setup".to_owned(),
        })
        .expect("Unknown is permitted");
        env.add_condition(Condition {
            condition_type: ConditionType::Sandboxed,
            status: ConditionStatus::True,
            observed_at: Utc::now() + Duration::seconds(2),
            message: "sandbox bound".to_owned(),
        })
        .expect("Unknown -> True is permitted");
        assert_eq!(env.execution.conditions.len(), 2);
    }

    #[test]
    fn canonical_conditions_for_pending_is_empty() {
        let env = make_envelope();
        assert!(env.canonical_conditions_for_phase().is_empty());
    }

    #[test]
    fn canonical_conditions_for_running_requires_policy_and_sandbox() {
        let mut env = make_envelope();
        env.transition_to(ActionPhase::Running).expect("setup");
        let required = env.canonical_conditions_for_phase();
        assert!(required.contains(&ConditionType::PolicyEvaluated));
        assert!(required.contains(&ConditionType::Sandboxed));
        assert_eq!(required.len(), 2);
    }

    #[test]
    fn canonical_conditions_for_succeeded_requires_four_observations() {
        let mut env = make_envelope();
        env.transition_to(ActionPhase::Running).expect("setup");
        env.transition_to(ActionPhase::Succeeded).expect("setup");
        let required = env.canonical_conditions_for_phase();
        assert_eq!(required.len(), 4);
        for ct in [
            ConditionType::PolicyEvaluated,
            ConditionType::Sandboxed,
            ConditionType::Executed,
            ConditionType::Verified,
        ] {
            assert!(required.contains(&ct), "Succeeded must require {ct:?}");
        }
    }

    #[test]
    fn canonical_conditions_for_failed_requires_only_policy_evaluated() {
        let mut env = make_envelope();
        env.transition_to(ActionPhase::Failed).expect("setup");
        let required = env.canonical_conditions_for_phase();
        // A Failed envelope may have many other conditions but only PolicyEvaluated is
        // *required* — pre-policy validation failures still observe a policy eval step.
        assert_eq!(required, vec![ConditionType::PolicyEvaluated]);
    }

    #[test]
    fn canonical_conditions_for_rolled_back_requires_rolled_back_marker() {
        let mut env = make_envelope();
        env.transition_to(ActionPhase::Running).expect("setup");
        env.transition_to(ActionPhase::RolledBack).expect("setup");
        let required = env.canonical_conditions_for_phase();
        assert!(required.contains(&ConditionType::RolledBack));
        assert!(required.contains(&ConditionType::Executed));
    }

    #[test]
    fn validate_phase_conditions_passes_for_properly_built_succeeded_envelope() {
        // Walk Pending -> Running -> Succeeded with the full canonical condition set.
        let mut env = make_envelope();
        push_true(&mut env, ConditionType::PolicyEvaluated, 0);
        push_true(&mut env, ConditionType::PolicyAccepted, 1);
        env.transition_to(ActionPhase::Running).expect("T6");
        push_true(&mut env, ConditionType::Sandboxed, 2);
        push_true(&mut env, ConditionType::Executed, 3);
        push_true(&mut env, ConditionType::Verified, 4);
        env.transition_to(ActionPhase::Succeeded).expect("T7");

        env.validate_phase_conditions()
            .expect("fully-conditioned Succeeded envelope must validate");
    }

    #[test]
    fn validate_phase_conditions_fails_when_succeeded_envelope_misses_verified() {
        // Same walk but omit Verified — the canonical Succeeded set is incomplete.
        let mut env = make_envelope();
        push_true(&mut env, ConditionType::PolicyEvaluated, 0);
        env.transition_to(ActionPhase::Running).expect("T6");
        push_true(&mut env, ConditionType::Sandboxed, 1);
        push_true(&mut env, ConditionType::Executed, 2);
        // No Verified.
        env.transition_to(ActionPhase::Succeeded).expect("T7");

        let err = env
            .validate_phase_conditions()
            .expect_err("Succeeded without Verified must fail validation");
        match err {
            TransitionError::PhaseConditionMismatch { phase, missing } => {
                assert_eq!(phase, ActionPhase::Succeeded);
                assert_eq!(missing, vec![ConditionType::Verified]);
            }
            other => panic!("expected PhaseConditionMismatch, got {other:?}"),
        }
    }

    #[test]
    fn validate_phase_conditions_fails_when_running_envelope_lacks_sandboxed() {
        // S0.1 §6.6: Running requires PolicyEvaluated + Sandboxed.
        let mut env = make_envelope();
        push_true(&mut env, ConditionType::PolicyEvaluated, 0);
        env.transition_to(ActionPhase::Running).expect("T6");
        // No Sandboxed.

        let err = env
            .validate_phase_conditions()
            .expect_err("Running without Sandboxed must fail validation");
        assert!(matches!(
            err,
            TransitionError::PhaseConditionMismatch {
                phase: ActionPhase::Running,
                ..
            }
        ));
    }

    #[test]
    fn full_lifecycle_round_trips_through_json_with_replay_validation() {
        // Build a complete Succeeded envelope, serialise to JSON, parse back, then
        // re-validate phase/conditions consistency. This is the canonical replay
        // pattern for evidence-log consumers.
        let mut original = make_envelope();
        push_true(&mut original, ConditionType::PolicyEvaluated, 0);
        push_true(&mut original, ConditionType::PolicyAccepted, 1);
        original.transition_to(ActionPhase::Running).expect("T6");
        push_true(&mut original, ConditionType::Sandboxed, 2);
        push_true(&mut original, ConditionType::Executed, 3);
        push_true(&mut original, ConditionType::Verified, 4);
        original.transition_to(ActionPhase::Succeeded).expect("T7");

        let json = serde_json::to_string(&original).expect("serialize");
        let reparsed: ActionEnvelope = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(reparsed.execution.phase, ActionPhase::Succeeded);
        assert_eq!(reparsed.execution.conditions.len(), 5);
        reparsed
            .validate_phase_conditions()
            .expect("reparsed envelope must still validate");
    }
}
