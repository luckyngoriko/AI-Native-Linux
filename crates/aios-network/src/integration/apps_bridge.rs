//! Apps bridge: `network ↔ aios-apps` — Flatpak-install declared endpoints → `OutboundGrant`.
//!
//! When a package (e.g. Flatpak) declares outbound endpoints in its manifest, this bridge
//! extracts them and produces an unsigned `OutboundGrant` proposal for downstream
//! signing and approval.

use serde_json::Value;

use aios_apps::AppPackage;

use crate::allowlist::{AllowlistEntry, AllowlistEntryKind};
use crate::ids::SubjectId;
use crate::inbound::PortPolicy;
use crate::outbound_grant::{OutboundDirectiveKind, OutboundGrant};
use crate::protocol::ProtocolFamily;

/// Extract endpoint hints from `AppPackage.manifest_bytes` JSON.
///
/// Expected shape (simple):
/// ```json
/// { "outbound_endpoints": ["dns.example.com:443", "api.example.com:80"] }
/// ```
///
/// If `manifest_bytes` does not parse as JSON or lacks the `outbound_endpoints` key,
/// returns a grant with an empty allowlist (no declared endpoints).
///
/// The returned grant is **unsigned** — `signature` and `signer_fingerprint` are empty,
/// and callers must sign before registration.
#[must_use]
pub fn package_declared_endpoints_to_grant_proposal(
    pkg: &AppPackage,
    subject: SubjectId,
) -> OutboundGrant {
    let allowlist: Vec<AllowlistEntry> = parse_outbound_endpoints(&pkg.manifest_bytes)
        .into_iter()
        .map(|endpoint| AllowlistEntry {
            kind: AllowlistEntryKind::HostFqdn,
            value: endpoint,
            port_policy: PortPolicy::RegisteredEphemeral,
            protocol: ProtocolFamily::Tcp,
        })
        .collect();

    OutboundGrant {
        grant_id: format!("grt_apps_{}", pkg.package_id.0),
        subject,
        allowlist,
        directive_kind: OutboundDirectiveKind::AllowListOnly,
        issued_at: chrono::Utc::now(),
        expires_at: None,
        signer_fingerprint: String::new(),
        signature: vec![],
    }
}

fn parse_outbound_endpoints(manifest_bytes: &[u8]) -> Vec<String> {
    let v: Value = match serde_json::from_slice(manifest_bytes) {
        Ok(v) => v,
        Err(_) => return vec![],
    };

    match v.get("outbound_endpoints") {
        Some(Value::Array(arr)) => arr
            .iter()
            .filter_map(|el| el.as_str().map(String::from))
            .collect(),
        _ => vec![],
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test"
)]
mod tests {
    use super::*;
    use aios_apps::PackageId;

    #[test]
    fn empty_manifest_produces_empty_allowlist() {
        let pkg = AppPackage {
            package_id: PackageId("pkg_test01".into()),
            name: "test-app".into(),
            version: "1.0.0".into(),
            manifest_bytes: b"{}".to_vec(),
            content_hash_blake3: String::new(),
            ed25519_signature: vec![],
            signer_public_key: vec![],
            registered_at: chrono::Utc::now(),
        };

        let grant = package_declared_endpoints_to_grant_proposal(&pkg, SubjectId("subj_01".into()));
        assert!(grant.allowlist.is_empty());
        assert!(
            grant.signer_fingerprint.is_empty(),
            "unsigned proposals have empty fingerprint"
        );
    }

    #[test]
    fn manifest_with_endpoints_populates_allowlist() {
        let manifest = r#"{"outbound_endpoints": ["api.example.com:443", "cdn.example.com:443"]}"#;
        let pkg = AppPackage {
            package_id: PackageId("pkg_test02".into()),
            name: "test-app2".into(),
            version: "1.0.0".into(),
            manifest_bytes: manifest.as_bytes().to_vec(),
            content_hash_blake3: String::new(),
            ed25519_signature: vec![],
            signer_public_key: vec![],
            registered_at: chrono::Utc::now(),
        };

        let grant = package_declared_endpoints_to_grant_proposal(&pkg, SubjectId("subj_02".into()));
        assert_eq!(grant.allowlist.len(), 2);
    }

    #[test]
    fn non_json_manifest_produces_empty_allowlist() {
        let pkg = AppPackage {
            package_id: PackageId("pkg_test03".into()),
            name: "binary-blob".into(),
            version: "1.0.0".into(),
            manifest_bytes: b"not-json".to_vec(),
            content_hash_blake3: String::new(),
            ed25519_signature: vec![],
            signer_public_key: vec![],
            registered_at: chrono::Utc::now(),
        };

        let grant = package_declared_endpoints_to_grant_proposal(&pkg, SubjectId("subj_03".into()));
        assert!(grant.allowlist.is_empty());
    }
}
