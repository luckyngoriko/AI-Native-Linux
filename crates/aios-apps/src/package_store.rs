//! S12.2 §4 — PackageStore async trait with Ed25519 + BLAKE3 integrity.
//!
//! The `PackageStore` trait defines the async contract for registering,
//! looking up, and listing package objects. `InMemoryPackageStore` is the
//! in-memory test harness backed by `RwLock<HashMap<...>>`.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::error::AppsError;
use crate::package::PackageId;

// ---------------------------------------------------------------------------
// AppPackage — the package object carrying manifest, signature, and hash
// ---------------------------------------------------------------------------

/// A package object with its manifest bytes, Ed25519 signature, and BLAKE3
/// content hash. The signature is over `manifest_bytes`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppPackage {
    /// Canonical package identifier (`pkg_<ulid26>`).
    pub package_id: PackageId,
    /// Human-readable package name (e.g. "firefox").
    pub name: String,
    /// Semver version string declared by the publisher.
    pub version: String,
    /// Raw manifest bytes — the exact input to both Ed25519 signing and
    /// BLAKE3 content hashing.
    pub manifest_bytes: Vec<u8>,
    /// BLAKE3 hex digest of `manifest_bytes`.
    pub content_hash_blake3: String,
    /// Ed25519 signature over `manifest_bytes`.
    pub ed25519_signature: Vec<u8>,
    /// Ed25519 public key of the signer (32 bytes).
    pub signer_public_key: Vec<u8>,
    /// When this package was registered into the store.
    pub registered_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// PackageStore trait
// ---------------------------------------------------------------------------

/// S12.2 §4 — async contract for package storage and verification.
///
/// Implementations must enforce Ed25519 authority checks and BLAKE3
/// content-hash integrity on every `register_package` call.
#[async_trait]
pub trait PackageStore: Send + Sync {
    /// Register a package after verifying its Ed25519 signature against a
    /// trusted authority and confirming the BLAKE3 content hash matches.
    ///
    /// # Errors
    ///
    /// Returns `ValidationFailed` when the signature is invalid, the
    /// signing authority is not trusted, or the content hash mismatches.
    async fn register_package(&self, package: AppPackage) -> Result<PackageId, AppsError>;

    /// Look up a package by its `PackageId`.
    ///
    /// # Errors
    ///
    /// Returns `PackageNotFound` when no package exists for the given id.
    async fn lookup_package(&self, id: &PackageId) -> Result<AppPackage, AppsError>;

    /// Return every registered package (unordered).
    async fn list_packages(&self) -> Result<Vec<AppPackage>, AppsError>;

    /// Verify the Ed25519 signature on a package's manifest bytes against
    /// the signer's public key carried in the package.
    ///
    /// Returns `true` when the signature is cryptographically valid.
    async fn verify_signature(&self, package: &AppPackage) -> Result<bool, AppsError>;

    /// Compute the deterministic BLAKE3 content hash over `manifest_bytes`.
    async fn compute_content_hash(&self, manifest_bytes: &[u8]) -> Result<String, AppsError>;

    /// Return every `PackageId` registered under the given package name, in
    /// registration order (oldest first).
    async fn list_versions_of(&self, name: &str) -> Result<Vec<PackageId>, AppsError>;
}

// ---------------------------------------------------------------------------
// InMemoryPackageStore
// ---------------------------------------------------------------------------

/// In-memory `PackageStore` harness backed by `RwLock<HashMap<...>>`.
///
/// Trusted authorities are a static map from Ed25519 public key bytes to a
/// human-readable authority name. Only packages signed by a key present in
/// this map pass `register_package`.
#[derive(Clone, Debug)]
pub struct InMemoryPackageStore {
    packages: Arc<RwLock<HashMap<PackageId, AppPackage>>>,
    name_index: Arc<RwLock<HashMap<String, Vec<PackageId>>>>,
    trusted_authorities: HashMap<Vec<u8>, String>,
}

impl InMemoryPackageStore {
    /// Create an empty store with the given trusted authority map.
    ///
    /// The map keys are raw Ed25519 public key bytes (32 bytes each); the
    /// values are human-readable authority names for diagnostics.
    #[must_use]
    pub fn new(trusted_authorities: HashMap<Vec<u8>, String>) -> Self {
        Self {
            packages: Arc::new(RwLock::new(HashMap::new())),
            name_index: Arc::new(RwLock::new(HashMap::new())),
            trusted_authorities,
        }
    }

    /// Return the number of registered packages (test seam).
    #[allow(dead_code)]
    pub async fn package_count(&self) -> usize {
        self.packages.read().await.len()
    }

    /// Return the number of distinct names tracked (test seam).
    #[allow(dead_code)]
    pub async fn name_count(&self) -> usize {
        self.name_index.read().await.len()
    }
}

#[async_trait]
impl PackageStore for InMemoryPackageStore {
    async fn register_package(&self, package: AppPackage) -> Result<PackageId, AppsError> {
        // 1. Verify the signing authority is trusted.
        if !self
            .trusted_authorities
            .contains_key(&package.signer_public_key)
        {
            return Err(AppsError::ValidationFailed(
                "manifest unknown authority: signer public key not in trusted set".into(),
            ));
        }

        // 2. Verify the Ed25519 signature.
        let valid = self.verify_signature(&package).await?;
        if !valid {
            return Err(AppsError::ValidationFailed(
                "manifest signature invalid: Ed25519 verification failed".into(),
            ));
        }

        // 3. Verify the BLAKE3 content hash.
        let computed = self.compute_content_hash(&package.manifest_bytes).await?;
        if computed != package.content_hash_blake3 {
            return Err(AppsError::ValidationFailed(format!(
                "content hash mismatch: expected {}, computed {}",
                package.content_hash_blake3, computed,
            )));
        }

        let id = package.package_id.clone();
        let name = package.name.clone();

        self.packages.write().await.insert(id.clone(), package);
        self.name_index
            .write()
            .await
            .entry(name)
            .or_default()
            .push(id.clone());

        Ok(id)
    }

    async fn lookup_package(&self, id: &PackageId) -> Result<AppPackage, AppsError> {
        self.packages
            .read()
            .await
            .get(id)
            .cloned()
            .ok_or_else(|| AppsError::PackageNotFound(id.0.clone()))
    }

    async fn list_packages(&self) -> Result<Vec<AppPackage>, AppsError> {
        Ok(self.packages.read().await.values().cloned().collect())
    }

    async fn verify_signature(&self, package: &AppPackage) -> Result<bool, AppsError> {
        let pk_bytes: [u8; 32] = package
            .signer_public_key
            .as_slice()
            .try_into()
            .map_err(|_| {
                AppsError::ValidationFailed("invalid public key length: expected 32 bytes".into())
            })?;

        let verifying_key = VerifyingKey::from_bytes(&pk_bytes)
            .map_err(|_| AppsError::ValidationFailed("invalid Ed25519 public key".into()))?;

        let sig_bytes: [u8; 64] =
            package
                .ed25519_signature
                .as_slice()
                .try_into()
                .map_err(|_| {
                    AppsError::ValidationFailed(
                        "invalid signature length: expected 64 bytes".into(),
                    )
                })?;

        let signature = Signature::from_bytes(&sig_bytes);

        Ok(verifying_key
            .verify_strict(&package.manifest_bytes, &signature)
            .is_ok())
    }

    async fn compute_content_hash(&self, manifest_bytes: &[u8]) -> Result<String, AppsError> {
        Ok(blake3::hash(manifest_bytes).to_hex().to_string())
    }

    async fn list_versions_of(&self, name: &str) -> Result<Vec<PackageId>, AppsError> {
        Ok(self
            .name_index
            .read()
            .await
            .get(name)
            .cloned()
            .unwrap_or_default())
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Compute the BLAKE3 hex digest of `data` — free function usable from tests
/// and other modules without going through the async trait.
#[must_use]
pub fn blake3_hex(data: &[u8]) -> String {
    blake3::hash(data).to_hex().to_string()
}
