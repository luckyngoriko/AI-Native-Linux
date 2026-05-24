//! [`BundleLoader`] — parses, validates, and Ed25519-verifies a
//! [`crate::bundle::PolicyBundle`] (S2.3 §12).
//!
//! ## Pipeline
//!
//! `load_from_bytes(bytes)` runs four ordered checks; on the first failure the
//! loader rejects the whole bundle (fail-closed per S2.3 §12.3 / §19.1):
//!
//! 1. **JSON parse** — `serde_json::from_slice::<PolicyBundle>(bytes)`.
//!    Unknown fields, type mismatches, and missing required fields all fail
//!    here with [`PolicyError::InvalidPolicyBundle`].
//! 2. **Version pin** — when the loader was constructed with
//!    `expected_bundle_version`, the bundle's `bundle_version` must match;
//!    otherwise [`PolicyError::BundleVersionMismatch`].
//! 3. **Authority lookup** — the bundle's `signing_authority` must resolve to
//!    a verifying key in the trust store; otherwise
//!    [`PolicyError::BundleUnknownAuthority`].
//! 4. **Signature verify** — Ed25519 verification over
//!    [`PolicyBundle::canonical_signed_body_bytes`]; mismatch ⇒
//!    [`PolicyError::BundleSignatureInvalid`].
//! 5. **Per-rule condition parse** — every string in `rule.conditions` is fed
//!    through [`crate::conditions_parser::parse`]; first failure rejects the
//!    whole bundle with `InvalidPolicyBundle("rule <rule_id> condition: <err>")`
//!    per S2.3 §19.1 ("rules parse").
//!
//! Steps run in the listed order so the cheaper checks short-circuit before
//! the expensive ones (parse before verify before per-rule walk).
//!
//! ## Trust store
//!
//! The loader holds a `HashMap<String, VerifyingKey>` keyed by the
//! `signing_authority` label that bundles declare. This mirrors the S2.3 §12.3
//! trust chain (`AIOS root key → Publisher key → Policy bundle`) at the
//! publisher-key level — the AIOS-root endorsement is a future amendment that
//! will require an upstream signature over the publisher key entries.

use std::collections::HashMap;
use std::path::Path;

use ed25519_dalek::{Signature, Verifier, VerifyingKey};

use crate::bundle::{PolicyBundle, RuleEffect};
use crate::conditions_parser::parse as parse_condition;
use crate::error::PolicyError;

/// Stateless loader that converts signed bundle bytes into a verified
/// [`PolicyBundle`] (S2.3 §12 / §15 / §19.1).
///
/// Constructed once at engine startup with the operator-curated trust store
/// and (optionally) a pinned `bundle_version`. The loader holds no mutable
/// state and is `Send + Sync`; the same instance may be used from every gRPC
/// worker.
#[derive(Debug, Clone)]
pub struct BundleLoader {
    /// `signing_authority` → publisher verifying key, keyed by the same label
    /// the bundle carries in its `signing_authority` field.
    trusted_authorities: HashMap<String, VerifyingKey>,
    /// When `Some`, every load enforces that the bundle's `bundle_version`
    /// equals the pinned value (S2.3 §13.1 determinism anchor).
    expected_bundle_version: Option<String>,
}

impl BundleLoader {
    /// Constructs a loader bound to the supplied trust store and no version pin.
    ///
    /// Use [`Self::with_version_pin`] to add a version constraint, or
    /// [`Self::pin_version`] / [`Self::clear_version_pin`] to mutate it later.
    #[must_use]
    pub const fn new(trusted_authorities: HashMap<String, VerifyingKey>) -> Self {
        Self {
            trusted_authorities,
            expected_bundle_version: None,
        }
    }

    /// Constructs a loader bound to the trust store AND pinned to a specific
    /// `bundle_version`. Loads of any other version fail with
    /// [`PolicyError::BundleVersionMismatch`].
    #[must_use]
    pub fn with_version_pin(
        trusted_authorities: HashMap<String, VerifyingKey>,
        expected_bundle_version: impl Into<String>,
    ) -> Self {
        Self {
            trusted_authorities,
            expected_bundle_version: Some(expected_bundle_version.into()),
        }
    }

    /// Pins the loader to the supplied `bundle_version` for subsequent loads.
    pub fn pin_version(&mut self, expected: impl Into<String>) {
        self.expected_bundle_version = Some(expected.into());
    }

    /// Clears the version pin if any was set.
    pub fn clear_version_pin(&mut self) {
        self.expected_bundle_version = None;
    }

    /// Returns the currently pinned version, or `None` if unpinned.
    #[must_use]
    pub fn expected_bundle_version(&self) -> Option<&str> {
        self.expected_bundle_version.as_deref()
    }

    /// Returns `true` if the loader's trust store contains the named authority.
    #[must_use]
    pub fn trusts_authority(&self, name: &str) -> bool {
        self.trusted_authorities.contains_key(name)
    }

    /// Parses, verifies, and returns a [`PolicyBundle`] from raw JSON bytes.
    ///
    /// See the module-level docs for the ordered failure modes.
    ///
    /// # Errors
    ///
    /// - [`PolicyError::InvalidPolicyBundle`] — JSON malformed, schema invalid,
    ///   unknown fields, an unspecified `RuleEffect`, or a per-rule condition
    ///   string that fails to parse.
    /// - [`PolicyError::BundleVersionMismatch`] — pinned version did not match.
    /// - [`PolicyError::BundleUnknownAuthority`] — `signing_authority` is not
    ///   in the trust store.
    /// - [`PolicyError::BundleSignatureInvalid`] — signature decoded but did
    ///   not verify against the publisher key.
    pub fn load_from_bytes(&self, bytes: &[u8]) -> Result<PolicyBundle, PolicyError> {
        // 1. JSON parse — fail-closed on any malformed input.
        let bundle: PolicyBundle = serde_json::from_slice(bytes)
            .map_err(|e| PolicyError::InvalidPolicyBundle(format!("JSON deserialise: {e}")))?;

        // 2. Version pin (cheap string compare before signature math).
        if let Some(expected) = &self.expected_bundle_version {
            if &bundle.bundle_version != expected {
                return Err(PolicyError::BundleVersionMismatch {
                    expected: expected.clone(),
                    found: bundle.bundle_version,
                });
            }
        }

        // 3. Authority lookup.
        let verifying_key = self
            .trusted_authorities
            .get(&bundle.signing_authority)
            .ok_or_else(|| PolicyError::BundleUnknownAuthority(bundle.signing_authority.clone()))?;

        // 4. Signature verify.
        let sig_bytes: [u8; 64] = bundle
            .signature_ed25519
            .as_slice()
            .try_into()
            .map_err(|_| PolicyError::BundleSignatureInvalid)?;
        let signature = Signature::from_bytes(&sig_bytes);
        let body = bundle.canonical_signed_body_bytes()?;
        verifying_key
            .verify(&body, &signature)
            .map_err(|_| PolicyError::BundleSignatureInvalid)?;

        // 5. Per-rule condition parse + reject `RuleEffect::Unspecified`.
        for rule in &bundle.rules {
            if matches!(rule.effect, RuleEffect::Unspecified) {
                return Err(PolicyError::InvalidPolicyBundle(format!(
                    "rule {} effect: UNSPECIFIED is reserved for proto3 wire compatibility",
                    rule.rule_id
                )));
            }
            for cond in &rule.conditions {
                parse_condition(cond).map_err(|e| {
                    PolicyError::InvalidPolicyBundle(format!(
                        "rule {} condition: {e}",
                        rule.rule_id
                    ))
                })?;
            }
        }

        Ok(bundle)
    }

    /// Convenience wrapper that reads the file at `path` and delegates to
    /// [`Self::load_from_bytes`].
    ///
    /// # Errors
    ///
    /// Returns [`PolicyError::InvalidPolicyBundle`] when the file cannot be
    /// read (the I/O failure is wrapped with a `"file read: …"` prefix so the
    /// caller can distinguish disk errors from parse errors). All other
    /// failure modes are the same as [`Self::load_from_bytes`].
    pub fn load_from_file(&self, path: &Path) -> Result<PolicyBundle, PolicyError> {
        let bytes = std::fs::read(path).map_err(|e| {
            PolicyError::InvalidPolicyBundle(format!("file read {}: {e}", path.display()))
        })?;
        self.load_from_bytes(&bytes)
    }
}
