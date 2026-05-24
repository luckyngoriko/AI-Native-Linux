//! Subject normalization — S2.3 §7.
//!
//! The Policy Kernel accepts the provisional `<type>:<name>[/<sub_id>]` subject string
//! from S0.1 and canonicalizes it through L4 identity into a [`HydratedSubject`]. If
//! hydration fails, the decision short-circuits to `DENY` with
//! `reason_code = SubjectUnauthenticated` (see [`crate::error::PolicyError`]).
//!
//! The hydrated subject is part of the enrichment snapshot (§8) and contributes to
//! determinism (§13).

use serde::{Deserialize, Serialize};

/// Subject taxonomy — S2.3 §7.
///
/// The wire form is the spec's lowercase identifier (`"human"`, `"agent"`, …).
/// `is_ai` in [`HydratedSubject`] is `true` exactly when `subject_type ∈ {Agent, Application}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubjectType {
    /// Interactive human operator.
    Human,
    /// Autonomous LLM/agent subject — `is_ai == true`.
    Agent,
    /// Long-running application subject — `is_ai == true`.
    Application,
    /// Non-AI system service (systemd unit, daemon, …).
    Service,
    /// Hardware/device subject.
    Device,
    /// Stored workflow / scheduled run.
    Workflow,
    /// Operator acting over a remote/admin channel.
    RemoteOperator,
}

/// A fully hydrated subject ready for §5 rule precedence evaluation (S2.3 §7).
///
/// Construction is the responsibility of the L4 identity service; this crate only
/// defines the shape so the Policy Kernel client and rule evaluator can consume it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HydratedSubject {
    /// Stable canonical id, e.g. `"agent:dev:01HX..."`.
    pub canonical_subject_id: String,
    /// Subject taxonomy class.
    pub subject_type: SubjectType,
    /// Group memberships used by `subjects:` rule matchers.
    pub groups: Vec<String>,
    /// Vault-granted capabilities (capability names, not raw secret material).
    pub capabilities: Vec<String>,
    /// Highest privacy ceiling subject is operating under (e.g. `"INTERNAL"`).
    pub session_class: String,
    /// `true` when operating under a recovery-mode credential.
    pub recovery_mode: bool,
    /// `true` when `subject_type ∈ {Agent, Application}` — anchors AI self-approval
    /// prevention (§17) and the `hd.secret_raw_read_by_ai` hard deny (§6).
    pub is_ai: bool,
}
