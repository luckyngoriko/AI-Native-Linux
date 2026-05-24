//! `ActionDispatcher` — pure decision helpers for `QueueClass` selection,
//! `ActionDispatchKind` selection, and AI-interactive downgrade marking, per
//! S10.1 §3.2 / §3.5 / §11.
//!
//! Every function here is **pure**: no I/O, no clock, no lock acquisition.
//! The dispatcher is consulted by [`crate::ActionLifecyclePipeline::step_queue`]
//! and [`crate::ActionLifecyclePipeline::step_execute`]; the actual queue
//! enrolment + adapter dispatch happens through
//! [`crate::DispatchQueue::enroll`] and (T-035) the adapter handle.
//!
//! The selectors mirror the spec's closed decision rules verbatim:
//!
//! - [`select_queue_class`] — §3.5 selection rule + §11.4 AI-interactive
//!   downgrade. Recovery-mode forces [`QueueClass::RecoveryPriority`] (§3.5
//!   row 4 — "Any action while `host.recovery_mode = true`").
//!
//! - [`select_dispatch_kind`] — §3.2 closed decision table verbatim. The
//!   `request.dry_run == SIMULATE` short-circuit is the topmost rule but
//!   the action envelope's `DryRunMode` is read by the caller; the
//!   dispatcher accepts a pre-computed `is_simulate` boolean to avoid
//!   re-binding `aios_action::DryRunMode` here.
//!
//! - [`apply_ai_interactive_downgrade`] — §11.4 downgrade marker. Returns
//!   `Some("AI_INTERACTIVE_QUEUE_DOWNGRADE")` when an AI subject is enrolled
//!   under [`QueueClass::Interactive`]; the marker is the evidence record
//!   type T-031 will emit.

use aios_action::ActionEnvelope;

use crate::adapter_manifest::AdapterManifest;
use crate::context::ActionContext;
use crate::dispatch::{ActionDispatchKind, AdapterStability, QueueClass};

/// Evidence record type emitted on the §11.4 AI-interactive downgrade.
///
/// The downgrade is silent at the action level (no failure) but loud at the
/// audit level (every downgrade is forensically visible). T-031 wires the
/// emission against `aios_evidence::RecordType::AiInteractiveQueueDowngrade`;
/// today the marker is returned as a string and the caller logs it.
pub const AI_INTERACTIVE_DOWNGRADE_MARKER: &str = "AI_INTERACTIVE_QUEUE_DOWNGRADE";

/// Stateless decision helper for §3.2 / §3.5 / §11.
///
/// Carries no fields; methods are `&self` for forward compatibility with
/// future state (operator-tunable cap ratios per §11.1, hydrated subjects
/// per T-030). T-029 ships the pure decision rules only.
#[derive(Debug, Default, Clone, Copy)]
pub struct ActionDispatcher;

impl ActionDispatcher {
    /// Construct a fresh stateless dispatcher.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Select the [`QueueClass`] for an envelope per §3.5's selection table
    /// + §11.4's silent downgrade.
    ///
    /// Rule order (closed):
    /// 1. `recovery_mode == true` → [`QueueClass::RecoveryPriority`]
    ///    (§3.5 row 4 preempts all other classes).
    /// 2. `is_ai == true` → [`QueueClass::AgentProposal`] (§3.5 row 2;
    ///    the §11.4 downgrade is folded into the selection so an AI subject
    ///    can never request `INTERACTIVE` in the first place — the marker
    ///    returned by [`apply_ai_interactive_downgrade`] is the forensic
    ///    breadcrumb).
    /// 3. otherwise → [`QueueClass::Interactive`] (§3.5 row 1; the spec
    ///    defines [`QueueClass::Background`] for "scheduled jobs, timers,
    ///    application-internal cleanup actions" — T-029 does not yet
    ///    classify a subject as background and defaults to `INTERACTIVE`
    ///    for any non-AI, non-recovery submission. T-030's hydrated
    ///    subject will surface the background distinction).
    #[must_use]
    pub const fn select_queue_class(envelope: &ActionEnvelope, recovery_mode: bool) -> QueueClass {
        if recovery_mode {
            return QueueClass::RecoveryPriority;
        }
        if envelope.identity.is_ai {
            return QueueClass::AgentProposal;
        }
        QueueClass::Interactive
    }

    /// Select the [`ActionDispatchKind`] per §3.2's closed decision table.
    ///
    /// Rule order (verbatim from §3.2 lines 128..138):
    /// 1. `is_simulate == true`                                    → `DRY_RUN`
    /// 2. `is_ai == true`                                          → `ISOLATED_SANDBOX`
    /// 3. `risk.privileged == true`                                → `ISOLATED_SANDBOX`
    /// 4. `manifest.dispatch_kind == SUBPROCESS_FORK`              → `SUBPROCESS_FORK`
    /// 5. `manifest.dispatch_kind == IN_PROCESS_RPC && STABLE`     → `IN_PROCESS_RPC`
    /// 6. otherwise                                                → `SUBPROCESS_FORK`
    ///
    /// `risk_privileged` is the `request.risk.privileged` flag from S0.1
    /// §4.7. Today's [`aios_action::Request`] does not surface the typed
    /// risk struct (the rev.2 envelope ships the lifecycle-critical subset
    /// only); callers pass the value explicitly so the dispatcher remains
    /// stable when the typed risk surface lands.
    #[must_use]
    pub fn select_dispatch_kind(
        manifest: &AdapterManifest,
        is_ai: bool,
        is_simulate: bool,
        risk_privileged: bool,
    ) -> ActionDispatchKind {
        if is_simulate {
            return ActionDispatchKind::DryRun;
        }
        if is_ai || risk_privileged {
            return ActionDispatchKind::IsolatedSandbox;
        }
        if manifest.dispatch_kind == ActionDispatchKind::InProcessRpc
            && manifest.declared_stability == AdapterStability::Stable
        {
            ActionDispatchKind::InProcessRpc
        } else {
            // SUBPROCESS_FORK is the §3.2 "Else" terminus and the explicit
            // case for `manifest.dispatch_kind == SUBPROCESS_FORK`. §3.2
            // line 140: "EXPERIMENTAL and DEPRECATED adapters never run
            // in-process regardless of manifest declaration" — folded into
            // the single fallback arm.
            ActionDispatchKind::SubprocessFork
        }
    }

    /// Return the §11.4 downgrade marker when an AI subject is being
    /// enrolled under [`QueueClass::Interactive`].
    ///
    /// Returns `Some(AI_INTERACTIVE_DOWNGRADE_MARKER)` exactly when both
    /// `is_ai == true` **and** `context.queue_class == Interactive`. In every
    /// other case returns `None` (no downgrade required).
    ///
    /// The marker is the evidence record type T-031 will emit
    /// (`AI_INTERACTIVE_QUEUE_DOWNGRADE`, retention `STANDARD_24M` per
    /// §13). The caller is responsible for rerouting the context's
    /// `queue_class` to [`QueueClass::AgentProposal`] before
    /// [`crate::DispatchQueue::enroll`].
    #[must_use]
    pub fn apply_ai_interactive_downgrade(
        context: &ActionContext,
        is_ai: bool,
    ) -> Option<&'static str> {
        if is_ai && context.queue_class == QueueClass::Interactive {
            Some(AI_INTERACTIVE_DOWNGRADE_MARKER)
        } else {
            None
        }
    }
}
