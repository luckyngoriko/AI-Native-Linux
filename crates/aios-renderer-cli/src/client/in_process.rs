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
use aios_fs::service as fs_service;
use aios_fs::InMemoryAiosFs;
use aios_policy::service as policy_service;
use aios_policy::InMemoryPolicyKernel;
use aios_policy::PolicyKernel;
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
    overrides: Arc<InMemoryOverrideBroker>,
    identity: Arc<IdentityCatalog>,
    audit: Arc<CapabilityAuditLog>,
    lifecycle: Arc<CapabilityLifecycleDriver>,
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

    /// Return the number of backend gRPC servers managed by this handle.
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
    /// Build the default in-memory policy/runtime/fs/vault backend graph.
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
            overrides,
            identity,
            audit,
            lifecycle,
        }
    }

    /// Spawn the default five-service backend and connect an [`AiosClient`].
    ///
    /// # Errors
    ///
    /// Returns [`RenderError`] when a server cannot bind or a client cannot
    /// connect to the spawned endpoint set.
    pub async fn spawn_and_connect() -> Result<(AiosClient, ShutdownHandle), RenderError> {
        Self::new_default().spawn().await
    }

    /// Spawn five services with a caller-supplied Policy Kernel.
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

        let endpoints = AiosEndpoints {
            policy: policy.endpoint,
            runtime: runtime.endpoint,
            fs: fs.endpoint,
            vault: vault.endpoint,
            verification: verification.endpoint,
            evidence: None,
        };
        let shutdown = ShutdownHandle::new(
            vec![
                policy.shutdown,
                runtime.shutdown,
                fs.shutdown,
                vault.shutdown,
                verification.shutdown,
            ],
            vec![
                policy.task,
                runtime.task,
                fs.task,
                vault.task,
                verification.task,
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
