//! In-process backend fixture for renderer gRPC client integration tests.

#![allow(
    clippy::module_name_repetitions,
    reason = "public names mirror the AIOS service vocabulary"
)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration as StdDuration;

use aios_capability_runtime::service as runtime_service;
use aios_capability_runtime::InMemoryCapabilityRuntime;
use aios_cognitive::service as cognitive_service;
use aios_cognitive::InMemoryCognitiveCore;
use aios_fs::service as fs_service;
use aios_fs::InMemoryAiosFs;
use aios_policy::service as policy_service;
use aios_policy::InMemoryPolicyKernel;
use aios_policy::PolicyKernel;
use aios_recovery::service as recovery_service;
use aios_recovery::{
    FirstBootDriver, InMemoryRecoveryBoundary, KernelPipelineDriver, RecoveryBoundary,
    RecoveryGuard,
};
use aios_sgr::service as sgr_service;
use aios_sgr::{
    GraphEvaluator, InMemoryServiceGraph, ServiceGraph, SgrAdapterRegistry, UnitFsmDriver,
};
use aios_vault::service as vault_service;
use aios_vault::{
    CapabilityAuditLog, CapabilityClass, CapabilityLifecycleDriver, IdentityCatalog,
    InMemoryOverrideBroker, InMemoryVaultBroker, IssueCapabilityRequest, KeyAlgorithm, SubjectRef,
    VaultBroker,
};
use aios_verification::service as verification_service;
use aios_verification::InMemoryVerificationEngine;
use chrono::{Duration as ChronoDuration, Utc};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tonic::transport::server::Router;

use crate::client::aios_client::AiosClient;
use crate::client::endpoint::AiosEndpoints;
use crate::RenderError;

/// In-memory backend graph used to spawn local gRPC services for tests.
#[derive(Clone)]
pub struct InProcessBackend {
    policy_kernel: Arc<dyn PolicyKernel>,
    runtime: Arc<InMemoryCapabilityRuntime>,
    fs: Arc<InMemoryAiosFs>,
    vault: Arc<InMemoryVaultBroker>,
    verification: Arc<InMemoryVerificationEngine>,
    sgr_graph: Arc<InMemoryServiceGraph>,
    sgr_fsm: Arc<UnitFsmDriver>,
    sgr_evaluator: Arc<GraphEvaluator>,
    sgr_registry: Arc<SgrAdapterRegistry>,
    recovery_boundary: Arc<InMemoryRecoveryBoundary>,
    first_boot: Arc<FirstBootDriver>,
    kernel_pipeline: Arc<KernelPipelineDriver>,
    recovery_guard: Arc<RecoveryGuard>,
    overrides: Arc<InMemoryOverrideBroker>,
    identity: Arc<IdentityCatalog>,
    audit: Arc<CapabilityAuditLog>,
    lifecycle: Arc<CapabilityLifecycleDriver>,
    cognitive_core: Arc<InMemoryCognitiveCore>,
}

/// Graceful shutdown handle for an in-process backend service set.
pub struct ShutdownHandle {
    shutdowns: Vec<oneshot::Sender<()>>,
    tasks: Vec<JoinHandle<Result<(), tonic::transport::Error>>>,
}

impl ShutdownHandle {
    const fn new(
        shutdowns: Vec<oneshot::Sender<()>>,
        tasks: Vec<JoinHandle<Result<(), tonic::transport::Error>>>,
    ) -> Self {
        Self { shutdowns, tasks }
    }

    /// Return the number of backend gRPC servers managed by this handle (now 8).
    #[must_use]
    pub const fn service_count(&self) -> usize {
        self.shutdowns.len()
    }

    /// Ask every server to stop and wait for each task to finish.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::Internal`] when a server task fails or panics.
    pub async fn shutdown(mut self) -> Result<(), RenderError> {
        for shutdown in self.shutdowns.drain(..) {
            let _already_closed = shutdown.send(()).is_err();
        }

        for task in self.tasks.drain(..) {
            match task.await {
                Ok(Ok(())) => {}
                Ok(Err(err)) => {
                    return Err(RenderError::Internal(format!(
                        "in-process backend server failed: {err}"
                    )));
                }
                Err(err) => {
                    return Err(RenderError::Internal(format!(
                        "in-process backend server task failed: {err}"
                    )));
                }
            }
        }

        Ok(())
    }
}

impl InProcessBackend {
    /// Build the default in-memory policy/runtime/fs/vault/recovery backend graph.
    #[must_use]
    pub fn new_default() -> Self {
        let policy_kernel: Arc<dyn PolicyKernel> = Arc::new(InMemoryPolicyKernel::new());
        Self::new_with_policy_kernel(policy_kernel)
    }

    fn new_with_policy_kernel(policy_kernel: Arc<dyn PolicyKernel>) -> Self {
        let runtime = Arc::new(
            InMemoryCapabilityRuntime::new().with_policy_kernel(Arc::clone(&policy_kernel)),
        );
        let fs = Arc::new(InMemoryAiosFs::new());
        let audit = Arc::new(CapabilityAuditLog::new());
        let vault = Arc::new(InMemoryVaultBroker::new().with_audit_log(Arc::clone(&audit)));
        let verification = Arc::new(InMemoryVerificationEngine::new());
        let sgr_graph = Arc::new(InMemoryServiceGraph::new());
        let sgr_graph_for_fsm: Arc<dyn ServiceGraph> = sgr_graph.clone();
        let sgr_graph_for_evaluator: Arc<dyn ServiceGraph> = sgr_graph.clone();
        let sgr_fsm = Arc::new(UnitFsmDriver::new(sgr_graph_for_fsm));
        let sgr_evaluator = Arc::new(GraphEvaluator::new(sgr_graph_for_evaluator));
        let sgr_registry = Arc::new(SgrAdapterRegistry::new());
        let recovery_boundary = Arc::new(InMemoryRecoveryBoundary::new());
        let recovery_boundary_for_first_boot: Arc<dyn RecoveryBoundary> = recovery_boundary.clone();
        let recovery_boundary_for_kernel: Arc<dyn RecoveryBoundary> = recovery_boundary.clone();
        let recovery_boundary_for_guard: Arc<dyn RecoveryBoundary> = recovery_boundary.clone();
        let first_boot = Arc::new(FirstBootDriver::new(recovery_boundary_for_first_boot));
        let kernel_pipeline = Arc::new(KernelPipelineDriver::new(recovery_boundary_for_kernel));
        let recovery_guard = Arc::new(RecoveryGuard::new(recovery_boundary_for_guard));
        let identity = Arc::new(IdentityCatalog::with_fixtures());
        let overrides = Arc::new(InMemoryOverrideBroker::new(Arc::clone(&identity)));
        let lifecycle = Arc::new(CapabilityLifecycleDriver::new(
            Arc::clone(&vault),
            Arc::clone(&audit),
        ));

        Self {
            policy_kernel,
            runtime,
            fs,
            vault,
            verification,
            sgr_graph,
            sgr_fsm,
            sgr_evaluator,
            sgr_registry,
            recovery_boundary,
            first_boot,
            kernel_pipeline,
            recovery_guard,
            overrides,
            identity,
            audit,
            lifecycle,
            cognitive_core: Arc::new(InMemoryCognitiveCore::new()),
        }
    }

    /// Spawn the default eight-service backend and connect an [`AiosClient`].
    ///
    /// # Errors
    ///
    /// Returns [`RenderError`] when a server cannot bind or a client cannot
    /// connect to the spawned endpoint set.
    pub async fn spawn_and_connect() -> Result<(AiosClient, ShutdownHandle), RenderError> {
        Self::new_default().spawn().await
    }

    /// Spawn eight services with a caller-supplied Policy Kernel.
    ///
    /// This is used by integration tests that need deterministic policy
    /// ALLOW/DENY responses while keeping runtime/fs/vault in-memory.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError`] when a server cannot bind or a client cannot
    /// connect to the spawned endpoint set.
    pub async fn spawn_and_connect_with_policy(
        policy_kernel: Arc<dyn PolicyKernel>,
    ) -> Result<(AiosClient, ShutdownHandle), RenderError> {
        Self::new_with_policy_kernel(policy_kernel).spawn().await
    }

    async fn spawn(self) -> Result<(AiosClient, ShutdownHandle), RenderError> {
        self.seed_default_vault_capability().await?;

        let policy = spawn_router(
            "policy",
            policy_service::build_router(self.policy_service()),
        )
        .await?;
        let runtime = spawn_router(
            "runtime",
            runtime_service::build_router(self.runtime_service()),
        )
        .await?;
        let fs = spawn_router("fs", fs_service::build_router(self.fs_service())).await?;
        let vault =
            spawn_router("vault", vault_service::build_router(self.vault_service())).await?;
        let verification = spawn_router(
            "verification",
            verification_service::build_router(self.verification_service()),
        )
        .await?;
        let recovery = spawn_router(
            "recovery",
            recovery_service::build_router(self.recovery_service()),
        )
        .await?;
        let sgr = spawn_router("sgr", sgr_service::build_router(self.sgr_service())).await?;
        let cognitive = spawn_router(
            "cognitive",
            cognitive_service::build_router(self.cognitive_service()),
        )
        .await?;

        let endpoints = AiosEndpoints {
            policy: policy.endpoint,
            runtime: runtime.endpoint,
            fs: fs.endpoint,
            vault: vault.endpoint,
            verification: verification.endpoint,
            recovery: recovery.endpoint,
            sgr: sgr.endpoint,
            cognitive: cognitive.endpoint,
            evidence: None,
        };
        let shutdown = ShutdownHandle::new(
            vec![
                policy.shutdown,
                runtime.shutdown,
                fs.shutdown,
                vault.shutdown,
                verification.shutdown,
                recovery.shutdown,
                sgr.shutdown,
                cognitive.shutdown,
            ],
            vec![
                policy.task,
                runtime.task,
                fs.task,
                vault.task,
                verification.task,
                recovery.task,
                sgr.task,
                cognitive.task,
            ],
        );
        let client = AiosClient::connect(&endpoints).await?;

        Ok((client, shutdown))
    }

    fn policy_service(&self) -> policy_service::PolicyKernelService {
        policy_service::PolicyKernelService::new(Arc::clone(&self.policy_kernel))
            .with_engine_id("renderer-inproc-policy")
            .with_bundle_version("polb_renderer_inproc")
    }

    fn runtime_service(&self) -> runtime_service::CapabilityRuntimeService {
        runtime_service::CapabilityRuntimeService::new(Arc::clone(&self.runtime))
            .with_runtime_id("renderer-inproc-runtime")
            .with_bundle_version("polb_renderer_inproc")
    }

    fn fs_service(&self) -> fs_service::AiosFsService {
        fs_service::AiosFsService::new(Arc::clone(&self.fs)).with_fs_id("renderer-inproc-fs")
    }

    fn vault_service(&self) -> vault_service::VaultBrokerService {
        vault_service::VaultBrokerService::new(
            Arc::clone(&self.vault),
            Arc::clone(&self.overrides),
            Arc::clone(&self.identity),
            Arc::clone(&self.audit),
            Arc::clone(&self.lifecycle),
        )
        .with_vault_id("renderer-inproc-vault")
    }

    fn verification_service(&self) -> verification_service::VerificationEngineService {
        verification_service::VerificationEngineService::new(Arc::clone(&self.verification))
            .with_engine_id("renderer-inproc-verification")
    }

    fn recovery_service(&self) -> recovery_service::RecoveryServiceImpl {
        recovery_service::RecoveryServiceImpl::new(
            Arc::clone(&self.recovery_boundary),
            Arc::clone(&self.first_boot),
            Arc::clone(&self.kernel_pipeline),
            Arc::clone(&self.recovery_guard),
        )
    }

    fn sgr_service(&self) -> sgr_service::SgrServiceImpl {
        sgr_service::SgrServiceImpl::new(
            Arc::clone(&self.sgr_graph),
            Arc::clone(&self.sgr_fsm),
            Arc::clone(&self.sgr_evaluator),
            Arc::clone(&self.sgr_registry),
        )
    }

    fn cognitive_service(&self) -> cognitive_service::CognitiveCoreServiceImpl {
        cognitive_service::CognitiveCoreServiceImpl::new(Arc::clone(&self.cognitive_core))
    }

    async fn seed_default_vault_capability(&self) -> Result<(), RenderError> {
        self.vault
            .issue_capability(IssueCapabilityRequest {
                class: CapabilityClass::KeyEncrypt,
                issued_to: SubjectRef("family:alice".to_owned()),
                expires_at: Some(Utc::now() + ChronoDuration::hours(1)),
                key_algorithm: KeyAlgorithm::Aes256Gcm,
                key_material_bytes: Some(vec![7; 32]),
            })
            .await
            .map_err(|err| RenderError::Internal(format!("seed vault capability failed: {err}")))?;
        Ok(())
    }
}

struct SpawnedServer {
    endpoint: String,
    shutdown: oneshot::Sender<()>,
    task: JoinHandle<Result<(), tonic::transport::Error>>,
}

async fn spawn_router(service: &str, router: Router) -> Result<SpawnedServer, RenderError> {
    let addr = pick_port(service).await?;
    let (shutdown, shutdown_rx) = oneshot::channel::<()>();
    let task = tokio::spawn(async move {
        router
            .serve_with_shutdown(addr, async move {
                let _ = shutdown_rx.await;
            })
            .await
    });
    tokio::time::sleep(StdDuration::from_millis(50)).await;

    Ok(SpawnedServer {
        endpoint: format!("http://{addr}"),
        shutdown,
        task,
    })
}

async fn pick_port(service: &str) -> Result<SocketAddr, RenderError> {
    let listener = TcpListener::bind("127.0.0.1:0").await.map_err(|err| {
        RenderError::Internal(format!("bind {service} in-process server failed: {err}"))
    })?;
    let addr = listener.local_addr().map_err(|err| {
        RenderError::Internal(format!(
            "read {service} in-process server local addr failed: {err}"
        ))
    })?;
    drop(listener);
    Ok(addr)
}
