//! Package-kind and install-scope vocabularies per S11.1 §3.4 / §3.5.
//!
//! `PackageKind` classifies what a package IS (app, agent, kernel candidate,
//! etc.) and determines which manifest fields are mandatory, what sandbox
//! profile is required, and whether the install is recovery-only.
//!
//! `InstallScope` maps to S4.1 namespace layout and determines which subject
//! must approve the install.

use serde::{Deserialize, Serialize};

/// Closed enum — 9 kinds per S11.1 §3.4.
///
/// | Variant                  | S11.1 label                | Recovery-only? | Sandbox? |
/// |--------------------------|----------------------------|----------------|----------|
/// | `App`                    | `APP`                      | No             | Yes      |
/// | `Agent`                  | `AGENT`                    | No             | Yes      |
/// | `Theme`                  | `THEME`                    | No             | No       |
/// | `InvariantBundle`        | `INVARIANT_BUNDLE`         | Yes            | N/A      |
/// | `PolicyBundle`           | `POLICY_BUNDLE`            | Conditional    | N/A      |
/// | `IdentityBundle`         | `IDENTITY_BUNDLE`          | Yes            | N/A      |
/// | `KernelCandidate`        | `KERNEL_CANDIDATE`         | Yes            | N/A      |
/// | `Adapter`                | `ADAPTER`                  | No             | Yes      |
/// | `CapabilityCatalogDelta` | `CAPABILITY_CATALOG_DELTA` | Yes            | N/A      |
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PackageKind {
    /// User-facing application — sandbox profile + capability declaration required.
    App,
    /// AI agent persona — auto-binds to AI subject scope, AI sandbox floor enforced.
    Agent,
    /// Visual theme bundle — declarative only (no executables permitted).
    Theme,
    /// L0 invariant signed bundle — `AiosOfficialRepo` only, recovery-only.
    InvariantBundle,
    /// S2.3 policy bundle — requires policy-authorship grant.
    PolicyBundle,
    /// L4.3 identity bundle — `AiosOfficialRepo` only, recovery-only.
    IdentityBundle,
    /// Dedicated kernel image — recovery-only install with A/B promotion.
    KernelCandidate,
    /// L3 adapter binary — signed manifest + capability declaration mandatory.
    Adapter,
    /// L5/S1.1 capability catalog updates — recovery-only.
    CapabilityCatalogDelta,
}

/// Closed enum — 4 scopes per S11.1 §3.5.
///
/// Deviation: spec §3.5 uses `SYSTEM_ONLY`, `GROUP_SCOPED`, `USER_SCOPED`,
/// `EITHER`.  T-187 uses task-authorised names that align with the operating
/// semantics rather than raw scope labels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallScope {
    /// Writes to `/aios/system/...` — recovery operator approval required.
    SystemWide,
    /// Writes to `/aios/groups/<group_id>/system/...` — group operator approval.
    PerGroup,
    /// Writes to `/aios/groups/<group_id>/users/<user_id>/...` — user approval.
    PerSubject,
    /// Recovery-only install — binds INV-012, S9.1 `RecoveryMutableScope`.
    RecoveryOnly,
}
