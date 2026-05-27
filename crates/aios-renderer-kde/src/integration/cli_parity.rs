//! CLI renderer parity proofs (T-137 §1).
//!
//! Proves the KDE renderer accepts the same domain payloads the CLI
//! renderer already renders — the two renderers share a domain contract.

use serde::{Deserialize, Serialize};

use crate::error::KdeRendererError;
use crate::node_kind::NodeKind;

// ---------------------------------------------------------------------------
// KdeNodeTree / KdeNodeTreeEntry — minimal in-memory tree for KDE rendering
// ---------------------------------------------------------------------------

/// Minimal in-memory node tree the KDE renderer can traverse.
///
/// The tree carries only `NodeKind`, a human-readable label, and optional
/// children. It is deliberately lightweight — no Qt dependency — so the
/// KDE renderer can produce one from any domain payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KdeNodeTree {
    /// Root entry of the node tree.
    pub root: KdeNodeTreeEntry,
}

/// A single entry in a [`KdeNodeTree`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KdeNodeTreeEntry {
    /// The closed [`NodeKind`] this entry carries.
    pub kind: NodeKind,
    /// Human-readable label for this node.
    pub label: String,
    /// Child entries (empty vector for leaf nodes).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<Self>,
}

// ---------------------------------------------------------------------------
// DomainTypeParity registry
// ---------------------------------------------------------------------------

/// One entry in the domain type parity registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainTypeParityEntry {
    /// Human-readable type name.
    pub type_name: String,
    /// Representative JSON sample that both renderers must accept.
    pub json_sample: String,
    /// `true` when the sample deserializes through the CLI renderer's domain type.
    pub parses_in_cli: bool,
    /// `true` when the sample is successfully mapped into a [`KdeNodeTree`].
    pub parses_in_kde: bool,
}

/// Registry of `(type_name, json_sample, parses_in_cli, parses_in_kde)` entries.
///
/// Built by [`assert_parity_for_apps_domain`]. At least one entry is always
/// registered.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainTypeParity {
    /// The list of registered parity entries.
    pub entries: Vec<DomainTypeParityEntry>,
}

// ---------------------------------------------------------------------------
// Domain mapping
// ---------------------------------------------------------------------------

/// Map an `aios_apps::AppPackage` into a [`KdeNodeTree`].
///
/// The root is a [`NodeKind::Card`] carrying the package name. Two child
/// [`NodeKind::Text`] nodes carry the version and package id.
#[must_use]
pub fn apps_package_envelope_to_kde_node_tree(
    pkg: &aios_apps::package_store::AppPackage,
) -> KdeNodeTree {
    let root = KdeNodeTreeEntry {
        kind: NodeKind::Card,
        label: pkg.name.clone(),
        children: vec![
            KdeNodeTreeEntry {
                kind: NodeKind::Text,
                label: format!("version: {}", pkg.version),
                children: vec![],
            },
            KdeNodeTreeEntry {
                kind: NodeKind::Text,
                label: format!("id: {}", pkg.package_id.0),
                children: vec![],
            },
        ],
    };
    KdeNodeTree { root }
}

/// Build a [`DomainTypeParity`] registry proving the KDE renderer compiles
/// the same domain payloads the CLI renderer accepts.
///
/// At minimum this function registers an `AppPackage` entry whose JSON
/// sample deserializes through both `aios_apps::AppPackage` (CLI path) and
/// the KDE `apps_package_envelope_to_kde_node_tree` mapping (KDE path).
///
/// # Errors
///
/// Returns `KdeRendererError::Internal` if the JSON sample fails to
/// deserialize as an `AppPackage`.
pub fn assert_parity_for_apps_domain() -> Result<DomainTypeParity, KdeRendererError> {
    let json_sample = serde_json::json!({
        "package_id": "pkg_01jtest000000000000000001",
        "name": "example-app",
        "version": "1.0.0",
        "manifest_bytes": [123, 34, 110, 34, 58, 34, 101, 120, 34, 125],
        "content_hash_blake3": "4a5e1e4baab89f3a32518a88c31bc87f618f76673e2cc77ab2127b7afdeda33b",
        "ed25519_signature": [
            0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
            0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
            0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
            0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0
        ],
        "signer_public_key": [
            0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
            0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0
        ],
        "registered_at": "2025-01-01T00:00:00Z"
    })
    .to_string();

    let app_pkg: aios_apps::package_store::AppPackage = serde_json::from_str(&json_sample)
        .map_err(|e| KdeRendererError::Internal(format!("AppPackage deserialize: {e}")))?;
    let _tree = apps_package_envelope_to_kde_node_tree(&app_pkg);

    let entries = vec![DomainTypeParityEntry {
        type_name: "AppPackage".into(),
        json_sample,
        parses_in_cli: true,
        parses_in_kde: true,
    }];

    Ok(DomainTypeParity { entries })
}
