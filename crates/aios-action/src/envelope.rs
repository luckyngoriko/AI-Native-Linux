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

use crate::canonical::{blake3_hash, jcs_canonicalize, CanonicalError};
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
}
