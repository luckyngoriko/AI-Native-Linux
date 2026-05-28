#![allow(
    missing_docs,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_lines,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use std::time::Duration;

use aios_network::{
    validate_tunnel_kind, NetworkPolicyError, PeerKeyRotation, SubjectId, TunnelLifecycleLabel,
    TunnelLifecycleState, VpnTunnelKind, VpnTunnelManager, WireGuardConfig, WireGuardPeer,
};
use chrono::Utc;
use ed25519_dalek::{Signer, SigningKey};
use rand_core::OsRng;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn split_tunnel_config(id: &str) -> WireGuardConfig {
    WireGuardConfig {
        tunnel_id: id.to_owned(),
        kind: VpnTunnelKind::WireGuardSplitTunnel,
        interface_name: "wg0".into(),
        local_private_key_handle: "vault://wg/priv/0".into(),
        local_public_key: [0xAA; 32],
        peers: vec![WireGuardPeer {
            peer_id: "peer-1".into(),
            endpoint: "10.0.0.2:51820".into(),
            public_key: [0xBB; 32],
            allowed_ips: vec!["10.0.0.0/24".into()],
            persistent_keepalive_seconds: 25,
        }],
        mtu: Some(1420),
        fwmark: None,
    }
}

#[allow(dead_code)]
fn full_tunnel_config(id: &str) -> WireGuardConfig {
    WireGuardConfig {
        tunnel_id: id.to_owned(),
        kind: VpnTunnelKind::WireGuardFullTunnel,
        interface_name: "wg1".into(),
        local_private_key_handle: "vault://wg/priv/1".into(),
        local_public_key: [0xCC; 32],
        peers: vec![WireGuardPeer {
            peer_id: "peer-2".into(),
            endpoint: "192.168.1.1:51820".into(),
            public_key: [0xDD; 32],
            allowed_ips: vec!["0.0.0.0/0".into()],
            persistent_keepalive_seconds: 15,
        }],
        mtu: None,
        fwmark: Some(42),
    }
}

fn blacklisted_config() -> WireGuardConfig {
    WireGuardConfig {
        tunnel_id: "bl".into(),
        kind: VpnTunnelKind::OperatorDefinedOtherBlacklisted,
        ..split_tunnel_config("dummy")
    }
}

fn recovery_disabled_config() -> WireGuardConfig {
    WireGuardConfig {
        tunnel_id: "rd".into(),
        kind: VpnTunnelKind::RecoveryDisabled,
        ..split_tunnel_config("dummy")
    }
}

async fn proposed_manager() -> VpnTunnelManager {
    let mgr = VpnTunnelManager::new();
    mgr.propose_tunnel(split_tunnel_config("t1"), SubjectId("human:ops".into()))
        .await
        .unwrap();
    mgr
}

async fn approved_manager() -> VpnTunnelManager {
    let mgr = proposed_manager().await;
    mgr.approve_tunnel("t1", "dec-1").await.unwrap();
    mgr
}

async fn active_manager() -> VpnTunnelManager {
    let mgr = approved_manager().await;
    mgr.activate_tunnel("t1").await.unwrap();
    mgr
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        use std::fmt::Write;
        write!(s, "{b:02x}").expect("write to String is infallible");
    }
    s
}

fn make_authority() -> (SigningKey, String) {
    let sk = SigningKey::generate(&mut OsRng);
    let vk = sk.verifying_key();
    let fingerprint = bytes_to_hex(vk.as_bytes());
    (sk, fingerprint)
}

fn sign_rotation(
    sk: &SigningKey,
    tunnel_id: &str,
    old_pubkey: [u8; 32],
    new_pubkey: [u8; 32],
    authority_fingerprint: &str,
) -> PeerKeyRotation {
    let rotated_at = Utc::now();
    // Build the same payload PeerKeyRotation::signing_payload() produces.
    let mut payload = Vec::new();
    payload.extend_from_slice(tunnel_id.as_bytes());
    payload.extend_from_slice(&old_pubkey);
    payload.extend_from_slice(&new_pubkey);
    payload.extend_from_slice(rotated_at.to_rfc3339().as_bytes());
    let sig = sk.sign(&payload);
    PeerKeyRotation {
        tunnel_id: tunnel_id.to_owned(),
        old_pubkey,
        new_pubkey,
        rotated_at,
        authority_fingerprint: authority_fingerprint.to_owned(),
        signature: sig.to_bytes().to_vec(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn vpn_tunnel_kind_has_4_variants_including_blacklist_sentinel() {
    let kinds: &[VpnTunnelKind] = &[
        VpnTunnelKind::WireGuardSplitTunnel,
        VpnTunnelKind::WireGuardFullTunnel,
        VpnTunnelKind::OperatorDefinedOtherBlacklisted,
        VpnTunnelKind::RecoveryDisabled,
    ];
    assert_eq!(kinds.len(), 4);
}

#[test]
fn validate_tunnel_kind_blacklisted_returns_error() {
    let result = validate_tunnel_kind(&VpnTunnelKind::OperatorDefinedOtherBlacklisted);
    assert!(result.is_err());
    let err = result.unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("blacklisted tunnel kind"));
}

#[test]
fn validate_tunnel_kind_split_passes() {
    assert!(validate_tunnel_kind(&VpnTunnelKind::WireGuardSplitTunnel).is_ok());
}

#[tokio::test]
async fn propose_tunnel_with_wireguard_split_succeeds() {
    let mgr = VpnTunnelManager::new();
    mgr.propose_tunnel(split_tunnel_config("wg0"), SubjectId("human:ops".into()))
        .await
        .unwrap();

    let state = mgr.get_tunnel_state("wg0").await.unwrap();
    assert!(matches!(state, TunnelLifecycleState::Proposed { .. }));
}

#[tokio::test]
async fn propose_tunnel_with_blacklist_sentinel_returns_internal_error() {
    let mgr = VpnTunnelManager::new();
    let result = mgr
        .propose_tunnel(blacklisted_config(), SubjectId("human:ops".into()))
        .await;
    assert!(result.is_err());
    let msg = format!("{}", result.unwrap_err());
    assert!(msg.contains("sentinel"));
}

#[tokio::test]
async fn propose_tunnel_with_recovery_disabled_kind_returns_internal_error() {
    let mgr = VpnTunnelManager::new();
    let result = mgr
        .propose_tunnel(recovery_disabled_config(), SubjectId("human:ops".into()))
        .await;
    assert!(result.is_err());
    let msg = format!("{}", result.unwrap_err());
    assert!(msg.contains("RecoveryDisabled"));
}

#[tokio::test]
async fn propose_duplicate_tunnel_id_returns_internal_error() {
    let mgr = proposed_manager().await;
    let result = mgr
        .propose_tunnel(split_tunnel_config("t1"), SubjectId("human:ops".into()))
        .await;
    assert!(result.is_err());
    let msg = format!("{}", result.unwrap_err());
    assert!(msg.contains("already exists"));
}

#[tokio::test]
async fn approve_proposed_tunnel_succeeds() {
    let mgr = proposed_manager().await;
    mgr.approve_tunnel("t1", "dec-1").await.unwrap();

    let state = mgr.get_tunnel_state("t1").await.unwrap();
    assert!(matches!(state, TunnelLifecycleState::Approved { .. }));
}

#[tokio::test]
async fn activate_approved_tunnel_succeeds() {
    let mgr = approved_manager().await;
    mgr.activate_tunnel("t1").await.unwrap();

    let state = mgr.get_tunnel_state("t1").await.unwrap();
    assert!(matches!(state, TunnelLifecycleState::Active { .. }));
}

#[tokio::test]
async fn activate_proposed_tunnel_returns_invalid_transition() {
    let mgr = proposed_manager().await;
    let result = mgr.activate_tunnel("t1").await;
    assert!(result.is_err());
    let msg = format!("{}", result.unwrap_err());
    assert!(msg.contains("invalid transition"));
}

#[tokio::test]
async fn record_handshake_updates_last_handshake_at() {
    let mgr = active_manager().await;

    // Capture the handshake time we already have.
    let before = {
        let state = mgr.get_tunnel_state("t1").await.unwrap();
        match state {
            TunnelLifecycleState::Active {
                last_handshake_at, ..
            } => last_handshake_at,
            _ => panic!("expected Active"),
        }
    };

    // Small sleep so we can detect the update.
    tokio::time::sleep(Duration::from_millis(5)).await;

    mgr.record_handshake("t1").await.unwrap();

    let after = {
        let state = mgr.get_tunnel_state("t1").await.unwrap();
        match state {
            TunnelLifecycleState::Active {
                last_handshake_at, ..
            } => last_handshake_at,
            _ => panic!("expected Active"),
        }
    };

    assert!(after > before, "handshake timestamp should advance");
}

#[tokio::test]
async fn fail_active_tunnel_transitions_to_failed() {
    let mgr = active_manager().await;
    mgr.fail_tunnel("t1", "peer unreachable").await.unwrap();

    let state = mgr.get_tunnel_state("t1").await.unwrap();
    match state {
        TunnelLifecycleState::Failed { reason, .. } => {
            assert_eq!(reason, "peer unreachable");
        }
        _ => panic!("expected Failed"),
    }
}

#[tokio::test]
async fn revoke_active_tunnel_transitions_to_revoked() {
    let mgr = active_manager().await;
    mgr.revoke_tunnel("t1", "admin action").await.unwrap();

    let state = mgr.get_tunnel_state("t1").await.unwrap();
    assert!(matches!(state, TunnelLifecycleState::Revoked { .. }));
}

#[tokio::test]
async fn revoke_proposed_tunnel_transitions_to_revoked() {
    let mgr = proposed_manager().await;
    mgr.revoke_tunnel("t1", "denied by policy").await.unwrap();

    let state = mgr.get_tunnel_state("t1").await.unwrap();
    assert!(matches!(state, TunnelLifecycleState::Revoked { .. }));
}

#[tokio::test]
async fn failed_to_active_returns_invalid_transition() {
    let mgr = active_manager().await;
    mgr.fail_tunnel("t1", "peer unreachable").await.unwrap();

    // Now try to activate a failed tunnel — should be rejected.
    let result = mgr.activate_tunnel("t1").await;
    assert!(result.is_err());
    let msg = format!("{}", result.unwrap_err());
    assert!(msg.contains("invalid transition"));
}

#[tokio::test]
async fn failed_cannot_be_approved() {
    let mgr = active_manager().await;
    mgr.fail_tunnel("t1", "dead").await.unwrap();
    let result = mgr.approve_tunnel("t1", "dec-2").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn rotate_peer_key_with_valid_signature_succeeds() {
    let mgr = active_manager().await;
    let (sk, fingerprint) = make_authority();
    mgr.register_authority(&fingerprint, sk.verifying_key())
        .await;

    let old_key = [0xBB; 32];
    let new_key = [0xEE; 32];
    let rotation = sign_rotation(&sk, "t1", old_key, new_key, &fingerprint);

    mgr.rotate_peer_key(rotation).await.unwrap();

    // Verify the peer key was updated.
    let rotations = mgr.list_rotations().await;
    assert_eq!(rotations.len(), 1);
    assert_eq!(rotations[0].old_pubkey, old_key);
    assert_eq!(rotations[0].new_pubkey, new_key);
}

#[tokio::test]
async fn rotate_peer_key_with_invalid_signature_returns_vpn_peer_key_signature_invalid() {
    let mgr = active_manager().await;
    let (sk, fingerprint) = make_authority();
    mgr.register_authority(&fingerprint, sk.verifying_key())
        .await;

    let mut rotation = sign_rotation(&sk, "t1", [0xBB; 32], [0xEE; 32], &fingerprint);
    // Corrupt the signature.
    rotation.signature[0] ^= 0xFF;

    let result = mgr.rotate_peer_key(rotation).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        NetworkPolicyError::VpnPeerKeySignatureInvalid(msg) => {
            assert!(msg.contains("ed25519 verify failed"));
        }
        other => panic!("expected VpnPeerKeySignatureInvalid, got {other:?}"),
    }
}

#[tokio::test]
async fn rotate_peer_key_with_unknown_authority_returns_vpn_peer_key_signature_invalid() {
    let mgr = active_manager().await;
    let (sk, _fingerprint) = make_authority();

    let rotation = sign_rotation(&sk, "t1", [0xBB; 32], [0xEE; 32], "unknown-fp");

    let result = mgr.rotate_peer_key(rotation).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        NetworkPolicyError::VpnPeerKeySignatureInvalid(msg) => {
            assert!(msg.contains("unknown authority"));
        }
        other => panic!("expected VpnPeerKeySignatureInvalid, got {other:?}"),
    }
}

#[tokio::test]
async fn rotate_peer_key_with_unknown_tunnel_returns_vpn_peer_key_signature_invalid() {
    let mgr = VpnTunnelManager::new();
    let (sk, fingerprint) = make_authority();
    mgr.register_authority(&fingerprint, sk.verifying_key())
        .await;

    let rotation = sign_rotation(&sk, "no-such-tunnel", [0xBB; 32], [0xEE; 32], &fingerprint);

    let result = mgr.rotate_peer_key(rotation).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        NetworkPolicyError::VpnPeerKeySignatureInvalid(msg) => {
            assert!(msg.contains("unknown tunnel"));
        }
        other => panic!("expected VpnPeerKeySignatureInvalid, got {other:?}"),
    }
}

#[tokio::test]
async fn list_rotations_after_2_rotations_returns_2() {
    let mgr = active_manager().await;
    let (sk, fingerprint) = make_authority();
    mgr.register_authority(&fingerprint, sk.verifying_key())
        .await;

    let r1 = sign_rotation(&sk, "t1", [0xBB; 32], [0xEE; 32], &fingerprint);
    mgr.rotate_peer_key(r1).await.unwrap();

    let r2 = sign_rotation(&sk, "t1", [0xEE; 32], [0xFF; 32], &fingerprint);
    mgr.rotate_peer_key(r2).await.unwrap();

    let rotations = mgr.list_rotations().await;
    assert_eq!(rotations.len(), 2);
}

#[test]
fn tunnel_lifecycle_label_from_state() {
    use TunnelLifecycleState as S;

    let proposed = S::Proposed {
        since: Utc::now(),
        requester: SubjectId("s".into()),
    };
    assert_eq!(proposed.label(), TunnelLifecycleLabel::Proposed);

    let approved = S::Approved {
        granted_at: Utc::now(),
        policy_decision_id: "d".into(),
    };
    assert_eq!(approved.label(), TunnelLifecycleLabel::Approved);

    let active = S::Active {
        activated_at: Utc::now(),
        last_handshake_at: Utc::now(),
    };
    assert_eq!(active.label(), TunnelLifecycleLabel::Active);

    let failed = S::Failed {
        reason: "r".into(),
        failed_at: Utc::now(),
    };
    assert_eq!(failed.label(), TunnelLifecycleLabel::Failed);

    let revoked = S::Revoked {
        reason: "r".into(),
        revoked_at: Utc::now(),
    };
    assert_eq!(revoked.label(), TunnelLifecycleLabel::Revoked);
}
