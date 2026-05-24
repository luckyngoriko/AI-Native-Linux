//! Fixed rule precedence — S2.3 §5.
//!
//! The Policy Kernel evaluates an envelope against a strictly ordered ladder of seven
//! precedence tiers. The order is **constitutional**: it MUST NOT be reordered by any
//! policy bundle, any operator override, or any test setup. The [`RulePrecedence`] enum
//! is the canonical machine-readable form of that ladder; [`RulePrecedence::iter`] yields
//! the tiers in evaluation order so call-sites (the [`crate::pipeline::DecisionPipeline`]
//! today, the audit / explain surface later) cannot accidentally diverge from S2.3 §5.
//!
//! ```text
//! 1. Invalid subject ............................. -> DENY
//! 2. Hard deny (§6) .............................. -> DENY
//! 3. Emergency override denylist (§16) ........... -> DENY
//! 4. Explicit scoped DENY rule ................... -> DENY
//! 5. Explicit scoped ALLOW rule .................. -> ALLOW or REQUIRE_APPROVAL
//! 6. AI self-approval prevention (§17) ........... -> may upgrade ALLOW to REQUIRE_APPROVAL
//! 7. Default ..................................... -> DENY
//! ```
//!
//! Default deny is mandatory (S2.3 §11). Tier 6 is a post-hoc filter applied after tier 5
//! produced an `ALLOW`. The pipeline maps each tier to a discrete step in S2.3 §3 (see
//! `pipeline.rs` for the 1:1 correspondence).

use serde::{Deserialize, Serialize};
use strum_macros::{EnumCount, EnumIter};

/// The seven precedence tiers from S2.3 §5 in fixed evaluation order.
///
/// The variant order is **load-bearing** — `RulePrecedence::iter()` returns the variants
/// in declaration order (via `strum`'s `EnumIter`), and the [`crate::pipeline`] crate
/// drives evaluation off that iteration. Reordering the variants reorders evaluation,
/// which would violate S2.3 §5.
///
/// `EnumCount` anchors the compile-time invariant `RulePrecedence::COUNT == 7`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "PascalCase")]
pub enum RulePrecedence {
    /// Tier 1 — subject hydration failed (`SubjectUnauthenticated`).
    InvalidSubject,
    /// Tier 2 — hard-deny class matched (S2.3 §6).
    HardDeny,
    /// Tier 3 — emergency-override denylist matched (S2.3 §16).
    EmergencyOverrideDenylist,
    /// Tier 4 — explicit scoped DENY rule in the active bundle.
    ExplicitScopedDeny,
    /// Tier 5 — explicit scoped ALLOW rule in the active bundle.
    ExplicitScopedAllow,
    /// Tier 6 — AI self-approval prevention post-filter (S2.3 §17).
    AiSelfApprovalUpgrade,
    /// Tier 7 — default deny (S2.3 §11; mandatory floor).
    DefaultDeny,
}

impl RulePrecedence {
    /// Yields the seven tiers in fixed §5 evaluation order.
    ///
    /// This is a thin re-export of `strum::IntoEnumIterator::iter()` chosen to give the
    /// call-site an obvious, named entry point — `RulePrecedence::iter()` — instead of
    /// requiring callers to import the strum trait directly.
    pub fn iter() -> impl Iterator<Item = Self> {
        <Self as strum::IntoEnumIterator>::iter()
    }
}
