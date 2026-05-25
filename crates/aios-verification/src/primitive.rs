//! Closed S2.4 verification primitive vocabulary.

use std::fmt;

use serde::{Deserialize, Serialize};
use strum_macros::{EnumCount, EnumIter};

/// Closed S2.4 primitive vocabulary after Wave 15 body close-out.
///
/// The count is 36: the original 12 primitive messages plus Wave 4, Wave 5,
/// Wave 6, Wave 8, Wave 10, and Wave 14 additions. `property_check` and
/// `composition` are represented separately in the spec and are not counted in
/// the primitive telemetry cardinality.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum VerificationPrimitive {
    /// `service_active` — service manager reports an active service.
    ServiceActive,
    /// `service_inactive` — service manager reports an inactive service.
    ServiceInactive,
    /// `package_installed` — package database contains a package/version.
    PackageInstalled,
    /// `port_open` — local or allowed network probe observes an open port.
    PortOpen,
    /// `port_closed` — local or allowed network probe observes a closed port.
    PortClosed,
    /// `http_ok` — idempotent HTTP probe observes the expected status/body.
    HttpOk,
    /// `file_exists` — read-only file or AIOS-FS object lookup succeeds.
    FileExists,
    /// `file_hash` — observed BLAKE3 hash matches the expected hash.
    FileHash,
    /// `repo_exists` — read-only repository metadata lookup succeeds.
    RepoExists,
    /// `aiosfs_pointer` — AIOS-FS pointer references the expected version.
    AiosfsPointer,
    /// `policy_decision` — policy decision log contains the expected outcome.
    PolicyDecision,
    /// `evidence_exists` — evidence log contains the referenced receipt.
    EvidenceExists,
    /// `network_subject_outbound_class` — subject outbound posture matches.
    NetworkSubjectOutboundClass,
    /// `network_active_exposure_class` — surface inbound exposure matches.
    NetworkActiveExposureClass,
    /// `network_external_model_call_brokered_only` — model calls use the broker.
    NetworkExternalModelCallBrokeredOnly,
    /// `dns_resolver_backend` — DNS resolver backend and transport match.
    DnsResolverBackend,
    /// `vpn_tunnel_active` — VPN tunnel is active with the expected kind.
    VpnTunnelActive,
    /// `mdns_posture` — mDNS posture matches the host policy.
    MdnsPosture,
    /// `aiosfs_path_in_namespace` — path resolves inside the expected namespace.
    AiosfsPathInNamespace,
    /// `surface_in_zone` — UI surface is rendered in the expected zone.
    SurfaceInZone,
    /// `tree_contains_kind` — UI tree contains or omits a node kind.
    TreeContainsKind,
    /// `tree_max_depth` — UI tree depth is within the expected bound.
    TreeMaxDepth,
    /// `theme_satisfies_invariants` — theme preserves visual invariants.
    ThemeSatisfiesInvariants,
    /// `theme_constitutional_icons_intact` — constitutional icons are intact.
    ThemeConstitutionalIconsIntact,
    /// `gpu_binding_class` — GPU binding class matches the expected class.
    GpuBindingClass,
    /// `web_renderer_bound_to` — web renderer is bound to the expected endpoint.
    WebRendererBoundTo,
    /// `web_chrome_z_index_at_least` — chrome z-index meets the minimum.
    WebChromeZIndexAtLeast,
    /// `aiosfs_path_owner_resolved` — path owner resolves as expected.
    AiosfsPathOwnerResolved,
    /// `aiosfs_path_recovery_treatment_set` — recovery treatment matches.
    AiosfsPathRecoveryTreatmentSet,
    /// `namespace_catalog_version` — namespace catalog version matches policy.
    NamespaceCatalogVersion,
    /// `status_indicator_visible` — required operator status indicator is visible.
    StatusIndicatorVisible,
    /// `subject_session_flag_state` — subject session flag matches expectation.
    SubjectSessionFlagState,
    /// `filesystem_root_intact` — constitutional filesystem root is intact.
    FilesystemRootIntact,
    /// `spec_consumes_table` — spec dependency table obeys direction rules.
    SpecConsumesTable,
    /// `approval_binding_state` — approval binding shape is valid.
    ApprovalBindingState,
    /// `secret_pattern_match` — evidence payload secret-pattern scan result.
    SecretPatternMatch,
}

impl VerificationPrimitive {
    /// Return the canonical `SCREAMING_SNAKE_CASE` enum token.
    #[must_use]
    pub const fn as_wire_str(self) -> &'static str {
        match self {
            Self::ServiceActive => "SERVICE_ACTIVE",
            Self::ServiceInactive => "SERVICE_INACTIVE",
            Self::PackageInstalled => "PACKAGE_INSTALLED",
            Self::PortOpen => "PORT_OPEN",
            Self::PortClosed => "PORT_CLOSED",
            Self::HttpOk => "HTTP_OK",
            Self::FileExists => "FILE_EXISTS",
            Self::FileHash => "FILE_HASH",
            Self::RepoExists => "REPO_EXISTS",
            Self::AiosfsPointer => "AIOSFS_POINTER",
            Self::PolicyDecision => "POLICY_DECISION",
            Self::EvidenceExists => "EVIDENCE_EXISTS",
            Self::NetworkSubjectOutboundClass => "NETWORK_SUBJECT_OUTBOUND_CLASS",
            Self::NetworkActiveExposureClass => "NETWORK_ACTIVE_EXPOSURE_CLASS",
            Self::NetworkExternalModelCallBrokeredOnly => {
                "NETWORK_EXTERNAL_MODEL_CALL_BROKERED_ONLY"
            }
            Self::DnsResolverBackend => "DNS_RESOLVER_BACKEND",
            Self::VpnTunnelActive => "VPN_TUNNEL_ACTIVE",
            Self::MdnsPosture => "MDNS_POSTURE",
            Self::AiosfsPathInNamespace => "AIOSFS_PATH_IN_NAMESPACE",
            Self::SurfaceInZone => "SURFACE_IN_ZONE",
            Self::TreeContainsKind => "TREE_CONTAINS_KIND",
            Self::TreeMaxDepth => "TREE_MAX_DEPTH",
            Self::ThemeSatisfiesInvariants => "THEME_SATISFIES_INVARIANTS",
            Self::ThemeConstitutionalIconsIntact => "THEME_CONSTITUTIONAL_ICONS_INTACT",
            Self::GpuBindingClass => "GPU_BINDING_CLASS",
            Self::WebRendererBoundTo => "WEB_RENDERER_BOUND_TO",
            Self::WebChromeZIndexAtLeast => "WEB_CHROME_Z_INDEX_AT_LEAST",
            Self::AiosfsPathOwnerResolved => "AIOSFS_PATH_OWNER_RESOLVED",
            Self::AiosfsPathRecoveryTreatmentSet => "AIOSFS_PATH_RECOVERY_TREATMENT_SET",
            Self::NamespaceCatalogVersion => "NAMESPACE_CATALOG_VERSION",
            Self::StatusIndicatorVisible => "STATUS_INDICATOR_VISIBLE",
            Self::SubjectSessionFlagState => "SUBJECT_SESSION_FLAG_STATE",
            Self::FilesystemRootIntact => "FILESYSTEM_ROOT_INTACT",
            Self::SpecConsumesTable => "SPEC_CONSUMES_TABLE",
            Self::ApprovalBindingState => "APPROVAL_BINDING_STATE",
            Self::SecretPatternMatch => "SECRET_PATTERN_MATCH",
        }
    }
}

impl fmt::Display for VerificationPrimitive {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_wire_str())
    }
}
