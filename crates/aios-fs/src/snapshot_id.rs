//! SNAPSHOT read identifier — S1.3 §11.

#![allow(
    clippy::module_name_repetitions,
    reason = "AIOS-FS public names mirror the spec vocabulary"
)]

use std::fmt;

use serde::{Deserialize, Serialize};

use aios_action::{blake3_truncated, jcs_canonicalize};

/// Content-addressed snapshot identifier: `"snap_" + 32 lowercase hex chars`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SnapshotId(
    /// Canonical `snap_<32-hex>` string.
    pub String,
);

impl SnapshotId {
    /// Canonical snapshot identifier prefix.
    pub const PREFIX: &'static str = "snap_";

    /// Compute a deterministic snapshot id from sorted object/pointer/version entries.
    ///
    /// S1.3 §11 defines the SNAPSHOT guarantee but is silent on the exact in-process
    /// field list for T-037. This baseline hashes three sorted sets:
    /// object-head entries, pointer-target entries, and version entries. The in-memory
    /// store feeds those entries as `(object_id/current_pointer_id)`,
    /// `(pointer_id/object_id/kind/current_version_id)`, and
    /// `(version_id/object_id/state/chunk_refs)` strings so equal visible FS state
    /// produces the same id and any visible pointer/version drift changes it.
    #[must_use]
    pub fn compute<Objects, ObjectEntry, Pointers, PointerEntry, Versions, VersionEntry>(
        object_ids_hashed: Objects,
        pointer_ids_hashed: Pointers,
        version_ids_hashed: Versions,
    ) -> Self
    where
        Objects: IntoIterator<Item = ObjectEntry>,
        ObjectEntry: AsRef<str>,
        Pointers: IntoIterator<Item = PointerEntry>,
        PointerEntry: AsRef<str>,
        Versions: IntoIterator<Item = VersionEntry>,
        VersionEntry: AsRef<str>,
    {
        #[derive(Serialize)]
        struct SnapshotPreimage {
            objects: Vec<String>,
            pointers: Vec<String>,
            versions: Vec<String>,
        }

        let preimage = SnapshotPreimage {
            objects: sorted_unique_strings(object_ids_hashed),
            pointers: sorted_unique_strings(pointer_ids_hashed),
            versions: sorted_unique_strings(version_ids_hashed),
        };

        let canonical = canonicalize_snapshot_preimage(&preimage);
        Self(format!(
            "{}{}",
            Self::PREFIX,
            blake3_truncated(canonical.as_bytes())
        ))
    }

    /// Borrow the canonical string form.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SnapshotId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for SnapshotId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

fn sorted_unique_strings<Entry, Iter>(entries: Iter) -> Vec<String>
where
    Entry: AsRef<str>,
    Iter: IntoIterator<Item = Entry>,
{
    let mut values: Vec<String> = entries
        .into_iter()
        .map(|entry| entry.as_ref().to_owned())
        .collect();
    values.sort_unstable();
    values.dedup();
    values
}

fn canonicalize_snapshot_preimage<T: Serialize>(preimage: &T) -> String {
    match jcs_canonicalize(preimage) {
        Ok(canonical) => canonical,
        Err(err) => {
            // The preimage is built from strings and vectors only, so this branch is
            // not expected. Keep the function total because `SnapshotId::compute`
            // intentionally has the simple infallible API requested by T-037.
            format!("{{\"canonicalization_error\":\"{err}\"}}")
        }
    }
}
