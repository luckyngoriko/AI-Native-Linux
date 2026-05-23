//! Caller identity carried on the action envelope (S0.1 §3.3 — provisional rev.2 shape).
//!
//! The full canonical-subject grammar (typed actor model, act-as policies, vault binding) is
//! deferred to L4 `03_identity_model.md`; the rev.2 envelope only carries an opaque
//! `subject_canonical_id` plus the flags the lifecycle FSM and renderers need today.

use serde::{Deserialize, Serialize};

/// Who issued this action. Immutable after `SubmitAction` returns.
///
/// `subject_canonical_id` is opaque at the rev.2 envelope level; L4 will define and
/// enforce the canonical `<type>:<name>[/<sub_id>]` grammar (S0.1 §4.4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Identity {
    /// Opaque canonical subject identifier (rev.2 provisional form; L4 owns the grammar).
    pub subject_canonical_id: String,

    /// `true` when the issuer is an AI agent; used by the Policy Kernel to apply stricter rules.
    ///
    /// Per the project-wide rule, AI agents **propose** typed actions; only the Capability
    /// Runtime executes, and only after a Policy decision.
    pub is_ai: bool,

    /// Optional session identifier (e.g. a chat-session ULID); `None` for stateless calls.
    pub session_id: Option<String>,
}

impl Identity {
    /// Convenience constructor for the common rev.2 case (no session).
    #[must_use]
    pub fn new(subject_canonical_id: impl Into<String>, is_ai: bool) -> Self {
        Self {
            subject_canonical_id: subject_canonical_id.into(),
            is_ai,
            session_id: None,
        }
    }
}
