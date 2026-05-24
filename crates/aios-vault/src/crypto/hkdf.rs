//! HKDF-SHA256 derivation helpers for `KDF_DERIVE`.

use ::hkdf::Hkdf;
use rand_core::{OsRng, RngCore};
use sha2::Sha256;

use crate::capability::KeyMaterialHandle;
use crate::VaultError;

/// Default broker-generated HKDF-SHA256 input key material length in bytes.
pub const IKM_LEN: usize = 32;

/// Broker-held derived material plus its deterministic opaque handle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DerivedKey {
    /// Opaque handle returned to callers.
    pub handle: KeyMaterialHandle,
    /// Derived bytes retained inside the broker.
    pub bytes: Vec<u8>,
}

/// Generate fresh HKDF-SHA256 input key material.
#[must_use]
pub fn generate_ikm() -> Vec<u8> {
    let mut ikm = [0_u8; IKM_LEN];
    OsRng.fill_bytes(&mut ikm);
    ikm.to_vec()
}

/// Derive broker-held bytes using HKDF-SHA256.
///
/// T-049 rejects zero-length outputs rather than minting empty key handles.
///
/// # Errors
///
/// Returns [`VaultError::CryptoError`] for zero length, length conversion
/// failure, or HKDF expansion failure.
pub fn derive(ikm: &[u8], info: &[u8], length: u32) -> Result<DerivedKey, VaultError> {
    if length == 0 {
        return Err(VaultError::CryptoError(
            "HKDF-SHA256 output length must be non-zero".to_owned(),
        ));
    }

    let length = usize::try_from(length).map_err(|_| {
        VaultError::CryptoError("HKDF-SHA256 output length is not representable".to_owned())
    })?;
    let hk = Hkdf::<Sha256>::new(None, ikm);
    let mut bytes = vec![0_u8; length];
    hk.expand(info, &mut bytes)
        .map_err(|_| VaultError::CryptoError("HKDF-SHA256 expansion failed".to_owned()))?;

    Ok(DerivedKey {
        handle: derived_handle(info, length, &bytes),
        bytes,
    })
}

fn derived_handle(info: &[u8], length: usize, bytes: &[u8]) -> KeyMaterialHandle {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"aios-vault:hkdf-sha256:v1");
    hasher.update(&length.to_be_bytes());
    hasher.update(info);
    hasher.update(bytes);
    let digest = hasher.finalize();
    let hex = digest.to_hex();
    KeyMaterialHandle(format!("vault-derived:hkdf-sha256:{}", &hex[..32]))
}
