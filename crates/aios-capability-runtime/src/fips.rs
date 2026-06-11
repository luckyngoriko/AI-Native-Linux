//! FIPS 140-3 crypto boundary module for AI-OS.NET — CMVP-validated
//! cryptographic provider selection, compliance-sensitive operation routing,
//! and FIPS_STRICT overlay enforcement (S16.5).
#![allow(clippy::doc_markdown)]
#![allow(clippy::missing_const_for_fn)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::uninlined_format_args)]
#![allow(clippy::match_same_arms)]

use std::collections::HashSet;
use std::fmt;

// ---------------------------------------------------------------------------
// FipsMode — FIPS enforcement mode (Strict / Standard)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum FipsMode {
    /// FIPS 140-3 strict enforcement: only FIPS-approved algorithms,
    /// CMVP-validated provider required.
    Strict,
    /// Default cryptographic posture; no FIPS enforcement.
    #[default]
    Standard,
}

impl FipsMode {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Strict => "FIPS_STRICT",
            Self::Standard => "FIPS_STANDARD",
        }
    }

    /// Whether FIPS strict mode is active and enforcing.
    #[must_use]
    pub fn is_active(self) -> bool {
        matches!(self, Self::Strict)
    }
}

impl fmt::Display for FipsMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

// ---------------------------------------------------------------------------
// CryptoProvider — CMVP-validated cryptographic module
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CryptoProvider {
    pub name: String,
    pub certificate: String,
    pub validated: bool,
    pub certificate_url: Option<String>,
}

impl CryptoProvider {
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        certificate: impl Into<String>,
        validated: bool,
        certificate_url: Option<String>,
    ) -> Self {
        Self {
            name: name.into(),
            certificate: certificate.into(),
            validated,
            certificate_url,
        }
    }

    /// Whether this provider is CMVP-validated (INV-FIPS-002).
    #[must_use]
    pub fn is_validated(&self) -> bool {
        self.validated
    }
}

impl fmt::Display for CryptoProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} (cert #{}, {})",
            self.name,
            self.certificate,
            if self.validated { "VALIDATED" } else { "UNVALIDATED" }
        )
    }
}

// ---------------------------------------------------------------------------
// ComplianceOperation — cryptographic operation categories
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ComplianceOperation {
    Encrypt,
    Decrypt,
    Sign,
    Verify,
    Hash,
    Kdf,
}

impl ComplianceOperation {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Encrypt => "Encrypt",
            Self::Decrypt => "Decrypt",
            Self::Sign => "Sign",
            Self::Verify => "Verify",
            Self::Hash => "Hash",
            Self::Kdf => "KDF",
        }
    }

    #[must_use]
    pub fn approved_algorithms(self) -> &'static [&'static str] {
        match self {
            Self::Encrypt | Self::Decrypt => &[
                "AES-128-GCM", "AES-192-GCM", "AES-256-GCM",
                "AES-128-CBC", "AES-256-CBC", "AES-128-CTR", "AES-256-CTR",
                "ChaCha20-Poly1305",
            ],
            Self::Sign | Self::Verify => &[
                "RSA-2048-SHA256", "RSA-3072-SHA384", "RSA-4096-SHA512",
                "ECDSA-P256-SHA256", "ECDSA-P384-SHA384", "ECDSA-P521-SHA512",
                "Ed25519",
            ],
            Self::Hash => &[
                "SHA-256", "SHA-384", "SHA-512",
                "SHA3-256", "SHA3-384", "SHA3-512",
            ],
            Self::Kdf => &[
                "HKDF-SHA256", "HKDF-SHA384", "HKDF-SHA512",
                "PBKDF2-SHA256", "PBKDF2-SHA384",
            ],
        }
    }

    pub const COUNT: usize = 6;

    #[must_use]
    pub fn all() -> [Self; Self::COUNT] {
        [Self::Encrypt, Self::Decrypt, Self::Sign, Self::Verify, Self::Hash, Self::Kdf]
    }
}

impl fmt::Display for ComplianceOperation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

// ---------------------------------------------------------------------------
// FipsBoundary — crypto boundary with active provider and algorithm policy
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct FipsBoundary {
    pub mode: FipsMode,
    pub active_provider: Option<CryptoProvider>,
    allowed_algorithms: HashSet<String>,
}

impl FipsBoundary {
    #[must_use]
    pub fn new(
        mode: FipsMode,
        active_provider: Option<CryptoProvider>,
        allowed: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            mode,
            active_provider,
            allowed_algorithms: allowed
                .into_iter()
                .map(|a| normalize_algorithm_name(&a.into()))
                .collect(),
        }
    }

    #[must_use]
    pub fn with_default_algorithms(
        mode: FipsMode,
        active_provider: Option<CryptoProvider>,
    ) -> Self {
        let algs: HashSet<String> = ComplianceOperation::all()
            .iter()
            .flat_map(|op| op.approved_algorithms().iter())
            .map(|a| normalize_algorithm_name(a))
            .collect();
        Self {
            mode,
            active_provider,
            allowed_algorithms: algs,
        }
    }

    #[must_use]
    pub fn validate_operation(&self, op: ComplianceOperation, algorithm: &str) -> bool {
        let alg = normalize_algorithm_name(algorithm);

        if !self.mode.is_active() {
            return true;
        }

        let Some(ref provider) = self.active_provider else {
            return false;
        };
        if !provider.is_validated() {
            return false;
        }

        if !self.allowed_algorithms.is_empty() {
            return self.allowed_algorithms.contains(&alg);
        }

        op.approved_algorithms()
            .iter()
            .any(|a| normalize_algorithm_name(a) == alg)
    }

    pub fn set_mode(&mut self, mode: FipsMode) {
        self.mode = mode;
    }

    pub fn set_provider(&mut self, provider: CryptoProvider) {
        self.active_provider = Some(provider);
    }

    pub fn allow_algorithm(&mut self, algorithm: impl Into<String>) {
        self.allowed_algorithms.insert(normalize_algorithm_name(&algorithm.into()));
    }

    pub fn deny_algorithm(&mut self, algorithm: impl Into<String>) {
        self.allowed_algorithms.remove(&normalize_algorithm_name(&algorithm.into()));
    }

    #[must_use]
    pub fn is_valid(&self) -> bool {
        if !self.mode.is_active() {
            return true;
        }
        self.active_provider
            .as_ref()
            .is_some_and(|p| p.is_validated())
    }
}

impl Default for FipsBoundary {
    fn default() -> Self {
        Self {
            mode: FipsMode::default(),
            active_provider: None,
            allowed_algorithms: HashSet::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn normalize_algorithm_name(name: &str) -> String {
    name.trim().to_uppercase()
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn vp() -> CryptoProvider {
        CryptoProvider::new("Test FIPS Module", "9999", true, None)
    }

    fn up() -> CryptoProvider {
        CryptoProvider::new("Uncertified", "0000", false, None)
    }

    // FIPS mode detection
    #[test]
    fn fips_mode_is_active() {
        assert!(FipsMode::Strict.is_active());
        assert!(!FipsMode::Standard.is_active());
    }

    #[test]
    fn fips_mode_default() {
        assert_eq!(FipsMode::default(), FipsMode::Standard);
    }

    // Provider validation
    #[test]
    fn provider_is_validated() {
        assert!(vp().is_validated());
        assert!(!up().is_validated());
    }

    // Operation coverage
    #[test]
    fn operation_count() {
        assert_eq!(ComplianceOperation::all().len(), 6);
    }

    // Boundary defaults
    #[test]
    fn boundary_default() {
        let b = FipsBoundary::default();
        assert!(!b.mode.is_active());
        assert!(b.active_provider.is_none());
    }

    // INV-FIPS-001: Strict mode rejects non-FIPS algorithms
    #[test]
    fn strict_rejects_disallowed() {
        let b = FipsBoundary::new(FipsMode::Strict, Some(vp()), Vec::<&str>::new());
        assert!(!b.validate_operation(ComplianceOperation::Encrypt, "DES"));
        assert!(!b.validate_operation(ComplianceOperation::Hash, "MD5"));
    }

    // INV-FIPS-001: Strict mode allows FIPS-approved
    #[test]
    fn strict_allows_fips_approved() {
        let b = FipsBoundary::with_default_algorithms(FipsMode::Strict, Some(vp()));
        assert!(b.validate_operation(ComplianceOperation::Encrypt, "AES-256-GCM"));
        assert!(b.validate_operation(ComplianceOperation::Hash, "SHA-512"));
        assert!(b.validate_operation(ComplianceOperation::Sign, "Ed25519"));
    }

    // INV-FIPS-002: Provider validation
    #[test]
    fn strict_rejects_unvalidated_provider() {
        let b = FipsBoundary::new(FipsMode::Strict, Some(up()), Vec::<&str>::new());
        assert!(!b.validate_operation(ComplianceOperation::Encrypt, "AES-256-GCM"));
    }

    #[test]
    fn strict_rejects_missing_provider() {
        let b = FipsBoundary::new(FipsMode::Strict, None, Vec::<&str>::new());
        assert!(!b.validate_operation(ComplianceOperation::Encrypt, "AES-256-GCM"));
    }

    // Standard mode is permissive
    #[test]
    fn standard_allows_anything() {
        let b = FipsBoundary::default();
        assert!(b.validate_operation(ComplianceOperation::Encrypt, "DES"));
        assert!(b.validate_operation(ComplianceOperation::Hash, "MD5"));
    }

    // INV-FIPS-003: Boundary validity
    #[test]
    fn boundary_validity_checks() {
        assert!(FipsBoundary::default().is_valid());
        assert!(FipsBoundary::new(FipsMode::Strict, Some(vp()), Vec::<&str>::new()).is_valid());
        assert!(!FipsBoundary::new(FipsMode::Strict, Some(up()), Vec::<&str>::new()).is_valid());
        assert!(!FipsBoundary::new(FipsMode::Strict, None, Vec::<&str>::new()).is_valid());
    }

    // INV-FIPS-005: Case insensitive
    #[test]
    fn case_insensitive_algorithms() {
        let b = FipsBoundary::with_default_algorithms(FipsMode::Strict, Some(vp()));
        assert!(b.validate_operation(ComplianceOperation::Encrypt, "aes-256-gcm"));
        assert!(b.validate_operation(ComplianceOperation::Hash, "sha-256"));
    }

    // Whitelist management
    #[test]
    fn dynamic_whitelist() {
        let mut b = FipsBoundary::new(FipsMode::Strict, Some(vp()), Vec::<&str>::new());
        assert!(b.validate_operation(ComplianceOperation::Encrypt, "AES-256-GCM"));
        b.allow_algorithm("ChaCha20-Poly1305");
        b.deny_algorithm("AES-256-GCM");
        assert!(b.validate_operation(ComplianceOperation::Encrypt, "ChaCha20-Poly1305"));
        assert!(!b.validate_operation(ComplianceOperation::Encrypt, "AES-256-GCM"));
    }

    // Mutation
    #[test]
    fn set_mode_mutation() {
        let mut b = FipsBoundary::default();
        b.set_mode(FipsMode::Strict);
        assert!(b.mode.is_active());
    }

    #[test]
    fn set_provider_mutation() {
        let mut b = FipsBoundary::default();
        b.set_provider(vp());
        assert!(b.active_provider.unwrap().is_validated());
    }
}
