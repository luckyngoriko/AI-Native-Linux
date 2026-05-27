//! Chrome shadow-root integrity monitoring (S7.5 INV I10).
//!
//! The AIOS chrome overlay renders inside a closed shadow root. The
//! `ChromeIntegrityMonitor` verifies Ed25519-signed subtree root hashes
//! from a trusted tree-signing authority, and detects extension interference
//! when an observed DOM root hash is not in the signed registry.

use chrono::{DateTime, Utc};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::error::WebRendererError;
use crate::evidence::WebEvidenceEmitter;

/// Outcome of a chrome shadow-root integrity check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IntegrityCheckOutcome {
    /// The observed hash was found in the signed registry — integrity holds.
    Ok,
    /// An untrusted browser extension interfered with the chrome shadow root.
    ExtensionInterferenceDetected {
        /// Classification of the detected mutation.
        mutation_kind: String,
    },
}

/// A record of a single integrity check for the audit history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntegrityCheckRecord {
    /// When the check was performed.
    pub at: DateTime<Utc>,
    /// The root hash that was observed from the DOM.
    pub root_hash: String,
    /// The outcome of the check.
    pub outcome: IntegrityCheckOutcome,
}

/// A signed fragment of the chrome shadow-root tree.
///
/// The tree-signing authority computes a canonicalized subtree hash and signs
/// it with its Ed25519 key. The monitor admits only fragments whose signature
/// verifies against the configured authority key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChromeTreeFragment {
    /// BLAKE3 hash of the canonicalized subtree (hex-encoded).
    pub root_hash: String,
    /// Ed25519 signature over `root_hash` bytes by the tree-signing authority.
    pub signature: Vec<u8>,
    /// When the fragment was signed.
    pub signed_at: DateTime<Utc>,
}

/// Chrome shadow-root integrity monitor (INV I10).
///
/// Maintains a registry of signed subtree root hashes and checks observed
/// hashes against it. Records every check outcome for forensic audit.
pub struct ChromeIntegrityMonitor {
    /// The trusted tree-signing authority public key.
    tree_signing_authority: VerifyingKey,
    /// The set of root hashes admitted via verified signed fragments.
    signed_root_hashes: RwLock<HashSet<String>>,
    /// Full history of every integrity check performed.
    integrity_checks: RwLock<Vec<IntegrityCheckRecord>>,
    /// Optional evidence emitter for extension interference events.
    evidence_emitter: Option<Arc<dyn WebEvidenceEmitter>>,
}

impl ChromeIntegrityMonitor {
    /// Create a new integrity monitor with the given tree-signing authority key.
    #[must_use]
    pub fn new(tree_signing_authority: VerifyingKey) -> Self {
        Self {
            tree_signing_authority,
            signed_root_hashes: RwLock::new(HashSet::new()),
            integrity_checks: RwLock::new(Vec::new()),
            evidence_emitter: None,
        }
    }

    /// Attach an optional evidence emitter for extension interference events.
    #[must_use]
    pub fn with_evidence_emitter(mut self, emitter: Arc<dyn WebEvidenceEmitter>) -> Self {
        self.evidence_emitter = Some(emitter);
        self
    }

    /// Admit a signed chrome tree fragment into the registry.
    ///
    /// Verifies the Ed25519 signature over the `root_hash` bytes against the
    /// tree-signing authority key. On success, stores the root hash in the
    /// signed registry.
    ///
    /// # Errors
    ///
    /// Returns `ChromeShadowRootIntegrityFailed` if the signature verification
    /// fails.
    pub async fn admit_signed_fragment(
        &self,
        fragment: ChromeTreeFragment,
    ) -> Result<(), WebRendererError> {
        let sig = Signature::from_slice(&fragment.signature).map_err(|_| {
            WebRendererError::ChromeShadowRootIntegrityFailed {
                reason: "invalid Ed25519 signature bytes".to_string(),
            }
        })?;
        let hash_bytes = fragment.root_hash.as_bytes();
        self.tree_signing_authority
            .verify(hash_bytes, &sig)
            .map_err(|_| WebRendererError::ChromeShadowRootIntegrityFailed {
                reason: "signature verification failed for root hash".to_string(),
            })?;
        self.signed_root_hashes
            .write()
            .await
            .insert(fragment.root_hash);
        Ok(())
    }

    /// Check an observed hash against the signed registry.
    ///
    /// If the hash is in the signed registry, records `IntegrityCheckOutcome::Ok`.
    /// Otherwise, records `ExtensionInterferenceDetected` with the given mutation
    /// kind and returns an error.
    ///
    /// # Errors
    ///
    /// Returns `ExtensionInterferenceDetected` if the observed hash is not in
    /// the signed registry.
    pub async fn check_observed_hash(&self, observed_hash: &str) -> Result<(), WebRendererError> {
        let guard = self.signed_root_hashes.read().await;
        let found = guard.contains(observed_hash);
        drop(guard);

        let now = Utc::now();
        let (outcome, result) = if found {
            (IntegrityCheckOutcome::Ok, Ok(()))
        } else {
            let outcome = IntegrityCheckOutcome::ExtensionInterferenceDetected {
                mutation_kind: "unknown-subtree".to_string(),
            };
            let err = Err(WebRendererError::ExtensionInterferenceDetected(
                "hash not in signed registry".to_string(),
            ));
            (outcome, err)
        };

        let record = IntegrityCheckRecord {
            at: now,
            root_hash: observed_hash.to_string(),
            outcome,
        };
        let mut checks = self.integrity_checks.write().await;
        checks.push(record);

        result
    }

    /// Return the full history of integrity checks.
    pub async fn history(&self) -> Vec<IntegrityCheckRecord> {
        let guard = self.integrity_checks.read().await;
        guard.clone()
    }
}
