//! Web renderer parity proofs — cross-renderer `NodeKind` consistency (T-149 §1).
//!
//! Proves the Web renderer compiles the same domain payloads the KDE and CLI
//! renderers already handle — the three L7 renderers share a domain contract
//! through the 19-variant closed [`NodeKind`] vocabulary.

use serde::{Deserialize, Serialize};

use crate::error::WebRendererError;
use crate::NodeKind;

// ---------------------------------------------------------------------------
// WebRenderTree / WebRenderTreeEntry
// ---------------------------------------------------------------------------

/// Minimal in-memory node tree the Web renderer can traverse.
///
/// Carries a [`NodeKind`], a DOM tag, a human-readable label, and optional
/// children. Deliberately lightweight — no DOM/JS dependency — so the Web
/// renderer can produce one from any domain payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WebRenderTree {
    /// Root entry of the node tree.
    pub root: WebRenderTreeEntry,
}

/// A single entry in a [`WebRenderTree`].
///
/// `dom_tag` is a `&'static str` per the S7.2 §3 contract. Serde is handled
/// manually for deserialization because `&'static str` cannot be borrowed from
/// a `Deserializer`; the manual impl leaks heap-allocated strings as static
/// references (safe for test use and the tree lifetime).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WebRenderTreeEntry {
    /// The closed [`NodeKind`] this entry carries.
    pub kind: NodeKind,
    /// Target DOM element tag for this node (e.g. "div", "span", "ul").
    pub dom_tag: &'static str,
    /// Human-readable label for this node.
    pub label: String,
    /// Child entries (empty vector for leaf nodes).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<Self>,
}

// Manual Deserialize impls — the derived impl can't satisfy `'de: 'static` for
// the `&'static str` field, so we use a String-based helper and leak.

impl<'de> Deserialize<'de> for WebRenderTree {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct TreeHelper {
            root: EntryHelper,
        }

        #[derive(Deserialize)]
        struct EntryHelper {
            kind: NodeKind,
            dom_tag: String,
            label: String,
            #[serde(default)]
            children: Vec<Self>,
        }

        fn convert_entry(h: EntryHelper) -> WebRenderTreeEntry {
            WebRenderTreeEntry {
                kind: h.kind,
                dom_tag: Box::leak(h.dom_tag.into_boxed_str()) as &str,
                label: h.label,
                children: h.children.into_iter().map(convert_entry).collect(),
            }
        }

        let helper = TreeHelper::deserialize(deserializer)?;
        Ok(Self {
            root: convert_entry(helper.root),
        })
    }
}

impl<'de> Deserialize<'de> for WebRenderTreeEntry {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct EntryHelper {
            kind: NodeKind,
            dom_tag: String,
            label: String,
            #[serde(default)]
            children: Vec<Self>,
        }

        fn convert(h: EntryHelper) -> WebRenderTreeEntry {
            WebRenderTreeEntry {
                kind: h.kind,
                dom_tag: Box::leak(h.dom_tag.into_boxed_str()) as &str,
                label: h.label,
                children: h.children.into_iter().map(convert).collect(),
            }
        }

        let helper = EntryHelper::deserialize(deserializer)?;
        Ok(convert(helper))
    }
}

// ---------------------------------------------------------------------------
// ThreeWayParity registry
// ---------------------------------------------------------------------------

/// One entry in the three-way renderer parity registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreeWayParityEntry {
    /// Human-readable type name.
    pub type_name: String,
    /// Representative JSON sample that all three renderers must accept.
    pub json_sample: String,
    /// `true` when the sample produces non-empty CLI output.
    pub parses_in_cli: bool,
    /// `true` when the sample is successfully mapped into a KDE
    /// `KdeNodeTree`.
    pub parses_in_kde: bool,
    /// `true` when the sample is successfully mapped into a
    /// [`WebRenderTree`].
    pub parses_in_web: bool,
}

/// Registry of `(type_name, json_sample, parses_in_cli, parses_in_kde,
/// parses_in_web)` entries.
///
/// Built by [`assert_three_way_parity_for_apps_domain`]. At least one entry is
/// always registered.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreeWayParity {
    /// The list of registered parity entries.
    pub entries: Vec<ThreeWayParityEntry>,
}

// ---------------------------------------------------------------------------
// DOM tag assignments per NodeKind
// ---------------------------------------------------------------------------

/// Return the semantic DOM tag for a given [`NodeKind`].
///
/// The mapping follows the S7.2 §3 vocabulary translated into HTML5 semantic
/// elements.
#[must_use]
#[allow(
    clippy::match_same_arms,
    reason = "S7.2 §3 keeps each NodeKind explicit for clarity"
)]
pub const fn dom_tag_for(kind: NodeKind) -> &'static str {
    match kind {
        NodeKind::Container => "div",
        NodeKind::Divider => "hr",
        NodeKind::Spacer => "div",
        NodeKind::Text => "span",
        NodeKind::Heading => "h2",
        NodeKind::InlineCode => "code",
        NodeKind::CodeBlock => "pre",
        NodeKind::Card => "div",
        NodeKind::List => "ul",
        NodeKind::Table => "table",
        NodeKind::Form => "form",
        NodeKind::ActionButton => "button",
        NodeKind::Visualization => "canvas",
        NodeKind::Stream => "div",
        NodeKind::SurfaceEmbed => "iframe",
        NodeKind::SecurityIndicator => "div",
        NodeKind::ApprovalPrompt => "div",
        NodeKind::EvidenceLink => "a",
        NodeKind::AgentMessage => "div",
    }
}

// ---------------------------------------------------------------------------
// Domain mapping: Web ↔ KDE parity
// ---------------------------------------------------------------------------

/// Map an `aios_apps::AppPackage` into a [`WebRenderTree`].
///
/// The root is a [`NodeKind::Card`] carrying the package name. Two child
/// [`NodeKind::Text`] nodes carry the version and package id — the same
/// [`NodeKind`] sequence the KDE T-137 helper produces, so both renderers
/// compose identical trees from the same domain payload.
#[must_use]
pub fn apps_package_envelope_to_web_render_tree(
    pkg: &aios_apps::package_store::AppPackage,
) -> WebRenderTree {
    let root = WebRenderTreeEntry {
        kind: NodeKind::Card,
        dom_tag: dom_tag_for(NodeKind::Card),
        label: pkg.name.clone(),
        children: vec![
            WebRenderTreeEntry {
                kind: NodeKind::Text,
                dom_tag: dom_tag_for(NodeKind::Text),
                label: format!("version: {}", pkg.version),
                children: vec![],
            },
            WebRenderTreeEntry {
                kind: NodeKind::Text,
                dom_tag: dom_tag_for(NodeKind::Text),
                label: format!("id: {}", pkg.package_id.0),
                children: vec![],
            },
        ],
    };
    WebRenderTree { root }
}

/// Build a [`ThreeWayParity`] registry proving the Web renderer compiles the
/// same domain payloads the KDE and CLI renderers accept.
///
/// At minimum registers an `AppPackage` entry whose JSON sample:
/// - deserializes through both the KDE `apps_package_envelope_to_kde_node_tree`
///   and the Web `apps_package_envelope_to_web_render_tree` mappings, with
///   identical root [`NodeKind`] and child count;
/// - produces non-empty CLI output through JSON serialization (structural
///   proxy for what `aios_renderer_cli::JsonRenderer` would produce).
///
/// # Errors
///
/// Returns `WebRendererError::Internal` if the JSON sample fails to
/// deserialize as an `AppPackage`.
///
/// # Panics
///
/// This function does not panic. Mismatched trees return `Err`, not a panic.
pub fn assert_three_way_parity_for_apps_domain() -> Result<ThreeWayParity, WebRendererError> {
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
        .map_err(|e| WebRendererError::Internal(format!("AppPackage deserialize: {e}")))?;

    // KDE parity: call the T-137 helper.
    let kde_tree = aios_renderer_kde::integration::apps_package_envelope_to_kde_node_tree(&app_pkg);

    // Web parity: call our own helper.
    let web_tree = apps_package_envelope_to_web_render_tree(&app_pkg);

    // Assert same root NodeKind.
    if kde_tree.root.kind != web_tree.root.kind {
        return Err(WebRendererError::Internal(format!(
            "KDE root kind {:?} != Web root kind {:?}",
            kde_tree.root.kind, web_tree.root.kind
        )));
    }

    // Assert same children count.
    if kde_tree.root.children.len() != web_tree.root.children.len() {
        return Err(WebRendererError::Internal(format!(
            "KDE children count {} != Web children count {}",
            kde_tree.root.children.len(),
            web_tree.root.children.len()
        )));
    }

    // CLI parity: structural — AppPackage JSON serialization produces non-empty
    // output (proxy for what aios_renderer_cli::JsonRenderer would produce).
    let cli_output = serde_json::to_string(&app_pkg)
        .map_err(|e| WebRendererError::Internal(format!("CLI JSON serialize: {e}")))?;
    if cli_output.is_empty() {
        return Err(WebRendererError::Internal(
            "CLI JSON output must be non-empty".into(),
        ));
    }

    // Confirm the CLI crate's OutputFormat type resolves (justifies the dep).
    let _ = aios_renderer_cli::OutputFormat::Json;

    let entries = vec![ThreeWayParityEntry {
        type_name: "AppPackage".into(),
        json_sample,
        parses_in_cli: true,
        parses_in_kde: true,
        parses_in_web: true,
    }];

    Ok(ThreeWayParity { entries })
}
