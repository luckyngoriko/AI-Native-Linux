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
use aios_evidence::service::conversions as evidence_conversions;
use aios_evidence::service::proto as evidence_proto;
use aios_evidence::service::EvidenceLogClient;
use aios_evidence::EvidenceReceipt;
use aios_fs::service::conversions as fs_conversions;
use aios_fs::service::proto as fs_proto;
use aios_fs::service::AiosFsClient;
use aios_fs::{
    ConsistencyClass, Object, ObjectId, ObjectWriteRequest, ObjectWriteResult, Predicate, Query,
    QueryField, QueryNamespace, QueryOperator, QueryValue, SnapshotId, TransactionId, Version,
    VersionId, View,
};
use aios_policy::service::conversions as policy_conversions;
use aios_policy::service::proto as policy_proto;
use aios_policy::service::{PolicyKernelClient, SCHEMA_VERSION as POLICY_SCHEMA_VERSION};
use aios_policy::PolicyDecision;
use aios_recovery::service::conversions as recovery_conversions;
use aios_recovery::service::proto as recovery_proto;
use aios_recovery::service::{RecoveryServiceClient, SCHEMA_VERSION as RECOVERY_SCHEMA_VERSION};
use aios_recovery::{FirstBootContext, KernelCandidate, RecoveryState};
use aios_vault::service::conversions as vault_conversions;
use aios_vault::service::proto as vault_proto;
use aios_vault::service::VaultBrokerClient;
use aios_vault::{
    CapabilityClass, CapabilityId, CapabilityState, KeyAlgorithm, KeyMaterialHandle, SubjectRef,
    VaultCapability,
};
use aios_verification::service::conversions as verification_conversions;
use aios_verification::service::proto as verification_proto;
use aios_verification::service::{
    VerificationEngineClient, SCHEMA_VERSION as VERIFICATION_SCHEMA_VERSION,
};
use aios_verification::{VerificationIntent, VerificationPrimitive, VerificationResult};
use serde_json::{json, Value};
use tonic::transport::Channel;

use crate::client::endpoint::AiosEndpoints;
use crate::RenderError;

const RENDERER_OPERATOR_GRANT: &str = "ovr_renderer_cli_operator";

/// Composed gRPC client for the backend services needed by the renderer CLI.
#[derive(Debug)]
pub struct AiosClient {
    policy: PolicyKernelClient<Channel>,
    runtime: CapabilityRuntimeClient<Channel>,
    fs: AiosFsClient<Channel>,
    vault: VaultBrokerClient<Channel>,
    verification: VerificationEngineClient<Channel>,
    /// Recovery Service client.
    pub recovery: RecoveryServiceClient<Channel>,
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
        let verification = VerificationEngineClient::connect(endpoints.verification.clone())
            .await
            .map_err(|err| client_connect_failed("verification", err.to_string()))?;
        let recovery = RecoveryServiceClient::connect(endpoints.recovery.clone())
            .await
            .map_err(|err| client_connect_failed("recovery", err.to_string()))?;
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
            verification,
            recovery,
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

    /// Read action status through the Capability Runtime `GetActionStatus` RPC.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::ClientCallFailed`] when the RPC fails or the
    /// response does not contain a valid [`ActionContext`].
    pub async fn action_status(&mut self, action_id: &str) -> Result<ActionContext, RenderError> {
        let response = self
            .runtime
            .get_action_status(runtime_proto::GetActionStatusRequest {
                action_request_id: action_id.to_owned(),
            })
            .await
            .map_err(|err| client_call_failed("runtime", "GetActionStatus", err.to_string()))?
            .into_inner();
        let context = response.context.as_ref().ok_or_else(|| {
            client_call_failed("runtime", "GetActionStatus", "missing ActionContext")
        })?;

        runtime_conversions::action_context_from_proto(context)
            .map_err(|err| client_call_failed("runtime", "GetActionStatus", err.to_string()))
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

    /// Materialize an object listing, optionally filtered by namespace class.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::ClientCallFailed`] when the `MaterializeView` RPC
    /// fails or the response cannot be converted.
    pub async fn list_objects(&mut self, namespace: Option<&str>) -> Result<View, RenderError> {
        let query = namespace.map_or_else(
            || Query::And(Vec::new()),
            |namespace| {
                Query::And(vec![Predicate {
                    namespace: QueryNamespace::Namespace,
                    field: QueryField::NamespaceClass,
                    op: QueryOperator::Eq,
                    rhs: QueryValue::String(namespace_token(namespace)),
                }])
            },
        );
        let response = self
            .fs
            .materialize_view(fs_proto::MaterializeViewRequestProto {
                query: Some(fs_conversions::query_to_proto(&query)),
                snapshot_id: String::new(),
            })
            .await
            .map_err(|err| client_call_failed("fs", "MaterializeView", err.to_string()))?
            .into_inner();

        fs_conversions::view_from_proto(&response)
            .map_err(|err| client_call_failed("fs", "MaterializeView", err.to_string()))
    }

    /// List versions for an AIOS-FS object.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::ClientCallFailed`] when the RPC fails or one
    /// version cannot be converted.
    pub async fn list_versions(&mut self, object_id: &str) -> Result<Vec<Version>, RenderError> {
        let response = self
            .fs
            .list_versions(fs_proto::ListVersionsRequestProto {
                object_id: object_id.to_owned(),
            })
            .await
            .map_err(|err| client_call_failed("fs", "ListVersions", err.to_string()))?
            .into_inner();

        response
            .versions
            .iter()
            .map(|version| {
                fs_conversions::version_from_proto(version)
                    .map_err(|err| client_call_failed("fs", "ListVersions", err.to_string()))
            })
            .collect()
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

    /// Issue a Vault capability to a subject.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::ClientCallFailed`] when the Vault RPC fails or
    /// the returned capability cannot be converted.
    pub async fn issue_capability(
        &mut self,
        class: CapabilityClass,
        subject: &str,
    ) -> Result<VaultCapability, RenderError> {
        let response = self
            .vault
            .issue_capability(vault_proto::IssueCapabilityRequest {
                class: i32::from(vault_conversions::capability_class_to_proto(class)),
                issued_to: subject.to_owned(),
                expires_at: None,
                key_algorithm: i32::from(key_algorithm_to_proto(default_key_algorithm(class))),
                key_material_bytes: None,
            })
            .await
            .map_err(|err| client_call_failed("vault", "IssueCapability", err.to_string()))?
            .into_inner();
        let capability = response.capability.as_ref().ok_or_else(|| {
            client_call_failed("vault", "IssueCapability", "missing VaultCapability")
        })?;

        vault_capability_from_proto(capability)
    }

    /// Query evidence receipts bound to an action.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::ClientCallFailed`] when the Evidence RPC fails or
    /// a receipt cannot be converted.
    pub async fn evidence_chain(
        &mut self,
        action_id: &str,
        last_n: Option<u32>,
    ) -> Result<crate::EvidenceChainView, RenderError> {
        let Some(evidence) = self.evidence.as_mut() else {
            return Ok(crate::EvidenceChainView::new(Vec::new()));
        };
        let mut stream = evidence
            .query(evidence_proto::QueryRequest {
                record_types_filter: Vec::new(),
                subject_filter: String::new(),
                correlation_id_filter: String::new(),
                action_id_filter: action_id.to_owned(),
                from_time: None,
                to_time: None,
                text_match: String::new(),
                limit: last_n.unwrap_or(100),
                subject: String::new(),
                caller_primary_group: String::new(),
                caller_is_ai: false,
                caller_is_recovery_mode: false,
            })
            .await
            .map_err(|err| client_call_failed("evidence", "Query", err.to_string()))?
            .into_inner();

        let mut receipts = Vec::new();
        while let Some(receipt) = stream
            .message()
            .await
            .map_err(|err| client_call_failed("evidence", "Query", err.to_string()))?
        {
            receipts.push(evidence_receipt_from_proto(receipt)?);
        }

        Ok(crate::EvidenceChainView::new(receipts))
    }

    /// Read one evidence receipt by id.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::ClientCallFailed`] when the Evidence RPC fails,
    /// no Evidence client is configured, or the receipt cannot be converted.
    pub async fn evidence_receipt(
        &mut self,
        receipt_id: &str,
    ) -> Result<EvidenceReceipt, RenderError> {
        let receipt = {
            let evidence = self.evidence.as_mut().ok_or_else(|| {
                client_call_failed(
                    "evidence",
                    "ReadReceipt",
                    "evidence endpoint is not configured",
                )
            })?;
            evidence
                .read_receipt(evidence_proto::ReadReceiptRequest {
                    receipt_id: receipt_id.to_owned(),
                })
                .await
                .map_err(|err| client_call_failed("evidence", "ReadReceipt", err.to_string()))?
                .into_inner()
        };

        evidence_receipt_from_proto(receipt)
    }

    /// Run a verification intent through the Verification Engine.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::ClientCallFailed`] when the Verification RPC
    /// fails or the returned result cannot be converted.
    pub async fn verify(
        &mut self,
        intent: VerificationIntent,
    ) -> Result<VerificationResult, RenderError> {
        let response = self
            .verification
            .run_verification(verification_proto::RunVerificationRequest {
                schema_version: VERIFICATION_SCHEMA_VERSION.to_owned(),
                action_id_proto: verification_conversions::action_id_to_proto(&intent.action_id),
                intent: Some(verification_conversions::verification_intent_to_proto(
                    &intent,
                )),
                subject: "operator:renderer-cli".to_owned(),
                simulate: true,
            })
            .await
            .map_err(|err| client_call_failed("verification", "RunVerification", err.to_string()))?
            .into_inner();

        verification_conversions::verification_result_from_proto(response)
            .map_err(|err| client_call_failed("verification", "RunVerification", err.to_string()))
    }

    /// List the Verification Engine primitive vocabulary.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::ClientCallFailed`] when the info RPC fails or one
    /// primitive token is not part of the closed Rust vocabulary.
    pub async fn list_primitives(&mut self) -> Result<Vec<VerificationPrimitive>, RenderError> {
        let response = self
            .verification
            .get_engine_info(())
            .await
            .map_err(|err| client_call_failed("verification", "GetEngineInfo", err.to_string()))?
            .into_inner();

        response
            .supported_primitives
            .into_iter()
            .map(|primitive| {
                serde_json::from_value::<VerificationPrimitive>(Value::String(primitive.clone()))
                    .map_err(|err| {
                        client_call_failed(
                            "verification",
                            "GetEngineInfo",
                            format!("unknown primitive `{primitive}`: {err}"),
                        )
                    })
            })
            .collect()
    }

    /// Enter recovery mode through the Recovery Service.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::ClientCallFailed`] when the RPC fails or the
    /// returned recovery state cannot be converted.
    pub async fn enter_recovery(&mut self, reason: &str) -> Result<RecoveryState, RenderError> {
        let response = self
            .recovery
            .enter_recovery(recovery_proto::EnterRecoveryRequestProto {
                schema_version: RECOVERY_SCHEMA_VERSION.to_owned(),
                reason: reason.to_owned(),
                operator_grant: Some(RENDERER_OPERATOR_GRANT.to_owned()),
                expected_phases: vec![i32::from(recovery_proto::BootPhaseProto::BootPhaseRecovery)],
                bundle: None,
                action_id_proto: Vec::new(),
                action_id_format: recovery_conversions::ACTION_ID_FORMAT_UNSPECIFIED,
            })
            .await
            .map_err(|err| client_call_failed("recovery", "EnterRecovery", err.to_string()))?
            .into_inner();

        recovery_conversions::recovery_state_from_proto(response)
            .map_err(|err| client_call_failed("recovery", "EnterRecovery", err.to_string()))
    }

    /// Exit recovery mode through the Recovery Service.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::ClientCallFailed`] when the RPC fails or the
    /// returned recovery state cannot be converted.
    pub async fn exit_recovery(&mut self, exit_token: &str) -> Result<RecoveryState, RenderError> {
        let response = self
            .recovery
            .exit_recovery(recovery_proto::ExitRecoveryRequestProto {
                schema_version: RECOVERY_SCHEMA_VERSION.to_owned(),
                exit_token: exit_token.to_owned(),
                action_id_proto: Vec::new(),
                action_id_format: recovery_conversions::ACTION_ID_FORMAT_UNSPECIFIED,
            })
            .await
            .map_err(|err| client_call_failed("recovery", "ExitRecovery", err.to_string()))?
            .into_inner();

        recovery_conversions::recovery_state_from_proto(response)
            .map_err(|err| client_call_failed("recovery", "ExitRecovery", err.to_string()))
    }

    /// Read the current recovery state through the Recovery Service.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::ClientCallFailed`] when the RPC fails or the
    /// returned recovery state cannot be converted.
    pub async fn get_recovery_state(&mut self) -> Result<RecoveryState, RenderError> {
        let response = self
            .recovery
            .get_recovery_state(recovery_proto::GetRecoveryStateRequest {
                schema_version: RECOVERY_SCHEMA_VERSION.to_owned(),
            })
            .await
            .map_err(|err| client_call_failed("recovery", "GetRecoveryState", err.to_string()))?
            .into_inner();

        recovery_conversions::recovery_state_from_proto(response)
            .map_err(|err| client_call_failed("recovery", "GetRecoveryState", err.to_string()))
    }

    /// Run first-boot provisioning through the Recovery Service.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::ClientCallFailed`] when the RPC fails or the
    /// returned first-boot context cannot be converted.
    pub async fn run_first_boot(&mut self) -> Result<FirstBootContext, RenderError> {
        let response = self
            .recovery
            .run_first_boot_provisioning(recovery_proto::RunFirstBootProvisioningRequest {
                schema_version: RECOVERY_SCHEMA_VERSION.to_owned(),
                action_id_proto: Vec::new(),
                action_id_format: recovery_conversions::ACTION_ID_FORMAT_UNSPECIFIED,
            })
            .await
            .map_err(|err| {
                client_call_failed("recovery", "RunFirstBootProvisioning", err.to_string())
            })?
            .into_inner();

        recovery_conversions::first_boot_context_from_proto(response).map_err(|err| {
            client_call_failed("recovery", "RunFirstBootProvisioning", err.to_string())
        })
    }

    /// List registered kernel candidates through the Recovery Service.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::ClientCallFailed`] when the RPC fails or one
    /// candidate cannot be converted.
    pub async fn list_kernel_candidates(&mut self) -> Result<Vec<KernelCandidate>, RenderError> {
        let response = self
            .recovery
            .list_kernel_candidates(recovery_proto::ListKernelCandidatesRequest {
                schema_version: RECOVERY_SCHEMA_VERSION.to_owned(),
            })
            .await
            .map_err(|err| client_call_failed("recovery", "ListKernelCandidates", err.to_string()))?
            .into_inner();

        response
            .candidates
            .into_iter()
            .map(|candidate| {
                recovery_conversions::kernel_candidate_from_proto(candidate).map_err(|err| {
                    client_call_failed("recovery", "ListKernelCandidates", err.to_string())
                })
            })
            .collect()
    }

    /// Activate a kernel candidate through the Recovery Service.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::ClientCallFailed`] when the RPC fails or the
    /// candidate cannot be converted.
    pub async fn activate_kernel(
        &mut self,
        candidate_id: &str,
    ) -> Result<KernelCandidate, RenderError> {
        let response = self
            .recovery
            .activate_kernel_candidate(recovery_proto::ActivateKernelCandidateRequest {
                schema_version: RECOVERY_SCHEMA_VERSION.to_owned(),
                candidate_id: candidate_id.to_owned(),
                action_id_proto: Vec::new(),
                action_id_format: recovery_conversions::ACTION_ID_FORMAT_UNSPECIFIED,
            })
            .await
            .map_err(|err| {
                client_call_failed("recovery", "ActivateKernelCandidate", err.to_string())
            })?
            .into_inner();

        recovery_conversions::kernel_candidate_from_proto(response).map_err(|err| {
            client_call_failed("recovery", "ActivateKernelCandidate", err.to_string())
        })
    }

    /// Roll back a kernel candidate through the Recovery Service.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::ClientCallFailed`] when the RPC fails or the
    /// candidate cannot be converted.
    pub async fn rollback_kernel(
        &mut self,
        candidate_id: &str,
    ) -> Result<KernelCandidate, RenderError> {
        let response = self
            .recovery
            .rollback_kernel_candidate(recovery_proto::RollbackKernelCandidateRequest {
                schema_version: RECOVERY_SCHEMA_VERSION.to_owned(),
                candidate_id: candidate_id.to_owned(),
                action_id_proto: Vec::new(),
                action_id_format: recovery_conversions::ACTION_ID_FORMAT_UNSPECIFIED,
            })
            .await
            .map_err(|err| {
                client_call_failed("recovery", "RollbackKernelCandidate", err.to_string())
            })?
            .into_inner();

        recovery_conversions::kernel_candidate_from_proto(response).map_err(|err| {
            client_call_failed("recovery", "RollbackKernelCandidate", err.to_string())
        })
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

const fn default_key_algorithm(class: CapabilityClass) -> KeyAlgorithm {
    match class {
        CapabilityClass::KeySign
        | CapabilityClass::KeyVerify
        | CapabilityClass::BootstrapKeySign => KeyAlgorithm::Ed25519,
        CapabilityClass::MacGenerate | CapabilityClass::MacVerify => KeyAlgorithm::HmacSha256,
        CapabilityClass::KeyEncrypt
        | CapabilityClass::KeyDecrypt
        | CapabilityClass::RandomGenerate
        | CapabilityClass::SecretGet => KeyAlgorithm::Aes256Gcm,
    }
}

const fn key_algorithm_to_proto(algorithm: KeyAlgorithm) -> vault_proto::KeyAlgorithm {
    match algorithm {
        KeyAlgorithm::Aes256Gcm => vault_proto::KeyAlgorithm::Aes256Gcm,
        KeyAlgorithm::HmacSha256 => vault_proto::KeyAlgorithm::HmacSha256,
        KeyAlgorithm::HkdfSha256 => vault_proto::KeyAlgorithm::HkdfSha256,
        KeyAlgorithm::Ed25519 => vault_proto::KeyAlgorithm::Ed25519,
        KeyAlgorithm::X25519 => vault_proto::KeyAlgorithm::X25519,
    }
}

fn namespace_token(namespace: &str) -> String {
    namespace
        .chars()
        .map(|ch| match ch {
            '-' | ' ' => '_',
            other => other.to_ascii_uppercase(),
        })
        .collect()
}

fn evidence_receipt_from_proto(
    receipt: evidence_proto::EvidenceReceipt,
) -> Result<EvidenceReceipt, RenderError> {
    let record_type = evidence_conversions::record_type_from_proto_i32(receipt.record_type)
        .map_err(|err| client_call_failed("evidence", "receipt conversion", err.to_string()))?;
    let recorded_at_proto = receipt.recorded_at.unwrap_or_default();
    let recorded_at = evidence_conversions::timestamp_to_datetime(&recorded_at_proto);
    let action_id = optional_string(receipt.action_id);
    let previous_receipt_hash = optional_string(receipt.previous_receipt_hash);
    let redaction_profile = if receipt.redaction_profile.is_empty() {
        "default".to_owned()
    } else {
        receipt.redaction_profile
    };
    let value = json!({
        "receipt_id": receipt.receipt_id,
        "recorded_at": recorded_at,
        "record_type": record_type,
        "retention_class": aios_evidence::record::retention_class_for(record_type),
        "subject_canonical_id": receipt.subject,
        "action_id": action_id,
        "previous_receipt_hash": previous_receipt_hash,
        "content_hash": receipt.payload_hash,
        "payload": Value::Null,
        "redaction_profile": redaction_profile,
    });

    serde_json::from_value(value)
        .map_err(|err| client_call_failed("evidence", "receipt conversion", err.to_string()))
}

fn optional_string(value: String) -> Value {
    if value.is_empty() {
        Value::Null
    } else {
        Value::String(value)
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
