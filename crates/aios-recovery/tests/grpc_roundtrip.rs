//! T-079 integration tests for `aios.recovery.v1alpha1.RecoveryService`.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    clippy::too_many_lines,
    clippy::items_after_statements,
    clippy::result_large_err,
    reason = "test code; panic-on-failure is the idiomatic contract signal"
)]

use std::error::Error;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use aios_recovery::service::conversions::{
    candidate_state_from_proto, candidate_state_to_proto, first_boot_context_from_proto,
    first_boot_context_to_proto, kernel_candidate_from_proto, kernel_candidate_to_proto,
    kernel_manifest_from_proto, kernel_manifest_to_proto, recovery_bundle_from_proto,
    recovery_bundle_to_proto, recovery_error_to_status, recovery_state_from_proto,
    recovery_state_to_proto,
};
use aios_recovery::service::proto::recovery_service_server::RecoveryService as _;
use aios_recovery::service::proto::{
    ActivateKernelCandidateRequest, CandidateStateProto, CheckRecoveryMutationRequest,
    EnterRecoveryRequestProto, ExitRecoveryRequestProto, GetActiveKernelRequest,
    GetFirstBootStatusRequest, GetRecoveryStateRequest, ListKernelCandidatesRequest,
    RecoveryModeProto, RegisterKernelCandidateRequest, RollbackKernelCandidateRequest,
    RunFirstBootProvisioningRequest, VerifyKernelCandidateRequest,
};
use aios_recovery::service::{
    RecoveryServiceClient, RecoveryServiceGrpcServer, RecoveryServiceImpl, SCHEMA_VERSION,
};
use aios_recovery::{
    BootId, CandidateId, CandidateState, FirstBootContext, FirstBootDriver, FirstBootPhase,
    FirstBootStatus, InMemoryRecoveryBoundary, KernelCandidate, KernelManifest,
    KernelPipelineDriver, RecoveryBoundary, RecoveryBundle, RecoveryError, RecoveryGuard,
    RecoveryMode, RecoveryState,
};
use chrono::{DateTime, Utc};
use ed25519_dalek::{Signer, SigningKey};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tonic::{Code, Request};

type TestResult<T = ()> = Result<T, Box<dyn Error + Send + Sync>>;

const AUTHORITY: &str = "aios-kernel-root";

struct Harness {
    svc: RecoveryServiceImpl,
    boundary: Arc<InMemoryRecoveryBoundary>,
    signing_key: SigningKey,
}

fn fixed_time() -> TestResult<DateTime<Utc>> {
    Ok(DateTime::parse_from_rfc3339("2026-05-25T10:00:00Z")?.with_timezone(&Utc))
}

fn signing_key(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
}

fn harness() -> Harness {
    let signing_key = signing_key(31);
    let boundary = Arc::new(InMemoryRecoveryBoundary::new());
    let boundary_for_first_boot: Arc<dyn RecoveryBoundary> = boundary.clone();
    let boundary_for_pipeline: Arc<dyn RecoveryBoundary> = boundary.clone();
    let boundary_for_guard: Arc<dyn RecoveryBoundary> = boundary.clone();
    let first_boot = Arc::new(FirstBootDriver::new(boundary_for_first_boot));
    let kernel_pipeline = Arc::new(
        KernelPipelineDriver::new(boundary_for_pipeline)
            .with_trusted_authority(AUTHORITY.to_owned(), signing_key.verifying_key()),
    );
    let guard = Arc::new(RecoveryGuard::new(boundary_for_guard));
    let svc = RecoveryServiceImpl::new(Arc::clone(&boundary), first_boot, kernel_pipeline, guard);

    Harness {
        svc,
        boundary,
        signing_key,
    }
}

fn operator_enter_request() -> EnterRecoveryRequestProto {
    EnterRecoveryRequestProto {
        schema_version: SCHEMA_VERSION.to_owned(),
        reason: "OPERATOR_INITIATED".to_owned(),
        operator_grant: Some("ovr_01HXY8K2JPQ7N3M4R5S6T7V8W9".to_owned()),
        expected_phases: vec![i32::from(
            aios_recovery::service::proto::BootPhaseProto::BootPhaseRecovery,
        )],
        bundle: None,
        action_id_proto: Vec::new(),
        action_id_format: 0,
    }
}

fn manifest(version: &str, requires_recovery_install: bool) -> KernelManifest {
    KernelManifest {
        version: version.to_owned(),
        min_aios_version: "0.1.0".to_owned(),
        requires_recovery_install,
        verification_intent: Some("verify dedicated kernel gates".to_owned()),
        tags: vec!["KSPP_STRICT".to_owned()],
    }
}

fn sign_manifest(manifest: &KernelManifest, signing_key: &SigningKey) -> TestResult<Vec<u8>> {
    Ok(signing_key
        .sign(&serde_json::to_vec(manifest)?)
        .to_bytes()
        .to_vec())
}

fn register_request(
    manifest: &KernelManifest,
    signing_key: &SigningKey,
) -> TestResult<RegisterKernelCandidateRequest> {
    Ok(RegisterKernelCandidateRequest {
        schema_version: SCHEMA_VERSION.to_owned(),
        manifest: Some(kernel_manifest_to_proto(manifest)),
        signature_ed25519: sign_manifest(manifest, signing_key)?,
        action_id_proto: Vec::new(),
        action_id_format: 0,
    })
}

async fn register_candidate(
    svc: &RecoveryServiceImpl,
    signing_key: &SigningKey,
    version: &str,
    requires_recovery_install: bool,
) -> TestResult<aios_recovery::service::proto::KernelCandidateProto> {
    let manifest = manifest(version, requires_recovery_install);
    Ok(svc
        .register_kernel_candidate(Request::new(register_request(&manifest, signing_key)?))
        .await?
        .into_inner())
}

async fn verified_candidate(
    svc: &RecoveryServiceImpl,
    signing_key: &SigningKey,
    version: &str,
    requires_recovery_install: bool,
) -> TestResult<aios_recovery::service::proto::KernelCandidateProto> {
    let registered =
        register_candidate(svc, signing_key, version, requires_recovery_install).await?;
    Ok(svc
        .verify_kernel_candidate(Request::new(VerifyKernelCandidateRequest {
            schema_version: SCHEMA_VERSION.to_owned(),
            candidate_id: registered.candidate_id.clone(),
            action_id_proto: Vec::new(),
            action_id_format: 0,
        }))
        .await?
        .into_inner())
}

async fn active_candidate(
    svc: &RecoveryServiceImpl,
    signing_key: &SigningKey,
    version: &str,
) -> TestResult<aios_recovery::service::proto::KernelCandidateProto> {
    let verified = verified_candidate(svc, signing_key, version, false).await?;
    Ok(svc
        .activate_kernel_candidate(Request::new(ActivateKernelCandidateRequest {
            schema_version: SCHEMA_VERSION.to_owned(),
            candidate_id: verified.candidate_id.clone(),
            action_id_proto: Vec::new(),
            action_id_format: 0,
        }))
        .await?
        .into_inner())
}

async fn pick_port() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind random port");
    let addr = listener.local_addr().expect("local_addr");
    drop(listener);
    addr
}

async fn spawn_server(
    svc: RecoveryServiceImpl,
) -> (SocketAddr, oneshot::Sender<()>, tokio::task::JoinHandle<()>) {
    let addr = pick_port().await;
    let (tx, rx) = oneshot::channel::<()>();
    let server =
        tonic::transport::Server::builder().add_service(RecoveryServiceGrpcServer::new(svc));
    let handle = tokio::spawn(async move {
        server
            .serve_with_shutdown(addr, async move {
                let _ = rx.await;
            })
            .await
            .expect("server task");
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    (addr, tx, handle)
}

#[tokio::test]
async fn enter_recovery_happy_path_returns_recovery_state_proto() -> TestResult {
    let Harness { svc, .. } = harness();

    let response = svc
        .enter_recovery(Request::new(operator_enter_request()))
        .await?
        .into_inner();

    assert_eq!(
        response.mode,
        i32::from(RecoveryModeProto::RecoveryModeRecovery)
    );
    assert_eq!(response.reason.as_deref(), Some("OPERATOR_INITIATED"));
    Ok(())
}

#[tokio::test]
async fn enter_recovery_when_already_active_maps_failed_precondition() -> TestResult {
    let Harness { svc, .. } = harness();
    let _state = svc
        .enter_recovery(Request::new(operator_enter_request()))
        .await?;

    let err = svc
        .enter_recovery(Request::new(operator_enter_request()))
        .await
        .expect_err("already-active recovery must reject");

    assert_eq!(err.code(), Code::FailedPrecondition);
    Ok(())
}

#[tokio::test]
async fn exit_recovery_returns_normal_mode() -> TestResult {
    let Harness { svc, boundary, .. } = harness();
    let _state = svc
        .enter_recovery(Request::new(operator_enter_request()))
        .await?;
    let token = boundary
        .current_exit_token()
        .await
        .ok_or_else(|| RecoveryError::Internal("missing exit token".to_owned()))?;

    let response = svc
        .exit_recovery(Request::new(ExitRecoveryRequestProto {
            schema_version: SCHEMA_VERSION.to_owned(),
            exit_token: token,
            action_id_proto: Vec::new(),
            action_id_format: 0,
        }))
        .await?
        .into_inner();

    assert_eq!(
        response.mode,
        i32::from(RecoveryModeProto::RecoveryModeNormal)
    );
    Ok(())
}

#[tokio::test]
async fn get_recovery_state_returns_current_mode() -> TestResult {
    let Harness { svc, .. } = harness();
    assert_eq!(
        svc.get_recovery_state(Request::new(GetRecoveryStateRequest {
            schema_version: SCHEMA_VERSION.to_owned(),
        }))
        .await?
        .into_inner()
        .mode,
        i32::from(RecoveryModeProto::RecoveryModeNormal)
    );

    let _state = svc
        .enter_recovery(Request::new(operator_enter_request()))
        .await?;

    assert_eq!(
        svc.get_recovery_state(Request::new(GetRecoveryStateRequest {
            schema_version: SCHEMA_VERSION.to_owned(),
        }))
        .await?
        .into_inner()
        .mode,
        i32::from(RecoveryModeProto::RecoveryModeRecovery)
    );
    Ok(())
}

#[tokio::test]
async fn register_kernel_candidate_valid_signature_returns_registered_candidate() -> TestResult {
    let Harness {
        svc, signing_key, ..
    } = harness();
    let manifest = manifest("linux-6.6.42-aios.1", false);

    let response = svc
        .register_kernel_candidate(Request::new(register_request(&manifest, &signing_key)?))
        .await?
        .into_inner();

    assert_eq!(
        response.state,
        i32::from(CandidateStateProto::CandidateRegistered)
    );
    assert!(response.candidate_id.starts_with(CandidateId::PREFIX));
    Ok(())
}

#[tokio::test]
async fn register_kernel_candidate_bad_signature_maps_permission_denied() -> TestResult {
    let Harness {
        svc, signing_key, ..
    } = harness();
    let manifest = manifest("linux-6.6.42-aios.2", false);
    let mut request = register_request(&manifest, &signing_key)?;
    request.signature_ed25519[0] ^= 0x01;

    let err = svc
        .register_kernel_candidate(Request::new(request))
        .await
        .expect_err("bad kernel signature must reject");

    assert_eq!(err.code(), Code::PermissionDenied);
    Ok(())
}

#[tokio::test]
async fn verify_kernel_candidate_registered_transitions_to_verified() -> TestResult {
    let Harness {
        svc, signing_key, ..
    } = harness();
    let registered = register_candidate(&svc, &signing_key, "linux-6.6.42-aios.3", false).await?;

    let verified = svc
        .verify_kernel_candidate(Request::new(VerifyKernelCandidateRequest {
            schema_version: SCHEMA_VERSION.to_owned(),
            candidate_id: registered.candidate_id,
            action_id_proto: Vec::new(),
            action_id_format: 0,
        }))
        .await?
        .into_inner();

    assert_eq!(
        verified.state,
        i32::from(CandidateStateProto::CandidateVerified)
    );
    Ok(())
}

#[tokio::test]
async fn activate_kernel_candidate_requiring_recovery_outside_recovery_rejects() -> TestResult {
    let Harness {
        svc, signing_key, ..
    } = harness();
    let verified = verified_candidate(&svc, &signing_key, "linux-6.6.42-aios.4", true).await?;

    let err = svc
        .activate_kernel_candidate(Request::new(ActivateKernelCandidateRequest {
            schema_version: SCHEMA_VERSION.to_owned(),
            candidate_id: verified.candidate_id,
            action_id_proto: Vec::new(),
            action_id_format: 0,
        }))
        .await
        .expect_err("recovery-required activation must reject outside recovery");

    assert_eq!(err.code(), Code::FailedPrecondition);
    Ok(())
}

#[tokio::test]
async fn rollback_kernel_candidate_restores_previous_active() -> TestResult {
    let Harness {
        svc, signing_key, ..
    } = harness();
    let first = active_candidate(&svc, &signing_key, "linux-6.6.42-aios.5").await?;
    let second = active_candidate(&svc, &signing_key, "linux-6.6.42-aios.6").await?;

    let rolled_back = svc
        .rollback_kernel_candidate(Request::new(RollbackKernelCandidateRequest {
            schema_version: SCHEMA_VERSION.to_owned(),
            candidate_id: second.candidate_id,
            action_id_proto: Vec::new(),
            action_id_format: 0,
        }))
        .await?
        .into_inner();
    let active = svc
        .get_active_kernel(Request::new(GetActiveKernelRequest {
            schema_version: SCHEMA_VERSION.to_owned(),
        }))
        .await?
        .into_inner()
        .active
        .ok_or_else(|| RecoveryError::Internal("missing active candidate".to_owned()))?;

    assert_eq!(
        rolled_back.state,
        i32::from(CandidateStateProto::CandidateRollback)
    );
    assert_eq!(active.candidate_id, first.candidate_id);
    Ok(())
}

#[tokio::test]
async fn list_kernel_candidates_returns_all_registered_candidates() -> TestResult {
    let Harness {
        svc, signing_key, ..
    } = harness();
    let first = register_candidate(&svc, &signing_key, "linux-6.6.42-aios.7", false).await?;
    let second = register_candidate(&svc, &signing_key, "linux-6.6.42-aios.8", false).await?;

    let response = svc
        .list_kernel_candidates(Request::new(ListKernelCandidatesRequest {
            schema_version: SCHEMA_VERSION.to_owned(),
        }))
        .await?
        .into_inner();
    let ids: Vec<String> = response
        .candidates
        .into_iter()
        .map(|candidate| candidate.candidate_id)
        .collect();

    assert_eq!(ids.len(), 2);
    assert!(ids.contains(&first.candidate_id));
    assert!(ids.contains(&second.candidate_id));
    Ok(())
}

#[tokio::test]
async fn run_first_boot_provisioning_happy_path_completes_context() -> TestResult {
    let Harness { svc, .. } = harness();

    let response = svc
        .run_first_boot_provisioning(Request::new(RunFirstBootProvisioningRequest {
            schema_version: SCHEMA_VERSION.to_owned(),
            action_id_proto: Vec::new(),
            action_id_format: 0,
        }))
        .await?
        .into_inner();

    assert_eq!(
        response.status,
        i32::from(aios_recovery::service::proto::FirstBootStatusProto::FirstBootStatusCompleted)
    );
    assert_eq!(
        response.performed_phases.len(),
        aios_recovery::first_boot::FIRST_BOOT_PROVISIONING_PHASES.len()
    );
    Ok(())
}

#[tokio::test]
async fn get_first_boot_status_returns_current_context() -> TestResult {
    let Harness { svc, .. } = harness();

    let response = svc
        .get_first_boot_status(Request::new(GetFirstBootStatusRequest {
            schema_version: SCHEMA_VERSION.to_owned(),
        }))
        .await?
        .into_inner();

    assert_eq!(
        response.status,
        i32::from(aios_recovery::service::proto::FirstBootStatusProto::FirstBootStatusNotStarted)
    );
    assert!(response.boot_id.starts_with(BootId::PREFIX));
    Ok(())
}

#[tokio::test]
async fn check_recovery_mutation_denies_normal_mode_and_allows_recovery_mode() -> TestResult {
    let Harness { svc, .. } = harness();
    let request = || CheckRecoveryMutationRequest {
        schema_version: SCHEMA_VERSION.to_owned(),
        path: "/aios/system/policy/active.bundle".to_owned(),
        subject: "_system:recovery:operator".to_owned(),
        is_ai: false,
        action_id_proto: Vec::new(),
        action_id_format: 0,
    };

    let err = svc
        .check_recovery_mutation(Request::new(request()))
        .await
        .expect_err("recovery-only path must reject in normal mode");
    assert_eq!(err.code(), Code::PermissionDenied);

    let _state = svc
        .enter_recovery(Request::new(operator_enter_request()))
        .await?;
    let response = svc
        .check_recovery_mutation(Request::new(request()))
        .await?
        .into_inner();
    assert!(response.allowed);
    Ok(())
}

#[test]
fn rust_proto_conversions_roundtrip() -> TestResult {
    let state = RecoveryState {
        mode: RecoveryMode::Recovery,
        entered_at: Some(fixed_time()?),
        exit_planned_at: None,
        reason: Some("BOOT_FAILURE_AUTO".to_owned()),
        operator_grant: Some("ovr_01HXY8K2JPQ7N3M4R5S6T7V8W9".to_owned()),
        active_sub_boundaries: Vec::new(),
    };
    assert_eq!(
        recovery_state_from_proto(recovery_state_to_proto(&state))?,
        state
    );

    let context = FirstBootContext {
        boot_id: BootId::new(),
        started_at: fixed_time()?,
        completed_at: Some(fixed_time()?),
        status: FirstBootStatus::Completed,
        performed_phases: vec![
            FirstBootPhase::StageInstallerMediaVerified,
            FirstBootPhase::StageFirstBootComplete,
        ],
    };
    assert_eq!(
        first_boot_context_from_proto(first_boot_context_to_proto(&context))?,
        context
    );

    let manifest = manifest("linux-6.6.42-aios.9", true);
    assert_eq!(
        kernel_manifest_from_proto(kernel_manifest_to_proto(&manifest))?,
        manifest
    );

    let candidate = KernelCandidate {
        candidate_id: CandidateId::new(),
        version: "linux-6.6.42-aios.9".to_owned(),
        kernel_blake3: blake3::hash(b"kernel image").to_hex().to_string(),
        signature_ed25519: vec![7; 64],
        signing_authority: AUTHORITY.to_owned(),
        registered_at: fixed_time()?,
        state: CandidateState::GatePassed,
        manifest,
    };
    assert_eq!(
        kernel_candidate_from_proto(kernel_candidate_to_proto(&candidate))?,
        candidate
    );

    let bundle = RecoveryBundle {
        bundle_id: "rb_01HXY8K2JPQ7N3M4R5S6T7V8W9".to_owned(),
        loaded_at: fixed_time()?,
        hard_deny_signatures: vec!["hard-deny:RecoveryRequiredForSystemMutation".to_owned()],
        override_signatures: vec!["override:STRONG_SOLO".to_owned()],
        signing_authority: "aios-recovery-root".to_owned(),
    };
    assert_eq!(
        recovery_bundle_from_proto(recovery_bundle_to_proto(&bundle))?,
        bundle
    );

    for state in [
        CandidateState::Building,
        CandidateState::Built,
        CandidateState::Gating,
        CandidateState::GatePassed,
        CandidateState::GateFailed,
        CandidateState::APromoted,
        CandidateState::BDemotedToA,
        CandidateState::Rollback,
        CandidateState::Retired,
    ] {
        assert_eq!(
            candidate_state_from_proto(candidate_state_to_proto(state))?,
            state
        );
    }

    assert_eq!(
        recovery_error_to_status(&RecoveryError::RecoveryNotActive).code(),
        Code::FailedPrecondition
    );
    assert_eq!(
        recovery_error_to_status(&RecoveryError::BundleSignatureInvalid).code(),
        Code::PermissionDenied
    );
    assert_eq!(
        recovery_error_to_status(&RecoveryError::CandidateNotFound(CandidateId::new())).code(),
        Code::NotFound
    );
    Ok(())
}

#[tokio::test]
async fn tonic_channel_smoke_test_enters_recovery() -> TestResult {
    let Harness { svc, .. } = harness();
    let (addr, shutdown, handle) = spawn_server(svc).await;

    let response = {
        let mut client = RecoveryServiceClient::connect(format!("http://{addr}")).await?;
        client
            .enter_recovery(operator_enter_request())
            .await?
            .into_inner()
    };

    assert_eq!(
        response.mode,
        i32::from(RecoveryModeProto::RecoveryModeRecovery)
    );
    let _ = shutdown.send(());
    handle.await?;
    Ok(())
}
