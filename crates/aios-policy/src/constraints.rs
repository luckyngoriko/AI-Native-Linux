//! Constraints + `ApprovalRequirement` vocabulary — S2.3 §10 / §11.2 / §15.
//!
//! [`Constraints`] is the **closed** set of execution bindings the Policy Kernel may
//! attach to an `ALLOW` or `REQUIRE_APPROVAL` [`crate::decision::PolicyDecision`]. The
//! 11-field shape matches the spec §10 proto definition verbatim — adding, removing or
//! renaming a field is a wire-format break.
//!
//! [`ApprovalRequirement`] is the closed shape attached when
//! [`crate::decision::Decision::RequireApproval`] is produced. Field set mirrors the
//! S2.3 §11.2 proto definition; the `approval_scope` discriminant is constrained to
//! [`ApprovalScope::ExactRequestHash`] because §15 states this is the only value
//! supported in rev.2.
//!
//! ## Closed enums
//!
//! Five constraint values are spec-enumerated (not free strings):
//!
//! - [`EvidenceGrade`] — `E2..E5` per S2.3 §10 (the policy floor; `E0`/`E1` exist in
//!   S3.1 but cannot be required by policy).
//! - [`NetworkPolicy`] — `LOCALHOST_ONLY` / `LAN_ALLOWED` / `INTERNET_ALLOWED`.
//! - [`SessionClass`] — `PUBLIC` / `INTERNAL` / `CONFIDENTIAL` / `RESTRICTED` /
//!   `RECOVERY` (S1.3 privacy ceiling lattice; see S1.3 §4.1).
//! - [`ApprovalScope`] — `EXACT_REQUEST_HASH` (only value in rev.2 per §15).
//! - [`ApproverClass`] — `HUMAN` / `OPERATOR` / `AGENT` / `APPLICATION` /
//!   `SERVICE` / `DEVICE` / `WORKFLOW` / `REMOTE_OPERATOR` per S2.3 §7 subject types.
//!
//! ## Validation
//!
//! [`Constraints::validate`] enforces field-level invariants that hold for every
//! decision regardless of bundle: TTL bounds (§13.2), non-negative budgets (proto
//! `uint32` already enforces non-negativity in-band but the bundle YAML parser may
//! supply zeros that violate the spec's "hard cap" semantics), and the
//! evidence-grade ordering when present. The kernel calls this between §3 step 11
//! (bind constraints) and step 12 (emit decision) so a malformed bundle constraint
//! can never reach evidence.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::PolicyError;

// ---------------------------------------------------------------------------
// Newtype wrappers
// ---------------------------------------------------------------------------

/// Spec-typed identifier for an L6 sandbox profile (S2.3 §10 row 1, S3.2 sandbox
/// composition). The string form is the catalog id — `"host-service-control"`,
/// `"net-restricted"`, etc.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SandboxProfileId(pub String);

/// Spec-typed identifier for a Vault capability the subject must hold (S2.3 §10
/// row 10; resolved by L4 `02_vault_broker.md`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct VaultCapabilityId(pub String);

// ---------------------------------------------------------------------------
// Closed enums
// ---------------------------------------------------------------------------

/// Minimum evidence grade required before the bound action may reach a terminal phase.
///
/// (S2.3 §10 row 5.) The four values mirror the S3.1 grade ladder above the
/// "no evidence" floor; `E0`/`E1` are intentionally absent because the spec
/// constrains the constraint to `"E2" .. "E5"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum EvidenceGrade {
    /// Build / typecheck artifact present (S3.1 grade ladder).
    E2,
    /// Unit / integration test pass.
    E3,
    /// End-to-end / recovery / release gate pass.
    E4,
    /// Live operational evidence.
    E5,
}

/// Network exposure ceiling bound to the decision (S2.3 §10 row 7).
///
/// The ordering `LocalhostOnly < LanAllowed < InternetAllowed` is the spec's
/// max-restriction lattice — the action's effective network policy is the
/// **minimum** of the constraint and the action's environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NetworkPolicy {
    /// Action may only contact loopback (the constitutional default per §1).
    LocalhostOnly,
    /// Action may contact the LAN but not the public Internet.
    LanAllowed,
    /// Action may contact the public Internet (requires explicit policy approval).
    InternetAllowed,
}

/// Subject session class (S2.3 §10 row 9, S1.3 §4.1 privacy lattice).
///
/// The ordering `Public < Internal < Confidential < Restricted < Recovery` is
/// the privacy ceiling lattice — `min_subject_session_class` asserts the
/// subject's session MUST be at this class **or below** (i.e.
/// equal-or-less-restrictive).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum SessionClass {
    /// Publicly readable objects only.
    Public,
    /// Operator-internal data.
    Internal,
    /// Confidential — limited sharing.
    Confidential,
    /// Restricted — narrow access list.
    Restricted,
    /// Recovery-mode session — highest privacy ceiling.
    Recovery,
}

/// Approval-binding scope (S2.3 §11.2 / §15).
///
/// The single rev.2 value `ExactRequestHash` means an approval is valid only
/// for the exact `request_hash` the decision was bound to; any mutation
/// invalidates the approval.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalScope {
    /// Approval binds to the exact `request_hash` of the decision (only value in rev.2).
    #[default]
    ExactRequestHash,
}

/// Subject classes accepted as approvers (S2.3 §15 / §11.2 `approver_classes`).
///
/// Mirrors the subject-type taxonomy from S2.3 §7 with the addition of
/// `Operator` (a recovery-mode-authorised human) per §15 / §16.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApproverClass {
    /// Regular human subject (default per §15).
    Human,
    /// Recovery-mode-authorised human operator (§16).
    Operator,
    /// AI agent subject — only permitted for non-risk self-management actions (§17.3).
    Agent,
    /// Application subject.
    Application,
    /// Service subject.
    Service,
    /// Device subject.
    Device,
    /// Workflow subject.
    Workflow,
    /// Remote-operator subject.
    RemoteOperator,
}

// ---------------------------------------------------------------------------
// Constraints — S2.3 §10
// ---------------------------------------------------------------------------

/// The full S2.3 §10 closed constraints vocabulary attached to `ALLOW` and
/// `REQUIRE_APPROVAL` decisions.
///
/// Every field is `Option<T>` except the three boolean flags and `ttl_seconds`,
/// which always carry a value (the proto3 wire form has no `Option` so the
/// default `false`/`0` is the unset signal; the spec assigns concrete meaning to
/// each default).
///
/// Default semantics (matching the bundle-author "unset" case):
///
/// - all `Option<…>` fields = `None` (no restriction beyond bundle-level)
/// - `verification_required = false`
/// - `dry_run_only = false`
/// - `require_human_co_signer = false`
/// - `ttl_seconds = Self::DEFAULT_TTL_SECONDS` (300 s per §13.2)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Constraints {
    /// Required sandbox profile; max-restriction with caller's request profile
    /// (S2.3 §10 row 1, S0.1 §9.2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sandbox_profile_id: Option<SandboxProfileId>,

    /// Hard wall-clock cap on adapter execution, in seconds (§10 row 2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_runtime_seconds: Option<u32>,

    /// Require non-empty verification intents regardless of caller (§10 row 3,
    /// S0.1 §3).
    #[serde(default)]
    pub verification_required: bool,

    /// Decision only valid for `dry_run ∈ {VALIDATE, SIMULATE}` (§10 row 4).
    #[serde(default)]
    pub dry_run_only: bool,

    /// Minimum evidence grade required before terminal phase (§10 row 5).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub require_evidence_grade: Option<EvidenceGrade>,

    /// Approval requires a second human subject (§10 row 6 / §15).
    #[serde(default)]
    pub require_human_co_signer: bool,

    /// Network exposure ceiling (§10 row 7). Max-restriction with the action's
    /// environment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network_policy: Option<NetworkPolicy>,

    /// Concurrency cap per subject (§10 row 8).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_concurrent_per_subject: Option<u32>,

    /// Subject's session must be at this class or below (§10 row 9).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_subject_session_class: Option<SessionClass>,

    /// Subject must hold this Vault capability (§10 row 10, resolved by L4
    /// `02_vault_broker.md`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vault_capability_required: Option<VaultCapabilityId>,

    /// Decision validity TTL (§10 row 11). Default 300 s, max 3600 s per §13.2.
    pub ttl_seconds: u32,

    /// Optional absolute expiry timestamp — when present, the decision is
    /// invalid after this instant regardless of `ttl_seconds`. Not in the §10
    /// proto wire shape but used by determinism audits (§13) and by the future
    /// gRPC `evaluated_at + ttl_seconds` reconstruction; serialised as a
    /// non-wire-form-affecting RFC 3339 string when present, omitted otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
}

impl Constraints {
    /// Default decision TTL per S2.3 §13.2 ("default 300 s").
    pub const DEFAULT_TTL_SECONDS: u32 = 300;

    /// Maximum decision TTL per S2.3 §13.2 ("max 3600 s, capped per rule").
    pub const MAX_TTL_SECONDS: u32 = 3600;

    /// Validates field-level invariants enforced at decision-emission time per
    /// S2.3 §10 + §13.2:
    ///
    /// - `ttl_seconds` is in `1..=MAX_TTL_SECONDS` (zero would mean immediately
    ///   expired; spec floor is implicit but enforced here for sanity).
    /// - `max_runtime_seconds`, when present, is non-zero (a zero cap means the
    ///   action cannot run at all — that case must be expressed as a `DENY`,
    ///   not as an `ALLOW` with `max_runtime_seconds = 0`).
    /// - `max_concurrent_per_subject`, when present, is non-zero (same reason).
    /// - `expires_at`, when present, is strictly in the future relative to the
    ///   call instant — a past timestamp would emit a stillborn decision.
    ///
    /// Returns [`PolicyError::ConstraintsInvalid`] with a human-readable English
    /// detail on the first violation.
    ///
    /// # Errors
    ///
    /// Returns [`PolicyError::ConstraintsInvalid`] when any of the following
    /// hold:
    ///
    /// - `ttl_seconds == 0` or `ttl_seconds > MAX_TTL_SECONDS` (S2.3 §13.2),
    /// - `max_runtime_seconds == Some(0)` (S2.3 §10 row 2 — zero cap must be
    ///   expressed as `DENY`, not as a constrained `ALLOW`),
    /// - `max_concurrent_per_subject == Some(0)` (S2.3 §10 row 8, same reason),
    /// - `expires_at` is not strictly in the future relative to the call
    ///   instant.
    pub fn validate(&self) -> Result<(), PolicyError> {
        if self.ttl_seconds == 0 {
            return Err(PolicyError::ConstraintsInvalid(
                "ttl_seconds must be non-zero (S2.3 §13.2)".to_string(),
            ));
        }
        if self.ttl_seconds > Self::MAX_TTL_SECONDS {
            return Err(PolicyError::ConstraintsInvalid(format!(
                "ttl_seconds {} exceeds max {} (S2.3 §13.2)",
                self.ttl_seconds,
                Self::MAX_TTL_SECONDS,
            )));
        }
        if self.max_runtime_seconds == Some(0) {
            return Err(PolicyError::ConstraintsInvalid(
                "max_runtime_seconds = 0 must be expressed as DENY, not constrained ALLOW (S2.3 §10 row 2)"
                    .to_string(),
            ));
        }
        if self.max_concurrent_per_subject == Some(0) {
            return Err(PolicyError::ConstraintsInvalid(
                "max_concurrent_per_subject = 0 must be expressed as DENY, not constrained ALLOW (S2.3 §10 row 8)"
                    .to_string(),
            ));
        }
        if let Some(expires_at) = self.expires_at {
            if expires_at <= Utc::now() {
                return Err(PolicyError::ConstraintsInvalid(format!(
                    "expires_at {expires_at} is not in the future"
                )));
            }
        }
        Ok(())
    }
}

impl Default for Constraints {
    fn default() -> Self {
        Self {
            sandbox_profile_id: None,
            max_runtime_seconds: None,
            verification_required: false,
            dry_run_only: false,
            require_evidence_grade: None,
            require_human_co_signer: false,
            network_policy: None,
            max_concurrent_per_subject: None,
            min_subject_session_class: None,
            vault_capability_required: None,
            ttl_seconds: Self::DEFAULT_TTL_SECONDS,
            expires_at: None,
        }
    }
}

// ---------------------------------------------------------------------------
// ApprovalRequirement — S2.3 §11.2 / §15
// ---------------------------------------------------------------------------

/// Approval requirement attached when [`crate::decision::Decision::RequireApproval`]
/// is produced (S2.3 §11.2 / §15).
///
/// Shape matches the §11.2 proto: 5 fields anchored to the wire form. The §15
/// recovery-mode amendment (recovery-mode mutations always require a human
/// approver) is enforced at the approval-pipeline layer by setting
/// `approver_classes = [Human]` — it is not a separate field on this struct.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalRequirement {
    /// Whether approval is required at all (§11.2 field 1). When `false` the
    /// remaining fields are meaningless and serialise with their zero values.
    #[serde(default)]
    pub required: bool,

    /// Binding scope — only [`ApprovalScope::ExactRequestHash`] in rev.2
    /// (§11.2 field 2 / §15).
    #[serde(default)]
    pub approval_scope: ApprovalScope,

    /// Approval validity window in seconds (§11.2 field 3). Bounded by the
    /// decision's [`Constraints::ttl_seconds`].
    #[serde(default)]
    pub ttl_seconds: u32,

    /// Closed set of subject classes accepted as approvers (§11.2 field 4).
    /// Defaults to `[Human]` per §15 ("default `[\"human\"]`").
    #[serde(default)]
    pub approver_classes: Vec<ApproverClass>,

    /// When true, approval requires a second human subject in addition to the
    /// primary approver (§11.2 field 5; same semantics as the Constraints flag
    /// of the same name — kept on both sides because the rule may set it
    /// without a corresponding constraint and vice versa).
    #[serde(default)]
    pub require_human_co_signer: bool,
}

impl Default for ApprovalRequirement {
    fn default() -> Self {
        Self {
            required: false,
            approval_scope: ApprovalScope::ExactRequestHash,
            ttl_seconds: 0,
            approver_classes: Vec::new(),
            require_human_co_signer: false,
        }
    }
}
