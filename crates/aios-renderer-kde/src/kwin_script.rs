//! [`KwinScriptLoader`] enforces INV I8 — `KWin` scripts loaded by the AIOS
//! renderer MUST reside under `/aios/system/renderers/kde/kwin-scripts/`.
//!
//! Scripts must also match their declared BLAKE3 hash AND carry a valid
//! Ed25519 signature from a trusted authority (S7.4 §3.1).
//! System-installed paths (`/usr/share/kwin/scripts`) and user-home paths
//! (`~/.local/share/kwin/scripts`) are blacklisted at load time.

// Methods are declared `async` per the S7.4 spec signature; gRPC integration
// lands in T-134. Until then, the lock operations are synchronous.
#![allow(clippy::unused_async)]

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::RwLock;

use chrono::{DateTime, Utc};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};

use crate::error::KdeRendererError;

/// Default allowed root for `KWin` scripts per INV I8.
pub const DEFAULT_ALLOWED_ROOT: &str = "/aios/system/renderers/kde/kwin-scripts";

/// Blacklisted paths that are never valid for AIOS-loaded `KWin` scripts.
const BLACKLISTED_PREFIXES: [&str; 2] = ["/usr/share/kwin/scripts", "~/.local/share/kwin/scripts"];

/// A `KWin` script submitted for loading by the renderer.
///
/// Carries its QML/JS source, a declared BLAKE3 hash of that source, and an
/// Ed25519 signature over the hash bytes from the identified authority.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KwinScript {
    /// Unique script identifier (e.g. `"aios-fullscreen-block"`).
    pub id: String,
    /// Canonical filesystem path the script was read from. Must start with
    /// [`DEFAULT_ALLOWED_ROOT`] and must not be a blacklisted system/user path.
    pub canonical_path: String,
    /// QML/JS source text of the script.
    pub source: String,
    /// BLAKE3 hex digest of `source` (all-lowercase hex).
    pub blake3_hash: String,
    /// Ed25519 signature over the BLAKE3 hash bytes (32 bytes → 64-byte sig).
    pub signature: Vec<u8>,
    /// Fingerprint (label) of the signing authority. The loader's trust store
    /// must contain a verifying key registered under this label.
    pub signer_key_fingerprint: String,
}

/// Record of a successfully verified `KWin` script held in the loader's
/// registry.
#[derive(Debug, Clone)]
pub struct KwinScriptRecord {
    /// The verified script.
    pub script: KwinScript,
    /// Wall-clock timestamp of successful verification.
    pub verified_at: DateTime<Utc>,
}

/// Stateless + thread-safe loader that enforces INV I8 for every `KWin` script
/// loaded by the AIOS renderer.
///
/// Constructed with an allowed root directory and an (initially empty) trust
/// store. Before calling [`Self::load_script`], register at least one authority
/// via [`Self::register_authority`].
///
/// The loader holds a [`RwLock`]-protected registry of successfully loaded
/// scripts. Evidence emission is deferred to T-135.
#[derive(Debug)]
pub struct KwinScriptLoader {
    /// Only scripts whose `canonical_path` starts with this prefix are accepted.
    allowed_root: PathBuf,
    /// Fingerprint label → Ed25519 verifying key.
    trusted_authorities: HashMap<String, VerifyingKey>,
    /// Script id → verified record.
    loaded: RwLock<HashMap<String, KwinScriptRecord>>,
}

impl KwinScriptLoader {
    /// Create a new loader with the given allowed root directory.
    ///
    /// To use the INV I8 default (`/aios/system/renderers/kde/kwin-scripts`),
    /// pass [`DEFAULT_ALLOWED_ROOT`].
    pub fn new(allowed_root: impl Into<PathBuf>) -> Self {
        Self {
            allowed_root: allowed_root.into(),
            trusted_authorities: HashMap::new(),
            loaded: RwLock::new(HashMap::new()),
        }
    }

    /// Register a trusted signing authority.
    ///
    /// The `fingerprint` is the label used to look up the key when a
    /// [`KwinScript::signer_key_fingerprint`] is presented during loading.
    pub fn register_authority(&mut self, fingerprint: &str, key: VerifyingKey) {
        self.trusted_authorities
            .insert(fingerprint.to_string(), key);
    }

    /// Verify and register a `KWin` script.
    ///
    /// Runs four ordered checks; the first failure rejects the script
    /// (fail-closed):
    ///
    /// 1. **Path check** — `canonical_path` must start with `allowed_root`.
    /// 2. **Path blacklist** — system and user-home `KWin` paths are blocked.
    /// 3. **Hash check** — BLAKE3 of `source` must match `blake3_hash`.
    /// 4. **Signature check** — Ed25519 signature over the BLAKE3 hash bytes
    ///    must verify against the registered authority's verifying key.
    ///
    /// Scripts that pass all checks are inserted into the loaded registry.
    ///
    /// # Errors
    ///
    /// Returns [`KdeRendererError::KwinScriptVerificationFailed`] on any
    /// verification failure.
    pub async fn load_script(&self, script: KwinScript) -> Result<(), KdeRendererError> {
        let script_id = script.id.clone();

        // 1. Path must reside under allowed_root.
        let canonical = PathBuf::from(&script.canonical_path);
        if !canonical.starts_with(&self.allowed_root) {
            return Err(KdeRendererError::KwinScriptVerificationFailed {
                script_id,
                reason: "path outside allowed root".to_string(),
            });
        }

        // 2. Blacklist system + user-home KWin script directories.
        for blocked in &BLACKLISTED_PREFIXES {
            if script.canonical_path.contains(blocked) {
                return Err(KdeRendererError::KwinScriptVerificationFailed {
                    script_id,
                    reason: "system/user-installed script path blocked".to_string(),
                });
            }
        }

        // 3. BLAKE3 hash of source must match the declared hash.
        let computed = blake3::hash(script.source.as_bytes());
        let computed_hex = computed.to_hex().to_string();
        if computed_hex != script.blake3_hash {
            return Err(KdeRendererError::KwinScriptVerificationFailed {
                script_id,
                reason: "blake3 mismatch".to_string(),
            });
        }

        // 4. Ed25519 signature over the BLAKE3 hash bytes must verify.
        let vk = self
            .trusted_authorities
            .get(&script.signer_key_fingerprint)
            .ok_or_else(|| KdeRendererError::KwinScriptVerificationFailed {
                script_id: script_id.clone(),
                reason: "unknown authority".to_string(),
            })?;

        let sig_bytes: [u8; 64] = script.signature.as_slice().try_into().map_err(|_| {
            KdeRendererError::KwinScriptVerificationFailed {
                script_id: script_id.clone(),
                reason: "invalid signature length".to_string(),
            }
        })?;

        let signature = Signature::from_bytes(&sig_bytes);
        vk.verify(computed.as_bytes(), &signature).map_err(|_| {
            KdeRendererError::KwinScriptVerificationFailed {
                script_id: script_id.clone(),
                reason: "invalid ed25519 signature".to_string(),
            }
        })?;

        // All checks passed — register.
        let record = KwinScriptRecord {
            script,
            verified_at: Utc::now(),
        };
        self.loaded
            .write()
            .map_err(|e| KdeRendererError::Internal(format!("lock poisoned: {e}")))?
            .insert(script_id, record);

        Ok(())
    }

    /// Return the ids of all currently loaded scripts.
    pub async fn list_loaded(&self) -> Vec<String> {
        self.loaded
            .read()
            .map(|guard| guard.keys().cloned().collect())
            .unwrap_or_default()
    }

    /// Remove a previously loaded script from the registry.
    ///
    /// # Errors
    ///
    /// Returns [`KdeRendererError::KwinScriptVerificationFailed`] when
    /// `script_id` is not present in the registry.
    pub async fn unload_script(&self, script_id: &str) -> Result<(), KdeRendererError> {
        let removed = self
            .loaded
            .write()
            .map_err(|e| KdeRendererError::Internal(format!("lock poisoned: {e}")))?
            .remove(script_id);
        if removed.is_none() {
            return Err(KdeRendererError::KwinScriptVerificationFailed {
                script_id: script_id.to_string(),
                reason: "script not found".to_string(),
            });
        }
        Ok(())
    }
}

impl Default for KwinScriptLoader {
    fn default() -> Self {
        Self::new(DEFAULT_ALLOWED_ROOT)
    }
}
