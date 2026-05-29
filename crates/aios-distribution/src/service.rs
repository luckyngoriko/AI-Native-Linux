//! gRPC `RepositoryService` + `PublisherService` surfaces (T-195, S11.1).
//!
//! `RepositoryServiceImpl` and `PublisherServiceImpl` implement the
//! tonic-generated server traits, delegating to the in-memory T-187..T-194
//! distribution logic. Each method converts proto request → Rust domain types,
//! calls the backing logic, and converts the result back to a proto reply.
//!
//! # Pattern
//!
//! Mirrors the `aios-integration` service module exactly:
//! - Generated proto module gated with `#[allow(clippy::all, clippy::pedantic)]`
//! - Service structs hold `Arc<Mutex<...>>` state
//! - Direct delegation to crate public APIs — no behavioural change.

#![allow(
    clippy::result_large_err,
    clippy::significant_drop_tightening,
    clippy::missing_const_for_fn,
    unused_imports
)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use tonic::{Request, Response, Status};

use crate::catalog::{PublisherCatalog, SigningKeyCatalog};
use crate::cve_binding::{apply_cve_binding, PackageCveBinding};
use crate::deplatform::{
    apply_deplatform, health_check_quarantine, InstalledPackageRecord, PublisherDeplatformEvent,
};
use crate::downgrade::VersionMonotonicCounter;
use crate::ids::{PackageId, PackageSigningKeyId, PublisherRootId};
use crate::install_pipeline::{run_install, InMemoryPipelineDeps, InstallOutcome};
use crate::install_state::{PackageInstallState, PackageVerificationResult};
use crate::manifest::{NetworkManifestRef, PackageManifest, SandboxProfileRef};
use crate::manifest_pipeline::verify_manifest;
use crate::mirror::MirrorSemantic;
use crate::mirror_blacklist::MirrorBlacklist;
use crate::mirror_policy::MirrorEndpoint;
use crate::package_kind::{InstallScope, PackageKind};
use crate::repository::{RepositoryKind, UpdateChannel};
use crate::rotation::{apply_publisher_rotation, PublisherRotationEvent};
use crate::takedown::TakedownReason;
use crate::trust::PublisherTrustLevel;
use crate::trust_chain::{AiosRootKey, LinkSignature, SignedPayload};
use crate::verifier::TrustChainVerifier;

// ── Generated proto module ─────────────────────────────────────────────────

/// Tonic-generated server/client stubs + proto messages.
#[allow(
    clippy::all,
    clippy::pedantic,
    clippy::nursery,
    clippy::cargo,
    missing_docs,
    unused_qualifications,
    clippy::default_trait_access,
    clippy::derive_partial_eq_without_eq,
    clippy::doc_markdown,
    clippy::empty_line_after_doc_comments,
    clippy::large_enum_variant,
    clippy::missing_const_for_fn,
    clippy::missing_errors_doc,
    clippy::module_name_repetitions,
    clippy::must_use_candidate,
    clippy::needless_borrow,
    clippy::option_option,
    clippy::similar_names,
    clippy::too_many_lines,
    clippy::trivially_copy_pass_by_ref,
    clippy::unused_async,
    clippy::use_self,
    clippy::wildcard_imports
)]
pub mod pb {
    tonic::include_proto!("aios.distribution.v1alpha1");
}

use pb::publisher_service_server::PublisherService;
use pb::repository_service_server::RepositoryService;

// ── Proto ↔ Rust enum conversions ──────────────────────────────────────────

#[allow(
    clippy::wildcard_imports,
    clippy::missing_const_for_fn,
    clippy::match_same_arms,
    clippy::manual_find,
    clippy::cast_lossless,
    clippy::unnecessary_map_or,
    dead_code,
    unused
)]
mod conv {
    use super::pb;
    use super::{
        DateTime, NetworkManifestRef, PackageManifest, PackageSigningKeyId, PublisherRootId,
        SandboxProfileRef, SignedPayload, Utc,
    };
    use crate::{
        install_state::PackageInstallState, install_state::PackageVerificationResult,
        mirror::MirrorSemantic, package_kind::InstallScope, package_kind::PackageKind,
        repository::RepositoryKind, repository::UpdateChannel, takedown::TakedownReason,
        trust::PublisherTrustLevel,
    };

    // PublisherTrustLevel
    pub fn rust_trust_from_proto(v: i32) -> PublisherTrustLevel {
        match v {
            1 => PublisherTrustLevel::AiosRoot,
            2 => PublisherTrustLevel::Verified,
            3 => PublisherTrustLevel::Community,
            4 => PublisherTrustLevel::Deprecated,
            5 => PublisherTrustLevel::Deplatformed,
            _ => PublisherTrustLevel::Community,
        }
    }

    pub fn proto_trust_from_rust(t: PublisherTrustLevel) -> i32 {
        match t {
            PublisherTrustLevel::AiosRoot => 1,
            PublisherTrustLevel::Verified => 2,
            PublisherTrustLevel::Community => 3,
            PublisherTrustLevel::Deprecated => 4,
            PublisherTrustLevel::Deplatformed => 5,
        }
    }

    // RepositoryKind
    pub fn rust_repo_kind_from_proto(v: i32) -> RepositoryKind {
        match v {
            1 => RepositoryKind::AiosRootRepo,
            2 => RepositoryKind::AiosVerifiedRepo,
            3 => RepositoryKind::AiosCommunityRepo,
            4 => RepositoryKind::AiosRecoveryRepo,
            5 => RepositoryKind::ExternalBridge,
            _ => RepositoryKind::AiosVerifiedRepo,
        }
    }

    // UpdateChannel
    pub fn rust_channel_from_proto(v: i32) -> UpdateChannel {
        match v {
            1 => UpdateChannel::Stable,
            2 => UpdateChannel::Beta,
            3 => UpdateChannel::RecoveryCritical,
            4 => UpdateChannel::DeprecatedRetention,
            _ => UpdateChannel::Stable,
        }
    }

    // PackageKind
    pub fn rust_package_kind_from_proto(v: i32) -> PackageKind {
        match v {
            1 => PackageKind::App,
            2 => PackageKind::Agent,
            3 => PackageKind::Theme,
            4 => PackageKind::InvariantBundle,
            5 => PackageKind::PolicyBundle,
            6 => PackageKind::IdentityBundle,
            7 => PackageKind::KernelCandidate,
            8 => PackageKind::Adapter,
            9 => PackageKind::CapabilityCatalogDelta,
            _ => PackageKind::App,
        }
    }

    // InstallScope
    pub fn rust_scope_from_proto(v: i32) -> InstallScope {
        match v {
            1 => InstallScope::SystemOnly,
            2 => InstallScope::GroupScoped,
            3 => InstallScope::UserScoped,
            4 => InstallScope::Either,
            _ => InstallScope::Either,
        }
    }

    // PackageVerificationResult
    pub fn proto_verification_from_rust(r: PackageVerificationResult) -> i32 {
        match r {
            PackageVerificationResult::VerifiedAiosRoot => 1,
            PackageVerificationResult::VerifiedPublisher => 2,
            PackageVerificationResult::SignatureFailed => 3,
            PackageVerificationResult::TrustChainBroken => 4,
            PackageVerificationResult::TrustChainTooDeep => 5,
            PackageVerificationResult::PublisherDeplatformed => 6,
            PackageVerificationResult::HashMismatch => 7,
            PackageVerificationResult::ManifestForged => 8,
            PackageVerificationResult::RepositoryKindMismatch => 9,
            PackageVerificationResult::CapabilityLie => 10,
            PackageVerificationResult::BundleTampered => 11,
        }
    }

    // PackageInstallState
    pub fn proto_install_state_from_rust(s: PackageInstallState) -> i32 {
        match s {
            PackageInstallState::Draft => 1,
            PackageInstallState::Validating => 2,
            PackageInstallState::AwaitingApproval => 3,
            PackageInstallState::Approved => 4,
            PackageInstallState::Installing => 5,
            PackageInstallState::Active => 6,
            PackageInstallState::Quarantined => 7,
            PackageInstallState::Uninstalling => 8,
            PackageInstallState::Removed => 9,
            PackageInstallState::InstallFailed => 10,
        }
    }

    pub fn rust_install_state_from_proto(v: i32) -> PackageInstallState {
        match v {
            1 => PackageInstallState::Draft,
            2 => PackageInstallState::Validating,
            3 => PackageInstallState::AwaitingApproval,
            4 => PackageInstallState::Approved,
            5 => PackageInstallState::Installing,
            6 => PackageInstallState::Active,
            7 => PackageInstallState::Quarantined,
            8 => PackageInstallState::Uninstalling,
            9 => PackageInstallState::Removed,
            10 => PackageInstallState::InstallFailed,
            _ => PackageInstallState::Draft,
        }
    }

    // MirrorSemantic
    pub fn rust_mirror_semantic_from_proto(v: i32) -> MirrorSemantic {
        match v {
            1 => MirrorSemantic::Origin,
            2 => MirrorSemantic::Cached,
            3 => MirrorSemantic::Local,
            _ => MirrorSemantic::Origin,
        }
    }

    // TakedownReason
    pub fn rust_takedown_reason_from_proto(v: i32) -> TakedownReason {
        match v {
            1 => TakedownReason::MaliciousBehaviorDetected,
            2 => TakedownReason::SupplyChainCompromise,
            3 => TakedownReason::CapabilityLieDetected,
            4 => TakedownReason::LegalRequirement,
            5 => TakedownReason::PublisherRequest,
            6 => TakedownReason::KeyCompromise,
            7 => TakedownReason::AbandonedAfterInactiveTtl,
            _ => TakedownReason::PublisherRequest,
        }
    }

    // Build a PackageManifest from a VerifyManifestRequest proto.
    pub fn manifest_from_proto(req: &pb::VerifyManifestRequest) -> PackageManifest {
        let issued_at = DateTime::parse_from_rfc3339(&req.issued_at_rfc3339)
            .map_or_else(|_| Utc::now(), |dt| dt.with_timezone(&Utc));

        PackageManifest {
            package_id: req.package_id.clone(),
            version: req.version.clone(),
            kind: rust_package_kind_from_proto(req.kind),
            publisher_trust: rust_trust_from_proto(req.publisher_trust),
            publisher_root_id: PublisherRootId(req.publisher_root_id.clone()),
            package_signing_key_id: PackageSigningKeyId(req.package_signing_key_id.clone()),
            content_hash: req.content_hash.clone(),
            manifest_canonical_hash: req.manifest_canonical_hash.clone(),
            ed25519_signature: req.ed25519_signature.clone(),
            installable_scope: rust_scope_from_proto(req.installable_scope),
            required_sandbox: SandboxProfileRef(req.required_sandbox.clone()),
            declared_capabilities: req.declared_capabilities.clone(),
            network_manifest: NetworkManifestRef(req.network_manifest.clone()),
            issued_at,
            eol_at: None,
            channel: rust_channel_from_proto(req.channel),
            originating_repository: rust_repo_kind_from_proto(req.originating_repository),
            mirror_url: req.mirror_url.clone(),
            mirror_semantic: rust_mirror_semantic_from_proto(req.mirror_semantic),
        }
    }

    // Enrich a manifest with catalog-based trust for verify_manifest (the
    // publisher_trust on the wire is the manifest's claim; verify crosses it
    // with the catalog).
    pub fn manifest_with_catalog_trust(
        mut manifest: PackageManifest,
        catalog_trust: PublisherTrustLevel,
    ) -> PackageManifest {
        manifest.publisher_trust = catalog_trust;
        manifest
    }

    // Build a signed payload from the VerifyTrustChainRequest.
    pub fn signed_payload_from_proto(req: &pb::VerifyTrustChainRequest) -> SignedPayload {
        SignedPayload {
            payload: req.payload.clone(),
            signature: req.payload_signature.clone(),
            package_signing_key_id: PackageSigningKeyId(req.package_signing_key_id.clone()),
            publisher_root_id: PublisherRootId(req.publisher_root_id.clone()),
        }
    }
}

// ── RepositoryServiceImpl ──────────────────────────────────────────────────

/// gRPC `RepositoryService` implementation delegating to in-memory T-187..T-194 logic.
#[derive(Clone)]
pub struct RepositoryServiceImpl {
    aios_root: Arc<AiosRootKey>,
    publisher_catalog: Arc<Mutex<PublisherCatalog>>,
    signing_catalogs: Arc<Mutex<HashMap<String, SigningKeyCatalog>>>,
    downgrade_counter: Arc<Mutex<VersionMonotonicCounter>>,
    mirror_blacklist: Arc<Mutex<MirrorBlacklist>>,
}

impl std::fmt::Debug for RepositoryServiceImpl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RepositoryServiceImpl")
            .finish_non_exhaustive()
    }
}

impl RepositoryServiceImpl {
    /// Construct a new `RepositoryServiceImpl` with the required state.
    #[must_use]
    pub fn new(
        aios_root: Arc<AiosRootKey>,
        publisher_catalog: Arc<Mutex<PublisherCatalog>>,
        signing_catalogs: Arc<Mutex<HashMap<String, SigningKeyCatalog>>>,
        downgrade_counter: Arc<Mutex<VersionMonotonicCounter>>,
        mirror_blacklist: Arc<Mutex<MirrorBlacklist>>,
    ) -> Self {
        Self {
            aios_root,
            publisher_catalog,
            signing_catalogs,
            downgrade_counter,
            mirror_blacklist,
        }
    }

    /// Builds a [`TrustChainVerifier`] from the current catalog state.
    fn make_verifier<'a>(
        &'a self,
        catalog: &'a PublisherCatalog,
        signing: &'a HashMap<String, SigningKeyCatalog>,
    ) -> TrustChainVerifier<'a> {
        TrustChainVerifier::new(&self.aios_root, catalog, signing)
    }
}

#[tonic::async_trait]
impl RepositoryService for RepositoryServiceImpl {
    async fn verify_trust_chain(
        &self,
        request: Request<pb::VerifyTrustChainRequest>,
    ) -> Result<Response<pb::VerificationReply>, Status> {
        let r = request.into_inner();
        let payload = conv::signed_payload_from_proto(&r);
        let publisher_root_link_sig = LinkSignature(r.publisher_root_link_sig);
        let signing_key_link_sig = LinkSignature(r.signing_key_link_sig);
        let now = Utc::now();

        let (catalog, signing) = {
            let cat = self
                .publisher_catalog
                .lock()
                .map_err(|e| Status::internal(format!("lock: {e}")))?;
            let sig = self
                .signing_catalogs
                .lock()
                .map_err(|e| Status::internal(format!("lock: {e}")))?;
            // We need to clone so we don't hold the lock across the await point.
            // Since these are trusted to be quick, we clone here.
            (cat.clone(), sig.clone())
        };

        let verifier = self.make_verifier(&catalog, &signing);
        let result = verifier.verify(
            &payload,
            &publisher_root_link_sig,
            &signing_key_link_sig,
            now,
            now,
        );

        Ok(Response::new(pb::VerificationReply {
            result: conv::proto_verification_from_rust(result),
        }))
    }

    async fn verify_manifest(
        &self,
        request: Request<pb::VerifyManifestRequest>,
    ) -> Result<Response<pb::VerificationReply>, Status> {
        let r = request.into_inner();
        let manifest = conv::manifest_from_proto(&r);
        let publisher_root_link_sig = LinkSignature(r.publisher_root_link_sig);
        let signing_key_link_sig = LinkSignature(r.signing_key_link_sig);
        let catalog_trust = conv::rust_trust_from_proto(r.publisher_trust_as_catalog);
        let manifest = conv::manifest_with_catalog_trust(manifest, catalog_trust);
        let now = Utc::now();

        let (catalog, signing) = {
            let cat = self
                .publisher_catalog
                .lock()
                .map_err(|e| Status::internal(format!("lock: {e}")))?;
            let sig = self
                .signing_catalogs
                .lock()
                .map_err(|e| Status::internal(format!("lock: {e}")))?;
            (cat.clone(), sig.clone())
        };

        let verifier = self.make_verifier(&catalog, &signing);
        let result = verify_manifest(
            &manifest,
            &verifier,
            &manifest.content_hash,
            &publisher_root_link_sig,
            &signing_key_link_sig,
            now,
        );

        Ok(Response::new(pb::VerificationReply {
            result: conv::proto_verification_from_rust(result),
        }))
    }

    async fn run_install(
        &self,
        request: Request<pb::RunInstallRequest>,
    ) -> Result<Response<pb::InstallOutcomeReply>, Status> {
        let r = request.into_inner();
        let manifest_req = r
            .manifest
            .as_ref()
            .ok_or_else(|| Status::invalid_argument("manifest required"))?;
        let manifest = conv::manifest_from_proto(manifest_req);
        let publisher_root_link_sig = LinkSignature(manifest_req.publisher_root_link_sig.clone());
        let signing_key_link_sig = LinkSignature(manifest_req.signing_key_link_sig.clone());
        let catalog_trust = conv::rust_trust_from_proto(manifest_req.publisher_trust_as_catalog);
        let manifest = conv::manifest_with_catalog_trust(manifest, catalog_trust);
        let now = Utc::now();

        let (catalog, signing) = {
            let cat = self
                .publisher_catalog
                .lock()
                .map_err(|e| Status::internal(format!("lock: {e}")))?;
            let sig = self
                .signing_catalogs
                .lock()
                .map_err(|e| Status::internal(format!("lock: {e}")))?;
            (cat.clone(), sig.clone())
        };

        let verifier = self.make_verifier(&catalog, &signing);

        let deps = InMemoryPipelineDeps {
            fetch_success: r.fetch_success,
            fetched_content_hash: r.fetched_content_hash.clone(),
            fetched_mirror_semantic: conv::rust_mirror_semantic_from_proto(
                r.fetched_mirror_semantic,
            ),
            ..Default::default()
        };

        let outcome: InstallOutcome = run_install(
            &manifest,
            &verifier,
            &deps,
            &publisher_root_link_sig,
            &signing_key_link_sig,
            now,
        );

        let failed_step_str = outcome
            .failed_step
            .map(|s| s.label().to_string())
            .unwrap_or_default();

        Ok(Response::new(pb::InstallOutcomeReply {
            final_state: conv::proto_install_state_from_rust(outcome.final_state),
            result: conv::proto_verification_from_rust(outcome.result),
            failed_step: failed_step_str,
        }))
    }

    async fn check_downgrade(
        &self,
        request: Request<pb::CheckDowngradeRequest>,
    ) -> Result<Response<pb::DowngradeReply>, Status> {
        let r = request.into_inner();
        let package_id = PackageId(r.package_id);
        let version = crate::version::parse(&r.version)
            .map_err(|e| Status::invalid_argument(format!("invalid version: {e}")))?;

        let mut counter = self
            .downgrade_counter
            .lock()
            .map_err(|e| Status::internal(format!("lock: {e}")))?;

        match counter.check_and_record(&package_id, &version) {
            Ok(()) => Ok(Response::new(pb::DowngradeReply {
                allowed: true,
                code: "ok".into(),
            })),
            Err(e) => Ok(Response::new(pb::DowngradeReply {
                allowed: false,
                code: format!("{:?}", e.code()),
            })),
        }
    }

    async fn resolve_mirror(
        &self,
        request: Request<pb::ResolveMirrorRequest>,
    ) -> Result<Response<pb::MirrorReply>, Status> {
        let r = request.into_inner();
        let semantic = conv::rust_mirror_semantic_from_proto(r.mirror_semantic);
        let endpoint = MirrorEndpoint::new(&r.mirror_url, semantic);

        let blacklist = self
            .mirror_blacklist
            .lock()
            .map_err(|e| Status::internal(format!("lock: {e}")))?;

        if let Err(e) = blacklist.pre_reject(&endpoint, Utc::now()) {
            return Err(Status::unavailable(format!("mirror blacklisted: {e}")));
        }

        Ok(Response::new(pb::MirrorReply {
            endpoint_url: r.mirror_url,
            semantic: r.mirror_semantic,
        }))
    }

    async fn evaluate_cve(
        &self,
        request: Request<pb::EvaluateCveRequest>,
    ) -> Result<Response<pb::CveReply>, Status> {
        let r = request.into_inner();
        let package_id = PackageId(r.package_id);
        let binding = PackageCveBinding::new(package_id, r.cve_id, r.cvss);

        let mut state = PackageInstallState::Active;
        let action = apply_cve_binding(&mut state, &binding);

        let action_str = match action {
            crate::cve_binding::CveAction::Recorded => "Recorded",
            crate::cve_binding::CveAction::Notified => "Notified",
            crate::cve_binding::CveAction::QuarantineCandidate => "QuarantineCandidate",
            crate::cve_binding::CveAction::Quarantined => "Quarantined",
        };

        Ok(Response::new(pb::CveReply {
            action: action_str.into(),
        }))
    }
}

// ── PublisherServiceImpl ───────────────────────────────────────────────────

/// gRPC `PublisherService` implementation delegating to in-memory T-188..T-192 logic.
#[derive(Clone)]
pub struct PublisherServiceImpl {
    aios_root: Arc<AiosRootKey>,
    publisher_catalog: Arc<Mutex<PublisherCatalog>>,
    signing_catalog: Arc<Mutex<SigningKeyCatalog>>,
    #[allow(dead_code)]
    installed: Arc<Mutex<Vec<InstalledPackageRecord>>>,
}

impl std::fmt::Debug for PublisherServiceImpl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PublisherServiceImpl")
            .finish_non_exhaustive()
    }
}

impl PublisherServiceImpl {
    /// Construct a new `PublisherServiceImpl` with the required state.
    #[must_use]
    pub fn new(
        aios_root: Arc<AiosRootKey>,
        publisher_catalog: Arc<Mutex<PublisherCatalog>>,
        signing_catalog: Arc<Mutex<SigningKeyCatalog>>,
        installed: Arc<Mutex<Vec<InstalledPackageRecord>>>,
    ) -> Self {
        Self {
            aios_root,
            publisher_catalog,
            signing_catalog,
            installed,
        }
    }
}

#[tonic::async_trait]
impl PublisherService for PublisherServiceImpl {
    async fn get_publisher(
        &self,
        request: Request<pb::GetPublisherRequest>,
    ) -> Result<Response<pb::PublisherReply>, Status> {
        let r = request.into_inner();
        let publisher_root_id = PublisherRootId(r.publisher_root_id);

        let catalog = self
            .publisher_catalog
            .lock()
            .map_err(|e| Status::internal(format!("lock: {e}")))?;

        let entry = catalog.lookup(&publisher_root_id).ok_or_else(|| {
            Status::not_found(format!("publisher {publisher_root_id:?} not found"))
        })?;

        let retired = entry.retired_at.is_some();
        let trust_level = conv::proto_trust_from_rust(entry.trust_level);

        Ok(Response::new(pb::PublisherReply {
            trust_level,
            retired,
        }))
    }

    async fn list_publishers(
        &self,
        _request: Request<pb::ListPublishersRequest>,
    ) -> Result<Response<pb::ListPublishersReply>, Status> {
        let catalog = self
            .publisher_catalog
            .lock()
            .map_err(|e| Status::internal(format!("lock: {e}")))?;

        let publishers: Vec<pb::PublisherEntry> = catalog
            .entries()
            .iter()
            .map(|e| pb::PublisherEntry {
                publisher_root_id: e.publisher_root_id.0.clone(),
                trust_level: conv::proto_trust_from_rust(e.trust_level),
                retired: e.retired_at.is_some(),
            })
            .collect();

        Ok(Response::new(pb::ListPublishersReply { publishers }))
    }

    async fn rotate_publisher_key(
        &self,
        request: Request<pb::RotateRequest>,
    ) -> Result<Response<pb::RotateReply>, Status> {
        let r = request.into_inner();

        let rotated_at = DateTime::parse_from_rfc3339(&r.rotated_at_rfc3339)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| Status::invalid_argument(format!("invalid rotated_at: {e}")))?;

        let compromise_window_start = if r.compromise_window_start_rfc3339.is_empty() {
            None
        } else {
            Some(
                DateTime::parse_from_rfc3339(&r.compromise_window_start_rfc3339)
                    .map(|dt| dt.with_timezone(&Utc))
                    .map_err(|e| {
                        Status::invalid_argument(format!("invalid compromise_window_start: {e}"))
                    })?,
            )
        };

        let reason = conv::rust_takedown_reason_from_proto(r.reason);

        let event = PublisherRotationEvent {
            publisher_root_id: PublisherRootId(r.publisher_root_id),
            old_public_key: r.old_public_key,
            new_public_key: r.new_public_key,
            old_root_signature_over_new: r.old_root_signature_over_new,
            aios_root_signature: r.aios_root_signature,
            rotated_at,
            reason,
            compromise_window_start,
        };

        let now = Utc::now();

        let (mut catalog, mut signing_cat) = {
            let cat = self
                .publisher_catalog
                .lock()
                .map_err(|e| Status::internal(format!("lock: {e}")))?;
            let sig = self
                .signing_catalog
                .lock()
                .map_err(|e| Status::internal(format!("lock: {e}")))?;
            (cat.clone(), sig.clone())
        };

        let aios_root = (*self.aios_root).clone();

        match apply_publisher_rotation(&mut catalog, &mut signing_cat, &event, &aios_root, now) {
            Ok(outcome) => {
                // Write back
                {
                    let mut cat_lock = self
                        .publisher_catalog
                        .lock()
                        .map_err(|e| Status::internal(format!("lock: {e}")))?;
                    *cat_lock = catalog;
                }
                {
                    let mut sig_lock = self
                        .signing_catalog
                        .lock()
                        .map_err(|e| Status::internal(format!("lock: {e}")))?;
                    *sig_lock = signing_cat;
                }

                let revoked_ids: Vec<String> = outcome
                    .revoked_signing_key_ids
                    .iter()
                    .map(|id| id.0.clone())
                    .collect();

                Ok(Response::new(pb::RotateReply {
                    ok: true,
                    revoked_signing_key_ids: revoked_ids,
                    reactive: outcome.reactive,
                }))
            }
            Err(e) => Err(Status::internal(format!("rotation failed: {e}"))),
        }
    }

    async fn deplatform_publisher(
        &self,
        request: Request<pb::DeplatformRequest>,
    ) -> Result<Response<pb::DeplatformReply>, Status> {
        let r = request.into_inner();

        let deplatformed_at = DateTime::parse_from_rfc3339(&r.deplatformed_at_rfc3339)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| Status::invalid_argument(format!("invalid deplatformed_at: {e}")))?;

        let grace_period_ends_at = DateTime::parse_from_rfc3339(&r.grace_period_ends_at_rfc3339)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| Status::invalid_argument(format!("invalid grace_period_ends_at: {e}")))?;

        let reason = conv::rust_takedown_reason_from_proto(r.reason);

        let event = PublisherDeplatformEvent {
            publisher_root_id: PublisherRootId(r.publisher_root_id),
            reason,
            deplatformed_at,
            grace_period_ends_at,
            evidence_pointer: r.evidence_pointer,
            aios_root_signature: r.aios_root_signature,
            extended: false,
        };

        let mut catalog = self
            .publisher_catalog
            .lock()
            .map_err(|e| Status::internal(format!("lock: {e}")))?;

        let aios_root = (*self.aios_root).clone();
        let now = Utc::now();

        match apply_deplatform(&mut catalog, &event, &aios_root, now) {
            Ok(()) => Ok(Response::new(pb::DeplatformReply { ok: true })),
            Err(e) => Err(Status::internal(format!("deplatform failed: {e}"))),
        }
    }

    async fn health_check_quarantine(
        &self,
        request: Request<pb::HealthCheckRequest>,
    ) -> Result<Response<pb::HealthCheckReply>, Status> {
        let r = request.into_inner();

        let mut installed: Vec<InstalledPackageRecord> = r
            .installed
            .iter()
            .map(|ip| InstalledPackageRecord {
                package_id: PackageId(ip.package_id.clone()),
                publisher_root_id: PublisherRootId(ip.publisher_root_id.clone()),
                signing_key_id: PackageSigningKeyId(ip.signing_key_id.clone()),
                state: conv::rust_install_state_from_proto(ip.state),
                installed_at: Utc::now(),
            })
            .collect();

        let revoked_keys: Vec<PackageSigningKeyId> = r
            .revoked_signing_key_ids
            .iter()
            .map(|id| PackageSigningKeyId(id.clone()))
            .collect();

        let catalog = self
            .publisher_catalog
            .lock()
            .map_err(|e| Status::internal(format!("lock: {e}")))?;

        let now = Utc::now();
        let quarantined = health_check_quarantine(&mut installed, &catalog, &revoked_keys, now);

        let ids: Vec<String> = quarantined.iter().map(|id| id.0.clone()).collect();

        Ok(Response::new(pb::HealthCheckReply {
            quarantined_package_ids: ids,
        }))
    }
}

// ── Pub-sub convenience re-export ─────────────────────────────────────────

/// Re-export the generated `PublisherServiceClient` and `PublisherServiceServer`.
pub use pb::publisher_service_client::PublisherServiceClient;
pub use pb::publisher_service_server::{
    PublisherService as PublisherServiceGrpc, PublisherServiceServer,
};
/// Re-export the generated `RepositoryServiceClient` and `RepositoryServiceServer`.
pub use pb::repository_service_client::RepositoryServiceClient;
pub use pb::repository_service_server::{
    RepositoryService as RepositoryServiceGrpc, RepositoryServiceServer,
};

/// Schema version string mirroring the proto3 package name.
pub const SCHEMA_VERSION: &str = "aios.distribution.v1alpha1";
