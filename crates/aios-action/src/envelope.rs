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

use serde::{Deserialize, Serialize};

use crate::{execution::Execution, identity::Identity, request::Request, trace::Trace};

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
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "panic-on-failure is the idiomatic test signal"
)]
mod tests {
    use super::{ActionEnvelope, SCHEMA_VERSION};
    use crate::{identity::Identity, phase::ActionPhase, request::Request, trace::Trace};

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
}
