//! gRPC `NetworkPolicyService` + `DnsVpnService` server adapters (T-160).
//!
//! [`NetworkPolicyServer`] mounts the network policy controller, exposure FSM,
//! grant registry, connection evaluator, AI cross-origin gate, and firewall
//! manager behind the tonic-generated `NetworkPolicyService` trait.
//!
//! [`DnsVpnServer`] mounts the resolver profile manager, VPN tunnel manager,
//! and mDNS gate behind the tonic-generated `DnsVpnService` trait.
//!
//! Each RPC method:
//! 1. Converts the proto request into Rust domain types via [`super::conversions`].
//! 2. Calls the backing implementation.
//! 3. Converts the Rust response back into a proto message.
//! 4. Maps [`NetworkPolicyError`] → [`tonic::Status`] via [`network_error_to_status`].

#![allow(clippy::result_large_err)]

use std::sync::Arc;

use tonic::{Request, Response, Status};

use crate::ai_discipline::AiCrossOriginGate;
use crate::connection_evaluator::ConnectionEvaluator;
use crate::controller::NetworkPolicyController;
use crate::dns::ResolverProfileManager;
use crate::exposure_fsm::{ExposureApprovalFsm, ExposureApprovalLabel};
use crate::firewall::FirewallManager;
use crate::grant_registry::OutboundGrantRegistry;
use crate::ids::SubjectId;
use crate::mdns::MdnsGate;
use crate::service::conversions::{
    ai_decision_to_resp, ai_eval_req_from_proto, eval_decision_to_resp, eval_req_from_proto,
    exposure_request_to_fsm_call, firewall_ruleset_from_proto, get_posture_to_resp, grant_from_req,
    inbound_exposure_from_req, list_directives_to_resp, manifest_to_resp,
    mdns_allowlist_from_proto, mdns_posture_from_str, network_error_to_status,
    peer_key_rotation_from_proto, posture_from_req, receipt_to_resp, resolver_allowlist_from_proto,
    resolver_profile_to_proto, set_directive_from_req, subject_directive_to_resp,
    tombstone_to_resp, vpn_tunnel_entry_to_proto, wireguard_config_from_proto,
};
use crate::service::proto;
use crate::service::proto::dns_vpn_service_server::DnsVpnService;
use crate::service::proto::network_policy_service_server::NetworkPolicyService;
use crate::vpn::VpnTunnelManager;

// ── NetworkPolicyServer ────────────────────────────────────────────────────

/// Mounts the network policy controller, exposure FSM, grant registry,
/// connection evaluator, AI cross-origin gate, and firewall manager behind
/// the gRPC `NetworkPolicyService` trait.
#[derive(Clone)]
pub struct NetworkPolicyServer {
    controller: Arc<dyn NetworkPolicyController>,
    exposure_fsm: Arc<ExposureApprovalFsm>,
    grant_registry: Arc<OutboundGrantRegistry>,
    evaluator: Arc<ConnectionEvaluator>,
    ai_gate: Arc<AiCrossOriginGate>,
    firewall: Arc<FirewallManager>,
}

impl std::fmt::Debug for NetworkPolicyServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NetworkPolicyServer")
            .finish_non_exhaustive()
    }
}

impl NetworkPolicyServer {
    /// Construct a server mounting all six backing components.
    #[must_use]
    pub fn new(
        controller: Arc<dyn NetworkPolicyController>,
        exposure_fsm: Arc<ExposureApprovalFsm>,
        grant_registry: Arc<OutboundGrantRegistry>,
        evaluator: Arc<ConnectionEvaluator>,
        ai_gate: Arc<AiCrossOriginGate>,
        firewall: Arc<FirewallManager>,
    ) -> Self {
        Self {
            controller,
            exposure_fsm,
            grant_registry,
            evaluator,
            ai_gate,
            firewall,
        }
    }
}

#[tonic::async_trait]
impl NetworkPolicyService for NetworkPolicyServer {
    // ── Posture (2 RPCs) ──────────────────────────────────────────────────

    async fn get_posture(
        &self,
        _request: Request<proto::GetPostureRequest>,
    ) -> Result<Response<proto::GetPostureResponse>, Status> {
        let posture = self.controller.current_posture().await;
        Ok(Response::new(get_posture_to_resp(posture)))
    }

    async fn set_posture(
        &self,
        request: Request<proto::SetPostureRequest>,
    ) -> Result<Response<proto::PostureChangeReceiptProto>, Status> {
        let r = request.into_inner();
        let (new_posture, actor) = posture_from_req(&r).map_err(|e| network_error_to_status(&e))?;
        let receipt = self
            .controller
            .set_posture(new_posture, actor)
            .await
            .map_err(|e| network_error_to_status(&e))?;
        Ok(Response::new(receipt_to_resp(&receipt)))
    }

    // ── Subject directives (4 RPCs) ───────────────────────────────────────

    async fn get_subject_directive(
        &self,
        request: Request<proto::GetSubjectDirectiveRequest>,
    ) -> Result<Response<proto::GetSubjectDirectiveResponse>, Status> {
        let r = request.into_inner();
        let subj = SubjectId(r.subject);
        let directive = self.controller.subject_directive(&subj).await;
        Ok(Response::new(subject_directive_to_resp(&directive)))
    }

    async fn set_subject_directive(
        &self,
        request: Request<proto::SetSubjectDirectiveRequest>,
    ) -> Result<Response<()>, Status> {
        let r = request.into_inner();
        let (subject, directive, actor) =
            set_directive_from_req(&r).map_err(|e| network_error_to_status(&e))?;
        self.controller
            .set_subject_directive(subject, directive, actor)
            .await
            .map_err(|e| network_error_to_status(&e))?;
        Ok(Response::new(()))
    }

    async fn revoke_subject_directive(
        &self,
        request: Request<proto::RevokeSubjectDirectiveRequest>,
    ) -> Result<Response<()>, Status> {
        let r = request.into_inner();
        let subj = SubjectId(r.subject);
        let actor = SubjectId(r.actor);
        self.controller
            .revoke_subject_directive(&subj, actor)
            .await
            .map_err(|e| network_error_to_status(&e))?;
        Ok(Response::new(()))
    }

    async fn list_directives(
        &self,
        _request: Request<proto::ListDirectivesRequest>,
    ) -> Result<Response<proto::ListDirectivesResponse>, Status> {
        let entries = self.controller.list_directives().await;
        Ok(Response::new(list_directives_to_resp(&entries)))
    }

    // ── Outbound grants (3 RPCs) ──────────────────────────────────────────

    async fn append_outbound_grant(
        &self,
        request: Request<proto::OutboundGrantProto>,
    ) -> Result<Response<()>, Status> {
        let r = request.into_inner();
        let grant = grant_from_req(&r).map_err(|e| network_error_to_status(&e))?;
        self.grant_registry
            .append_grant(grant)
            .await
            .map_err(|e| network_error_to_status(&e))?;
        Ok(Response::new(()))
    }

    async fn revoke_outbound_grant(
        &self,
        request: Request<proto::RevokeOutboundGrantRequest>,
    ) -> Result<Response<proto::GrantTombstoneProto>, Status> {
        let r = request.into_inner();
        let revoker = SubjectId(r.revoker);
        let tombstone = self
            .grant_registry
            .revoke_grant(&r.grant_id, revoker, &r.reason)
            .await
            .map_err(|e| network_error_to_status(&e))?;
        Ok(Response::new(tombstone_to_resp(&tombstone)))
    }

    async fn get_subject_manifest(
        &self,
        request: Request<proto::GetSubjectManifestRequest>,
    ) -> Result<Response<proto::NetworkOutboundManifestProto>, Status> {
        let r = request.into_inner();
        let subj = SubjectId(r.subject);
        match self.grant_registry.get_manifest(&subj).await {
            Some(m) => Ok(Response::new(manifest_to_resp(&m))),
            None => Err(Status::not_found(format!(
                "no manifest for subject {}",
                subj.0
            ))),
        }
    }

    // ── Exposure FSM (6 RPCs) ─────────────────────────────────────────────

    async fn request_exposure(
        &self,
        request: Request<proto::RequestExposureRequest>,
    ) -> Result<Response<()>, Status> {
        let r = request.into_inner();
        let (class, requester, recovery_session_id) =
            inbound_exposure_from_req(&r).map_err(|e| network_error_to_status(&e))?;
        let reason =
            exposure_request_to_fsm_call(&class, &requester, recovery_session_id.as_deref())
                .map_err(|e| network_error_to_status(&e))?;
        match reason {
            crate::exposure_fsm::ExposureTransitionReason::LanRequest { requester } => {
                self.exposure_fsm
                    .request_lan(requester)
                    .await
                    .map_err(|e| network_error_to_status(&e))?;
            }
            crate::exposure_fsm::ExposureTransitionReason::PublicRequest {
                requester,
                recovery_session_id,
            } => {
                self.exposure_fsm
                    .request_public(requester, &recovery_session_id)
                    .await
                    .map_err(|e| network_error_to_status(&e))?;
            }
            _ => {}
        }
        Ok(Response::new(()))
    }

    async fn apply_exposure_policy_decision(
        &self,
        request: Request<proto::ApplyExposurePolicyDecisionRequest>,
    ) -> Result<Response<()>, Status> {
        let r = request.into_inner();
        // Route based on current FSM state.
        let state = self.exposure_fsm.current().await;
        match state.label() {
            ExposureApprovalLabel::LanPending => {
                self.exposure_fsm
                    .apply_lan_policy_decision(&r.decision_id)
                    .await
                    .map_err(|e| network_error_to_status(&e))?;
            }
            ExposureApprovalLabel::PublicPending => {
                // For PUBLIC we need co_signer + ttl — escalate handles that;
                // apply_policy_decision for public is a no-op stub here.
                return Err(Status::failed_precondition(
                    "use EscalateExposureToPublic for PUBLIC approval",
                ));
            }
            label => {
                return Err(Status::failed_precondition(format!(
                    "cannot apply policy decision from state {label}"
                )));
            }
        }
        Ok(Response::new(()))
    }

    async fn activate_exposure(
        &self,
        _request: Request<proto::ActivateExposureRequest>,
    ) -> Result<Response<()>, Status> {
        let state = self.exposure_fsm.current().await;
        match state.label() {
            ExposureApprovalLabel::LanApproved => {
                self.exposure_fsm
                    .activate_lan()
                    .await
                    .map_err(|e| network_error_to_status(&e))?;
            }
            ExposureApprovalLabel::PublicApproved => {
                self.exposure_fsm
                    .activate_public()
                    .await
                    .map_err(|e| network_error_to_status(&e))?;
            }
            label => {
                return Err(Status::failed_precondition(format!(
                    "cannot activate exposure from state {label}"
                )));
            }
        }
        Ok(Response::new(()))
    }

    async fn record_exposure_heartbeat(
        &self,
        _request: Request<proto::RecordExposureHeartbeatRequest>,
    ) -> Result<Response<()>, Status> {
        let state = self.exposure_fsm.current().await;
        match state.label() {
            ExposureApprovalLabel::LanActive => {
                self.exposure_fsm
                    .record_lan_heartbeat()
                    .await
                    .map_err(|e| network_error_to_status(&e))?;
            }
            ExposureApprovalLabel::PublicActive => {
                self.exposure_fsm
                    .record_public_heartbeat()
                    .await
                    .map_err(|e| network_error_to_status(&e))?;
            }
            label => {
                return Err(Status::failed_precondition(format!(
                    "cannot record heartbeat from state {label}"
                )));
            }
        }
        Ok(Response::new(()))
    }

    async fn escalate_exposure_to_public(
        &self,
        request: Request<proto::EscalateExposureToPublicRequest>,
    ) -> Result<Response<()>, Status> {
        let r = request.into_inner();
        let co_signer = SubjectId(r.co_signer);
        let ttl = r.ttl_expires_at.map(|ts| {
            #[allow(clippy::cast_sign_loss)]
            chrono::TimeZone::timestamp_opt(&chrono::Utc, ts.seconds, ts.nanos as u32)
                .single()
                .unwrap_or_else(chrono::Utc::now)
        });
        self.exposure_fsm
            .apply_public_co_signer_approval(
                &r.decision_id,
                co_signer,
                ttl.unwrap_or_else(|| chrono::Utc::now() + chrono::Duration::hours(4)),
            )
            .await
            .map_err(|e| network_error_to_status(&e))?;
        Ok(Response::new(()))
    }

    async fn revoke_exposure(
        &self,
        request: Request<proto::RevokeExposureRequest>,
    ) -> Result<Response<()>, Status> {
        let r = request.into_inner();
        self.exposure_fsm
            .revoke(&r.reason)
            .await
            .map_err(|e| network_error_to_status(&e))?;
        Ok(Response::new(()))
    }

    // ── Connection evaluation (2 RPCs) ────────────────────────────────────

    async fn evaluate_connection(
        &self,
        request: Request<proto::EvaluateConnectionRequest>,
    ) -> Result<Response<proto::ConnectionDecisionProto>, Status> {
        let r = request.into_inner();
        let req = eval_req_from_proto(&r).map_err(|e| network_error_to_status(&e))?;
        let decision = self
            .evaluator
            .evaluate(req)
            .await
            .map_err(|e| network_error_to_status(&e))?;
        Ok(Response::new(eval_decision_to_resp(&decision)))
    }

    async fn register_subject_group(
        &self,
        request: Request<proto::RegisterSubjectGroupRequest>,
    ) -> Result<Response<()>, Status> {
        let r = request.into_inner();
        let subject = SubjectId(r.subject);
        let group = crate::ids::GroupId(r.group);
        self.evaluator
            .register_subject_group(subject, group)
            .await
            .map_err(|e| network_error_to_status(&e))?;
        Ok(Response::new(()))
    }

    // ── AI external call (2 RPCs) ─────────────────────────────────────────

    async fn evaluate_ai_external_call(
        &self,
        request: Request<proto::EvaluateAiExternalCallRequest>,
    ) -> Result<Response<proto::AiExternalCallDecisionProto>, Status> {
        let r = request.into_inner();
        let req = ai_eval_req_from_proto(&r);
        let decision = self
            .ai_gate
            .evaluate_external_call(req)
            .await
            .map_err(|e| network_error_to_status(&e))?;
        Ok(Response::new(ai_decision_to_resp(&decision)))
    }

    async fn set_ai_posture(
        &self,
        request: Request<proto::SetAiPostureRequest>,
    ) -> Result<Response<()>, Status> {
        let r = request.into_inner();
        let subject = SubjectId(r.subject);
        let setter = SubjectId(r.setter);
        let posture = crate::service::conversions::ai_posture_from_proto(r.posture)
            .map_err(|e| network_error_to_status(&e))?;
        self.ai_gate
            .set_posture(subject, posture, setter)
            .await
            .map_err(|e| network_error_to_status(&e))?;
        Ok(Response::new(()))
    }

    // ── Firewall (2 RPCs) ─────────────────────────────────────────────────

    async fn apply_firewall_ruleset(
        &self,
        request: Request<proto::ApplyFirewallRulesetRequest>,
    ) -> Result<Response<()>, Status> {
        let r = request.into_inner();
        let ruleset = r
            .ruleset
            .ok_or_else(|| Status::invalid_argument("ruleset field is required"))?;
        let ruleset =
            firewall_ruleset_from_proto(&ruleset).map_err(|e| network_error_to_status(&e))?;
        self.firewall
            .apply_ruleset(ruleset)
            .await
            .map_err(|e| network_error_to_status(&e))?;
        Ok(Response::new(()))
    }

    async fn get_firewall_status(
        &self,
        _request: Request<proto::GetFirewallStatusRequest>,
    ) -> Result<Response<proto::GetFirewallStatusResponse>, Status> {
        let fallback_active = self.firewall.is_in_fallback().await;
        let active = self.firewall.active_ruleset().await;
        let history_count = self.firewall.history().await.len() as u64;

        let active_proto = active.map(|rs| proto::FirewallRulesetProto {
            backend: match rs.backend {
                crate::firewall::FirewallBackend::Nftables => {
                    proto::FirewallBackendProto::Nftables as i32
                }
                crate::firewall::FirewallBackend::IptablesFallback => {
                    proto::FirewallBackendProto::IptablesFallback as i32
                }
            },
            rules: rs
                .rules
                .iter()
                .map(|rule| proto::FirewallRuleProto {
                    rule_id: rule.rule_id.clone(),
                    chain: match rule.chain {
                        crate::firewall::FirewallChain::Input => {
                            proto::FirewallChainProto::FwInput as i32
                        }
                        crate::firewall::FirewallChain::Output => {
                            proto::FirewallChainProto::FwOutput as i32
                        }
                        crate::firewall::FirewallChain::Forward => {
                            proto::FirewallChainProto::FwForward as i32
                        }
                        crate::firewall::FirewallChain::Prerouting => {
                            proto::FirewallChainProto::FwPrerouting as i32
                        }
                        crate::firewall::FirewallChain::Postrouting => {
                            proto::FirewallChainProto::FwPostrouting as i32
                        }
                    },
                    priority: rule.priority,
                    match_kind: 0, // simplified — full match conversion deferred
                    match_value: String::new(),
                    match_port: None,
                    match_protocol: None,
                    action: match rule.action {
                        crate::firewall::FirewallAction::Accept => {
                            proto::FirewallActionProto::FwAccept as i32
                        }
                        crate::firewall::FirewallAction::Drop => {
                            proto::FirewallActionProto::FwDrop as i32
                        }
                        crate::firewall::FirewallAction::Reject => {
                            proto::FirewallActionProto::FwReject as i32
                        }
                        crate::firewall::FirewallAction::Log => {
                            proto::FirewallActionProto::FwLog as i32
                        }
                        crate::firewall::FirewallAction::Return => {
                            proto::FirewallActionProto::FwReturn as i32
                        }
                    },
                    comment: rule.comment.clone(),
                })
                .collect(),
            generation: rs.generation,
            built_at: Some(prost_types::Timestamp {
                seconds: rs.built_at.timestamp(),
                #[allow(clippy::cast_possible_wrap)]
                nanos: rs.built_at.timestamp_subsec_nanos() as i32,
            }),
        });

        Ok(Response::new(proto::GetFirewallStatusResponse {
            fallback_active,
            active_ruleset: active_proto,
            history_count,
        }))
    }
}

// ── DnsVpnServer ──────────────────────────────────────────────────────────

/// Mounts the resolver profile manager, VPN tunnel manager, and mDNS gate
/// behind the gRPC `DnsVpnService` trait.
#[derive(Clone)]
pub struct DnsVpnServer {
    resolver_mgr: Arc<ResolverProfileManager>,
    vpn_mgr: Arc<VpnTunnelManager>,
    mdns_gate: Arc<MdnsGate>,
}

impl std::fmt::Debug for DnsVpnServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DnsVpnServer").finish_non_exhaustive()
    }
}

impl DnsVpnServer {
    /// Construct a server mounting all three backing components.
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn new(
        resolver_mgr: Arc<ResolverProfileManager>,
        vpn_mgr: Arc<VpnTunnelManager>,
        mdns_gate: Arc<MdnsGate>,
    ) -> Self {
        Self {
            resolver_mgr,
            vpn_mgr,
            mdns_gate,
        }
    }
}

#[tonic::async_trait]
impl DnsVpnService for DnsVpnServer {
    // ── DNS (3 RPCs) ──────────────────────────────────────────────────────

    async fn admit_resolver_allowlist(
        &self,
        request: Request<proto::ResolverAllowlistProto>,
    ) -> Result<Response<()>, Status> {
        let r = request.into_inner();
        let list = resolver_allowlist_from_proto(&r).map_err(|e| network_error_to_status(&e))?;
        self.resolver_mgr
            .admit_allowlist(list)
            .await
            .map_err(|e| network_error_to_status(&e))?;
        Ok(Response::new(()))
    }

    async fn rotate_active_resolver_list(
        &self,
        request: Request<proto::RotateActiveResolverListRequest>,
    ) -> Result<Response<()>, Status> {
        let r = request.into_inner();
        self.resolver_mgr
            .rotate_active_list(&r.new_list_id)
            .await
            .map_err(|e| network_error_to_status(&e))?;
        Ok(Response::new(()))
    }

    async fn get_resolver_profile(
        &self,
        _request: Request<proto::GetResolverProfileRequest>,
    ) -> Result<Response<proto::ResolverProfileProto>, Status> {
        let profile = self.resolver_mgr.current_profile().await;
        Ok(Response::new(resolver_profile_to_proto(&profile)))
    }

    // ── VPN lifecycle (7 RPCs) ────────────────────────────────────────────

    async fn propose_vpn_tunnel(
        &self,
        request: Request<proto::ProposeVpnTunnelRequest>,
    ) -> Result<Response<()>, Status> {
        let r = request.into_inner();
        let (config, requester_str) =
            wireguard_config_from_proto(&r).map_err(|e| network_error_to_status(&e))?;
        let requester = SubjectId(requester_str);
        self.vpn_mgr
            .propose_tunnel(config, requester)
            .await
            .map_err(|e| network_error_to_status(&e))?;
        Ok(Response::new(()))
    }

    async fn approve_vpn_tunnel(
        &self,
        request: Request<proto::ApproveVpnTunnelRequest>,
    ) -> Result<Response<()>, Status> {
        let r = request.into_inner();
        self.vpn_mgr
            .approve_tunnel(&r.tunnel_id, &r.decision_id)
            .await
            .map_err(|e| network_error_to_status(&e))?;
        Ok(Response::new(()))
    }

    async fn activate_vpn_tunnel(
        &self,
        request: Request<proto::ActivateVpnTunnelRequest>,
    ) -> Result<Response<()>, Status> {
        let r = request.into_inner();
        self.vpn_mgr
            .activate_tunnel(&r.tunnel_id)
            .await
            .map_err(|e| network_error_to_status(&e))?;
        Ok(Response::new(()))
    }

    async fn record_vpn_handshake(
        &self,
        request: Request<proto::RecordVpnHandshakeRequest>,
    ) -> Result<Response<()>, Status> {
        let r = request.into_inner();
        self.vpn_mgr
            .record_handshake(&r.tunnel_id)
            .await
            .map_err(|e| network_error_to_status(&e))?;
        Ok(Response::new(()))
    }

    async fn revoke_vpn_tunnel(
        &self,
        request: Request<proto::RevokeVpnTunnelRequest>,
    ) -> Result<Response<()>, Status> {
        let r = request.into_inner();
        self.vpn_mgr
            .revoke_tunnel(&r.tunnel_id, &r.reason)
            .await
            .map_err(|e| network_error_to_status(&e))?;
        Ok(Response::new(()))
    }

    async fn rotate_vpn_peer_key(
        &self,
        request: Request<proto::PeerKeyRotationProto>,
    ) -> Result<Response<()>, Status> {
        let r = request.into_inner();
        let rotation = peer_key_rotation_from_proto(&r).map_err(|e| network_error_to_status(&e))?;
        self.vpn_mgr
            .rotate_peer_key(rotation)
            .await
            .map_err(|e| network_error_to_status(&e))?;
        Ok(Response::new(()))
    }

    async fn list_vpn_tunnels(
        &self,
        _request: Request<proto::ListVpnTunnelsRequest>,
    ) -> Result<Response<proto::ListVpnTunnelsResponse>, Status> {
        let tunnels = self.vpn_mgr.list_tunnels().await;
        let entries: Vec<proto::VpnTunnelEntry> = tunnels
            .iter()
            .map(|(id, label)| vpn_tunnel_entry_to_proto(id, *label))
            .collect();
        Ok(Response::new(proto::ListVpnTunnelsResponse {
            tunnels: entries,
        }))
    }

    // ── mDNS (3 RPCs) ─────────────────────────────────────────────────────

    async fn set_mdns_posture(
        &self,
        request: Request<proto::SetMdnsPostureRequest>,
    ) -> Result<Response<()>, Status> {
        let r = request.into_inner();
        let posture = mdns_posture_from_str(&r.posture, r.allowlist_id.as_deref())
            .map_err(|e| network_error_to_status(&e))?;
        self.mdns_gate
            .set_posture(posture)
            .await
            .map_err(|e| network_error_to_status(&e))?;
        Ok(Response::new(()))
    }

    async fn admit_mdns_allowlist(
        &self,
        request: Request<proto::MdnsAdvertisementAllowlistProto>,
    ) -> Result<Response<()>, Status> {
        let r = request.into_inner();
        let list = mdns_allowlist_from_proto(&r).map_err(|e| network_error_to_status(&e))?;
        self.mdns_gate
            .admit_allowlist(list)
            .await
            .map_err(|e| network_error_to_status(&e))?;
        Ok(Response::new(()))
    }

    #[allow(clippy::cast_possible_truncation)]
    async fn check_mdns_advertisement(
        &self,
        request: Request<proto::CheckMdnsAdvertisementRequest>,
    ) -> Result<Response<()>, Status> {
        let r = request.into_inner();
        self.mdns_gate
            .check_advertisement(&r.service_type, &r.instance_name, r.port as u16)
            .await
            .map_err(|e| network_error_to_status(&e))?;
        Ok(Response::new(()))
    }
}

// ── Bootstrap helpers ──────────────────────────────────────────────────────

/// Build a `tonic::transport::server::Router` with `NetworkPolicyService` mounted.
#[must_use]
pub fn build_network_router(svc: NetworkPolicyServer) -> tonic::transport::server::Router {
    tonic::transport::server::Server::builder()
        .add_service(proto::network_policy_service_server::NetworkPolicyServiceServer::new(svc))
}

/// Build a `tonic::transport::server::Router` with `DnsVpnService` mounted.
#[must_use]
pub fn build_dnsvpn_router(svc: DnsVpnServer) -> tonic::transport::server::Router {
    tonic::transport::server::Server::builder()
        .add_service(proto::dns_vpn_service_server::DnsVpnServiceServer::new(svc))
}
