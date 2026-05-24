//! AES-256-GCM operation helpers for `KEY_ENCRYPT` / `KEY_DECRYPT`.

use ::aes_gcm::aead::{Aead, Payload};
use ::aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use rand_core::{OsRng, RngCore};

use crate::VaultError;

/// AES-256 key length in bytes.
pub const KEY_LEN: usize = 32;
/// GCM nonce length in bytes.
pub const NONCE_LEN: usize = 12;

/// Encryption output with nonce split out for broker metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncryptionOutput {
    /// Nonce-prefixed ciphertext consumed by this T-049 stable operation API.
    pub ciphertext: Vec<u8>,
    /// Random 96-bit AES-GCM nonce.
    pub nonce: Vec<u8>,
}

/// Generate fresh AES-256-GCM key material.
#[must_use]
pub fn generate_key() -> Vec<u8> {
    let mut key = [0_u8; KEY_LEN];
    OsRng.fill_bytes(&mut key);
    key.to_vec()
}

/// Encrypt plaintext with AES-256-GCM and random 96-bit nonce.
///
/// The T-047 `VaultOperation::Decrypt` surface has no nonce field. To keep that
/// surface stable, T-049 prefixes the returned ciphertext with the 12-byte
/// nonce and also returns the nonce as explicit metadata.
///
/// # Errors
///
/// Returns [`VaultError::CryptoError`] for invalid key length or AEAD failure.
pub fn encrypt(key: &[u8], plaintext: &[u8], aad: &[u8]) -> Result<EncryptionOutput, VaultError> {
    let cipher = cipher(key)?;
    let mut nonce = [0_u8; NONCE_LEN];
    OsRng.fill_bytes(&mut nonce);

    let encrypted = cipher
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|_| VaultError::CryptoError("AES-256-GCM encryption failed".to_owned()))?;

    let mut ciphertext = Vec::with_capacity(NONCE_LEN + encrypted.len());
    ciphertext.extend_from_slice(&nonce);
    ciphertext.extend_from_slice(&encrypted);

    Ok(EncryptionOutput {
        ciphertext,
        nonce: nonce.to_vec(),
    })
}

/// Decrypt nonce-prefixed AES-256-GCM ciphertext.
///
/// # Errors
///
/// Returns [`VaultError::CryptoError`] for invalid key length, missing nonce,
/// or authentication failure.
pub fn decrypt(
    key: &[u8],
    nonce_prefixed_ciphertext: &[u8],
    aad: &[u8],
) -> Result<Vec<u8>, VaultError> {
    if nonce_prefixed_ciphertext.len() < NONCE_LEN {
        return Err(VaultError::CryptoError(
            "AES-256-GCM ciphertext is missing nonce prefix".to_owned(),
        ));
    }

    let (nonce, ciphertext) = nonce_prefixed_ciphertext.split_at(NONCE_LEN);
    cipher(key)?
        .decrypt(
            Nonce::from_slice(nonce),
            Payload {
                msg: ciphertext,
                aad,
            },
        )
        .map_err(|_| VaultError::CryptoError("AES-256-GCM decryption failed".to_owned()))
}

fn cipher(key: &[u8]) -> Result<Aes256Gcm, VaultError> {
    Aes256Gcm::new_from_slice(key).map_err(|_| {
        VaultError::CryptoError(format!(
            "AES-256-GCM key must be {KEY_LEN} bytes, got {}",
            key.len()
        ))
    })
}
