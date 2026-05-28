#![allow(
    missing_docs,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use std::sync::Arc;

use aios_network::{
    ConnectionDecision, EvaluateConnectionRequest, InMemoryNetworkPolicyController,
    NetworkPolicyController, NetworkPolicyErrorCode, NetworkPosture, OutboundDirective,
    ProtocolFamily, SubjectId,
};

// ── construction ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn new_controller_starts_at_lan_local() {
    let ctrl = InMemoryNetworkPolicyController::new();
    assert_eq!(ctrl.current_posture().await, NetworkPosture::LanLocal);
}

#[tokio::test]
async fn new_with_posture_loopback_only_starts_at_loopback_only() {
    let ctrl = InMemoryNetworkPolicyController::new_with_posture(NetworkPosture::LoopbackOnly);
    assert_eq!(ctrl.current_posture().await, NetworkPosture::LoopbackOnly);
}

// ── posture ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn set_posture_records_receipt() {
    let ctrl = InMemoryNetworkPolicyController::new();
    let receipt = ctrl
        .set_posture(NetworkPosture::Airgap, SubjectId("human:op".into()))
        .await
        .unwrap();
    assert_eq!(receipt.from, NetworkPosture::LanLocal);
    assert_eq!(receipt.to, NetworkPosture::Airgap);
    assert_eq!(ctrl.current_posture().await, NetworkPosture::Airgap);
}

#[tokio::test]
async fn posture_history_after_3_changes_returns_3_receipts() {
    let ctrl = InMemoryNetworkPolicyController::new();
    let actor = SubjectId("human:op".into());
    ctrl.set_posture(NetworkPosture::Airgap, actor.clone())
        .await
        .unwrap();
    ctrl.set_posture(NetworkPosture::LoopbackOnly, actor.clone())
        .await
        .unwrap();
    ctrl.set_posture(NetworkPosture::LanLocal, actor.clone())
        .await
        .unwrap();
    let history = ctrl.posture_history().await;
    assert_eq!(history.len(), 3);
}

// ── subject directives ───────────────────────────────────────────────────────

#[tokio::test]
async fn subject_directive_with_no_rule_returns_deny_all() {
    let ctrl = InMemoryNetworkPolicyController::new();
    assert_eq!(
        ctrl.subject_directive(&SubjectId("unknown:subject".into()))
            .await,
        OutboundDirective::DenyAll
    );
}

#[tokio::test]
async fn set_subject_directive_then_get_round_trip() {
    let ctrl = InMemoryNetworkPolicyController::new();
    let subj = SubjectId("human:lucky".into());
    ctrl.set_subject_directive(
        subj.clone(),
        OutboundDirective::AllowInternet,
        SubjectId("human:op".into()),
    )
    .await
    .unwrap();
    assert_eq!(
        ctrl.subject_directive(&subj).await,
        OutboundDirective::AllowInternet
    );
}

#[tokio::test]
async fn list_directives_after_2_sets_returns_2() {
    let ctrl = InMemoryNetworkPolicyController::new();
    let actor = SubjectId("human:op".into());
    ctrl.set_subject_directive(
        SubjectId("human:a".into()),
        OutboundDirective::AllowInternet,
        actor.clone(),
    )
    .await
    .unwrap();
    ctrl.set_subject_directive(
        SubjectId("agent:b".into()),
        OutboundDirective::AllowLoopbackOnly,
        actor,
    )
    .await
    .unwrap();
    assert_eq!(ctrl.list_directives().await.len(), 2);
}

#[tokio::test]
async fn revoke_subject_directive_then_get_returns_deny_all() {
    let ctrl = InMemoryNetworkPolicyController::new();
    let subj = SubjectId("human:tmp".into());
    let actor = SubjectId("human:op".into());
    ctrl.set_subject_directive(
        subj.clone(),
        OutboundDirective::AllowInternet,
        actor.clone(),
    )
    .await
    .unwrap();
    ctrl.revoke_subject_directive(&subj, actor).await.unwrap();
    assert_eq!(
        ctrl.subject_directive(&subj).await,
        OutboundDirective::DenyAll
    );
}

#[tokio::test]
async fn revoke_unknown_subject_is_idempotent_ok() {
    let ctrl = InMemoryNetworkPolicyController::new();
    let result = ctrl
        .revoke_subject_directive(
            &SubjectId("never:existed".into()),
            SubjectId("human:op".into()),
        )
        .await;
    assert!(result.is_ok());
}

// ── connection evaluation (T-152 stub) ───────────────────────────────────────

#[tokio::test]
async fn evaluate_connection_deny_all_returns_denied() {
    let ctrl = InMemoryNetworkPolicyController::new();
    let req = EvaluateConnectionRequest {
        subject: SubjectId("unknown:subj".into()),
        destination: "example.com:443".into(),
        protocol: ProtocolFamily::Tcp,
    };
    assert_eq!(
        ctrl.evaluate_connection(req).await.unwrap(),
        ConnectionDecision::Denied {
            code: NetworkPolicyErrorCode::DefaultDeny
        }
    );
}

#[tokio::test]
async fn evaluate_connection_loopback_127_0_0_1_allowed() {
    let ctrl = InMemoryNetworkPolicyController::new();
    let subj = SubjectId("service:lb".into());
    ctrl.set_subject_directive(
        subj.clone(),
        OutboundDirective::AllowLoopbackOnly,
        SubjectId("human:op".into()),
    )
    .await
    .unwrap();

    let req = EvaluateConnectionRequest {
        subject: subj,
        destination: "127.0.0.1:8080".into(),
        protocol: ProtocolFamily::Tcp,
    };
    assert_eq!(
        ctrl.evaluate_connection(req).await.unwrap(),
        ConnectionDecision::Allowed {
            matched_rule_id: "allow-loopback".into()
        }
    );
}

#[tokio::test]
async fn evaluate_connection_loopback_localhost_allowed() {
    let ctrl = InMemoryNetworkPolicyController::new();
    let subj = SubjectId("service:lb".into());
    ctrl.set_subject_directive(
        subj.clone(),
        OutboundDirective::AllowLoopbackOnly,
        SubjectId("human:op".into()),
    )
    .await
    .unwrap();

    let req = EvaluateConnectionRequest {
        subject: subj,
        destination: "localhost:3000".into(),
        protocol: ProtocolFamily::Tcp,
    };
    assert_eq!(
        ctrl.evaluate_connection(req).await.unwrap(),
        ConnectionDecision::Allowed {
            matched_rule_id: "allow-loopback".into()
        }
    );
}

#[tokio::test]
async fn evaluate_connection_non_loopback_with_loopback_only_denied() {
    let ctrl = InMemoryNetworkPolicyController::new();
    let subj = SubjectId("service:lb".into());
    ctrl.set_subject_directive(
        subj.clone(),
        OutboundDirective::AllowLoopbackOnly,
        SubjectId("human:op".into()),
    )
    .await
    .unwrap();

    let req = EvaluateConnectionRequest {
        subject: subj,
        destination: "192.168.1.1:443".into(),
        protocol: ProtocolFamily::Tcp,
    };
    assert_eq!(
        ctrl.evaluate_connection(req).await.unwrap(),
        ConnectionDecision::Denied {
            code: NetworkPolicyErrorCode::DefaultDeny
        }
    );
}

#[tokio::test]
async fn evaluate_connection_allow_internet_allowed() {
    let ctrl = InMemoryNetworkPolicyController::new();
    let subj = SubjectId("human:lucky".into());
    ctrl.set_subject_directive(
        subj.clone(),
        OutboundDirective::AllowInternet,
        SubjectId("human:op".into()),
    )
    .await
    .unwrap();

    let req = EvaluateConnectionRequest {
        subject: subj,
        destination: "example.com:443".into(),
        protocol: ProtocolFamily::Tcp,
    };
    assert_eq!(
        ctrl.evaluate_connection(req).await.unwrap(),
        ConnectionDecision::Allowed {
            matched_rule_id: "allow-internet".into()
        }
    );
}

#[tokio::test]
async fn evaluate_connection_allow_list_only_denied_in_t152() {
    let ctrl = InMemoryNetworkPolicyController::new();
    let subj = SubjectId("service:wl".into());
    ctrl.set_subject_directive(
        subj.clone(),
        OutboundDirective::AllowListOnly {
            allowlist_id: "wl-01".into(),
        },
        SubjectId("human:op".into()),
    )
    .await
    .unwrap();

    let req = EvaluateConnectionRequest {
        subject: subj,
        destination: "example.com:443".into(),
        protocol: ProtocolFamily::Tcp,
    };
    assert_eq!(
        ctrl.evaluate_connection(req).await.unwrap(),
        ConnectionDecision::Denied {
            code: NetworkPolicyErrorCode::DefaultDeny
        }
    );
}

#[tokio::test]
async fn evaluate_connection_allow_vpn_only_denied_in_t152() {
    let ctrl = InMemoryNetworkPolicyController::new();
    let subj = SubjectId("service:vpn".into());
    ctrl.set_subject_directive(
        subj.clone(),
        OutboundDirective::AllowVpnOnly {
            tunnel_id: "vpn-01".into(),
        },
        SubjectId("human:op".into()),
    )
    .await
    .unwrap();

    let req = EvaluateConnectionRequest {
        subject: subj,
        destination: "10.0.0.1:443".into(),
        protocol: ProtocolFamily::Udp,
    };
    assert_eq!(
        ctrl.evaluate_connection(req).await.unwrap(),
        ConnectionDecision::Denied {
            code: NetworkPolicyErrorCode::DefaultDeny
        }
    );
}

// ── concurrency ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn concurrent_set_directive_3_subjects_no_panic() {
    let ctrl = Arc::new(InMemoryNetworkPolicyController::new());
    let actor = SubjectId("human:op".into());

    let mut handles = Vec::new();
    for i in 0..3 {
        let ctrl = Arc::clone(&ctrl);
        let actor = actor.clone();
        handles.push(tokio::spawn(async move {
            ctrl.set_subject_directive(
                SubjectId(format!("subject:{i}")),
                OutboundDirective::AllowInternet,
                actor,
            )
            .await
            .unwrap();
        }));
    }
    for h in handles {
        h.await.unwrap();
    }
    assert_eq!(ctrl.list_directives().await.len(), 3);
}
