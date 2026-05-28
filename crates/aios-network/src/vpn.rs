//! VPN model: WireGuard tunnel lifecycle (S8.4 §5).
//!
//! Closed [`VpnTunnelKind`] with 4 variants, [`WireGuardConfig`],
//! [`VpnTunnelManager`] enforcing a per-tunnel lifecycle FSM (Proposed →
//! Approved → Active → Failed / Revoked), and Ed25519-signed peer key rotation
//! from registered authorities.  Local private keys stay in vault handles per
//! INV-018 — never in config.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use ed25519_dalek::{Signature, VerifyingKey};
use tokio::sync::RwLock;

use crate::error::NetworkPolicyError;
use crate::evidence::{NetworkEvidenceEmitter, WithEmitter};
use crate::ids::SubjectId;

// ---------------------------------------------------------------------------
// VpnTunnelKind — closed 4-variant enum (S8.4 §5)
// ---------------------------------------------------------------------------

/// Closed tunnel kind vocabulary (S8.4 §5).
///
/// Four variants — two selectable `WireGuard` modes, a sentinel that can never
/// be selected, and a recovery-mode default that disables tunnels.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum VpnTunnelKind {
    /// Default for daily use — split-tunnel `WireGuard`.
    WireGuardSplitTunnel,
    /// Elevated for sensitive workloads — full-tunnel `WireGuard`.
    WireGuardFullTunnel,
    /// Sentinel — never selectable; exists to make intent explicit.
    OperatorDefinedOtherBlacklisted,
    /// Recovery-mode default; no tunnels active.
    RecoveryDisabled,
}

/// Gate: reject the blacklisted sentinel; all other variants pass.
///
/// # Errors
///
/// Returns [`NetworkPolicyError::Internal`] when
/// [`VpnTunnelKind::OperatorDefinedOtherBlacklisted`] is supplied.
pub fn validate_tunnel_kind(kind: &VpnTunnelKind) -> Result<(), NetworkPolicyError> {
    match kind {
        VpnTunnelKind::OperatorDefinedOtherBlacklisted => Err(NetworkPolicyError::Internal(
            "blacklisted tunnel kind".into(),
        )),
        _ => Ok(()),
    }
}

// ---------------------------------------------------------------------------
// WireGuard peer + config
// ---------------------------------------------------------------------------

/// A single `WireGuard` peer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WireGuardPeer {
    /// Unique peer identifier.
    pub peer_id: String,
    /// Remote endpoint in `host:port` form.
    pub endpoint: String,
    /// `Ed25519` / `Curve25519` public key (32 bytes).
    pub public_key: [u8; 32],
    /// CIDR list of allowed IPs for this peer.
    pub allowed_ips: Vec<String>,
    /// NAT keepalive interval in seconds.
    pub persistent_keepalive_seconds: u32,
}

/// `WireGuard` tunnel configuration — never carries raw private keys (INV-018).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WireGuardConfig {
    /// Unique tunnel identifier.
    pub tunnel_id: String,
    /// Tunnel kind (split / full / etc.).
    pub kind: VpnTunnelKind,
    /// `WireGuard` interface name (e.g. `"wg0"`).
    pub interface_name: String,
    /// Vault Broker handle — NEVER the raw key (INV-018).
    pub local_private_key_handle: String,
    /// Local `Ed25519`/`Curve25519` public key (32 bytes).
    pub local_public_key: [u8; 32],
    /// Peers for this tunnel.
    pub peers: Vec<WireGuardPeer>,
    /// Optional MTU override.
    pub mtu: Option<u32>,
    /// Optional `fwmark` for routing policy.
    pub fwmark: Option<u32>,
}

// ---------------------------------------------------------------------------
// PeerKeyRotation — Ed25519-signed rotation record
// ---------------------------------------------------------------------------

/// A signed peer key rotation record.
///
/// The signing payload is the canonical concatenation
/// `tunnel_id || old_pubkey || new_pubkey || rotated_at` (RFC 3339).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerKeyRotation {
    /// Tunnel this rotation applies to.
    pub tunnel_id: String,
    /// Previous peer public key being rotated out.
    pub old_pubkey: [u8; 32],
    /// New peer public key being rotated in.
    pub new_pubkey: [u8; 32],
    /// Timestamp of rotation.
    pub rotated_at: DateTime<Utc>,
    /// Hex fingerprint of the signing authority.
    pub authority_fingerprint: String,
    /// `Ed25519` signature over the canonical payload.
    pub signature: Vec<u8>,
}

impl PeerKeyRotation {
    /// Canonical bytes signed by the authority:
    /// `tunnel_id || old_pubkey || new_pubkey || rotated_at`.
    fn signing_payload(&self) -> Vec<u8> {
        let mut payload = Vec::with_capacity(self.tunnel_id.len() + 32 + 32 + 24);
        payload.extend_from_slice(self.tunnel_id.as_bytes());
        payload.extend_from_slice(&self.old_pubkey);
        payload.extend_from_slice(&self.new_pubkey);
        payload.extend_from_slice(self.rotated_at.to_rfc3339().as_bytes());
        payload
    }
}

// ---------------------------------------------------------------------------
// Tunnel lifecycle FSM
// ---------------------------------------------------------------------------

/// Per-tunnel lifecycle state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TunnelLifecycleState {
    /// Proposal submitted, awaiting policy decision.
    Proposed {
        /// When the proposal was created.
        since: DateTime<Utc>,
        /// Who requested the tunnel.
        requester: SubjectId,
    },
    /// Approved by policy; eligible for activation.
    Approved {
        /// When approval was granted.
        granted_at: DateTime<Utc>,
        /// Policy decision ID that authorised this.
        policy_decision_id: String,
    },
    /// Active and carrying traffic.
    Active {
        /// When the tunnel was activated.
        activated_at: DateTime<Utc>,
        /// Timestamp of most recent handshake.
        last_handshake_at: DateTime<Utc>,
    },
    /// Terminal — tunnel is down due to a failure.
    Failed {
        /// Why the tunnel failed.
        reason: String,
        /// When the failure was recorded.
        failed_at: DateTime<Utc>,
    },
    /// Terminal — tunnel was administratively revoked.
    Revoked {
        /// Why the tunnel was revoked.
        reason: String,
        /// When revocation was recorded.
        revoked_at: DateTime<Utc>,
    },
}

/// Compact label for list views.  Derived from [`TunnelLifecycleState`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TunnelLifecycleLabel {
    /// See [`TunnelLifecycleState::Proposed`].
    Proposed,
    /// See [`TunnelLifecycleState::Approved`].
    Approved,
    /// See [`TunnelLifecycleState::Active`].
    Active,
    /// See [`TunnelLifecycleState::Failed`].
    Failed,
    /// See [`TunnelLifecycleState::Revoked`].
    Revoked,
}

impl TunnelLifecycleState {
    /// Return the compact label for this state.
    #[must_use]
    pub const fn label(&self) -> TunnelLifecycleLabel {
        match self {
            Self::Proposed { .. } => TunnelLifecycleLabel::Proposed,
            Self::Approved { .. } => TunnelLifecycleLabel::Approved,
            Self::Active { .. } => TunnelLifecycleLabel::Active,
            Self::Failed { .. } => TunnelLifecycleLabel::Failed,
            Self::Revoked { .. } => TunnelLifecycleLabel::Revoked,
        }
    }
}

// ---------------------------------------------------------------------------
// VpnTunnelManager
// ---------------------------------------------------------------------------

/// Central VPN tunnel lifecycle manager (S8.4 §5).
///
/// Manages per-tunnel [`WireGuardConfig`] + [`TunnelLifecycleState`] pairs
/// across the full FSM: Proposed → Approved → Active → Failed / Revoked.
/// Peer key rotation is gated on an `Ed25519` signature from a registered
/// authority over the [`PeerKeyRotation`] payload.
pub struct VpnTunnelManager {
    /// All known tunnels keyed by `tunnel_id`.
    tunnels: RwLock<HashMap<String, (WireGuardConfig, TunnelLifecycleState)>>,
    /// Trusted signing authorities keyed by hex fingerprint.
    peer_authorities: RwLock<HashMap<String, VerifyingKey>>,
    /// Append-only rotation history.
    rotation_history: RwLock<Vec<PeerKeyRotation>>,
    /// Optional evidence emitter.
    emitter: RwLock<Option<Arc<dyn NetworkEvidenceEmitter>>>,
}

impl VpnTunnelManager {
    /// Create an empty manager with no tunnels and no authorities.
    #[must_use]
    pub fn new() -> Self {
        Self {
            tunnels: RwLock::new(HashMap::new()),
            peer_authorities: RwLock::new(HashMap::new()),
            rotation_history: RwLock::new(Vec::new()),
            emitter: RwLock::new(None),
        }
    }

    /// Register a trusted peer-key authority by its hex fingerprint.
    pub async fn register_authority(&self, fingerprint: &str, key: VerifyingKey) {
        self.peer_authorities
            .write()
            .await
            .insert(fingerprint.to_owned(), key);
    }

    // ------------------------------------------------------------------
    // Proposal
    // ------------------------------------------------------------------

    /// Propose a new tunnel.
    ///
    /// # Errors
    ///
    /// Returns `Internal` if the tunnel kind is blacklisted, is
    /// `RecoveryDisabled`, or if `tunnel_id` already exists.
    pub async fn propose_tunnel(
        &self,
        config: WireGuardConfig,
        requester: SubjectId,
    ) -> Result<(), NetworkPolicyError> {
        if config.kind == VpnTunnelKind::OperatorDefinedOtherBlacklisted {
            return Err(NetworkPolicyError::Internal(
                "OperatorDefinedOtherBlacklisted is sentinel; cannot create".into(),
            ));
        }
        if config.kind == VpnTunnelKind::RecoveryDisabled {
            return Err(NetworkPolicyError::Internal(
                "RecoveryDisabled cannot host a tunnel".into(),
            ));
        }

        let mut tunnels = self.tunnels.write().await;
        if tunnels.contains_key(&config.tunnel_id) {
            return Err(NetworkPolicyError::Internal(
                "tunnel_id already exists".into(),
            ));
        }

        let state = TunnelLifecycleState::Proposed {
            since: Utc::now(),
            requester,
        };
        let tunnel_id = config.tunnel_id.clone();
        tunnels.insert(tunnel_id.clone(), (config, state));
        drop(tunnels);

        if let Some(ref e) = *self.emitter.read().await {
            let _ = e
                .emit_vpn_tunnel_event(&tunnel_id, TunnelLifecycleLabel::Proposed)
                .await;
        }

        Ok(())
    }

    // ------------------------------------------------------------------
    // Approve
    // ------------------------------------------------------------------

    /// Approve a proposed tunnel, recording the policy decision id.
    ///
    /// # Errors
    ///
    /// Returns `Internal` if the tunnel is unknown or is not in `Proposed`.
    pub async fn approve_tunnel(
        &self,
        tunnel_id: &str,
        decision_id: &str,
    ) -> Result<(), NetworkPolicyError> {
        let mut tunnels = self.tunnels.write().await;
        let (_cfg, state) = tunnels
            .get_mut(tunnel_id)
            .ok_or_else(|| NetworkPolicyError::Internal("unknown tunnel_id".into()))?;

        if let TunnelLifecycleState::Proposed { .. } = state {
            *state = TunnelLifecycleState::Approved {
                granted_at: Utc::now(),
                policy_decision_id: decision_id.to_owned(),
            };
            drop(tunnels);

            if let Some(ref e) = *self.emitter.read().await {
                let _ = e
                    .emit_vpn_tunnel_event(tunnel_id, TunnelLifecycleLabel::Approved)
                    .await;
            }
            Ok(())
        } else {
            let label = state.label();
            drop(tunnels);
            Err(NetworkPolicyError::Internal(format!(
                "invalid transition {label:?} -> Approved"
            )))
        }
    }

    // ------------------------------------------------------------------
    // Activate
    // ------------------------------------------------------------------

    /// Activate an approved tunnel.
    ///
    /// # Errors
    ///
    /// Returns `Internal` if the tunnel is unknown or is not in `Approved`.
    pub async fn activate_tunnel(&self, tunnel_id: &str) -> Result<(), NetworkPolicyError> {
        let mut tunnels = self.tunnels.write().await;
        let (_cfg, state) = tunnels
            .get_mut(tunnel_id)
            .ok_or_else(|| NetworkPolicyError::Internal("unknown tunnel_id".into()))?;

        if let TunnelLifecycleState::Approved { .. } = state {
            let now = Utc::now();
            *state = TunnelLifecycleState::Active {
                activated_at: now,
                last_handshake_at: now,
            };
            drop(tunnels);

            if let Some(ref e) = *self.emitter.read().await {
                let _ = e
                    .emit_vpn_tunnel_event(tunnel_id, TunnelLifecycleLabel::Active)
                    .await;
            }
            Ok(())
        } else {
            let label = state.label();
            drop(tunnels);
            Err(NetworkPolicyError::Internal(format!(
                "invalid transition {label:?} -> Active"
            )))
        }
    }

    // ------------------------------------------------------------------
    // Handshake
    // ------------------------------------------------------------------

    /// Record a successful `WireGuard` handshake, updating `last_handshake_at`.
    ///
    /// # Errors
    ///
    /// Returns `Internal` if the tunnel is unknown or is not in `Active`.
    pub async fn record_handshake(&self, tunnel_id: &str) -> Result<(), NetworkPolicyError> {
        let mut tunnels = self.tunnels.write().await;
        let (_cfg, state) = tunnels
            .get_mut(tunnel_id)
            .ok_or_else(|| NetworkPolicyError::Internal("unknown tunnel_id".into()))?;

        if let TunnelLifecycleState::Active {
            ref mut last_handshake_at,
            ..
        } = state
        {
            *last_handshake_at = Utc::now();
            drop(tunnels);

            if let Some(ref e) = *self.emitter.read().await {
                let _ = e
                    .emit_vpn_tunnel_event(tunnel_id, TunnelLifecycleLabel::Active)
                    .await;
            }
            Ok(())
        } else {
            let label = state.label();
            drop(tunnels);
            Err(NetworkPolicyError::Internal(format!(
                "invalid transition {label:?} -> Active (handshake)"
            )))
        }
    }

    // ------------------------------------------------------------------
    // Fail
    // ------------------------------------------------------------------

    /// Transition an active tunnel to `Failed`.
    ///
    /// # Errors
    ///
    /// Returns `Internal` if the tunnel is unknown or is not in `Active`.
    pub async fn fail_tunnel(
        &self,
        tunnel_id: &str,
        reason: &str,
    ) -> Result<(), NetworkPolicyError> {
        let mut tunnels = self.tunnels.write().await;
        let (_cfg, state) = tunnels
            .get_mut(tunnel_id)
            .ok_or_else(|| NetworkPolicyError::Internal("unknown tunnel_id".into()))?;

        if let TunnelLifecycleState::Active { .. } = state {
            *state = TunnelLifecycleState::Failed {
                reason: reason.to_owned(),
                failed_at: Utc::now(),
            };
            drop(tunnels);

            if let Some(ref e) = *self.emitter.read().await {
                let _ = e
                    .emit_vpn_tunnel_event(tunnel_id, TunnelLifecycleLabel::Failed)
                    .await;
            }
            Ok(())
        } else {
            let label = state.label();
            drop(tunnels);
            Err(NetworkPolicyError::Internal(format!(
                "invalid transition {label:?} -> Failed"
            )))
        }
    }

    // ------------------------------------------------------------------
    // Revoke
    // ------------------------------------------------------------------

    /// Revoke a tunnel from any non-terminal state.
    ///
    /// # Errors
    ///
    /// Returns `Internal` if already `Failed` or `Revoked`.
    pub async fn revoke_tunnel(
        &self,
        tunnel_id: &str,
        reason: &str,
    ) -> Result<(), NetworkPolicyError> {
        let mut tunnels = self.tunnels.write().await;
        let (_cfg, state) = tunnels
            .get_mut(tunnel_id)
            .ok_or_else(|| NetworkPolicyError::Internal("unknown tunnel_id".into()))?;

        if matches!(
            state,
            TunnelLifecycleState::Failed { .. } | TunnelLifecycleState::Revoked { .. }
        ) {
            let label = state.label();
            drop(tunnels);
            Err(NetworkPolicyError::Internal(format!(
                "invalid transition {label:?} -> Revoked"
            )))
        } else {
            *state = TunnelLifecycleState::Revoked {
                reason: reason.to_owned(),
                revoked_at: Utc::now(),
            };
            drop(tunnels);

            if let Some(ref e) = *self.emitter.read().await {
                let _ = e
                    .emit_vpn_tunnel_event(tunnel_id, TunnelLifecycleLabel::Revoked)
                    .await;
            }
            Ok(())
        }
    }

    // ------------------------------------------------------------------
    // Peer key rotation
    // ------------------------------------------------------------------

    /// Rotate a peer's public key.
    ///
    /// Verifies the `Ed25519` signature on the rotation payload against the
    /// authority identified by `authority_fingerprint`.  On success the peer
    /// with `old_pubkey` in the named tunnel is updated and the rotation is
    /// appended to the history.
    ///
    /// # Errors
    ///
    /// Returns [`NetworkPolicyError::VpnPeerKeySignatureInvalid`] on
    /// unknown authority, bad signature, or unknown tunnel / peer.
    pub async fn rotate_peer_key(
        &self,
        rotation: PeerKeyRotation,
    ) -> Result<(), NetworkPolicyError> {
        let payload = rotation.signing_payload();

        let authorities = self.peer_authorities.read().await;
        let vk = authorities
            .get(&rotation.authority_fingerprint)
            .ok_or_else(|| {
                NetworkPolicyError::VpnPeerKeySignatureInvalid("unknown authority".into())
            })?;

        let sig = Signature::from_slice(&rotation.signature).map_err(|_| {
            NetworkPolicyError::VpnPeerKeySignatureInvalid(
                "ed25519 verify failed: invalid signature bytes".into(),
            )
        })?;

        vk.verify_strict(&payload, &sig).map_err(|_| {
            NetworkPolicyError::VpnPeerKeySignatureInvalid("ed25519 verify failed".into())
        })?;
        drop(authorities);

        let mut tunnels = self.tunnels.write().await;
        let (cfg, _state) = tunnels.get_mut(&rotation.tunnel_id).ok_or_else(|| {
            NetworkPolicyError::VpnPeerKeySignatureInvalid("unknown tunnel".into())
        })?;

        let peer = cfg
            .peers
            .iter_mut()
            .find(|p| p.public_key == rotation.old_pubkey)
            .ok_or_else(|| {
                NetworkPolicyError::VpnPeerKeySignatureInvalid("ed25519 verify failed".into())
            })?;

        peer.public_key = rotation.new_pubkey;
        drop(tunnels);

        if let Some(ref e) = *self.emitter.read().await {
            let _ = e.emit_vpn_peer_key_rotated(&rotation).await;
        }

        self.rotation_history.write().await.push(rotation);
        Ok(())
    }

    // ------------------------------------------------------------------
    // Read-only queries
    // ------------------------------------------------------------------

    /// List all tunnels with their compact lifecycle labels.
    pub async fn list_tunnels(&self) -> Vec<(String, TunnelLifecycleLabel)> {
        self.tunnels
            .read()
            .await
            .iter()
            .map(|(id, (_cfg, state))| (id.clone(), state.label()))
            .collect()
    }

    /// Return a clone of the lifecycle state for `tunnel_id`, or `None`.
    pub async fn get_tunnel_state(&self, tunnel_id: &str) -> Option<TunnelLifecycleState> {
        self.tunnels
            .read()
            .await
            .get(tunnel_id)
            .map(|(_cfg, state)| state.clone())
    }

    /// Return a clone of the append-only rotation history.
    pub async fn list_rotations(&self) -> Vec<PeerKeyRotation> {
        self.rotation_history.read().await.clone()
    }
}

impl WithEmitter for VpnTunnelManager {
    fn with_emitter(mut self, emitter: Option<Arc<dyn NetworkEvidenceEmitter>>) -> Self {
        self.emitter = RwLock::new(emitter);
        self
    }
}

impl Default for VpnTunnelManager {
    #[allow(clippy::missing_const_for_fn)]
    fn default() -> Self {
        Self::new()
    }
}
