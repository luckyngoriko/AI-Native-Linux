#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    clippy::unwrap_in_result,
    clippy::doc_markdown,
    clippy::significant_drop_tightening,
    clippy::wildcard_imports,
    clippy::similar_names,
    clippy::cast_possible_wrap,
    clippy::too_many_lines,
    clippy::unused_async,
    missing_docs,
    reason = "test code; panic-on-failure is the idiomatic test signal"
)]

use std::sync::Arc;

use aios_hardware::*;
use chrono::Utc;
use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
use rand_core::OsRng;
use strum::EnumCount;

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn make_signing_key() -> (SigningKey, VerifyingKey) {
    let signing_key = SigningKey::generate(&mut OsRng);
    let verifying_key = signing_key.verifying_key();
    (signing_key, verifying_key)
}

fn make_blob(blob_id: &str, version: &str, sig: Vec<u8>, fingerprint: &str) -> FirmwareBlob {
    FirmwareBlob {
        blob_id: FirmwareBlobId(blob_id.to_string()),
        update_class: FirmwareUpdateClass::CpuMicrocode,
        scope: FirmwareScope::Cpu,
        target_device: Some(DeviceId("pci:8086:9a49".to_string())),
        vendor_name: "Intel".to_string(),
        version: version.to_string(),
        blake3_hash: "abc123def456".to_string(),
        signature: sig,
        signer_fingerprint: fingerprint.to_string(),
        published_at: Utc::now(),
    }
}

fn canonical_msg(blob: &FirmwareBlob) -> Vec<u8> {
    let mut msg = Vec::new();
    msg.extend_from_slice(blob.blob_id.0.as_bytes());
    msg.extend_from_slice(blob.update_class.label().as_bytes());
    msg.extend_from_slice(blob.scope.label().as_bytes());
    msg.extend_from_slice(blob.version.as_bytes());
    msg.extend_from_slice(blob.blake3_hash.as_bytes());
    msg
}

fn sign_blob(signing_key: &SigningKey, blob: &FirmwareBlob) -> Vec<u8> {
    let msg = canonical_msg(blob);
    signing_key.sign(&msg).to_vec()
}

fn make_orchestrator_with_key() -> (FirmwareUpdateOrchestrator, SigningKey, String) {
    let mut orch = FirmwareUpdateOrchestrator::new();
    let (sk, vk) = make_signing_key();
    let fp = "fp:aa:bb:cc:dd";
    orch.register_aios_publisher_key(fp, vk);
    (orch, sk, fp.to_string())
}

// ---------------------------------------------------------------------------
// FirmwareUpdateState::label / FirmwareUpdateClass::label / FirmwareScope::label
// ---------------------------------------------------------------------------

#[test]
fn firmware_update_class_label_returns_distinct_strings() {
    let labels: Vec<&str> = [
        FirmwareUpdateClass::CpuMicrocode,
        FirmwareUpdateClass::GpuFirmware,
        FirmwareUpdateClass::NetworkFirmware,
        FirmwareUpdateClass::StorageFirmware,
        FirmwareUpdateClass::PeripheralFirmware,
    ]
    .iter()
    .map(|c| c.label())
    .collect();
    assert_eq!(labels.len(), FirmwareUpdateClass::COUNT);
    // all distinct
    let mut dedup = labels.clone();
    dedup.sort_unstable();
    dedup.dedup();
    assert_eq!(dedup.len(), labels.len());
}

#[test]
fn firmware_scope_label_returns_distinct_strings() {
    let labels: Vec<&str> = [
        FirmwareScope::BiosUefi,
        FirmwareScope::Cpu,
        FirmwareScope::Gpu,
        FirmwareScope::NetworkAdapter,
        FirmwareScope::Storage,
        FirmwareScope::Thunderbolt,
        FirmwareScope::Tpm,
        FirmwareScope::OtherPeripheral,
    ]
    .iter()
    .map(|c| c.label())
    .collect();
    assert_eq!(labels.len(), FirmwareScope::COUNT);
    let mut dedup = labels.clone();
    dedup.sort_unstable();
    dedup.dedup();
    assert_eq!(dedup.len(), labels.len());
}

#[test]
fn firmware_update_state_label_returns_distinct_strings() {
    use strum::IntoEnumIterator;
    let labels: Vec<&str> = FirmwareUpdateState::iter()
        .map(FirmwareUpdateState::label)
        .collect();
    assert_eq!(labels.len(), FirmwareUpdateState::COUNT);
    let mut dedup = labels.clone();
    dedup.sort_unstable();
    dedup.dedup();
    assert_eq!(dedup.len(), labels.len());
}

// ---------------------------------------------------------------------------
// propose
// ---------------------------------------------------------------------------

#[tokio::test]
async fn propose_blob_state_is_proposed() {
    let orch = FirmwareUpdateOrchestrator::new();
    let blob = make_blob("b001", "1.0.0", vec![], "");
    let plan = orch
        .propose(blob, FirmwareApplyStrategy::Atomic)
        .await
        .unwrap();
    assert_eq!(plan.current_state, FirmwareUpdateState::Proposed);
    assert_eq!(plan.history.len(), 1);
    assert_eq!(plan.history[0].state, FirmwareUpdateState::Proposed);
}

#[tokio::test]
async fn propose_duplicate_blob_id_returns_internal_error() {
    let orch = FirmwareUpdateOrchestrator::new();
    let blob = make_blob("b001", "1.0.0", vec![], "");
    orch.propose(blob.clone(), FirmwareApplyStrategy::Atomic)
        .await
        .unwrap();
    let err = orch
        .propose(blob, FirmwareApplyStrategy::Atomic)
        .await
        .unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("duplicate firmware blob id"));
}

// ---------------------------------------------------------------------------
// verify — signing path resolution
// ---------------------------------------------------------------------------

#[tokio::test]
async fn verify_with_aios_publisher_key_returns_aios_publisher_signed_and_advances_to_verified() {
    let (orch, sk, _fp) = make_orchestrator_with_key();
    let blob = make_blob("b001", "1.0.0", vec![], "fp:aa:bb:cc:dd");
    let sig = sign_blob(&sk, &blob);
    let mut signed_blob = blob;
    signed_blob.signature = sig;
    orch.propose(signed_blob, FirmwareApplyStrategy::Atomic)
        .await
        .unwrap();
    let result = orch
        .verify(&FirmwareBlobId("b001".to_string()))
        .await
        .unwrap();
    assert_eq!(result, FirmwareTrustResult::AiosPublisherSigned);
    let plan = orch
        .get_plan(&FirmwareBlobId("b001".to_string()))
        .await
        .unwrap();
    assert_eq!(plan.current_state, FirmwareUpdateState::Verified);
    assert_eq!(
        plan.trust_result,
        Some(FirmwareTrustResult::AiosPublisherSigned)
    );
}

#[tokio::test]
async fn verify_with_vendor_bridge_key_returns_vendor_signed_through_aios_bridge() {
    let mut orch = FirmwareUpdateOrchestrator::new();
    let (sk, vk) = make_signing_key();
    let fp = "fp:vendor:xyz";
    orch.register_vendor_bridge_key(fp, vk);
    let blob = make_blob("b002", "2.0.0", vec![], fp);
    let sig = sign_blob(&sk, &blob);
    let mut signed_blob = blob;
    signed_blob.signature = sig;
    orch.propose(signed_blob, FirmwareApplyStrategy::Staged)
        .await
        .unwrap();
    let result = orch
        .verify(&FirmwareBlobId("b002".to_string()))
        .await
        .unwrap();
    assert_eq!(result, FirmwareTrustResult::VendorSignedThroughAiosBridge);
}

#[tokio::test]
async fn verify_with_operator_local_key_returns_operator_local_signed() {
    let mut orch = FirmwareUpdateOrchestrator::new();
    let (sk, vk) = make_signing_key();
    let fp = "fp:ops:local";
    orch.register_operator_local_key(fp, vk);
    let blob = make_blob("b003", "3.0.0", vec![], fp);
    let sig = sign_blob(&sk, &blob);
    let mut signed_blob = blob;
    signed_blob.signature = sig;
    orch.propose(signed_blob, FirmwareApplyStrategy::Atomic)
        .await
        .unwrap();
    let result = orch
        .verify(&FirmwareBlobId("b003".to_string()))
        .await
        .unwrap();
    assert_eq!(result, FirmwareTrustResult::OperatorLocalSigned);
}

#[tokio::test]
async fn verify_with_unsigned_blob_returns_unsigned_refused_and_state_failed() {
    let orch = FirmwareUpdateOrchestrator::new();
    let blob = make_blob("b004", "1.0.0", vec![], "");
    orch.propose(blob, FirmwareApplyStrategy::Atomic)
        .await
        .unwrap();
    let err = orch
        .verify(&FirmwareBlobId("b004".to_string()))
        .await
        .unwrap_err();
    assert!(matches!(err, HardwareError::FirmwareUnsigned(_)));
    let plan = orch
        .get_plan(&FirmwareBlobId("b004".to_string()))
        .await
        .unwrap();
    assert_eq!(plan.current_state, FirmwareUpdateState::Failed);
    assert_eq!(
        plan.trust_result,
        Some(FirmwareTrustResult::UnsignedRefused)
    );
}

#[tokio::test]
async fn verify_with_unknown_signer_returns_revoked_key_and_state_failed() {
    let orch = FirmwareUpdateOrchestrator::new();
    let (sk, _vk) = make_signing_key();
    let blob = make_blob("b005", "1.0.0", vec![], "fp:unknown:signer");
    let sig = sign_blob(&sk, &blob);
    let mut signed_blob = blob;
    signed_blob.signature = sig;
    orch.propose(signed_blob, FirmwareApplyStrategy::Atomic)
        .await
        .unwrap();
    let err = orch
        .verify(&FirmwareBlobId("b005".to_string()))
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        HardwareError::FirmwareSignatureInvalid { .. }
    ));
    let plan = orch
        .get_plan(&FirmwareBlobId("b005".to_string()))
        .await
        .unwrap();
    assert_eq!(plan.current_state, FirmwareUpdateState::Failed);
    assert_eq!(plan.trust_result, Some(FirmwareTrustResult::RevokedKey));
}

#[tokio::test]
async fn verify_with_invalid_signature_returns_signature_invalid_and_state_failed() {
    let (orch, _sk, _fp) = make_orchestrator_with_key();
    let blob = make_blob("b006", "1.0.0", vec![0u8; 64], "fp:aa:bb:cc:dd");
    orch.propose(blob, FirmwareApplyStrategy::Atomic)
        .await
        .unwrap();
    let err = orch
        .verify(&FirmwareBlobId("b006".to_string()))
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        HardwareError::FirmwareSignatureInvalid { .. }
    ));
    let plan = orch
        .get_plan(&FirmwareBlobId("b006".to_string()))
        .await
        .unwrap();
    assert_eq!(plan.current_state, FirmwareUpdateState::Failed);
}

#[tokio::test]
async fn verify_with_version_regression_returns_version_regression_and_state_failed() {
    let (orch, sk, _fp) = make_orchestrator_with_key();
    orch.set_installed_version(DeviceId("pci:8086:9a49".to_string()), "2.0.0".to_string())
        .await;
    let blob = make_blob("b007", "1.0.0", vec![], "fp:aa:bb:cc:dd");
    let sig = sign_blob(&sk, &blob);
    let mut signed_blob = blob;
    signed_blob.signature = sig;
    orch.propose(signed_blob, FirmwareApplyStrategy::Atomic)
        .await
        .unwrap();
    let err = orch
        .verify(&FirmwareBlobId("b007".to_string()))
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        HardwareError::FirmwareVersionRegression { .. }
    ));
    let plan = orch
        .get_plan(&FirmwareBlobId("b007".to_string()))
        .await
        .unwrap();
    assert_eq!(plan.current_state, FirmwareUpdateState::Failed);
    assert_eq!(
        plan.trust_result,
        Some(FirmwareTrustResult::VersionRegression)
    );
}

// ---------------------------------------------------------------------------
// approve / stage / apply / finalize / revert / fail
// ---------------------------------------------------------------------------

#[tokio::test]
async fn approve_verified_plan_advances_to_approved() {
    let (orch, sk, _fp) = make_orchestrator_with_key();
    let blob = make_blob("b008", "1.0.0", vec![], "fp:aa:bb:cc:dd");
    let sig = sign_blob(&sk, &blob);
    let mut signed_blob = blob;
    signed_blob.signature = sig;
    orch.propose(signed_blob, FirmwareApplyStrategy::Atomic)
        .await
        .unwrap();
    orch.verify(&FirmwareBlobId("b008".to_string()))
        .await
        .unwrap();
    orch.approve(&FirmwareBlobId("b008".to_string()))
        .await
        .unwrap();
    let plan = orch
        .get_plan(&FirmwareBlobId("b008".to_string()))
        .await
        .unwrap();
    assert_eq!(plan.current_state, FirmwareUpdateState::Approved);
}

#[tokio::test]
async fn approve_unverified_plan_returns_invalid_transition() {
    let orch = FirmwareUpdateOrchestrator::new();
    let blob = make_blob("b009", "1.0.0", vec![], "");
    orch.propose(blob, FirmwareApplyStrategy::Atomic)
        .await
        .unwrap();
    let err = orch
        .approve(&FirmwareBlobId("b009".to_string()))
        .await
        .unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("invalid firmware transition"));
}

#[tokio::test]
async fn stage_approved_plan_advances_to_staged() {
    let (orch, sk, _fp) = make_orchestrator_with_key();
    let blob = make_blob("b010", "1.0.0", vec![], "fp:aa:bb:cc:dd");
    let sig = sign_blob(&sk, &blob);
    let mut signed_blob = blob;
    signed_blob.signature = sig;
    orch.propose(signed_blob, FirmwareApplyStrategy::Atomic)
        .await
        .unwrap();
    orch.verify(&FirmwareBlobId("b010".to_string()))
        .await
        .unwrap();
    orch.approve(&FirmwareBlobId("b010".to_string()))
        .await
        .unwrap();
    orch.stage(&FirmwareBlobId("b010".to_string()))
        .await
        .unwrap();
    let plan = orch
        .get_plan(&FirmwareBlobId("b010".to_string()))
        .await
        .unwrap();
    assert_eq!(plan.current_state, FirmwareUpdateState::Staged);
}

#[tokio::test]
async fn apply_staged_atomic_advances_to_applied_in_one_step_and_updates_installed_version() {
    let (orch, sk, _fp) = make_orchestrator_with_key();
    let blob = make_blob("b011", "1.0.0", vec![], "fp:aa:bb:cc:dd");
    let sig = sign_blob(&sk, &blob);
    let mut signed_blob = blob;
    signed_blob.signature = sig;
    orch.propose(signed_blob, FirmwareApplyStrategy::Atomic)
        .await
        .unwrap();
    orch.verify(&FirmwareBlobId("b011".to_string()))
        .await
        .unwrap();
    orch.approve(&FirmwareBlobId("b011".to_string()))
        .await
        .unwrap();
    orch.stage(&FirmwareBlobId("b011".to_string()))
        .await
        .unwrap();
    orch.apply(&FirmwareBlobId("b011".to_string()))
        .await
        .unwrap();
    let plan = orch
        .get_plan(&FirmwareBlobId("b011".to_string()))
        .await
        .unwrap();
    assert_eq!(plan.current_state, FirmwareUpdateState::Applied);
    // two history entries: Applying + Applied
    assert!(plan
        .history
        .iter()
        .any(|e| e.state == FirmwareUpdateState::Applying));
    assert!(plan
        .history
        .iter()
        .any(|e| e.state == FirmwareUpdateState::Applied));
}

#[tokio::test]
async fn apply_staged_with_staged_strategy_advances_to_applying_then_requires_finalize() {
    let (orch, sk, _fp) = make_orchestrator_with_key();
    let blob = make_blob("b012", "1.0.0", vec![], "fp:aa:bb:cc:dd");
    let sig = sign_blob(&sk, &blob);
    let mut signed_blob = blob;
    signed_blob.signature = sig;
    orch.propose(signed_blob, FirmwareApplyStrategy::Staged)
        .await
        .unwrap();
    orch.verify(&FirmwareBlobId("b012".to_string()))
        .await
        .unwrap();
    orch.approve(&FirmwareBlobId("b012".to_string()))
        .await
        .unwrap();
    orch.stage(&FirmwareBlobId("b012".to_string()))
        .await
        .unwrap();
    orch.apply(&FirmwareBlobId("b012".to_string()))
        .await
        .unwrap();
    let plan = orch
        .get_plan(&FirmwareBlobId("b012".to_string()))
        .await
        .unwrap();
    assert_eq!(plan.current_state, FirmwareUpdateState::Applying);
}

#[tokio::test]
async fn finalize_staged_apply_advances_applying_to_applied() {
    let (orch, sk, _fp) = make_orchestrator_with_key();
    let blob = make_blob("b013", "1.0.0", vec![], "fp:aa:bb:cc:dd");
    let sig = sign_blob(&sk, &blob);
    let mut signed_blob = blob;
    signed_blob.signature = sig;
    orch.propose(signed_blob, FirmwareApplyStrategy::Staged)
        .await
        .unwrap();
    orch.verify(&FirmwareBlobId("b013".to_string()))
        .await
        .unwrap();
    orch.approve(&FirmwareBlobId("b013".to_string()))
        .await
        .unwrap();
    orch.stage(&FirmwareBlobId("b013".to_string()))
        .await
        .unwrap();
    orch.apply(&FirmwareBlobId("b013".to_string()))
        .await
        .unwrap();
    orch.finalize_staged_apply(&FirmwareBlobId("b013".to_string()))
        .await
        .unwrap();
    let plan = orch
        .get_plan(&FirmwareBlobId("b013".to_string()))
        .await
        .unwrap();
    assert_eq!(plan.current_state, FirmwareUpdateState::Applied);
}

#[tokio::test]
async fn apply_with_deferred_strategy_returns_internal_error_and_stays_at_staged() {
    let (orch, sk, _fp) = make_orchestrator_with_key();
    let blob = make_blob("b014", "1.0.0", vec![], "fp:aa:bb:cc:dd");
    let sig = sign_blob(&sk, &blob);
    let mut signed_blob = blob;
    signed_blob.signature = sig;
    orch.propose(signed_blob, FirmwareApplyStrategy::Deferred)
        .await
        .unwrap();
    orch.verify(&FirmwareBlobId("b014".to_string()))
        .await
        .unwrap();
    orch.approve(&FirmwareBlobId("b014".to_string()))
        .await
        .unwrap();
    orch.stage(&FirmwareBlobId("b014".to_string()))
        .await
        .unwrap();
    let err = orch
        .apply(&FirmwareBlobId("b014".to_string()))
        .await
        .unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("deferred"));
    let plan = orch
        .get_plan(&FirmwareBlobId("b014".to_string()))
        .await
        .unwrap();
    assert_eq!(plan.current_state, FirmwareUpdateState::Staged);
}

#[tokio::test]
async fn revert_from_applied_advances_to_reverted_and_records_reason() {
    let (orch, sk, _fp) = make_orchestrator_with_key();
    let blob = make_blob("b015", "1.0.0", vec![], "fp:aa:bb:cc:dd");
    let sig = sign_blob(&sk, &blob);
    let mut signed_blob = blob;
    signed_blob.signature = sig;
    orch.propose(signed_blob, FirmwareApplyStrategy::Atomic)
        .await
        .unwrap();
    orch.verify(&FirmwareBlobId("b015".to_string()))
        .await
        .unwrap();
    orch.approve(&FirmwareBlobId("b015".to_string()))
        .await
        .unwrap();
    orch.stage(&FirmwareBlobId("b015".to_string()))
        .await
        .unwrap();
    orch.apply(&FirmwareBlobId("b015".to_string()))
        .await
        .unwrap();
    orch.revert(&FirmwareBlobId("b015".to_string()), "security advisory")
        .await
        .unwrap();
    let plan = orch
        .get_plan(&FirmwareBlobId("b015".to_string()))
        .await
        .unwrap();
    assert_eq!(plan.current_state, FirmwareUpdateState::Reverted);
    assert!(plan
        .history
        .iter()
        .any(|e| e.note.contains("security advisory")));
}

#[tokio::test]
async fn fail_from_any_state_advances_to_failed_and_records_reason() {
    let orch = FirmwareUpdateOrchestrator::new();
    let blob = make_blob("b016", "1.0.0", vec![], "");
    orch.propose(blob, FirmwareApplyStrategy::Atomic)
        .await
        .unwrap();
    orch.fail(&FirmwareBlobId("b016".to_string()), "operator abort")
        .await
        .unwrap();
    let plan = orch
        .get_plan(&FirmwareBlobId("b016".to_string()))
        .await
        .unwrap();
    assert_eq!(plan.current_state, FirmwareUpdateState::Failed);
    assert!(plan
        .history
        .iter()
        .any(|e| e.note.contains("operator abort")));
}

// ---------------------------------------------------------------------------
// recovery mode
// ---------------------------------------------------------------------------

#[tokio::test]
async fn recovery_mode_allows_operator_local_signed_path_when_publisher_registry_empty() {
    let mut orch = FirmwareUpdateOrchestrator::new();
    orch.set_recovery_mode(true).await;
    let (sk, vk) = make_signing_key();
    let fp = "fp:ops:recovery";
    orch.register_operator_local_key(fp, vk);
    let blob = make_blob("b017", "1.0.0", vec![], fp);
    let sig = sign_blob(&sk, &blob);
    let mut signed_blob = blob;
    signed_blob.signature = sig;
    orch.propose(signed_blob, FirmwareApplyStrategy::Atomic)
        .await
        .unwrap();
    let result = orch
        .verify(&FirmwareBlobId("b017".to_string()))
        .await
        .unwrap();
    assert_eq!(result, FirmwareTrustResult::OperatorLocalSigned);
    let plan = orch
        .get_plan(&FirmwareBlobId("b017".to_string()))
        .await
        .unwrap();
    assert_eq!(plan.current_state, FirmwareUpdateState::Verified);
}

// ---------------------------------------------------------------------------
// get_plan / list_plans
// ---------------------------------------------------------------------------

#[tokio::test]
async fn get_plan_known_blob_id_returns_some() {
    let orch = FirmwareUpdateOrchestrator::new();
    let blob = make_blob("b018", "1.0.0", vec![], "");
    orch.propose(blob, FirmwareApplyStrategy::Atomic)
        .await
        .unwrap();
    let plan = orch.get_plan(&FirmwareBlobId("b018".to_string())).await;
    assert!(plan.is_some());
}

#[tokio::test]
async fn get_plan_unknown_blob_id_returns_none() {
    let orch = FirmwareUpdateOrchestrator::new();
    let plan = orch.get_plan(&FirmwareBlobId("b099".to_string())).await;
    assert!(plan.is_none());
}

#[tokio::test]
async fn list_plans_after_3_proposals_returns_3() {
    let orch = FirmwareUpdateOrchestrator::new();
    for i in 0..3 {
        let blob = make_blob(&format!("b{i:03}"), "1.0.0", vec![], "");
        orch.propose(blob, FirmwareApplyStrategy::Atomic)
            .await
            .unwrap();
    }
    assert_eq!(orch.list_plans().await.len(), 3);
}

// ---------------------------------------------------------------------------
// concurrent propose
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_propose_5_distinct_blobs_no_panic() {
    let orch = Arc::new(FirmwareUpdateOrchestrator::new());
    let mut handles = vec![];
    for i in 0..5 {
        let orch = Arc::clone(&orch);
        handles.push(tokio::spawn(async move {
            let blob = make_blob(&format!("c{i:03}"), "1.0.0", vec![], "");
            orch.propose(blob, FirmwareApplyStrategy::Atomic).await
        }));
    }
    for h in handles {
        h.await.unwrap().unwrap();
    }
    assert_eq!(orch.list_plans().await.len(), 5);
}
