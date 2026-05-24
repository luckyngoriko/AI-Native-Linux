//! Ed25519 helpers for `KEY_SIGN`, `KEY_VERIFY`, and bootstrap signing.

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand_core::OsRng;

use crate::VaultError;

/// Ed25519 signing key seed length in bytes.
pub const KEY_LEN: usize = 32;
/// Ed25519 signature length in bytes.
pub const SIGNATURE_LEN: usize = 64;

/// Generate fresh Ed25519 signing key material.
#[must_use]
pub fn generate_signing_key() -> Vec<u8> {
    SigningKey::generate(&mut OsRng).to_bytes().to_vec()
}

/// Sign message bytes with an Ed25519 signing key seed.
///
/// # Errors
///
/// Returns [`VaultError::CryptoError`] when key material is malformed.
pub fn sign(key: &[u8], message: &[u8]) -> Result<Vec<u8>, VaultError> {
    let signing_key = signing_key(key)?;
    Ok(signing_key.sign(message).to_bytes().to_vec())
}

/// Verify an Ed25519 signature.
///
/// T-049 accepts either a 32-byte public verifying key or a 32-byte signing
/// seed for test harness pairing with `KEY_SIGN`; no key bytes are returned.
/// Same-length negative verification returns `Ok(false)`.
///
/// # Errors
///
/// Returns [`VaultError::CryptoError`] when key material or signature length is
/// malformed.
pub fn verify(key: &[u8], message: &[u8], signature: &[u8]) -> Result<bool, VaultError> {
    let signature: [u8; SIGNATURE_LEN] = signature.try_into().map_err(|_| {
        VaultError::CryptoError(format!(
            "Ed25519 signature must be {SIGNATURE_LEN} bytes, got {}",
            signature.len()
        ))
    })?;
    let signature = Signature::from_bytes(&signature);
    let key_bytes = key_bytes(key)?;

    if VerifyingKey::from_bytes(&key_bytes)
        .is_ok_and(|verifying_key| verifying_key.verify(message, &signature).is_ok())
    {
        return Ok(true);
    }

    let signing_key = SigningKey::from_bytes(&key_bytes);
    Ok(signing_key
        .verifying_key()
        .verify(message, &signature)
        .is_ok())
}

fn signing_key(key: &[u8]) -> Result<SigningKey, VaultError> {
    Ok(SigningKey::from_bytes(&key_bytes(key)?))
}

fn key_bytes(key: &[u8]) -> Result<[u8; KEY_LEN], VaultError> {
    key.try_into().map_err(|_| {
        VaultError::CryptoError(format!(
            "Ed25519 key material must be {KEY_LEN} bytes, got {}",
            key.len()
        ))
    })
}
