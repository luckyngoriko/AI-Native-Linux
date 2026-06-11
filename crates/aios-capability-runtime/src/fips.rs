//! FIPS crypto boundary module for AIOS regulated workloads (R3-W1.6).
#![allow(clippy::doc_markdown, clippy::missing_const_for_fn)]

/// Crypto operation classification per FIPS 140-3 / ISO 19790 taxonomy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CryptoOperation {
    Encrypt,
    Decrypt,
    Sign,
    Verify,
    Hash,
    KeyGen,
    KeyDerive,
    Random,
}

impl CryptoOperation {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Encrypt => "Encrypt",
            Self::Decrypt => "Decrypt",
            Self::Sign => "Sign",
            Self::Verify => "Verify",
            Self::Hash => "Hash",
            Self::KeyGen => "KeyGen",
            Self::KeyDerive => "KeyDerive",
            Self::Random => "Random",
        }
    }
}

/// Provider validation level per FIPS 140-3 cryptographic module
/// validation programme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CryptoProvider {
    FipsValidated,
    Standard,
    Legacy,
}

impl CryptoProvider {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::FipsValidated => "FipsValidated",
            Self::Standard => "Standard",
            Self::Legacy => "Legacy",
        }
    }
}

/// Defines which crypto operations require a validated provider and
/// which are permitted on standard or legacy implementations.
#[derive(Debug, Clone)]
pub struct FipsBoundary {
    boundaries: Vec<(CryptoOperation, CryptoProvider)>,
}

/// Decision from the FIPS boundary evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CryptoBoundaryDecision {
    Allowed,
    RequiresValidated,
    Blocked,
}

impl CryptoBoundaryDecision {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Allowed => "Allowed",
            Self::RequiresValidated => "RequiresValidated",
            Self::Blocked => "Blocked",
        }
    }
}

impl FipsBoundary {
    #[must_use]
    pub fn evaluate(
        &self,
        op: CryptoOperation,
        provider: CryptoProvider,
    ) -> CryptoBoundaryDecision {
        for &(bound_op, required_provider) in &self.boundaries {
            if bound_op == op {
                return match required_provider {
                    CryptoProvider::FipsValidated => match provider {
                        CryptoProvider::FipsValidated => CryptoBoundaryDecision::Allowed,
                        CryptoProvider::Standard => CryptoBoundaryDecision::RequiresValidated,
                        CryptoProvider::Legacy => CryptoBoundaryDecision::Blocked,
                    },
                    CryptoProvider::Standard => match provider {
                        CryptoProvider::FipsValidated | CryptoProvider::Standard => {
                            CryptoBoundaryDecision::Allowed
                        }
                        CryptoProvider::Legacy => CryptoBoundaryDecision::Blocked,
                    },
                    CryptoProvider::Legacy => CryptoBoundaryDecision::Allowed,
                };
            }
        }
        CryptoBoundaryDecision::Allowed
    }
}

/// Maps every crypto operation to a required provider level.
#[derive(Debug, Clone)]
pub struct FipsPolicy {
    boundary: FipsBoundary,
}

impl FipsPolicy {
    #[must_use]
    pub fn default_strict() -> Self {
        use CryptoOperation::*;
        use CryptoProvider::FipsValidated;

        let boundaries = vec![
            (Encrypt, FipsValidated),
            (Decrypt, FipsValidated),
            (Sign, FipsValidated),
            (Verify, FipsValidated),
            (Hash, FipsValidated),
            (KeyGen, FipsValidated),
            (KeyDerive, FipsValidated),
            (Random, FipsValidated),
        ];

        Self {
            boundary: FipsBoundary { boundaries },
        }
    }

    #[must_use]
    pub fn default_standard() -> Self {
        use CryptoOperation::*;
        use CryptoProvider::{FipsValidated, Standard};

        let boundaries = vec![
            (Encrypt, Standard),
            (Decrypt, Standard),
            (Sign, FipsValidated),
            (Verify, Standard),
            (Hash, Standard),
            (KeyGen, FipsValidated),
            (KeyDerive, Standard),
            (Random, Standard),
        ];

        Self {
            boundary: FipsBoundary { boundaries },
        }
    }

    #[must_use]
    pub fn evaluate(
        &self,
        op: CryptoOperation,
        provider: CryptoProvider,
    ) -> CryptoBoundaryDecision {
        self.boundary.evaluate(op, provider)
    }

    #[must_use]
    pub fn custom(boundaries: Vec<(CryptoOperation, CryptoProvider)>) -> Self {
        Self {
            boundary: FipsBoundary { boundaries },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fips_validated_provider_allows_all_ops_in_strict() {
        let policy = FipsPolicy::default_strict();
        let ops = [
            CryptoOperation::Encrypt,
            CryptoOperation::Decrypt,
            CryptoOperation::Sign,
            CryptoOperation::Verify,
            CryptoOperation::Hash,
            CryptoOperation::KeyGen,
            CryptoOperation::KeyDerive,
            CryptoOperation::Random,
        ];
        for op in ops {
            assert_eq!(
                policy.evaluate(op, CryptoProvider::FipsValidated),
                CryptoBoundaryDecision::Allowed,
            );
        }
    }

    #[test]
    fn standard_provider_blocked_on_keygen_in_strict() {
        let policy = FipsPolicy::default_strict();
        assert_eq!(
            policy.evaluate(CryptoOperation::KeyGen, CryptoProvider::Standard),
            CryptoBoundaryDecision::RequiresValidated,
        );
    }

    #[test]
    fn standard_provider_blocked_on_sign_in_strict() {
        let policy = FipsPolicy::default_strict();
        assert_eq!(
            policy.evaluate(CryptoOperation::Sign, CryptoProvider::Standard),
            CryptoBoundaryDecision::RequiresValidated,
        );
    }

    #[test]
    fn standard_policy_allows_encrypt_on_standard_provider() {
        let policy = FipsPolicy::default_standard();
        assert_eq!(
            policy.evaluate(CryptoOperation::Encrypt, CryptoProvider::Standard),
            CryptoBoundaryDecision::Allowed,
        );
    }

    #[test]
    fn standard_policy_requires_validated_for_keygen() {
        let policy = FipsPolicy::default_standard();
        assert_eq!(
            policy.evaluate(CryptoOperation::KeyGen, CryptoProvider::Standard),
            CryptoBoundaryDecision::RequiresValidated,
        );
    }

    #[test]
    fn standard_policy_requires_validated_for_sign() {
        let policy = FipsPolicy::default_standard();
        assert_eq!(
            policy.evaluate(CryptoOperation::Sign, CryptoProvider::Standard),
            CryptoBoundaryDecision::RequiresValidated,
        );
    }

    #[test]
    fn custom_policy_only_encrypt_requires_validated() {
        use CryptoOperation::*;
        use CryptoProvider::FipsValidated;

        let policy = FipsPolicy::custom(vec![(Encrypt, FipsValidated)]);
        assert_eq!(
            policy.evaluate(Encrypt, CryptoProvider::Standard),
            CryptoBoundaryDecision::RequiresValidated,
        );
        assert_eq!(
            policy.evaluate(Decrypt, CryptoProvider::Standard),
            CryptoBoundaryDecision::Allowed,
        );
        assert_eq!(
            policy.evaluate(KeyGen, CryptoProvider::Legacy),
            CryptoBoundaryDecision::Allowed,
        );
    }

    #[test]
    fn legacy_provider_blocked_for_any_validated_operation() {
        let policy = FipsPolicy::default_strict();
        assert_eq!(
            policy.evaluate(CryptoOperation::Hash, CryptoProvider::Legacy),
            CryptoBoundaryDecision::Blocked,
        );
    }

    #[test]
    fn legacy_provider_blocked_in_standard_policy_for_sensitive_ops() {
        let policy = FipsPolicy::default_standard();
        assert_eq!(
            policy.evaluate(CryptoOperation::Sign, CryptoProvider::Legacy),
            CryptoBoundaryDecision::Blocked,
        );
        assert_eq!(
            policy.evaluate(CryptoOperation::Hash, CryptoProvider::Legacy),
            CryptoBoundaryDecision::Blocked,
        );
    }

    #[test]
    fn crypto_operation_as_str_is_screaming_snake() {
        assert_eq!(CryptoOperation::Encrypt.as_str(), "Encrypt");
        assert_eq!(CryptoOperation::KeyDerive.as_str(), "KeyDerive");
        assert_eq!(CryptoOperation::Random.as_str(), "Random");
    }

    #[test]
    fn crypto_provider_as_str_is_screaming_snake() {
        assert_eq!(CryptoProvider::FipsValidated.as_str(), "FipsValidated");
        assert_eq!(CryptoProvider::Standard.as_str(), "Standard");
        assert_eq!(CryptoProvider::Legacy.as_str(), "Legacy");
    }

    #[test]
    fn crypto_provider_ordering_is_variant_order() {
        assert!(CryptoProvider::FipsValidated < CryptoProvider::Standard);
        assert!(CryptoProvider::Standard < CryptoProvider::Legacy);
    }
}
