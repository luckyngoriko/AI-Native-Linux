//! Tonic client composition for renderer-facing backend calls.

#![allow(
    clippy::module_name_repetitions,
    reason = "public names mirror the AIOS service vocabulary"
)]

use aios_action::ActionEnvelope;
use aios_capability_runtime::service::conversions as runtime_conversions;
use aios_capability_runtime::service::proto as runtime_proto;
use aios_capability_runtime::service::CapabilityRuntimeClient;
use aios_capability_runtime::ActionContext;
use aios_evidence::service::EvidenceLogClient;
use aios_fs::service::conversions as fs_conversions;
use aios_fs::service::proto as fs_proto;
use aios_fs::service::AiosFsClient;
use aios_fs::{
    ConsistencyClass, Object, ObjectId, ObjectWriteRequest, ObjectWriteResult, SnapshotId,
    TransactionId, VersionId,
};
use aios_policy::service::conversions as policy_conversions;
use aios_policy::service::proto as policy_proto;
use aios_policy::service::{PolicyKernelClient, SCHEMA_VERSION as POLICY_SCHEMA_VERSION};
use aios_policy::PolicyDecision;
use aios_vault::service::conversions as vault_conversions;
use aios_vault::service::proto as vault_proto;
use aios_vault::service::VaultBrokerClient;
use aios_vault::{CapabilityId, CapabilityState, KeyMaterialHandle, SubjectRef, VaultCapability};
use tonic::transport::Channel;

use crate::client::endpoint::AiosEndpoints;
use crate::RenderError;

/// Composed gRPC client for the backend services needed by the renderer CLI.
#[derive(Debug)]
pub struct AiosClient {
    policy: PolicyKernelClient<Channel>,
    runtime: CapabilityRuntimeClient<Channel>,
    fs: AiosFsClient<Channel>,
    vault: VaultBrokerClient<Channel>,
    evidence: Option<EvidenceLogClient<Channel>>,
}

impl AiosClient {
    /// Connect to all configured backend service endpoints.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::ClientConnectFailed`] for the first backend
    /// endpoint that tonic cannot connect to.
    pub async fn connect(endpoints: &AiosEndpoints) -> Result<Self, RenderError> {
        let policy = PolicyKernelClient::connect(endpoints.policy.clone())
            .await
            .map_err(|err| client_connect_failed("policy", err.to_string()))?;
        let runtime = CapabilityRuntimeClient::connect(endpoints.runtime.clone())
            .await
            .map_err(|err| client_connect_failed("runtime", err.to_string()))?;
        let fs = AiosFsClient::connect(endpoints.fs.clone())
            .await
            .map_err(|err| client_connect_failed("fs", err.to_string()))?;
        let vault = VaultBrokerClient::connect(endpoints.vault.clone())
            .await
            .map_err(|err| client_connect_failed("vault", err.to_string()))?;
        let evidence = match &endpoints.evidence {
            Some(endpoint) => Some(
                EvidenceLogClient::connect(endpoint.clone())
                    .await
                    .map_err(|err| client_connect_failed("evidence", err.to_string()))?,
            ),
            None => None,
        };

        Ok(Self {
            policy,
            runtime,
            fs,
            vault,
            evidence,
        })
    }

    /// Return true when an optional Evidence Log client is connected.
    #[must_use]
    pub const fn has_evidence_client(&self) -> bool {
        self.evidence.is_some()
    }

    /// Submit an action through the Capability Runtime `ExecuteAction` RPC.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::ClientCallFailed`] when envelope encoding fails,
    /// the RPC returns a non-OK status, or the response lacks/contains an
    /// invalid `ActionContext`.
    pub async fn submit_action(
        &mut self,
        envelope: ActionEnvelope,
    ) -> Result<ActionContext, RenderError> {
        let envelope_proto = runtime_conversions::envelope_to_bytes(&envelope)
            .map_err(|err| client_call_failed("runtime", "ExecuteAction", err.to_string()))?;
        let response = self
            .runtime
            .execute_action(runtime_proto::ExecuteActionRequest {
                action_request_id: String::new(),
                envelope_proto,
                approval_binding_id: String::new(),
            })
            .await
            .map_err(|err| client_call_failed("runtime", "ExecuteAction", err.to_string()))?
            .into_inner();
        let context = response.context.as_ref().ok_or_else(|| {
            client_call_failed("runtime", "ExecuteAction", "missing ActionContext")
        })?;

        runtime_conversions::action_context_from_proto(context)
            .map_err(|err| client_call_failed("runtime", "ExecuteAction", err.to_string()))
    }

    /// Evaluate an action envelope through the Policy Kernel.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::ClientCallFailed`] when envelope encoding fails
    /// or the Policy Kernel RPC returns a non-OK status.
    pub async fn evaluate_policy(
        &mut self,
        envelope: ActionEnvelope,
    ) -> Result<PolicyDecision, RenderError> {
        let envelope_proto = policy_conversions::envelope_to_bytes(&envelope)
            .map_err(|err| client_call_failed("policy", "EvaluatePolicy", err.to_string()))?;
        let response = self
            .policy
            .evaluate_policy(policy_proto::EvaluatePolicyRequest {
                schema_version: POLICY_SCHEMA_VERSION.to_owned(),
                envelope_proto,
            })
            .await
            .map_err(|err| client_call_failed("policy", "EvaluatePolicy", err.to_string()))?
            .into_inner();

        Ok(policy_conversions::policy_decision_from_proto(&response))
    }

    /// Write an object through the AIOS-FS `WriteObject` RPC.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::ClientCallFailed`] when the RPC fails or the
    /// response contains malformed identifiers.
    pub async fn write_object(
        &mut self,
        request: ObjectWriteRequest,
    ) -> Result<ObjectWriteResult, RenderError> {
        let response = self
            .fs
            .write_object(object_write_request_to_proto(request))
            .await
            .map_err(|err| client_call_failed("fs", "WriteObject", err.to_string()))?
            .into_inner();

        object_write_result_from_proto(&response)
    }

    /// Read an object through the AIOS-FS `ReadObject` RPC.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::ClientCallFailed`] when the RPC fails, the
    /// response has no object, or the object conversion fails.
    pub async fn read_object(&mut self, object_id: &str) -> Result<Object, RenderError> {
        let response = self
            .fs
            .read_object(fs_proto::ReadObjectRequestProto {
                object_id: object_id.to_owned(),
                snapshot_id: String::new(),
                consistency_class: i32::from(fs_conversions::consistency_class_to_proto(
                    ConsistencyClass::Snapshot,
                )),
            })
            .await
            .map_err(|err| client_call_failed("fs", "ReadObject", err.to_string()))?
            .into_inner();
        let object = response
            .object
            .as_ref()
            .ok_or_else(|| client_call_failed("fs", "ReadObject", "missing object"))?;

        fs_conversions::object_from_proto(object)
            .map_err(|err| client_call_failed("fs", "ReadObject", err.to_string()))
    }

    /// List Vault capabilities issued to a subject.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::ClientCallFailed`] when the Vault RPC fails or
    /// one returned capability cannot be converted to the Rust model.
    pub async fn list_capabilities(
        &mut self,
        subject: &str,
    ) -> Result<Vec<VaultCapability>, RenderError> {
        let response = self
            .vault
            .list_capabilities(vault_proto::ListCapabilitiesRequest {
                subject: subject.to_owned(),
            })
            .await
            .map_err(|err| client_call_failed("vault", "ListCapabilities", err.to_string()))?
            .into_inner();

        response
            .capabilities
            .iter()
            .map(vault_capability_from_proto)
            .collect()
    }
}

fn object_write_request_to_proto(request: ObjectWriteRequest) -> fs_proto::ObjectWriteRequestProto {
    fs_proto::ObjectWriteRequestProto {
        object_id: request
            .object_id
            .as_ref()
            .map_or_else(String::new, ToString::to_string),
        parent_version_ids: request
            .parent_version_ids
            .iter()
            .map(ToString::to_string)
            .collect(),
        chunk_refs: request
            .chunks
            .iter()
            .map(|chunk_ref| chunk_ref.0.to_string())
            .collect(),
        metadata_delta: Some(fs_conversions::json_to_struct(&request.metadata_delta)),
        action_id_proto: request
            .action_id
            .as_ref()
            .map_or_else(Vec::new, |id| id.as_str().as_bytes().to_vec()),
        subject: request.subject.0,
        expected_snapshot_id: String::new(),
        consistency_class: i32::from(fs_conversions::consistency_class_to_proto(
            ConsistencyClass::Snapshot,
        )),
    }
}

fn object_write_result_from_proto(
    response: &fs_proto::ObjectWriteResultProto,
) -> Result<ObjectWriteResult, RenderError> {
    Ok(ObjectWriteResult {
        object_id: ObjectId::parse(&response.object_id)
            .map_err(|err| client_call_failed("fs", "WriteObject", err))?,
        version_id: VersionId::parse(&response.version_id)
            .map_err(|err| client_call_failed("fs", "WriteObject", err))?,
        transaction_id: TransactionId::parse(&response.transaction_id)
            .map_err(|err| client_call_failed("fs", "WriteObject", err))?,
        snapshot_id_after: SnapshotId(response.snapshot_id_after.clone()),
    })
}

fn vault_capability_from_proto(
    capability: &vault_proto::VaultCapabilityProto,
) -> Result<VaultCapability, RenderError> {
    let class = vault_proto::VaultCapabilityClass::try_from(capability.class)
        .map_err(|_| {
            client_call_failed(
                "vault",
                "ListCapabilities",
                format!("unknown capability class {}", capability.class),
            )
        })
        .and_then(|class| {
            vault_conversions::capability_class_from_proto(class)
                .map_err(|err| client_call_failed("vault", "ListCapabilities", err.to_string()))
        })?;
    let state = vault_capability_state_from_proto(capability.state)?;

    Ok(VaultCapability {
        capability_id: CapabilityId::parse(&capability.capability_id)
            .map_err(|err| client_call_failed("vault", "ListCapabilities", err))?,
        class,
        issued_to: SubjectRef(capability.issued_to.clone()),
        issued_at: vault_conversions::datetime_from_proto(capability.issued_at.unwrap_or_default()),
        expires_at: capability
            .expires_at
            .map(vault_conversions::datetime_from_proto),
        state,
        key_material_handle: KeyMaterialHandle(capability.key_material_handle.clone()),
    })
}

fn vault_capability_state_from_proto(state: i32) -> Result<CapabilityState, RenderError> {
    match vault_proto::CapabilityState::try_from(state).map_err(|_| {
        client_call_failed(
            "vault",
            "ListCapabilities",
            format!("unknown capability state {state}"),
        )
    })? {
        vault_proto::CapabilityState::Draft => Ok(CapabilityState::Draft),
        vault_proto::CapabilityState::Active => Ok(CapabilityState::Active),
        vault_proto::CapabilityState::Expired => Ok(CapabilityState::Expired),
        vault_proto::CapabilityState::Revoked => Ok(CapabilityState::Revoked),
        vault_proto::CapabilityState::Rotated => Ok(CapabilityState::Rotated),
        vault_proto::CapabilityState::Discarded => Ok(CapabilityState::Discarded),
        vault_proto::CapabilityState::Unspecified => Err(client_call_failed(
            "vault",
            "ListCapabilities",
            "capability state is unspecified",
        )),
    }
}

fn client_connect_failed(service: impl Into<String>, reason: impl Into<String>) -> RenderError {
    RenderError::ClientConnectFailed {
        service: service.into(),
        reason: reason.into(),
    }
}

fn client_call_failed(
    service: impl Into<String>,
    rpc: impl Into<String>,
    status: impl Into<String>,
) -> RenderError {
    RenderError::ClientCallFailed {
        service: service.into(),
        rpc: rpc.into(),
        status: status.into(),
    }
}
