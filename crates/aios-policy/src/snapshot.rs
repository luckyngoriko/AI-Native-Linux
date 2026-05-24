//! [`EnrichmentSnapshot`] — content-addressed S2.3 §8 / §13 snapshot of the
//! resource metadata the kernel reads at evaluation time.
//!
//! ## What this lands (T-024)
//!
//! The T-017 form was a one-field stub (`snapshot_id: String`) so callers
//! could pass a placeholder while the AIOS-FS read-path was still on the
//! drawing board. T-024 expands the struct to the **full §8 shape** —
//! object metadata (`privacy_class`, `policy_tags`, `kind`, `lifecycle_state`,
//! `created_by`), adapter-manifest enrichment (`risk_template`,
//! `default_sandbox_profile_id`), and the explicit `snapshot_id` field that
//! anchors the §13 determinism triple.
//!
//! ## What is still stubbed (M4 deferred)
//!
//! The AIOS-FS read-path itself is M4 scope. T-024 ships the typed shape
//! and the content-address computation; the **fields are caller-provided**.
//! The decision pipeline reads them through the same accessors the future
//! M4 enricher will populate; the gRPC adapter mints an empty snapshot
//! anchored on the envelope's request hash until M4 lands. This keeps the
//! determinism contract honest at the trait surface today — the snapshot is
//! real, content-addressed, and round-trippable — without faking a read-path
//! that does not exist.
//!
//! ## Determinism contract (S2.3 §13.1)
//!
//! Per S2.3 §13.1 the triple `(request_hash, bundle_version,
//! enrichment_snapshot_id)` must produce the same [`crate::PolicyDecision`].
//! The third component is computed from the snapshot via
//! [`EnrichmentSnapshot::compute_id`] using RFC-8785 JSON canonicalization
//! (same canonicaliser as S0.1 §8.5 request-hash) + BLAKE3 + lowercase-hex
//! truncation to 32 chars. Same `(object_metadata, adapter_metadata)` ⇒ same
//! id; any field flip ⇒ a different id (content-addressed by construction).
//!
//! ## Cache-key role (S2.3 §13.3)
//!
//! Per S2.3 §13.3 the canonical cache key is `(request_hash, bundle_version)`
//! — the snapshot id is **not** part of the key (TTL-bounded re-evaluation
//! lives at the §13.2 cache-invalidation policy, not at the key level).
//! Determinism is anchored on the full triple; cache hits are anchored on
//! the two-tuple. The asymmetry is constitutional and is what
//! [`crate::cache::CacheKey`] implements.

use blake3::Hasher;
use serde::{Deserialize, Serialize};

use crate::error::PolicyError;

/// Object-side enrichment fields (S2.3 §8 row 1 — AIOS-FS `ReadObject`,
/// SNAPSHOT consistency per S1.3 §11).
///
/// All fields are `Option<...>` because the snapshot is only populated when
/// the action references an object target; system-level actions (e.g.
/// `service.restart`) have no object enrichment. Empty is a valid state and
/// produces a stable `compute_id` value.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObjectEnrichment {
    /// `request.target` object's privacy classification — `PUBLIC`,
    /// `INTERNAL`, `CONFIDENTIAL`, `RESTRICTED`, `RECOVERY` per S1.3.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub privacy_class: Option<String>,

    /// Bundle-author-readable tag set attached to the object (S1.3
    /// `policy_tags`). Ordered to keep the canonical JCS bytes stable.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub policy_tags: Vec<String>,

    /// Object kind (e.g. `file`, `note`, `secret-binding`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,

    /// AIOS-FS lifecycle state — `ACTIVE`, `ARCHIVED`, `RETIRED`, etc.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lifecycle_state: Option<String>,

    /// Canonical subject id of the original creator.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
}

/// Adapter-manifest enrichment fields (S2.3 §8 row 2 — L3 adapter manifest).
///
/// Populated by the kernel at evaluation time from the adapter-family
/// declaration. The kernel does not invent these values; the adapter
/// manifest is the single source of truth.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdapterEnrichment {
    /// Adapter-declared `risk_template` (S0.1 §3 / S2.3 §8 row 2). Free-form
    /// label that bundle rules can match against via the §9.2 vocabulary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub risk_template: Option<String>,

    /// Adapter-declared `default_sandbox_profile_id` (S2.3 §8 row 2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_sandbox_profile_id: Option<String>,
}

/// Resource-enrichment snapshot — S2.3 §8 / §13.
///
/// The third component of the §13.1 determinism triple. Constructed by the
/// kernel at evaluation time from a SNAPSHOT-consistent AIOS-FS read
/// (S1.3 §11) + the adapter manifest; T-024 ships the typed shape, M4 wires
/// the AIOS-FS read-path.
///
/// ## ID assembly
///
/// `snapshot_id` is the content-address of `(object, adapter)` plus any
/// extra fields a future revision adds. Always recomputable via
/// [`Self::compute_id`]; the field is held explicitly so callers that
/// receive a snapshot from the wire don't have to recompute on every read.
///
/// ## Default
///
/// `Default` produces an empty snapshot with `snapshot_id == ""`. Callers
/// must call [`Self::recompute_id`] (or construct via [`Self::with_fields`])
/// to seal the id. The empty `snapshot_id` is a sentinel — `compute_id` on
/// an empty snapshot is a fixed, well-known string the audit tooling can
/// recognise.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnrichmentSnapshot {
    /// Stable id for the snapshot (S2.3 §8 / §13).
    ///
    /// Per S2.3 §13.1 the triple
    /// `(request_hash, bundle_version, enrichment_snapshot_id)` must produce
    /// a deterministic decision; this id is the third component.
    /// Convention: `"polb_snap_" + hex_lower(BLAKE3(JCS(self)))[:32]`.
    pub snapshot_id: String,

    /// AIOS-FS object-side metadata (S2.3 §8 row 1).
    #[serde(default, skip_serializing_if = "is_default_object")]
    pub object: ObjectEnrichment,

    /// L3 adapter-manifest enrichment (S2.3 §8 row 2).
    #[serde(default, skip_serializing_if = "is_default_adapter")]
    pub adapter: AdapterEnrichment,
}

fn is_default_object(o: &ObjectEnrichment) -> bool {
    *o == ObjectEnrichment::default()
}

fn is_default_adapter(a: &AdapterEnrichment) -> bool {
    *a == AdapterEnrichment::default()
}

impl EnrichmentSnapshot {
    /// Canonical prefix on the content-addressed snapshot id.
    ///
    /// `"polb_snap_"` is the documented anchor; the audit tooling recognises
    /// it as a stable, non-routable resource-name space.
    pub const SNAPSHOT_ID_PREFIX: &'static str = "polb_snap_";

    /// Length (in lowercase-hex chars) of the BLAKE3-derived id suffix
    /// (S2.3 §12.2 mirrors this convention; we reuse the same truncation
    /// width for cross-rev consistency).
    pub const ID_HEX_LEN: usize = 32;

    /// Construct an [`EnrichmentSnapshot`] from `(object, adapter)` and seal
    /// the [`Self::snapshot_id`] via [`Self::compute_id`].
    ///
    /// # Errors
    ///
    /// Returns [`PolicyError::InvalidPolicyBundle`] when the JCS canonical
    /// serialiser fails — this is an unreachable defensive path because
    /// every field is a serde-derived primitive, but the typed `Result`
    /// keeps the contract honest for forward compatibility.
    pub fn with_fields(
        object: ObjectEnrichment,
        adapter: AdapterEnrichment,
    ) -> Result<Self, PolicyError> {
        let mut s = Self {
            snapshot_id: String::new(),
            object,
            adapter,
        };
        s.snapshot_id = s.compute_id()?;
        Ok(s)
    }

    /// Compute the content-addressed snapshot id over `(object, adapter)`.
    ///
    /// Algorithm (matches S0.1 §8.5 request-hash convention + S2.3 §12.2
    /// bundle-version convention):
    ///
    /// 1. Construct a transient `SnapshotIdView` (object + adapter only —
    ///    the snapshot id itself is excluded from its own input).
    /// 2. RFC-8785 JCS-canonicalise via [`serde_jcs::to_vec`].
    /// 3. BLAKE3 the canonical bytes.
    /// 4. Lowercase-hex the digest and truncate to [`Self::ID_HEX_LEN`].
    /// 5. Prefix with [`Self::SNAPSHOT_ID_PREFIX`].
    ///
    /// Determinism: JCS guarantees byte-equal canonical output across
    /// processes and architectures (§13.1 hard contract).
    ///
    /// # Errors
    ///
    /// Returns [`PolicyError::InvalidPolicyBundle`] if the JCS canonicaliser
    /// fails. The fail is wrapped behind a defensive code path because the
    /// transient view holds only serde-primitive fields; production should
    /// never see this error.
    pub fn compute_id(&self) -> Result<String, PolicyError> {
        #[derive(Serialize)]
        struct SnapshotIdView<'a> {
            object: &'a ObjectEnrichment,
            adapter: &'a AdapterEnrichment,
        }
        let view = SnapshotIdView {
            object: &self.object,
            adapter: &self.adapter,
        };
        let canonical = serde_jcs::to_vec(&view).map_err(|e| {
            PolicyError::InvalidPolicyBundle(format!("enrichment snapshot canonicalize: {e}"))
        })?;
        let mut hasher = Hasher::new();
        hasher.update(&canonical);
        let digest = hasher.finalize();
        let hex_full = digest.to_hex();
        let hex_str = hex_full.as_str();
        let truncated = &hex_str[..Self::ID_HEX_LEN.min(hex_str.len())];
        Ok(format!("{}{truncated}", Self::SNAPSHOT_ID_PREFIX))
    }

    /// Recompute and replace [`Self::snapshot_id`] from the current fields.
    ///
    /// Use after mutating `object` / `adapter` to keep the id consistent
    /// with the content. The fluent / typed alternative is
    /// [`Self::with_fields`] which seals the id at construction time.
    ///
    /// # Errors
    ///
    /// Same as [`Self::compute_id`].
    pub fn recompute_id(&mut self) -> Result<(), PolicyError> {
        self.snapshot_id = self.compute_id()?;
        Ok(())
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    reason = "test code; panic-on-failure is the idiomatic test signal"
)]
mod tests {
    use super::*;

    #[test]
    fn empty_snapshot_has_stable_compute_id() {
        let s1 = EnrichmentSnapshot::default();
        let s2 = EnrichmentSnapshot::default();
        let id1 = s1.compute_id().unwrap();
        let id2 = s2.compute_id().unwrap();
        assert_eq!(id1, id2);
        assert!(id1.starts_with(EnrichmentSnapshot::SNAPSHOT_ID_PREFIX));
        assert_eq!(
            id1.len(),
            EnrichmentSnapshot::SNAPSHOT_ID_PREFIX.len() + EnrichmentSnapshot::ID_HEX_LEN
        );
    }

    #[test]
    fn changing_object_field_changes_compute_id() {
        let a = EnrichmentSnapshot::with_fields(
            ObjectEnrichment {
                privacy_class: Some("PUBLIC".into()),
                ..Default::default()
            },
            AdapterEnrichment::default(),
        )
        .unwrap();
        let b = EnrichmentSnapshot::with_fields(
            ObjectEnrichment {
                privacy_class: Some("CONFIDENTIAL".into()),
                ..Default::default()
            },
            AdapterEnrichment::default(),
        )
        .unwrap();
        assert_ne!(a.snapshot_id, b.snapshot_id);
    }

    #[test]
    fn changing_adapter_field_changes_compute_id() {
        let a = EnrichmentSnapshot::with_fields(
            ObjectEnrichment::default(),
            AdapterEnrichment {
                risk_template: Some("low".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let b = EnrichmentSnapshot::with_fields(
            ObjectEnrichment::default(),
            AdapterEnrichment {
                risk_template: Some("high".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_ne!(a.snapshot_id, b.snapshot_id);
    }

    #[test]
    fn recompute_id_is_idempotent_when_fields_unchanged() {
        let mut s = EnrichmentSnapshot::with_fields(
            ObjectEnrichment {
                kind: Some("note".into()),
                ..Default::default()
            },
            AdapterEnrichment::default(),
        )
        .unwrap();
        let id1 = s.snapshot_id.clone();
        s.recompute_id().unwrap();
        assert_eq!(s.snapshot_id, id1);
    }

    #[test]
    fn snapshot_id_excludes_itself_from_input() {
        // Setting an arbitrary snapshot_id field then recomputing must
        // produce the same id as constructing fresh — the id is content
        // -addressed over (object, adapter) only.
        let fresh = EnrichmentSnapshot::with_fields(
            ObjectEnrichment {
                privacy_class: Some("INTERNAL".into()),
                ..Default::default()
            },
            AdapterEnrichment::default(),
        )
        .unwrap();
        let mut tainted = EnrichmentSnapshot {
            snapshot_id: "garbage".into(),
            object: ObjectEnrichment {
                privacy_class: Some("INTERNAL".into()),
                ..Default::default()
            },
            adapter: AdapterEnrichment::default(),
        };
        tainted.recompute_id().unwrap();
        assert_eq!(fresh.snapshot_id, tainted.snapshot_id);
    }
}
