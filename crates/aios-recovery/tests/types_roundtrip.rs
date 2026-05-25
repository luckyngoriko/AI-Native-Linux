//! Round-trip coverage for the T-074 S9 typed core skeleton.

use std::error::Error;

use aios_action::ActionId;
use aios_recovery::{
    BootId, BootPhase, CandidateId, CandidateState, FirstBootContext, FirstBootPhase,
    FirstBootStatus, KernelCandidate, KernelManifest, RecoveryBundle, RecoveryError, RecoveryMode,
    RecoveryState,
};
use chrono::{DateTime, Utc};
use strum::EnumCount;

type TestResult<T = ()> = Result<T, Box<dyn Error + Send + Sync>>;

fn fixed_time() -> TestResult<DateTime<Utc>> {
    Ok(DateTime::parse_from_rfc3339("2026-05-25T10:00:00Z")?.with_timezone(&Utc))
}

fn recovery_state() -> TestResult<RecoveryState> {
    Ok(RecoveryState {
        mode: RecoveryMode::Recovery,
        entered_at: Some(fixed_time()?),
        exit_planned_at: None,
        reason: Some("operator selected recovery".to_owned()),
        operator_grant: Some("ovr_01HXY8K2JPQ7N3M4R5S6T7V8W9".to_owned()),
    })
}

fn first_boot_context() -> TestResult<FirstBootContext> {
    Ok(FirstBootContext {
        boot_id: BootId::new(),
        started_at: fixed_time()?,
        completed_at: Some(fixed_time()?),
        status: FirstBootStatus::Completed,
        performed_phases: vec![
            FirstBootPhase::StageInstallerMediaVerified,
            FirstBootPhase::StageFirstBootComplete,
        ],
    })
}

fn kernel_manifest() -> KernelManifest {
    KernelManifest {
        version: "linux-6.6.42-aios.1".to_owned(),
        min_aios_version: "0.1.0".to_owned(),
        requires_recovery_install: true,
        verification_intent: Some("verify dedicated kernel gates".to_owned()),
        tags: vec!["KSPP_STRICT".to_owned(), "RECOVERY_REHEARSAL".to_owned()],
    }
}

fn kernel_candidate() -> TestResult<KernelCandidate> {
    Ok(KernelCandidate {
        candidate_id: CandidateId::new(),
        version: "linux-6.6.42-aios.1".to_owned(),
        kernel_blake3: blake3::hash(b"kernel image").to_hex().to_string(),
        signature_ed25519: vec![7; 64],
        signing_authority: "aios-root".to_owned(),
        registered_at: fixed_time()?,
        state: CandidateState::GatePassed,
        manifest: kernel_manifest(),
    })
}

fn recovery_bundle() -> TestResult<RecoveryBundle> {
    Ok(RecoveryBundle {
        bundle_id: "rb_01HXY8K2JPQ7N3M4R5S6T7V8W9".to_owned(),
        loaded_at: fixed_time()?,
        hard_deny_signatures: vec!["hard-deny:RecoveryRequiredForSystemMutation".to_owned()],
        override_signatures: vec!["override:STRONG_SOLO".to_owned()],
        signing_authority: "aios-recovery-root".to_owned(),
    })
}

#[test]
fn recovery_mode_count_matches_s91() {
    assert_eq!(RecoveryMode::COUNT, 4);
}

#[test]
fn boot_phase_count_matches_t074_contract() {
    assert_eq!(BootPhase::COUNT, 5);
}

#[test]
fn first_boot_phase_count_matches_s92() {
    assert_eq!(FirstBootPhase::COUNT, 15);
}

#[test]
fn candidate_state_count_matches_s93() {
    assert_eq!(CandidateState::COUNT, 9);
}

#[test]
fn recovery_state_roundtrips_through_json() -> TestResult {
    let state = recovery_state()?;
    let encoded = serde_json::to_string(&state)?;
    let decoded: RecoveryState = serde_json::from_str(&encoded)?;
    assert_eq!(decoded, state);
    Ok(())
}

#[test]
fn first_boot_context_roundtrips_through_json() -> TestResult {
    let context = first_boot_context()?;
    let encoded = serde_json::to_string(&context)?;
    let decoded: FirstBootContext = serde_json::from_str(&encoded)?;
    assert_eq!(decoded, context);
    Ok(())
}

#[test]
fn kernel_candidate_roundtrips_through_json() -> TestResult {
    let candidate = kernel_candidate()?;
    let encoded = serde_json::to_string(&candidate)?;
    let decoded: KernelCandidate = serde_json::from_str(&encoded)?;
    assert_eq!(decoded, candidate);
    Ok(())
}

#[test]
fn kernel_manifest_roundtrips_through_json() -> TestResult {
    let manifest = kernel_manifest();
    let encoded = serde_json::to_string(&manifest)?;
    let decoded: KernelManifest = serde_json::from_str(&encoded)?;
    assert_eq!(decoded, manifest);
    Ok(())
}

#[test]
fn recovery_bundle_roundtrips_through_json() -> TestResult {
    let bundle = recovery_bundle()?;
    let encoded = serde_json::to_string(&bundle)?;
    let decoded: RecoveryBundle = serde_json::from_str(&encoded)?;
    assert_eq!(decoded, bundle);
    Ok(())
}

#[test]
fn newtype_ids_use_spec_prefixes() {
    let boot_id = BootId::new();
    let candidate_id = CandidateId::new();

    assert!(boot_id.as_str().starts_with("boot_"));
    assert_eq!(boot_id.as_str().len(), "boot_".len() + 26);
    assert!(candidate_id.as_str().starts_with("kc_"));
    assert_eq!(candidate_id.as_str().len(), "kc_".len() + 26);
}

#[test]
fn recovery_error_display_strings_are_present() {
    let errors = [
        RecoveryError::RecoveryNotActive,
        RecoveryError::AlreadyInRecovery,
        RecoveryError::BundleSignatureInvalid,
        RecoveryError::BundleUnknownAuthority("unknown-root".to_owned()),
        RecoveryError::FirstBootAlreadyCompleted,
        RecoveryError::CandidateNotFound(CandidateId::new()),
        RecoveryError::InvalidCandidateTransition {
            from: CandidateState::Gating,
            to: CandidateState::APromoted,
        },
        RecoveryError::KernelSignatureInvalid,
        RecoveryError::Internal("clock drift".to_owned()),
    ];

    for error in errors {
        assert!(!error.to_string().is_empty());
    }
}

#[test]
fn action_id_cross_crate_import_and_use_compiles() -> TestResult {
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct RecoveryActionFixture {
        action_id: ActionId,
        state: RecoveryState,
    }

    let action_id = ActionId::new();
    let fixture = RecoveryActionFixture {
        action_id: action_id.clone(),
        state: recovery_state()?,
    };

    assert_eq!(fixture.action_id, action_id);
    assert_eq!(fixture.state.mode, RecoveryMode::Recovery);
    Ok(())
}
