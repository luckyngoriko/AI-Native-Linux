//! Mirror-semantic vocabulary per S11.1 §3.9.
//!
//! `MirrorSemantic` classifies the fetch source for a package.  The cardinal
//! rule (§10) is that mirrors **never re-sign** packages — they serve the
//! same signed bytes verbatim or fail.  Tampering is detected by the host-side
//! content-hash check before unpacking.

use serde::{Deserialize, Serialize};

/// Closed enum — 3 semantics per S11.1 §3.9.
///
/// | Variant               | S11.1 label | Re-signs? | Tampering detection        |
/// |-----------------------|-------------|-----------|----------------------------|
/// | `OriginAuthoritative` | `ORIGIN`    | N/A       | Signature chain only       |
/// | `MirrorPassthrough`   | `CACHED`    | NEVER     | Host-side BLAKE3 hash check|
/// | `MirrorCacheOnly`     | `LOCAL`     | NEVER     | Operator self-attests      |
///
/// Deviation: spec §3.8 uses `ORIGIN`, `CACHED`, `LOCAL`.  T-187 uses
/// task-authorised names that make the semantic explicit in the variant name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MirrorSemantic {
    /// The publisher's authoritative server — canonical fetch target.
    OriginAuthoritative,
    /// Third-party mirror — serves same signed bytes; **cannot** re-sign or modify.
    MirrorPassthrough,
    /// Operator's own offline mirror (airgap installs) — same content-hash discipline.
    MirrorCacheOnly,
}
