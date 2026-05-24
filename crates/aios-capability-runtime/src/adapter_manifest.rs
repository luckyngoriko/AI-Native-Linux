//! `AdapterManifest` — closed adapter manifest schema per S10.1 §10.1.
//!
//! Each adapter is registered through a typed `AdapterManifest`. Registration
//! is itself a typed action (`runtime.adapter.register`) that flows through
//! the runtime. Self-registration is not allowed; the registration must be
//! authored by an operator-class subject and signed.
//!
//! T-026 lands the **shape only**. Signature verification, trust-store
//! integration, action-kind exclusivity enforcement, and registration
//! orchestration are queued for T-028 (adapter registry).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::dispatch::{ActionDispatchKind, AdapterIOMode, AdapterStability};

/// Per-action declaration nested inside an [`AdapterManifest`].
///
/// One `AdapterActionDeclaration` per `action_kind` the adapter supports.
/// `target_schema` and `response_schema` are kept as opaque
/// `serde_json::Value` here; T-028 will validate them against the L5
/// capability catalog at registration time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdapterActionDeclaration {
    /// `action_kind` — dotted name from the L5 capability catalog (S1.1 §6.4),
    /// e.g. `pkg.install`, `service.restart`, `fs.write`.
    pub action_kind: String,
    /// `target_schema` — JSON-schema-shaped struct validated against the
    /// envelope's `request.target` payload.
    pub target_schema: serde_json::Value,
    /// `response_schema` — JSON-schema-shaped struct validated against the
    /// adapter's `ExecuteAction` response.
    pub response_schema: serde_json::Value,
    /// `rollback_strategy` — opaque string here (full `RollbackStrategy` enum
    /// is owned by `04_adapter_model.md`, out of scope for T-026). Common
    /// values: `NONE`, `IDEMPOTENT_REAPPLY`, `INVERSE_ACTION`,
    /// `SNAPSHOT_RESTORE`.
    pub rollback_strategy: String,
    /// `timeout_seconds` — overrides manifest default for this `action_kind`.
    pub timeout_seconds: u32,
    /// `template_string` — populated only when the manifest's
    /// [`AdapterIOMode`] is [`AdapterIOMode::TemplateParameters`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template_string: Option<String>,
    /// `template_substitution_variables` — closed list of allowed variable
    /// names referenced by `template_string`. Empty for
    /// [`AdapterIOMode::TypedParametersOnly`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub template_substitution_variables: Vec<String>,
}

/// `AdapterManifest` — S10.1 §10.1 closed schema.
///
/// The manifest is signed by an Ed25519 key recognised by the AIOS root or a
/// recognised publisher; the trust chain mirrors S2.3 §12.3 policy bundle
/// trust. Signature verification, trust-store lookup, and
/// `manifest_expires_at` enforcement land in T-028.
///
/// `adapter_signature` and `signing_key_id` are kept as opaque strings here
/// (`hex_lower` for the signature; recognised key id for the signer). T-028
/// will introduce the typed signature wrapper and the trust-store interface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdapterManifest {
    /// `adapter_id` — `"adapter:<vendor>:<name>:<version>"`. Canonical key
    /// for adapter lookup.
    pub adapter_id: String,
    /// `adapter_version` — `SemVer`; advisory.
    pub adapter_version: String,
    /// `vendor` — free-form; echoed in `adapter_id`.
    pub vendor: String,
    /// `name` — free-form; echoed in `adapter_id`.
    pub name: String,
    /// `declared_stability` — the maximum stability the adapter may claim.
    /// The runtime treats this as an upper bound; actual operating stability
    /// is set by an operator through `runtime.adapter.set_stability` (§10.3).
    pub declared_stability: AdapterStability,
    /// `io_mode` — typed-parameters-only or template-parameters.
    pub io_mode: AdapterIOMode,
    /// `dispatch_kind` — adapter's preferred dispatch kind. The runtime may
    /// override per the §3.2 decision table (e.g. AI-origin actions always
    /// upgrade to `ISOLATED_SANDBOX`).
    pub dispatch_kind: ActionDispatchKind,
    /// `declared_actions` — one declaration per `action_kind` the adapter
    /// supports. Action-kind exclusivity (§10.5) is enforced at registration.
    pub declared_actions: Vec<AdapterActionDeclaration>,
    /// `declared_invariants_supported` — closed list of L0 INV-XXX ids the
    /// adapter respects (e.g. `INV-013`, `INV-021`).
    pub declared_invariants_supported: Vec<String>,
    /// `default_adapter_timeout_seconds` — bounded by §15 performance
    /// budgets. Overridable per-action via
    /// [`AdapterActionDeclaration::timeout_seconds`].
    pub default_adapter_timeout_seconds: u32,
    /// `default_sandbox_profile_id` — S3.2 profile id; runtime may compose to
    /// stricter.
    pub default_sandbox_profile_id: String,
    /// `adapter_signature` — Ed25519 over JCS of all other fields; `hex_lower`.
    pub adapter_signature: String,
    /// `signing_key_id` — identity service or recognised publisher key id.
    pub signing_key_id: String,
    /// `manifest_created_at` — issuance timestamp.
    pub manifest_created_at: DateTime<Utc>,
    /// `manifest_expires_at` — mandatory. An adapter whose manifest expires
    /// is automatically de-registered (§10.4).
    pub manifest_expires_at: DateTime<Utc>,
}
