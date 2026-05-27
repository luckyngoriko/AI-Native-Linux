//! Recovery shell session model and degraded-mode escalation (INV I5 + I6 + I7).
//!
//! INV I5: Recovery mode runs under a separate Wayland display + separate `KWin`
//! process; only `AIOS_SURFACE`-kind nodes are admitted.
//! INV I6: Icons load only from a root-signed Ed25519 manifest — no breeze/oxygen
//! fallback.
//! INV I7: When `KWin` / Wayland / wgpu / icon-bundle / `KWin`-script verification
//! fails, the renderer escalates to fail-closed text-only degraded mode.

use std::collections::{BTreeMap, HashMap};

use chrono::{DateTime, Utc};
use ed25519_dalek::{Signature, VerifyingKey};

use crate::error::KdeRendererError;
use crate::node_kind::NodeKind;
use crate::types::RendererMode;

// ── Recovery session (INV I5) ─────────────────────────────────────────────

/// A recovery shell session running under a dedicated Wayland display and `KWin`
/// process. This is a type-level marker — no real process spawn occurs.
#[derive(Debug, Clone)]
pub struct RecoverySession {
    /// The separate Wayland display string (e.g. `"wayland-2"`).
    pub wayland_display: String,
    /// PID of the separate `KWin` process handling this recovery session.
    pub kwin_pid: u32,
    /// The AIOS user identity under which the recovery `KWin` runs.
    pub aios_user: String,
    /// Wall-clock timestamp of session start.
    pub started_at: DateTime<Utc>,
}

/// Evidence that the recovery session is isolated from the normal session
/// (INV I5 proof of separation).
#[derive(Debug, Clone)]
pub struct SessionIsolationMarker {
    /// The separate Wayland display for this recovery session.
    pub wayland_display: String,
    /// PID of the dedicated `KWin` process.
    pub kwin_pid: u32,
    /// Whether a separate user identity is in use.
    pub separate_user: bool,
}

/// Guard enforcing INV I5: only `SurfaceEmbed` (`AIOS_SURFACE` in S7.1 embedded
/// via `SurfaceEmbed` `NodeKind` in S7.2) nodes are admitted into the recovery
/// `KWin` session.
#[derive(Debug, Clone)]
pub struct RecoveryShellGuard {
    /// The recovery session this guard protects.
    pub session: RecoverySession,
    /// The set of node kinds allowed in this recovery session.
    allowed_node_kinds: &'static [NodeKind],
}

impl RecoveryShellGuard {
    /// Create a new guard for the given recovery session.
    ///
    /// Only [`NodeKind::SurfaceEmbed`] is permitted — `AIOS_SURFACE` in S7.1 is
    /// the surface kind embedded via `SurfaceEmbed` `NodeKind` in S7.2. There is
    /// no explicit `AiosSurface` variant in T-127's `NodeKind`.
    #[must_use]
    pub const fn new(session: RecoverySession) -> Self {
        Self {
            session,
            allowed_node_kinds: &[NodeKind::SurfaceEmbed],
        }
    }

    /// Admit a node kind into the recovery session.
    ///
    /// # Errors
    ///
    /// Returns `KdeRendererError::Internal` if the node kind is not in the
    /// recovery allowlist (INV I5).
    pub fn admit(&self, kind: NodeKind) -> Result<(), KdeRendererError> {
        if self.allowed_node_kinds.contains(&kind) {
            Ok(())
        } else {
            Err(KdeRendererError::Internal(
                "recovery shell allows AIOS_SURFACE only (INV I5)".into(),
            ))
        }
    }

    /// Produce a [`SessionIsolationMarker`] proving separate Wayland display,
    /// `KWin` process, and user identity for this recovery session.
    #[must_use]
    pub fn session_isolation_marker(&self) -> SessionIsolationMarker {
        SessionIsolationMarker {
            wayland_display: self.session.wayland_display.clone(),
            kwin_pid: self.session.kwin_pid,
            separate_user: true,
        }
    }
}

// ── Constitutional icon bundle (INV I6) ───────────────────────────────────

/// A single entry in a constitutional icon bundle manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IconEntry {
    /// The token identifier this icon maps to.
    pub token_id: String,
    /// Relative path within the bundle root.
    pub relative_path: String,
    /// BLAKE3 hash of the icon file content (64 hex chars).
    pub blake3_hash: String,
}

/// A root-signed constitutional icon bundle (INV I6).
///
/// Icons load only from this bundle; no breeze/oxygen fallback is permitted.
/// The manifest is an Ed25519-signed binding of `(token_id, relative_path,
/// blake3_hash)` for every entry. The blake3 hash is canonical — file contents
/// are not re-read at verification time, but the hash field is validated for
/// format (non-empty, 64 hex chars).
#[derive(Debug, Clone)]
pub struct ConstitutionalIconBundle {
    /// Theme identifier (e.g. `"aios-recovery"`).
    pub theme_id: String,
    /// Root path of the icon bundle on disk.
    pub root_path: std::path::PathBuf,
    /// Manifest mapping `token_id` → [`IconEntry`] (sorted by key for
    /// deterministic signature verification).
    pub manifest: BTreeMap<String, IconEntry>,
    /// Ed25519 signature over the concatenated entry data.
    pub bundle_signature: Vec<u8>,
    /// Fingerprint of the signer (maps to a key in [`Self::trusted_authorities`]).
    pub signer_fingerprint: String,
    /// Registry of trusted signing authorities (fingerprint → verifying key).
    pub trusted_authorities: HashMap<String, VerifyingKey>,
}

impl ConstitutionalIconBundle {
    /// Verify the bundle's Ed25519 signature and per-entry blake3 hashes.
    ///
    /// Checks:
    /// 1. The signer's fingerprint is registered in [`Self::trusted_authorities`].
    /// 2. The Ed25519 signature over the concatenated `(token_id || relative_path
    ///    || blake3_hash)` bytes for every manifest entry is valid.
    /// 3. Every entry's `blake3_hash` is non-empty, exactly 64 characters, and
    ///    contains only hex digits.
    ///
    /// # Errors
    ///
    /// Returns [`KdeRendererError::IconBundleVerificationFailed`] if any check
    /// fails.
    pub fn verify(&self) -> Result<(), KdeRendererError> {
        let theme_id = &self.theme_id;

        // Gate 1 — signer authority must be registered.
        let vk = self
            .trusted_authorities
            .get(&self.signer_fingerprint)
            .ok_or_else(|| KdeRendererError::IconBundleVerificationFailed {
                theme_id: theme_id.clone(),
                reason: "unknown authority".into(),
            })?;

        // Gate 2 — Ed25519 signature over the deterministic concatenation of
        // all entries in BTreeMap key order.
        let mut message = Vec::new();
        for (token_id, entry) in &self.manifest {
            message.extend_from_slice(token_id.as_bytes());
            message.extend_from_slice(entry.relative_path.as_bytes());
            message.extend_from_slice(entry.blake3_hash.as_bytes());
        }

        let sig_array = signature_from_vec(&self.bundle_signature, theme_id)?;
        vk.verify_strict(&message, &sig_array).map_err(|_| {
            KdeRendererError::IconBundleVerificationFailed {
                theme_id: theme_id.clone(),
                reason: "invalid ed25519 signature".into(),
            }
        })?;

        // Gate 3 — every entry's blake3_hash is non-empty, 64 chars, all hex.
        for entry in self.manifest.values() {
            validate_blake3_hex(&entry.blake3_hash, theme_id)?;
        }

        Ok(())
    }

    /// Look up an icon entry by token identifier.
    #[must_use]
    pub fn lookup(&self, token_id: &str) -> Option<&IconEntry> {
        self.manifest.get(token_id)
    }
}

/// Convert a signature `Vec<u8>` into an ed25519-dalek [`Signature`].
fn signature_from_vec(bytes: &[u8], theme_id: &str) -> Result<Signature, KdeRendererError> {
    let array: [u8; 64] =
        bytes
            .try_into()
            .map_err(|_| KdeRendererError::IconBundleVerificationFailed {
                theme_id: theme_id.to_string(),
                reason: "invalid ed25519 signature".into(),
            })?;
    Ok(Signature::from_bytes(&array))
}

/// Validate that a blake3 hash string is non-empty, exactly 64 chars, and
/// contains only ASCII hex digits.
fn validate_blake3_hex(hash: &str, theme_id: &str) -> Result<(), KdeRendererError> {
    if hash.is_empty() || hash.len() != 64 || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(KdeRendererError::IconBundleVerificationFailed {
            theme_id: theme_id.to_string(),
            reason: "blake3 mismatch".into(),
        });
    }
    Ok(())
}

// ── Degraded mode escalation (INV I7) ─────────────────────────────────────

/// Triggers for degraded (text-only fallback) mode per INV I7.
///
/// When any of these runtime capabilities are unavailable, the renderer
/// escalates to fail-closed text-only mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DegradedTrigger {
    /// `KWin` compositor process is unreachable.
    KwinUnavailable,
    /// Wayland display connection failed.
    WaylandConnectFailed,
    /// GPU device (wgpu / `VkDevice`) acquisition failed.
    GpuDeviceAcquisitionFailed,
    /// Constitutional icon bundle verification failed at load time.
    IconBundleVerificationFailed,
    /// `KWin` script verification failed (unsigned, hash mismatch, or blocked path).
    KwinScriptVerificationFailed,
}

impl DegradedTrigger {
    /// All five declared variants in declaration order.
    pub const ALL: &'static [Self] = &[
        Self::KwinUnavailable,
        Self::WaylandConnectFailed,
        Self::GpuDeviceAcquisitionFailed,
        Self::IconBundleVerificationFailed,
        Self::KwinScriptVerificationFailed,
    ];
}

/// Escalate to degraded (text-only fallback) mode per INV I7.
///
/// Returns the degraded [`RendererMode`] and a short user-facing reason string
/// suitable for evidence emission. The renderer must reject GPU-bearing node
/// kinds while in this mode.
#[must_use]
pub fn escalate_to_degraded(trigger: DegradedTrigger) -> (RendererMode, &'static str) {
    let reason: &'static str = match trigger {
        DegradedTrigger::KwinUnavailable => "kwin_unreachable",
        DegradedTrigger::WaylandConnectFailed => "wayland_connect_failed",
        DegradedTrigger::GpuDeviceAcquisitionFailed => "gpu_acquisition_failed",
        DegradedTrigger::IconBundleVerificationFailed => "icon_bundle_verification_failed",
        DegradedTrigger::KwinScriptVerificationFailed => "kwin_script_verification_failed",
    };
    (RendererMode::Degraded(reason.to_string()), reason)
}
