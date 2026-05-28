use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::StandardSubscriptionId;

/// Closed taxonomy of regulatory / compliance standards.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StandardKind {
    /// NIST SP 800-53 Rev.5 — Security and Privacy Controls.
    Nist80053Rev5,
    /// NIST SP 800-218 — Secure Software Development Framework (SSDF).
    NistSp800218Ssdf,
    /// NIST SP 800-207 — Zero Trust Architecture.
    NistSp800207ZeroTrust,
    /// NIST SP 800-193 — Platform Firmware Resiliency.
    NistSp800193Firmware,
    /// DISA Security Technical Implementation Guide.
    DisaStig,
    /// CIS Controls v8.
    CisControlsV8,
    /// FIPS 140-3 — Cryptographic Module Validation.
    Fips1403,
    /// General Data Protection Regulation (EU).
    Gdpr,
    /// Health Insurance Portability and Accountability Act (US).
    Hipaa,
    /// ISO/IEC 27001 — Information Security Management.
    Iso27001,
    /// SOC 2 — Service Organization Control 2.
    Soc2,
}

/// A subscription to a compliance standard with timed review window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StandardSubscription {
    /// Unique subscription identifier.
    pub subscription_id: StandardSubscriptionId,
    /// The standard this subscription tracks.
    pub standard: StandardKind,
    /// URL to the official catalog / publication.
    pub catalog_url: String,
    /// Currently tracked revision.
    pub current_revision: String,
    /// When the subscription was last reviewed.
    pub last_reviewed_at: DateTime<Utc>,
    /// Next mandatory review deadline.
    pub next_review_due_at: DateTime<Utc>,
    /// Canonical identity of the responsible party.
    pub responsible_canonical_id: String,
}
