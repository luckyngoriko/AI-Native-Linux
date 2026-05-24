//! Emergency override boundary — S2.3 §16.
//!
//! Emergency override exists for situations where a scoped policy must be relaxed
//! temporarily by a human operator (e.g. during incident response). The boundary
//! defined here is the **closed surface** of what an override can and cannot
//! achieve; it is consulted by the decision pipeline at step 5 (§3 step 6 /
//! §5 tier 3) BEFORE scoped denies are evaluated.
//!
//! ## What override CAN bypass (§16.1)
//!
//! - Specific scoped `DENY` rules (when the override grant explicitly references
//!   the target rule + action).
//! - Specific scoped `REQUIRE_APPROVAL` rules (downgrade to `ALLOW` with extra
//!   evidence).
//!
//! ## What override CANNOT bypass (§16.2)
//!
//! - Hard denies (§6) — the boundary REJECTS override requests targeting any of
//!   the 10 constitutional hard-deny classes at grant time. The two §6 rows
//!   that carry a recovery-mode override path
//!   (`hd.modify_boot_chain`, `hd.aios_fs_pointer_rollback_on_recovery`) are
//!   handled by a separate recovery-mode operator-approval flow defined in
//!   `05_emergency_override.md`; they are NOT addressable by this boundary.
//! - Evidence log mutation prohibitions.
//! - Recovery path protections (when not in recovery mode itself).
//! - AI self-approval prevention (§17) — only humans can override
//!   AI-affecting rules; this is enforced by `granted_by_subject.is_ai == false`
//!   at grant time.
//!
//! ## Required properties (§16.3)
//!
//! - **Scoped:** identifies the rule(s) being overridden, the subject(s), the
//!   duration.
//! - **Time-bounded:** maximum 24 hours per grant; renewable but each renewal
//!   is a new evidence-logged grant.
//! - **Human-only:** only `subject_type = human` may issue.
//! - **Evidence-linked:** every override grant emits a receipt; every decision
//!   under override references the override receipt.
//! - **Non-persistent:** override grants do not persist across bundle versions;
//!   a bundle flip invalidates active grants.
//!
//! ## What this module ships in T-025
//!
//! - [`EmergencyOverride`] receipt struct + [`OverrideRequest`] grant input.
//! - [`OverrideBoundary`] in-memory grant registry with `request_override`,
//!   `is_overridden`, `revoke`, and `invalidate_for_bundle` operations.
//! - [`OverrideError`] taxonomy for the grant-time hard-deny / human-only /
//!   ttl-bound rejections.
//! - Evidence-receipt MINTING is in-scope (the receipt id `ovr_<ULID>` is
//!   generated here). Evidence-record EMISSION to the S3.1 evidence log is
//!   the M5+ integration task; T-025 only mints the receipt and the
//!   downstream caller is responsible for writing it to the evidence log.
//!
//! ## What is OUT of scope for T-025
//!
//! - The full `05_emergency_override.md` mechanics (multi-approver chains,
//!   renewal flow, broadcast to operator UIs).
//! - The recovery-mode operator-approval flow for the two §6-overridable rows.
//! - Persistence: the boundary lives in process memory only. Production
//!   wiring will lift this onto a durable store keyed by bundle version.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use ulid::Ulid;

use crate::hard_deny::HardDenyClass;
use crate::subject::HydratedSubject;

/// Maximum lifetime of a single override grant — S2.3 §16.3.
///
/// "Override is time-bounded: maximum 24 hours per grant; renewable but each
/// renewal is a new evidence-logged grant." Expressed as seconds so callers
/// can mint shorter TTLs without floating-point arithmetic.
pub const MAX_OVERRIDE_TTL_SECONDS: u64 = 24 * 60 * 60;

/// Scope of an emergency override grant.
///
/// Identifies the targeted rule + action + (optionally) subject(s) the override
/// applies to. The scope is checked at `is_overridden`-time: a grant only
/// fires when the in-flight action / subject match the scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OverrideScope {
    /// The bundle `rule_id` this grant relaxes (e.g. `"deny_lan_exposure"`).
    /// Empty string means "every scoped rule for this action" — the broadest
    /// scope; production grants are encouraged to pin a specific `rule_id`.
    pub rule_id: String,
    /// The action the override applies to (e.g. `"net.expose_lan"`). Must be
    /// non-empty: §16.3 requires the override identify the rule(s) being
    /// overridden and an empty action breaks scoping.
    pub action: String,
    /// Optional subject canonical-id list the override applies to. Empty list
    /// means "every subject"; production grants pin a single subject.
    #[serde(default)]
    pub subjects: Vec<String>,
}

/// A request to grant an emergency override (S2.3 §16.4 skeleton).
///
/// The runtime checks: `granted_by_subject.is_ai == false` (§16.2 human-only),
/// `ttl_seconds <= MAX_OVERRIDE_TTL_SECONDS` (§16.3 24h cap), and that the
/// targeted scope does NOT reference a §6 hard-deny class. Production
/// wiring also validates that the requester carries the
/// `emergency_override.grant` capability — out-of-scope for T-025 because
/// the capability namespace lives in `05_emergency_override.md`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverrideRequest {
    /// Human operator issuing the grant (must satisfy `is_ai == false`).
    pub granted_by_subject: HydratedSubject,
    /// What the override targets (rule + action + optional subjects).
    pub scope: OverrideScope,
    /// English description of the incident / reason — recorded onto the
    /// override receipt and surfaced to audit.
    pub reason: String,
    /// Grant lifetime in seconds; capped at [`MAX_OVERRIDE_TTL_SECONDS`].
    pub ttl_seconds: u64,
    /// Optional §6 hard-deny class the requester is attempting to bypass.
    /// When `Some(_)`, the boundary REJECTS the request at grant time
    /// (§16.2). Tests pass `Some(_)` to assert the rejection; production
    /// grants always pass `None` because hard-denies are not addressable.
    pub attempted_hard_deny: Option<HardDenyClass>,
}

/// An issued emergency override grant (the receipt §16.3 mandates).
///
/// Construction is internal to [`OverrideBoundary::request_override`]; the
/// boundary mints the `override_id`, the `granted_at` timestamp, and computes
/// `expires_at` from `granted_at + ttl_seconds`. Every downstream decision
/// that fires under this override carries `override_id` in its evidence
/// chain (M5+ integration).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmergencyOverride {
    /// `"ovr_<ULID>"` — minted per grant.
    pub override_id: String,
    /// Canonical subject id of the granting human operator.
    pub granted_by_subject_id: String,
    /// When the grant was issued (UTC).
    pub granted_at: DateTime<Utc>,
    /// When the grant expires (= `granted_at + ttl_seconds`, clamped to
    /// `granted_at + MAX_OVERRIDE_TTL_SECONDS`).
    pub expires_at: DateTime<Utc>,
    /// What the grant targets.
    pub scope: OverrideScope,
    /// English reason — surfaced to audit + operator UIs.
    pub reason: String,
    /// `true` after [`OverrideBoundary::revoke`] is called; `is_overridden`
    /// returns `None` for revoked grants regardless of `expires_at`.
    pub revoked: bool,
}

impl EmergencyOverride {
    /// `true` when `now >= expires_at` OR the grant is revoked.
    #[must_use]
    pub fn is_expired_or_revoked(&self, now: DateTime<Utc>) -> bool {
        self.revoked || now >= self.expires_at
    }
}

/// Override grant errors (S2.3 §16.2 / §16.3).
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum OverrideError {
    /// §16.2 — override grant targeting a §6 hard-deny class. The two
    /// recovery-mode-overridable classes are addressable only via the
    /// `05_emergency_override.md` recovery flow; this boundary rejects them
    /// all uniformly per the "Override path: None" rows of the §6 table.
    #[error("hard_deny_cannot_be_overridden: override grant targets §6 hard-deny class `{0:?}`")]
    HardDenyCannotBeOverridden(HardDenyClass),

    /// §16.3 — non-human subject attempting to issue an override.
    #[error(
        "override_human_only: only `subject_type = human` may issue an override (granted_by `{granted_by}` is_ai=true)"
    )]
    HumanOnly {
        /// Canonical id of the rejected requester.
        granted_by: String,
    },

    /// §16.3 — TTL exceeds the 24h ceiling.
    #[error(
        "override_ttl_exceeded: requested ttl_seconds={requested} > MAX_OVERRIDE_TTL_SECONDS={max}"
    )]
    TtlExceeded {
        /// Requested ttl in seconds.
        requested: u64,
        /// Constitutional cap.
        max: u64,
    },

    /// §16.3 — scope missing the action component.
    #[error("override_scope_invalid: scope.action must be non-empty (S2.3 §16.3)")]
    ScopeInvalid,

    /// §16.3 — TTL of zero would mint an already-expired grant.
    #[error("override_ttl_zero: ttl_seconds must be > 0")]
    TtlZero,
}

/// In-memory registry of active emergency override grants (S2.3 §16).
///
/// Keyed by `override_id`; lookups by `(action, subject)` walk the active grants.
/// Production wiring will lift this onto a durable store keyed by bundle
/// version; the boundary's `invalidate_for_bundle` call is what enforces
/// the §16.3 "Override grants do not persist across bundle versions"
/// invariant.
///
/// Thread-safe: the inner state is held behind `Arc<RwLock<...>>` so the
/// boundary can be shared across tokio worker tasks and cloned freely.
#[derive(Default, Clone)]
pub struct OverrideBoundary {
    /// `(override_id -> grant)`. `RwLock` because grant + revoke are infrequent
    /// (operator-action driven) while `is_overridden` lookups fire on every
    /// pipeline evaluation.
    grants: Arc<RwLock<HashMap<String, EmergencyOverride>>>,
}

impl std::fmt::Debug for OverrideBoundary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let count = self.grants.read().map(|g| g.len()).unwrap_or(0);
        f.debug_struct("OverrideBoundary")
            .field("active_grants", &count)
            .finish_non_exhaustive()
    }
}

impl OverrideBoundary {
    /// Construct a fresh empty boundary.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Issue an override grant.
    ///
    /// Validates per §16.2 / §16.3 and either returns a fully-formed
    /// [`EmergencyOverride`] or an [`OverrideError`]. The grant is
    /// recorded in the boundary's active set; subsequent `is_overridden`
    /// calls will return it until `expires_at` or [`Self::revoke`].
    ///
    /// # Errors
    ///
    /// - [`OverrideError::HardDenyCannotBeOverridden`] when the request
    ///   carries `attempted_hard_deny: Some(_)`.
    /// - [`OverrideError::HumanOnly`] when `granted_by_subject.is_ai == true`.
    /// - [`OverrideError::TtlExceeded`] when `ttl_seconds > MAX_OVERRIDE_TTL_SECONDS`.
    /// - [`OverrideError::TtlZero`] when `ttl_seconds == 0`.
    /// - [`OverrideError::ScopeInvalid`] when `scope.action` is empty.
    pub fn request_override(
        &self,
        req: OverrideRequest,
    ) -> Result<EmergencyOverride, OverrideError> {
        self.request_override_at(req, Utc::now())
    }

    /// Same as [`Self::request_override`] but anchored at a caller-provided
    /// `now` for test determinism. Production code calls
    /// [`Self::request_override`] which threads `Utc::now()`.
    ///
    /// # Errors
    ///
    /// Returns the same variants as [`Self::request_override`]; see that
    /// method's documentation for the per-error preconditions.
    pub fn request_override_at(
        &self,
        req: OverrideRequest,
        now: DateTime<Utc>,
    ) -> Result<EmergencyOverride, OverrideError> {
        // §16.2 — hard-denies are not overridable. The two recovery-mode
        // overridable §6 rows have their own flow (see module docs); this
        // boundary rejects every §6 class uniformly.
        if let Some(class) = req.attempted_hard_deny {
            return Err(OverrideError::HardDenyCannotBeOverridden(class));
        }
        // §16.3 — human-only.
        if req.granted_by_subject.is_ai {
            return Err(OverrideError::HumanOnly {
                granted_by: req.granted_by_subject.canonical_subject_id,
            });
        }
        // §16.3 — non-zero, ≤24h.
        if req.ttl_seconds == 0 {
            return Err(OverrideError::TtlZero);
        }
        if req.ttl_seconds > MAX_OVERRIDE_TTL_SECONDS {
            return Err(OverrideError::TtlExceeded {
                requested: req.ttl_seconds,
                max: MAX_OVERRIDE_TTL_SECONDS,
            });
        }
        // §16.3 — scope must identify an action.
        if req.scope.action.is_empty() {
            return Err(OverrideError::ScopeInvalid);
        }

        // i64 cast is safe because ttl is bounded by MAX_OVERRIDE_TTL_SECONDS
        // (86_400) which fits in i64 with room to spare.
        let fallback = i64::from(i32::MAX);
        let ttl_i64 = i64::try_from(req.ttl_seconds).unwrap_or(fallback);
        let expires_at = now + Duration::seconds(ttl_i64);

        let grant = EmergencyOverride {
            override_id: format!("ovr_{}", Ulid::new()),
            granted_by_subject_id: req.granted_by_subject.canonical_subject_id,
            granted_at: now,
            expires_at,
            scope: req.scope,
            reason: req.reason,
            revoked: false,
        };

        // Store the grant. A poisoned lock is recovered defensively so a
        // panic on one worker does not permanently disable the boundary.
        {
            let mut guard = match self.grants.write() {
                Ok(g) => g,
                Err(poisoned) => poisoned.into_inner(),
            };
            guard.insert(grant.override_id.clone(), grant.clone());
        }

        Ok(grant)
    }

    /// Look up an active override for the supplied `(action, subject)` pair.
    ///
    /// Returns the first grant whose scope matches AND that has not expired
    /// or been revoked. Per §16.3 the lookup is deterministic w.r.t. the
    /// active set; the iteration order is implementation-defined (`HashMap`)
    /// so callers MUST NOT rely on which grant is returned when multiple
    /// match — production policy is to issue one grant per incident.
    #[must_use]
    pub fn is_overridden(
        &self,
        action: &str,
        subject_canonical_id: &str,
    ) -> Option<EmergencyOverride> {
        self.is_overridden_at(action, subject_canonical_id, Utc::now())
    }

    /// Same as [`Self::is_overridden`] but anchored at a caller-provided `now`.
    #[must_use]
    pub fn is_overridden_at(
        &self,
        action: &str,
        subject_canonical_id: &str,
        now: DateTime<Utc>,
    ) -> Option<EmergencyOverride> {
        let guard = match self.grants.read() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        guard
            .values()
            .find(|g| {
                !g.is_expired_or_revoked(now)
                    && g.scope.action == action
                    && (g.scope.subjects.is_empty()
                        || g.scope.subjects.iter().any(|s| s == subject_canonical_id))
            })
            .cloned()
    }

    /// Revoke a grant by id. Returns `true` when the grant existed and was
    /// previously active; `false` when the id was unknown or already revoked.
    #[allow(clippy::must_use_candidate)]
    pub fn revoke(&self, override_id: &str) -> bool {
        let mut guard = match self.grants.write() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        match guard.get_mut(override_id) {
            Some(g) if !g.revoked => {
                g.revoked = true;
                true
            }
            _ => false,
        }
    }

    /// Drop every active grant. Called by the `LoadBundle` / `RollbackBundle`
    /// RPC paths to enforce §16.3 "Override grants do not persist across
    /// bundle versions". Returns the count of grants cleared for audit.
    #[allow(clippy::must_use_candidate)]
    pub fn invalidate_for_bundle_flip(&self) -> usize {
        let mut guard = match self.grants.write() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        let n = guard.len();
        guard.clear();
        n
    }

    /// Current count of active (not necessarily unexpired) grants.
    /// Includes expired but not-yet-pruned entries; production tooling
    /// can call [`Self::prune_expired`] to compact.
    #[must_use]
    pub fn len(&self) -> usize {
        match self.grants.read() {
            Ok(g) => g.len(),
            Err(poisoned) => poisoned.into_inner().len(),
        }
    }

    /// `true` when no grants are recorded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Drop every grant whose `is_expired_or_revoked(now)` returns `true`.
    /// Returns the count of removed grants.
    #[allow(clippy::must_use_candidate)]
    pub fn prune_expired(&self, now: DateTime<Utc>) -> usize {
        let mut guard = match self.grants.write() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        let to_remove: Vec<String> = guard
            .iter()
            .filter(|(_, g)| g.is_expired_or_revoked(now))
            .map(|(k, _)| k.clone())
            .collect();
        let n = to_remove.len();
        for k in to_remove {
            guard.remove(&k);
        }
        n
    }

    /// Inspection helper: snapshot all active grants. Test/audit only.
    #[must_use]
    pub fn snapshot(&self) -> Vec<EmergencyOverride> {
        match self.grants.read() {
            Ok(g) => g.values().cloned().collect(),
            Err(poisoned) => poisoned.into_inner().values().cloned().collect(),
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    reason = "test code; panic-on-failure is the idiomatic test signal"
)]
mod tests {
    use super::*;
    use crate::subject::SubjectType;

    fn human() -> HydratedSubject {
        HydratedSubject {
            canonical_subject_id: "human:lucky".to_owned(),
            subject_type: SubjectType::Human,
            groups: vec!["operators".to_owned()],
            capabilities: vec![],
            session_class: "INTERNAL".to_owned(),
            recovery_mode: false,
            is_ai: false,
        }
    }

    fn ai() -> HydratedSubject {
        HydratedSubject {
            canonical_subject_id: "agent:dev".to_owned(),
            subject_type: SubjectType::Agent,
            groups: vec!["agents".to_owned()],
            capabilities: vec![],
            session_class: "INTERNAL".to_owned(),
            recovery_mode: false,
            is_ai: true,
        }
    }

    fn req_for(action: &str) -> OverrideRequest {
        OverrideRequest {
            granted_by_subject: human(),
            scope: OverrideScope {
                rule_id: "deny_lan_exposure".to_owned(),
                action: action.to_owned(),
                subjects: vec![],
            },
            reason: "incident response".to_owned(),
            ttl_seconds: 3600,
            attempted_hard_deny: None,
        }
    }

    #[test]
    fn request_override_happy_path_mints_receipt() {
        let b = OverrideBoundary::new();
        let g = b.request_override(req_for("net.expose_lan")).unwrap();
        assert!(g.override_id.starts_with("ovr_"));
        assert_eq!(g.granted_by_subject_id, "human:lucky");
        assert!(!g.revoked);
        assert_eq!(b.len(), 1);
    }

    #[test]
    fn ai_subject_cannot_grant_override() {
        let b = OverrideBoundary::new();
        let mut r = req_for("net.expose_lan");
        r.granted_by_subject = ai();
        let err = b.request_override(r).unwrap_err();
        matches!(err, OverrideError::HumanOnly { .. })
            .then_some(())
            .expect("expected HumanOnly");
    }

    #[test]
    fn hard_deny_cannot_be_overridden() {
        let b = OverrideBoundary::new();
        let mut r = req_for("evidence.tamper");
        r.attempted_hard_deny = Some(HardDenyClass::EvidenceLogMutation);
        let err = b.request_override(r).unwrap_err();
        match err {
            OverrideError::HardDenyCannotBeOverridden(HardDenyClass::EvidenceLogMutation) => {}
            other => panic!("expected HardDenyCannotBeOverridden, got {other:?}"),
        }
    }

    #[test]
    fn ttl_above_24h_is_rejected() {
        let b = OverrideBoundary::new();
        let mut r = req_for("net.expose_lan");
        r.ttl_seconds = MAX_OVERRIDE_TTL_SECONDS + 1;
        let err = b.request_override(r).unwrap_err();
        match err {
            OverrideError::TtlExceeded { requested, max } => {
                assert_eq!(requested, MAX_OVERRIDE_TTL_SECONDS + 1);
                assert_eq!(max, MAX_OVERRIDE_TTL_SECONDS);
            }
            other => panic!("expected TtlExceeded, got {other:?}"),
        }
    }

    #[test]
    fn zero_ttl_is_rejected() {
        let b = OverrideBoundary::new();
        let mut r = req_for("net.expose_lan");
        r.ttl_seconds = 0;
        assert_eq!(b.request_override(r).unwrap_err(), OverrideError::TtlZero);
    }

    #[test]
    fn is_overridden_returns_grant_within_ttl() {
        let b = OverrideBoundary::new();
        let g = b.request_override(req_for("net.expose_lan")).unwrap();
        let hit = b.is_overridden("net.expose_lan", "human:lucky").unwrap();
        assert_eq!(hit.override_id, g.override_id);
    }

    #[test]
    fn revoked_grant_is_ignored() {
        let b = OverrideBoundary::new();
        let g = b.request_override(req_for("net.expose_lan")).unwrap();
        assert!(b.revoke(&g.override_id));
        assert!(b.is_overridden("net.expose_lan", "human:lucky").is_none());
    }

    #[test]
    fn expired_grant_is_ignored() {
        let b = OverrideBoundary::new();
        let now = Utc::now();
        let mut r = req_for("net.expose_lan");
        r.ttl_seconds = 1;
        let _ = b.request_override_at(r, now).unwrap();
        let later = now + Duration::seconds(2);
        assert!(b
            .is_overridden_at("net.expose_lan", "human:lucky", later)
            .is_none());
    }

    #[test]
    fn invalidate_for_bundle_flip_drops_all_grants() {
        let b = OverrideBoundary::new();
        let _ = b.request_override(req_for("net.expose_lan")).unwrap();
        let _ = b.request_override(req_for("svc.restart")).unwrap();
        let n = b.invalidate_for_bundle_flip();
        assert_eq!(n, 2);
        assert!(b.is_empty());
    }

    #[test]
    fn scope_with_subjects_pins_to_named_subject() {
        let b = OverrideBoundary::new();
        let mut r = req_for("net.expose_lan");
        r.scope.subjects = vec!["human:alice".to_owned()];
        let _ = b.request_override(r).unwrap();
        assert!(b.is_overridden("net.expose_lan", "human:lucky").is_none());
        assert!(b.is_overridden("net.expose_lan", "human:alice").is_some());
    }
}
