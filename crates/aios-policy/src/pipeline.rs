//! The 12-step decision pipeline — S2.3 §3.
//!
//! `EvaluatePolicy(envelope) -> PolicyDecision` is the single hot-path entry point of the
//! Policy Kernel; this module implements that pipeline as twelve discrete steps that each
//! either short-circuit with a terminal [`PolicyDecision`] or pass `(envelope, context,
//! partial state)` to the next step. Per S2.3 §3: **no silent fall-through is allowed; every
//! envelope produces a decision.**
//!
//! ## What is real vs stubbed in T-017
//!
//! T-017 lands the **trait surface, the step skeleton, the precedence ladder, and
//! the active default-deny floor** (S2.3 §11). Steps 4 (hard denies), 5 (emergency-override
//! denylist), 6 (scoped denies), 7 (scoped allows), 8 (AI self-approval prevention), 10
//! (constraint binding), and 12 (evidence emission) are deliberately stubbed and return
//! `PipelineState::Continue` with the partial state untouched. Each stub names the task
//! that lands the real implementation (T-018..T-025) so the placeholder cannot drift
//! silently. Step 1 (schema validation), step 2 (subject normalization), step 9 (default
//! deny), and step 11 (decision emission) are real and exercised by the tests.
//!
//! ## Why a step enum
//!
//! The [`PipelineState`] enum makes the short-circuit semantics explicit at the type
//! level: the pipeline driver loop terminates the moment a step returns `ShortCircuit`,
//! which is exactly the contract S2.3 §3 demands (no silent fall-through, every step is
//! authoritative when it fires). The test suite verifies this by counting executed steps
//! after an injected short-circuit.

use chrono::Utc;
use ulid::Ulid;

use aios_action::ActionEnvelope;

use crate::constraints::{ApprovalRequirement, ApprovalScope, ApproverClass, Constraints};
use crate::decision::{Decision, PolicyDecision};
use crate::hard_deny_engine::{reason_code_for, reason_message_for, HardDenyEngine};
use crate::kernel::PolicyContext;
use crate::override_boundary::OverrideBoundary;
use crate::subject::HydratedSubject;

/// Outcome of a single pipeline step.
///
/// `Continue` carries the partial state forward to the next step. `ShortCircuit` halts
/// the pipeline immediately with the supplied terminal [`PolicyDecision`]. Per S2.3 §3
/// there is no third option — every step's result must be either "advance" or "decide".
///
/// `ShortCircuit` boxes the decision because [`PolicyDecision`] carries 14 fields
/// (S2.3 §4), which dwarfs the zero-byte `Continue` variant. Without the box, every
/// `PipelineState` value would carry the decision's size by value and inflate every
/// step's return frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PipelineState {
    /// Step did not produce a terminal outcome; the pipeline continues.
    Continue,
    /// Step produced a terminal decision; the pipeline halts here.
    ShortCircuit(Box<PolicyDecision>),
}

/// Reason-code constants used by the steps that DO land in T-017 — held centrally so the
/// test suite and downstream consumers (audit, explain) refer to a single source of truth.
///
/// Bundle-authored codes (`"ScopedAllow"`, `"hd.<class>"`, etc.) are NOT in this set; they
/// arrive through the policy bundle in T-018..T-022.
pub mod reason_code {
    /// Step 1 — envelope schema validation failed (S2.3 §3 step 1 / S0.1).
    pub const SCHEMA_INVALID: &str = "SchemaInvalid";
    /// Step 9 — no rule matched; default-deny floor fired (S2.3 §11).
    pub const DEFAULT_DENY: &str = "DefaultDeny";
    /// Step 2 — subject hydration via L4 identity failed (S2.3 §3 step 2 / §7).
    ///
    /// Decision short-circuits to `DENY` whether the provisional id was unknown,
    /// expired, or revoked — §7 deliberately collapses the three failure modes
    /// at the policy boundary so identity-existence cannot leak.
    pub const SUBJECT_UNAUTHENTICATED: &str = "SubjectUnauthenticated";
    /// Step 8 — §17 AI self-approval prevention upgraded a scoped `ALLOW` to
    /// `REQUIRE_APPROVAL` because the subject is AI and at least one risk flag
    /// is set on the request.
    pub const AI_SELF_APPROVAL_UPGRADE: &str = "AiSelfApprovalUpgrade";
    /// Step 5 — emergency override matched and relaxed a scoped rule.
    ///
    /// (S2.3 §3 step 6 / §5 tier 3 / §16.) The decision is `ALLOW` and
    /// the active override receipt id is recorded in the decision's
    /// `reason_message`; downstream evidence integration attaches the
    /// override receipt id to the `evidence_receipt_id` linkage chain.
    pub const EMERGENCY_OVERRIDE_RELAXED: &str = "EmergencyOverrideRelaxed";
}

/// Truthiness of a risk flag inside `request.target.risk.<flag>`.
///
/// Per S2.3 §17.1 the risk fields are `request.risk.destructive`,
/// `request.risk.privileged`, `request.risk.network_exposure`,
/// `request.risk.secret_access`, `request.risk.recovery_path_affected`. The
/// envelope carries the request payload as a free-form `serde_json::Value`
/// (S0.1 §4.3 — the adapter manifest restores schema at the adapter layer);
/// §17 only needs the boolean projection. A missing or non-boolean field is
/// treated as `false`.
fn risk_flag(envelope: &ActionEnvelope, flag: &str) -> bool {
    envelope
        .request
        .target
        .get("risk")
        .and_then(|r| r.get(flag))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
}

/// `true` when **any** of the five §17 risk flags on the envelope's request is `true`.
///
/// Mirrors §17.1: `destructive ∨ privileged ∨ network_exposure ∨ secret_access ∨
/// recovery_path_affected`. The five flags are constitutional — adding a sixth
/// triggers a new bundle-load shape, so the list is closed.
fn any_risk_flag(envelope: &ActionEnvelope) -> bool {
    risk_flag(envelope, "destructive")
        || risk_flag(envelope, "privileged")
        || risk_flag(envelope, "network_exposure")
        || risk_flag(envelope, "secret_access")
        || risk_flag(envelope, "recovery_path_affected")
}

/// Pure §17 evaluator — given the current `(decision, subject, envelope)`,
/// return the upgraded [`ApprovalRequirement`] when §17 applies, or `None`
/// when the decision is unchanged.
///
/// §17 fires only when **all three** hold:
/// 1. `decision == Decision::Allow` (it is a post-§5-step-5 filter; never
///    downgrades a `DENY`, never touches `REQUIRE_APPROVAL` on its own — §17.2).
/// 2. `subject.is_ai == true` (AI agents and applications — §7).
/// 3. At least one risk flag on the envelope's request is true (§17.1).
///
/// The §17.3 carve-out is also applied: when all five risk flags are `false`
/// the subject's AI nature does **not** trigger an upgrade. The pure
/// `any_risk_flag(envelope) == false` short-circuit before the `is_ai` check
/// implements this carve-out without a separate code path.
#[must_use]
pub fn evaluate_ai_self_approval_prevention(
    decision: Decision,
    subject: &HydratedSubject,
    envelope: &ActionEnvelope,
) -> Option<ApprovalRequirement> {
    if decision != Decision::Allow {
        return None;
    }
    if !subject.is_ai {
        return None;
    }
    if !any_risk_flag(envelope) {
        // §17.3 carve-out: self-management low-risk actions remain ALLOW for
        // AI subjects. Returning `None` leaves the decision unchanged.
        return None;
    }
    Some(ApprovalRequirement {
        required: true,
        approval_scope: ApprovalScope::ExactRequestHash,
        // Approval validity inherits from §13.2 default (300 s); the rule
        // does not pin a value, the bundle / approval-mechanics layer does.
        ttl_seconds: 300,
        // §17.1: "approval.approver_classes must include 'human' (and exclude
        // AI types)". The minimal §17 contract is `[Human]`; bundle authors
        // may widen to `[Human, Operator]` etc. but never include AI classes.
        approver_classes: vec![ApproverClass::Human],
        require_human_co_signer: false,
    })
}

/// Fields the [`DecisionPipeline::emit_decision`] helper needs to assemble a
/// fully populated [`PolicyDecision`].
///
/// Exists as a struct purely to keep the assembler's arity below the
/// `clippy::too_many_arguments` threshold without losing field-name clarity at the
/// call sites — every site passes a named-field literal.
#[derive(Clone, Copy)]
struct EmitDecision<'a> {
    envelope: &'a ActionEnvelope,
    context: &'a PolicyContext,
    request_hash: &'a str,
    decision: Decision,
    reason_code: &'a str,
    reason_message: &'a str,
    rules_consulted: u32,
}

/// The 12-step decision pipeline driver.
///
/// Holds the in-flight `(envelope, context)` and exposes one method per step so that
/// (a) the test suite can call individual steps in isolation to verify their semantics,
/// and (b) the future T-024 caching layer can wrap individual steps without re-running
/// the whole pipeline.
///
/// The driver is **stateless across evaluations** — `evaluate` clones nothing, owns
/// nothing across calls, and is safe to share between async tasks. State that should
/// outlive a single evaluation (bundle index, cache, rate-limit token buckets) belongs
/// in the impl that constructs the pipeline, not in the pipeline itself.
#[derive(Debug, Default, Clone, Copy)]
pub struct DecisionPipeline;

impl DecisionPipeline {
    /// Construct a fresh, empty pipeline driver.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Drive the full 12-step pipeline for one envelope.
    ///
    /// Returns the terminal [`PolicyDecision`] from the first step that short-circuits,
    /// or the `DefaultDeny` decision minted by step 9 if no earlier step short-circuits.
    ///
    /// Equivalent to [`Self::evaluate_with_hard_deny_engine`] with `engine = None`.
    /// Retained for the T-017 baseline tests that construct a bare
    /// [`crate::kernel::InMemoryPolicyKernel`] via `new()` and expect the §6
    /// stub semantics (step 4 is a no-op pass-through).
    #[must_use]
    pub fn evaluate(self, envelope: &ActionEnvelope, context: &PolicyContext) -> PolicyDecision {
        self.evaluate_with_hard_deny_engine(envelope, context, None)
    }

    /// Drive the full 12-step pipeline for one envelope with an optional
    /// [`HardDenyEngine`] attached.
    ///
    /// When `engine` is `Some`, step 4 calls it and short-circuits with a
    /// `Decision::Deny` carrying `reason_code = "HardDeny:<Variant>"` (per
    /// [`reason_code_for`]) and `reason_message` per [`reason_message_for`].
    /// When `engine` is `None`, step 4 remains a pass-through stub (T-017
    /// semantics).
    #[must_use]
    pub fn evaluate_with_hard_deny_engine(
        self,
        envelope: &ActionEnvelope,
        context: &PolicyContext,
        engine: Option<&HardDenyEngine>,
    ) -> PolicyDecision {
        self.evaluate_with_chain(envelope, context, Some(context), engine)
    }

    /// Drive the full pipeline with subject hydration already resolved by the
    /// kernel (T-021).
    ///
    /// `hydrated_context` is what step 2 would have produced if the kernel had
    /// run hydration inline; the kernel runs `async` hydration outside the
    /// pipeline driver and passes the result in. `None` signals
    /// `SubjectUnauthenticated` per §7 — step 2 short-circuits to `DENY` with
    /// `reason_code = SubjectUnauthenticated`. `Some(c)` means hydration
    /// succeeded; the pipeline uses `c` for the rest of evaluation. When the
    /// kernel has no hydrator attached, callers pass `Some(context)` (the
    /// original context, T-017 baseline).
    ///
    /// `original_context` is only used to mint the `SubjectUnauthenticated`
    /// short-circuit decision so the rejected decision still references the
    /// pre-hydration bundle / enrichment ids — the decision must be assembleable
    /// even when hydration failed.
    #[must_use]
    pub fn evaluate_with_chain(
        self,
        envelope: &ActionEnvelope,
        original_context: &PolicyContext,
        hydrated_context: Option<&PolicyContext>,
        engine: Option<&HardDenyEngine>,
    ) -> PolicyDecision {
        self.evaluate_with_chain_full(envelope, original_context, hydrated_context, engine, None)
    }

    /// Full-chain evaluator with override boundary (T-025).
    ///
    /// Identical to [`Self::evaluate_with_chain`] except step 5 consults
    /// the supplied `boundary` (S2.3 §3 step 6 / §16). The two ctors
    /// `evaluate_with_chain` (without boundary) and this overload are
    /// kept distinct so T-017..T-024 baseline tests continue to compile
    /// against the pre-T-025 signature.
    #[must_use]
    pub fn evaluate_with_chain_full(
        self,
        envelope: &ActionEnvelope,
        original_context: &PolicyContext,
        hydrated_context: Option<&PolicyContext>,
        engine: Option<&HardDenyEngine>,
        boundary: Option<&OverrideBoundary>,
    ) -> PolicyDecision {
        let request_hash = compute_request_hash(envelope);

        // Step 1 — schema validation. Uses the original context so the
        // assembled decision still has bundle_version + enrichment snapshot id
        // when the malformed envelope never reaches hydration.
        if let PipelineState::ShortCircuit(d) =
            self.step_1_validate_schema(envelope, original_context, &request_hash)
        {
            return *d;
        }

        // Step 2 — subject normalization (S2.3 §7). When the kernel's
        // hydrator returned `Err(SubjectUnauthenticated)`, the kernel passed
        // `hydrated_context = None`; we short-circuit to DENY here.
        let Some(context) = hydrated_context else {
            return Self::emit_decision(EmitDecision {
                envelope,
                context: original_context,
                request_hash: &request_hash,
                decision: Decision::Deny,
                reason_code: reason_code::SUBJECT_UNAUTHENTICATED,
                reason_message:
                    "subject hydration failed: unknown, expired, or revoked subject (S2.3 §7)",
                rules_consulted: 1,
            });
        };

        // Step 3 — enrichment (stub; T-018+ populates EnrichmentSnapshot).
        if let PipelineState::ShortCircuit(d) = Self::step_3_enrich_resources() {
            return *d;
        }

        // Step 4 — hard denies. T-018 wires the HardDenyEngine; when absent
        // (T-017 baseline kernel), the stub passes through.
        if let PipelineState::ShortCircuit(d) =
            self.step_4_evaluate_hard_denies_with_engine(envelope, context, &request_hash, engine)
        {
            return *d;
        }

        // Step 5 — emergency-override denylist (T-025). When a boundary is
        // attached and a matching active grant exists, the step short-circuits
        // to ALLOW with `reason_code = EmergencyOverrideRelaxed` and the
        // override receipt id recorded in the reason message. Per §16.2 the
        // boundary cannot be reached on a hard-deny path (step 4 fires first)
        // and grants targeting §6 classes are rejected at request time, so
        // the override path is constitutionally safe.
        if let PipelineState::ShortCircuit(d) =
            self.step_5_emergency_override_with_boundary(envelope, context, &request_hash, boundary)
        {
            return *d;
        }

        // Step 6 — scoped denies (stub; T-022 bundle loader feeds the rule index).
        if let PipelineState::ShortCircuit(d) = Self::step_6_evaluate_scoped_denies() {
            return *d;
        }

        // Step 7 — scoped allows (stub; T-022 bundle loader). When a future
        // task lands the rule index, scoped ALLOW partial decisions flow from
        // here into step 8 (§17 filter) via [`Self::apply_step_8`]; for now
        // the stub returns `Continue` and step 8 is a no-op on the end-to-end
        // path. Step 8's pure §17 evaluator is tested directly.
        if let PipelineState::ShortCircuit(d) = Self::step_7_evaluate_scoped_allows() {
            return *d;
        }

        // Step 8 — AI self-approval prevention (S2.3 §17). Today the stub
        // upstream means no ALLOW partial state reaches this point; the
        // pure evaluator [`evaluate_ai_self_approval_prevention`] is wired
        // into [`Self::apply_step_8`] for when T-022 lands the scoped-allow
        // path.
        if let PipelineState::ShortCircuit(d) = Self::step_8_ai_self_approval_prevention() {
            return *d;
        }

        // Step 9 — default deny floor (REAL — S2.3 §11 mandates this fires whenever
        // no earlier step short-circuited).
        match self.step_9_apply_default_deny(envelope, context, &request_hash) {
            PipelineState::ShortCircuit(d) => *d,
            PipelineState::Continue => {
                // Defensive: step 9 is constitutionally a short-circuit producer. If
                // a future refactor accidentally returns Continue, we still produce
                // SOMETHING (the spec's no-silent-fall-through rule) — a synthetic
                // DefaultDeny so the contract holds even under impl drift.
                Self::emit_decision(EmitDecision {
                    envelope,
                    context,
                    request_hash: &request_hash,
                    decision: Decision::Deny,
                    reason_code: reason_code::DEFAULT_DENY,
                    reason_message: "default deny (S2.3 §11)",
                    rules_consulted: 0,
                })
            }
        }
    }

    // --- Individual steps -------------------------------------------------

    /// Step 1 — validate envelope schema (S2.3 §3 step 1 / S0.1).
    ///
    /// In T-017 this is a structural floor check: a non-empty `schema_version`, a
    /// non-empty `request.action`, and a non-empty `subject_canonical_id`. The full
    /// S0.1 schema/idempotency validation runs upstream at `SubmitAction`; this step
    /// exists so a malformed envelope reaching the policy kernel directly is rejected
    /// rather than processed.
    #[must_use]
    pub fn step_1_validate_schema(
        self,
        envelope: &ActionEnvelope,
        context: &PolicyContext,
        request_hash: &str,
    ) -> PipelineState {
        if envelope.schema_version.is_empty()
            || envelope.request.action.is_empty()
            || envelope.identity.subject_canonical_id.is_empty()
        {
            return PipelineState::ShortCircuit(Box::new(Self::emit_decision(EmitDecision {
                envelope,
                context,
                request_hash,
                decision: Decision::Deny,
                reason_code: reason_code::SCHEMA_INVALID,
                reason_message: "envelope failed schema validation (S2.3 §3 step 1)",
                rules_consulted: 1,
            })));
        }
        PipelineState::Continue
    }

    /// Step 2 — normalize subject (S2.3 §3 step 2 / §7).
    ///
    /// T-017 trusts the [`crate::subject::HydratedSubject`] passed in via
    /// [`PolicyContext`]; the full L4 identity hydrator (group resolution, capability
    /// propagation, recovery-mode credential check) is T-021.
    #[must_use]
    pub const fn step_2_normalize_subject() -> PipelineState {
        PipelineState::Continue
    }

    /// Step 3 — enrich resources (S2.3 §3 step 3 / §8).
    ///
    /// **STUB** — T-018+ populates [`crate::kernel::EnrichmentSnapshot`] with the
    /// SNAPSHOT-consistent AIOS-FS reads (`privacy_class`, `policy_tags`, …) and the
    /// adapter-manifest `risk_template`. Today the snapshot is empty.
    #[must_use]
    pub const fn step_3_enrich_resources() -> PipelineState {
        PipelineState::Continue
    }

    /// Step 4 — evaluate hard denies (S2.3 §3 step 5 / §6) — **stub form**.
    ///
    /// Retained as a const no-op for the T-017 baseline tests that pin the
    /// stub contract. The real engine-driven path lives on
    /// [`Self::step_4_evaluate_hard_denies_with_engine`]; the driver loop in
    /// [`Self::evaluate_with_hard_deny_engine`] dispatches between the two
    /// based on whether an engine is attached.
    #[must_use]
    pub const fn step_4_evaluate_hard_denies() -> PipelineState {
        PipelineState::Continue
    }

    /// Step 4 — evaluate hard denies (S2.3 §3 step 5 / §6) — **engine-driven form**.
    ///
    /// When `engine` is `Some(e)` this calls `e.check(envelope, &context.subject)`
    /// and short-circuits with a `Decision::Deny` carrying a
    /// `HardDeny:<Variant>` `reason_code` on the first matching §6 class. When
    /// `engine` is `None`, this returns `Continue` (matching the T-017 stub
    /// semantics).
    ///
    /// Per §6 the 10 hard-deny classes are constitutional and cannot be
    /// overridden except as listed in the spec table. The two overridable
    /// rows (`hd.modify_boot_chain`, `hd.aios_fs_pointer_rollback_on_recovery`)
    /// still produce `DENY` here; the override path lives downstream (T-025)
    /// and produces an evidence-linked override receipt without flipping the
    /// engine's verdict.
    #[must_use]
    pub fn step_4_evaluate_hard_denies_with_engine(
        self,
        envelope: &ActionEnvelope,
        context: &PolicyContext,
        request_hash: &str,
        engine: Option<&HardDenyEngine>,
    ) -> PipelineState {
        let Some(engine) = engine else {
            return PipelineState::Continue;
        };
        let Some(class) = engine.check(envelope, &context.subject) else {
            return PipelineState::Continue;
        };
        let reason_code = reason_code_for(class);
        let reason_message = reason_message_for(class);
        PipelineState::ShortCircuit(Box::new(Self::emit_decision(EmitDecision {
            envelope,
            context,
            request_hash,
            decision: Decision::Deny,
            reason_code: &reason_code,
            reason_message: &reason_message,
            // The engine consulted exactly one constitutional table; account
            // for it in the audit count so a §6 fire is visible in the
            // `rules_consulted` histogram.
            rules_consulted: 1,
        })))
    }

    /// Step 5 — evaluate emergency-override denylist (S2.3 §3 step 6 / §16) —
    /// **stub form**.
    ///
    /// Retained as a const no-op for the T-017 baseline tests that pin the
    /// stub contract. The real boundary-driven path lives on
    /// [`Self::step_5_emergency_override_with_boundary`]; the driver loop
    /// dispatches between the two based on whether a boundary is attached.
    #[must_use]
    pub const fn step_5_emergency_override_denylist() -> PipelineState {
        PipelineState::Continue
    }

    /// Step 5 — evaluate emergency-override denylist (S2.3 §3 step 6 / §16) —
    /// **boundary-driven form** (T-025).
    ///
    /// When `boundary` is `Some(b)` this calls `b.is_overridden(action,
    /// subject)` and short-circuits with `Decision::Allow` carrying
    /// `reason_code = "EmergencyOverrideRelaxed"` and the override receipt
    /// id in the reason message when a matching active grant is found. When
    /// `boundary` is `None`, this returns `Continue` (the T-017 stub
    /// semantics).
    ///
    /// Per §16.2 the override CANNOT bypass hard denies; this is enforced
    /// upstream (step 4 fires first, and the boundary's
    /// `request_override` rejects hard-deny-targeting grants at grant
    /// time). Step 5 is therefore reachable only after the hard-deny gate
    /// has passed, which is the constitutional guarantee §16.2 needs.
    #[must_use]
    pub fn step_5_emergency_override_with_boundary(
        self,
        envelope: &ActionEnvelope,
        context: &PolicyContext,
        request_hash: &str,
        boundary: Option<&OverrideBoundary>,
    ) -> PipelineState {
        let Some(boundary) = boundary else {
            return PipelineState::Continue;
        };
        let Some(grant) = boundary.is_overridden(
            &envelope.request.action,
            &context.subject.canonical_subject_id,
        ) else {
            return PipelineState::Continue;
        };
        let reason_message = format!(
            "scoped rule `{}` relaxed by emergency override `{}` (granted_by={}, reason={}) — S2.3 §16",
            grant.scope.rule_id,
            grant.override_id,
            grant.granted_by_subject_id,
            grant.reason
        );
        PipelineState::ShortCircuit(Box::new(Self::emit_decision(EmitDecision {
            envelope,
            context,
            request_hash,
            decision: Decision::Allow,
            reason_code: reason_code::EMERGENCY_OVERRIDE_RELAXED,
            reason_message: &reason_message,
            // The boundary consulted one grant table; account for it in
            // the audit count so a §16 fire is visible in
            // `rules_consulted`.
            rules_consulted: 1,
        })))
    }

    /// Step 6 — evaluate scoped denies (S2.3 §3 step 7 / §5 tier 4).
    ///
    /// **STUB** — T-022 lands the bundle loader; the rule index it produces is what
    /// this step consults.
    #[must_use]
    pub const fn step_6_evaluate_scoped_denies() -> PipelineState {
        PipelineState::Continue
    }

    /// Step 7 — evaluate scoped allows (S2.3 §3 step 8 / §5 tier 5).
    ///
    /// **STUB** — see step 6 for the dependency on T-022.
    #[must_use]
    pub const fn step_7_evaluate_scoped_allows() -> PipelineState {
        PipelineState::Continue
    }

    /// Step 8 — apply AI self-approval prevention (S2.3 §3 step 9 / §17).
    ///
    /// **Pipeline-driver hook (no-op until scoped allows land).** The full §17
    /// filter is implemented as the pure function
    /// [`evaluate_ai_self_approval_prevention`] and applied to scoped-allow
    /// partial decisions via [`Self::apply_step_8`]. T-021 ships both
    /// pieces; the driver's step-7 stub means no ALLOW partial state reaches
    /// this point on the end-to-end path until T-022 lands the scoped-allow
    /// rule index. The hook remains a `Continue` no-op for the driver loop
    /// so the precedence ladder stays honest (no fake ALLOWs minted here).
    #[must_use]
    pub const fn step_8_ai_self_approval_prevention() -> PipelineState {
        PipelineState::Continue
    }

    /// Apply step 8 (§17 AI self-approval prevention) to a partial-state
    /// scoped-ALLOW [`PolicyDecision`].
    ///
    /// This is the API the future scoped-allow path (T-022) calls **after**
    /// step 7 produces a partial ALLOW decision and **before** step 9 emits
    /// the terminal result. It is also what the T-021 integration tests call
    /// to anchor the §17 contract end-to-end without depending on the
    /// rule-index implementation.
    ///
    /// Semantics:
    ///
    /// - When the input decision is not `Allow`, the decision is returned
    ///   unchanged (§17.2: never downgrades a `DENY`; never touches an existing
    ///   `REQUIRE_APPROVAL` on its own).
    /// - When the input is `Allow` and `subject.is_ai` is true and at least
    ///   one risk flag on the request is true, the decision is upgraded:
    ///     - `decision` → `RequireApproval`,
    ///     - `reason_code` → `"AiSelfApprovalUpgrade"`,
    ///     - `reason_message` → English §17 explanation,
    ///     - `approval` → `ApprovalRequirement { required: true, ...,
    ///       approver_classes: [Human] }`.
    /// - When the input is `Allow` but all risk flags are `false`, the §17.3
    ///   carve-out (self-management low-risk actions) applies and the
    ///   decision is returned unchanged.
    ///
    /// The original `policy_decision_id`, `request_hash`, `bundle_version`,
    /// `enrichment_snapshot_id`, `evaluated_at` and `rules_consulted` are
    /// preserved so the upgrade is auditable — the same decision id appears
    /// in both the partial `ALLOW` evidence record and the upgraded
    /// `REQUIRE_APPROVAL` emission, and an explain-decision query reconstructs
    /// the full §5 ladder.
    #[must_use]
    pub fn apply_step_8(
        decision: PolicyDecision,
        subject: &HydratedSubject,
        envelope: &ActionEnvelope,
    ) -> PolicyDecision {
        let Some(approval) =
            evaluate_ai_self_approval_prevention(decision.decision, subject, envelope)
        else {
            return decision;
        };
        PolicyDecision {
            decision: Decision::RequireApproval,
            reason_code: reason_code::AI_SELF_APPROVAL_UPGRADE.to_owned(),
            reason_message:
                "AI subject self-approval prevented; human approval required (S2.3 §17)".to_owned(),
            approval,
            // Step 8 consults one constitutional rule (§17). Count it so the
            // §19 budget audit sees the upgrade.
            rules_consulted: decision.rules_consulted.saturating_add(1),
            ..decision
        }
    }

    /// Step 9 — apply default deny (S2.3 §3 step 10 / §11).
    ///
    /// **REAL** — when no earlier step short-circuited, this step ALWAYS produces a
    /// terminal `DENY` with `reason_code = "DefaultDeny"`. Default deny is
    /// constitutional (S2.3 §11) and cannot be downgraded by any bundle.
    #[must_use]
    pub fn step_9_apply_default_deny(
        self,
        envelope: &ActionEnvelope,
        context: &PolicyContext,
        request_hash: &str,
    ) -> PipelineState {
        PipelineState::ShortCircuit(Box::new(Self::emit_decision(EmitDecision {
            envelope,
            context,
            request_hash,
            decision: Decision::Deny,
            reason_code: reason_code::DEFAULT_DENY,
            reason_message: "no matching policy rule; default deny (S2.3 §11)",
            // Stubs in 3..=8 do not consult rules; the only consultable steps in T-017
            // are 1 (schema) and 2 (subject). Reported as 0 to keep the audit honest.
            rules_consulted: 0,
        })))
    }

    /// Step 10 — bind constraints (S2.3 §3 step 11 / §10).
    ///
    /// **STUB** — T-020 lands the full §10 constraint vocabulary. Today an empty
    /// [`Constraints`] is attached to every emitted decision.
    #[must_use]
    pub fn step_10_bind_constraints() -> Constraints {
        Constraints::default()
    }

    /// Step 11 — emit decision (S2.3 §3 step 12, partial — the decision struct).
    ///
    /// **REAL** — assembles a fully populated [`PolicyDecision`] from the supplied
    /// parts. Always invoked via [`Self::emit_decision`] so the field assembly is
    /// kept in one place. Step 12 (evidence emission to S3.1) is a separate hook
    /// landed in M5+; the `evidence_receipt_id` field is left empty here.
    fn emit_decision(parts: EmitDecision<'_>) -> PolicyDecision {
        PolicyDecision {
            policy_decision_id: format!("poldec_{}", Ulid::new()),
            action_id: envelope_action_id(parts.envelope),
            request_hash: parts.request_hash.to_owned(),
            bundle_version: parts.context.bundle_version.clone(),
            enrichment_snapshot_id: parts.context.enrichment.snapshot_id.clone(),
            decision: parts.decision,
            reason_code: parts.reason_code.to_owned(),
            reason_message: parts.reason_message.to_owned(),
            constraints: Self::step_10_bind_constraints(),
            approval: ApprovalRequirement::default(),
            // Step 12 evidence-emission hook — populated by the evidence-log
            // integration in M5+. For T-017 the receipt id is the empty string;
            // a non-empty value here is a future invariant the evidence pipeline
            // will enforce.
            evidence_receipt_id: String::new(),
            evaluated_at: Utc::now(),
            rules_consulted: parts.rules_consulted,
            simulated: false,
        }
    }
}

/// Compute the `request_hash` for the supplied envelope (S2.3 §3 step 4 / S0.1 §8.5).
///
/// On failure (the canonicaliser cannot serialise the request payload — vanishingly rare
/// because `Request` is a serde-derived struct of well-typed primitives), we fall back to
/// an empty string so the pipeline still produces a deterministic outcome. The decision
/// taken with an empty `request_hash` is still a `DENY` (step 1 will have already fired
/// if the envelope is malformed); the empty hash is a sentinel a downstream auditor can
/// recognise as "canonicalisation failed".
#[must_use]
fn compute_request_hash(envelope: &ActionEnvelope) -> String {
    envelope.request.request_hash().unwrap_or_default()
}

/// Bridge: derive an `ActionId` for the supplied envelope.
///
/// `aios_action::Identity` does not carry an `ActionId` today (the envelope's identity
/// section is "who issued this action", not the action's own id). The rev.2 envelope's
/// action-id binding (S0.1 §3.2 / §8.5 content-addressing) is owned by the Capability
/// Runtime (T-002 / T-006) which mints the id at `SubmitAction` time and threads it
/// through the envelope. The Policy Kernel does not control this binding.
///
/// For T-017, until the Capability Runtime lands and the envelope carries a concrete
/// `ActionId` field, the pipeline mints a fresh ULID on each evaluation. The tests
/// that exercise `action_id` (the 14-field populated check) assert the field is non-empty
/// per evaluation; cross-evaluation stability lands once the envelope carries the id.
/// This is the correct constitutional shape: T-017 must not invent an id binding that
/// does not exist in the envelope contract.
fn envelope_action_id(_envelope: &ActionEnvelope) -> aios_action::ActionId {
    aios_action::ActionId::new()
}
