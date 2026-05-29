//! Identifier newtypes per S11.1 vocabulary.
//!
//! Each identifier is a newtype wrapper around `String` so that the type system
//! prevents accidental interchange of e.g. a `PackageId` with a `PublisherId`.
//! All identifiers are serializable (transparent JSON string) and implement
//! `Debug`, `Clone`, `PartialEq`, `Eq`, and `Hash`.

use serde::{Deserialize, Serialize};

/// Package identifier — `pkg:<vendor>:<name>` per S11.1 §5.1.
///
/// Regex: `^pkg:[a-z0-9-]{1,64}:[a-z0-9-]{1,128}$`.
/// The `vendor` segment must equal the `vendor` segment of the package's
/// `publisher_root_id`; cross-checked at manifest-validation time (T-189).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PackageId(pub String);

/// Publisher identifier — `pub:<vendor>` per S11.1 §4.2.
///
/// Regex: `^pub:[a-z0-9-]{1,64}$`.
/// Must be present in the active publisher catalog with `retired_at` unset;
/// absent publisher → `TrustChainBroken` (T-188).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PublisherId(pub String);

/// Publisher root identifier — maps to the publisher catalog entry per S11.1 §4.2.
///
/// The publisher root is an Ed25519 public key signed by the AIOS root key.
/// This ID is the catalog lookup key used during trust-chain verification (T-188).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PublisherRootId(pub String);

/// Package signing key identifier — `pks:<vendor>:<role>` per S11.1 §4.3.
///
/// Regex: `^pks:[a-z0-9-]{1,64}:[a-z0-9-]{1,64}$`.
/// Must be present in the publisher's signing-key catalog with `revoked_at` unset;
/// revoked key → `RevokedKey` verification result (T-188).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PackageSigningKeyId(pub String);

/// Repository identifier — unique handle for a package source per S11.1 §3.2.
///
/// Derived deterministically from the repository's source URL prefix at fetch
/// time.  Used for repository-kind cross-check and mirror-blacklist tracking.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RepositoryId(pub String);

/// Manifest identifier — content-addressed per S11.1 §5.2.
///
/// Computed as `hex_lower(BLAKE3(JCS(manifest with signature cleared)))[:32]`.
/// This is the signing surface: the Ed25519 signature is computed over the
/// ASCII bytes of this lowercase-hex string.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ManifestId(pub String);
