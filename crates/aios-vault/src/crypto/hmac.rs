//! HMAC-SHA256 operation helpers for `MAC_GENERATE` / `MAC_VERIFY`.

use ::hmac::{Hmac, Mac};
use rand_core::{OsRng, RngCore};
use sha2::Sha256;

use crate::VaultError;

type HmacSha256 = Hmac<Sha256>;

/// Default broker-generated HMAC-SHA256 key length in bytes.
pub const KEY_LEN: usize = 32;
/// HMAC-SHA256 tag length in bytes.
pub const TAG_LEN: usize = 32;

/// Generate fresh HMAC-SHA256 key material.
#[must_use]
pub fn generate_key() -> Vec<u8> {
    let mut key = [0_u8; KEY_LEN];
    OsRng.fill_bytes(&mut key);
    key.to_vec()
}

/// Generate a 32-byte HMAC-SHA256 tag.
///
/// # Errors
///
/// Returns [`VaultError::CryptoError`] if the HMAC key is rejected by the
/// backend primitive.
pub fn generate(key: &[u8], message: &[u8]) -> Result<Vec<u8>, VaultError> {
    let mut mac = mac(key)?;
    mac.update(message);
    Ok(mac.finalize().into_bytes().to_vec())
}

/// Verify a 32-byte HMAC-SHA256 tag.
///
/// A same-length negative verification returns `Ok(false)`. A malformed tag
/// length is caller error and returns [`VaultError::CryptoError`].
///
/// # Errors
///
/// Returns [`VaultError::CryptoError`] for malformed tag length or rejected key.
pub fn verify(key: &[u8], message: &[u8], tag: &[u8]) -> Result<bool, VaultError> {
    if tag.len() != TAG_LEN {
        return Err(VaultError::CryptoError(format!(
            "HMAC-SHA256 tag must be {TAG_LEN} bytes, got {}",
            tag.len()
        )));
    }

    let mut mac = mac(key)?;
    mac.update(message);
    Ok(mac.verify_slice(tag).is_ok())
}

fn mac(key: &[u8]) -> Result<HmacSha256, VaultError> {
    HmacSha256::new_from_slice(key)
        .map_err(|_| VaultError::CryptoError("HMAC-SHA256 key rejected".to_owned()))
}
