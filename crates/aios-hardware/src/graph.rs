#![allow(missing_docs, clippy::missing_errors_doc)]

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use ed25519_dalek::{Signer, SigningKey, Verifier, VerifyingKey};

use crate::device_record::HardwareDeviceRecord;
use crate::error::HardwareError;
use crate::ids::{DeviceId, HardwareGraphId};

/// Compute the deterministic canonical byte stream for a hardware graph.
///
/// Order: BTreeMap-ordered device canonical bytes, then host canonical id.
fn graph_canonical_bytes(
    devices: &BTreeMap<DeviceId, HardwareDeviceRecord>,
    host_canonical_id: &str,
) -> Vec<u8> {
    let mut buf = Vec::new();
    for record in devices.values() {
        buf.extend_from_slice(&record.canonical_bytes());
    }
    buf.extend_from_slice(host_canonical_id.as_bytes());
    buf
}

/// A content-addressed, Ed25519-signed snapshot of the hardware graph.
#[derive(Debug, Clone)]
pub struct HardwareGraph {
    pub id: HardwareGraphId,
    pub devices: BTreeMap<DeviceId, HardwareDeviceRecord>,
    pub built_at: DateTime<Utc>,
    pub host_canonical_id: String,
    pub signer_fingerprint: String,
    pub signature: Vec<u8>,
}

/// Builds a [`HardwareGraph`] one device at a time and then signs it.
pub struct HardwareGraphBuilder {
    pending: BTreeMap<DeviceId, HardwareDeviceRecord>,
    host_canonical_id: String,
}

impl HardwareGraphBuilder {
    pub fn new(host_canonical_id: impl Into<String>) -> Self {
        Self {
            pending: BTreeMap::new(),
            host_canonical_id: host_canonical_id.into(),
        }
    }

    pub fn add_device(&mut self, record: HardwareDeviceRecord) -> Result<(), HardwareError> {
        if self.pending.contains_key(&record.device_id) {
            return Err(HardwareError::Internal("duplicate device_id".into()));
        }
        self.pending.insert(record.device_id.clone(), record);
        Ok(())
    }

    pub fn build_and_sign(
        self,
        signing_key: &SigningKey,
        signer_fingerprint: impl Into<String>,
    ) -> Result<HardwareGraph, HardwareError> {
        let canonical = graph_canonical_bytes(&self.pending, &self.host_canonical_id);

        let digest = blake3::hash(&canonical);
        let hex_full = hex::encode(digest.as_bytes());
        let hex32 = &hex_full[..32];
        let graph_id = HardwareGraphId(format!("hwgraph_{hex32}"));

        let signature = signing_key.sign(&canonical).to_bytes().to_vec();

        Ok(HardwareGraph {
            id: graph_id,
            devices: self.pending,
            built_at: Utc::now(),
            host_canonical_id: self.host_canonical_id,
            signer_fingerprint: signer_fingerprint.into(),
            signature,
        })
    }
}

impl HardwareGraph {
    /// Recompute canonical bytes and verify the Ed25519 signature.
    pub fn verify(&self, authority_key: &VerifyingKey) -> Result<(), HardwareError> {
        let canonical = graph_canonical_bytes(&self.devices, &self.host_canonical_id);

        let sig = ed25519_dalek::Signature::from_slice(&self.signature)
            .map_err(|_| HardwareError::GraphSnapshotSignatureInvalid(self.id.clone()))?;

        authority_key
            .verify(&canonical, &sig)
            .map_err(|_| HardwareError::GraphSnapshotSignatureInvalid(self.id.clone()))
    }
}

// -- helpers ----------------------------------------------------------------

mod hex {
    pub fn encode(bytes: &[u8]) -> String {
        bytes
            .iter()
            .fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
                use std::fmt::Write;
                #[allow(clippy::unwrap_used)]
                write!(s, "{b:02x}").unwrap();
                s
            })
    }
}
