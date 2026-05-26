use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::{ProfileId, SandboxError, SandboxProfile};

/// Typed subject reference — opaque string identifier for the requesting subject.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SubjectRef(pub String);

impl SubjectRef {
    /// Create a new `SubjectRef` from a string-like value.
    #[must_use]
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
}

impl std::fmt::Display for SubjectRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Request to compose a sandbox profile from up to 6 sources (S3.2 §5.1 + §18.1).
///
/// Sources are applied in spec order: `adapter_default` → `app_manifest` →
/// `user_request` → `policy_required` → `group_floor` → `runtime_safety_floor`.
/// Each source tightens the accumulated profile per the most-restrictive-wins
/// rule. When `base_profile_id` is provided, the stored profile is loaded from
/// the catalog and used as the starting point instead of `adapter_default`.
#[derive(Debug, Clone)]
pub struct ComposeRequest {
    /// Who is requesting this composition (for audit).
    pub subject: SubjectRef,
    /// The action kind for which the profile is being composed.
    pub action_kind: String,
    /// Optional catalog lookup — if set, loaded as the starting profile
    /// before any source merge.
    pub base_profile_id: Option<ProfileId>,
    /// Source 1 — adapter's declared default sandbox profile.
    pub adapter_default: Option<SandboxProfile>,
    /// Source 2 — app manifest's requested sandbox constraints.
    pub app_manifest: Option<SandboxProfile>,
    /// Source 3 — user/operator explicit overrides.
    pub user_request: Option<SandboxProfile>,
    /// Source 4 — policy-mandated required constraints.
    pub policy_required: Option<SandboxProfile>,
    /// Source 5 — group minimum isolation floor.
    pub group_floor: Option<SandboxProfile>,
    /// Source 6 — system runtime safety floor (wins unconditionally).
    pub runtime_safety_floor: Option<SandboxProfile>,
    /// Whether the system is in recovery mode.
    pub recovery_mode: bool,
    /// Whether the requesting subject is an AI agent.
    pub is_ai: bool,
}

/// Result of a successful profile composition.
#[derive(Debug, Clone)]
pub struct ComposeResult {
    /// The merged sandbox profile with a fresh `ProfileId`.
    pub profile: SandboxProfile,
    /// Ordered list of source names that contributed to the merge.
    pub merged_sources: Vec<String>,
    /// Whether recovery-mode post-processing rules were applied.
    pub recovery_mode_enforced: bool,
    /// Whether AI-mode post-processing rules were applied.
    pub ai_mode_enforced: bool,
}

/// Sandbox profile composer (S3.2 §19.1).
///
/// Merges up to 6 sources with most-restrictive-wins semantics and applies
/// recovery-mode and AI-mode post-processing rules. Maintains a catalog of
/// stored profiles for lookup by `ProfileId`.
#[async_trait]
pub trait SandboxComposer: Send + Sync {
    /// Compose a sandbox profile from the given request.
    ///
    /// Applies the 6-source merge algorithm, recovery-mode rules, and AI-mode
    /// rules. Returns a fresh `SandboxProfile` with a new `ProfileId`.
    ///
    /// # Errors
    ///
    /// Returns `ProfileNotFound` if `base_profile_id` is set but not found in
    /// the catalog.
    async fn compose(&self, request: ComposeRequest) -> Result<ComposeResult, SandboxError>;

    /// Store a profile in the catalog keyed by its `ProfileId`.
    ///
    /// # Errors
    ///
    /// Returns `InvalidProfile` if the profile fails validation.
    async fn store_profile(&self, profile: SandboxProfile) -> Result<ProfileId, SandboxError>;

    /// Retrieve a profile by its `ProfileId`.
    ///
    /// # Errors
    ///
    /// Returns `ProfileNotFound` if the profile is not in the catalog.
    async fn get_profile(&self, profile_id: &ProfileId) -> Result<SandboxProfile, SandboxError>;

    /// List all stored profiles in the catalog.
    async fn list_profiles(&self) -> Result<Vec<SandboxProfile>, SandboxError>;
}
