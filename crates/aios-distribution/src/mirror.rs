//! Mirror-semantic vocabulary per S11.1 §3.8.
//!
//! `MirrorSemantic` classifies the fetch source for a package.  The cardinal
//! rule (§10) is that mirrors **never re-sign** packages — they serve the
//! same signed bytes verbatim or fail.  Tampering is detected by the host-side
//! content-hash check before unpacking.

use serde::{Deserialize, Serialize};

/// Closed enum — 3 semantics per S11.1 §3.8.
///
/// | Variant  | S11.1 label | Re-signs? | Tampering detection           |
/// |----------|-------------|-----------|-------------------------------|
/// | `Origin` | `ORIGIN`    | N/A       | Signature chain only          |
/// | `Cached` | `CACHED`    | NEVER     | Host-side BLAKE3 hash check   |
/// | `Local`  | `LOCAL`     | NEVER     | Operator self-attests         |
///
/// Mirrors **never re-sign** packages. They serve the same signed bytes
/// verbatim or fail. Mirror tampering is detected by the host-side
/// content-hash check before unpacking (§5 step 5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MirrorSemantic {
    /// The publisher's authoritative server — canonical fetch target.
    Origin,
    /// Third-party mirror — serves same signed bytes; **cannot** re-sign or modify.
    Cached,
    /// Operator's own offline mirror (airgap installs) — same content-hash discipline.
    Local,
}
