//! T-014 — Privacy ceiling integration test (S3.1 §10 + §23.2 + §11.4).
//!
//! Exercises the gRPC `Query` privacy ceiling end-to-end:
//!
//! 1. Spin up the in-memory backend over a real tonic transport.
//! 2. Append a mixed-subject corpus (operator-1@ops, operator-2@finance,
//!    ai-1@ops, plus system audit records).
//! 3. Query as each persona and assert:
//!    - the visible receipt count matches the §10 rules,
//!    - the `x-aios-suppressed-count` initial-metadata header reports the
//!      number of receipts silently filtered.
//!
//! The harness mirrors `grpc_roundtrip.rs` for consistency.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    clippy::significant_drop_tightening,
    reason = "test harness; panics are the desired failure signal; drop tightening lint is noise inside async-tokio test bodies that share a tonic Channel"
)]

use std::net::SocketAddr;

use ed25519_dalek::SigningKey;
use tokio::sync::oneshot;
use tokio_stream::StreamExt;
use tonic::transport::Channel;

use aios_evidence::service::proto::evidence_log_client::EvidenceLogClient;
use aios_evidence::service::proto::{AppendRequest, QueryRequest, RecordType, SubscribeRequest};
use aios_evidence::service::{build_router, InMemoryEvidenceLog, SUPPRESSED_COUNT_TRAILER};

// ─────────────────────────────────────────────────────────────────────
// Harness
// ─────────────────────────────────────────────────────────────────────

fn test_backend() -> InMemoryEvidenceLog {
    let sk = SigningKey::from_bytes(&[0x42; 32]);
    InMemoryEvidenceLog::new(sk)
}

async fn spawn_server() -> (SocketAddr, oneshot::Sender<()>) {
    let backend = test_backend();
    let router = build_router(backend);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("local addr");
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let stream = tokio_stream::wrappers::TcpListenerStream::new(listener);
    tokio::spawn(async move {
        let _ = router
            .serve_with_incoming_shutdown(stream, async move {
                let _ = shutdown_rx.await;
            })
            .await;
    });
    // tiny settle to let serve() reach the accept loop.
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    (addr, shutdown_tx)
}

async fn connect(addr: SocketAddr) -> EvidenceLogClient<Channel> {
    let uri = format!("http://{addr}");
    EvidenceLogClient::connect(uri).await.expect("connect")
}

fn append(record_type: RecordType, subject: &str) -> AppendRequest {
    AppendRequest {
        schema_version: "aios.evidence.v1alpha1".to_owned(),
        payload: None,
        record_type: i32::from(record_type),
        subject: subject.to_owned(),
        action_id: String::new(),
        correlation_id: String::new(),
        trace_id: String::new(),
        simulated: false,
    }
}

fn query_for(caller: &str, group: &str, is_ai: bool, recovery: bool) -> QueryRequest {
    QueryRequest {
        record_types_filter: vec![],
        subject_filter: String::new(),
        correlation_id_filter: String::new(),
        action_id_filter: String::new(),
        from_time: None,
        to_time: None,
        text_match: String::new(),
        limit: 0,
        subject: caller.to_owned(),
        caller_primary_group: group.to_owned(),
        caller_is_ai: is_ai,
        caller_is_recovery_mode: recovery,
    }
}

// ─────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn privacy_ceiling_query_three_subjects() {
    let (addr, shutdown) = spawn_server().await;
    let mut client = connect(addr).await;

    // Seed the log with 10 receipts covering the three subjects + system.
    //   - 3x operator-1 (ops group), private records (ActionReceived).
    //   - 2x operator-2 (finance group), private records (ActionReceived).
    //   - 2x ai-1 (ops group), private records (ModelCall).
    //   - 2x system PolicyDecision (constitutionally public).
    //   - 1x operator-2 ExecutionCompleted (FOREVER retention).
    let seeds: &[(RecordType, &str)] = &[
        (RecordType::ActionReceived, "human:ops/operator-1"),
        (RecordType::ActionReceived, "human:ops/operator-1"),
        (RecordType::ActionReceived, "human:ops/operator-1"),
        (RecordType::ActionReceived, "human:finance/operator-2"),
        (RecordType::ActionReceived, "human:finance/operator-2"),
        (RecordType::ModelCall, "ai:ops/ai-1"),
        (RecordType::ModelCall, "ai:ops/ai-1"),
        (RecordType::PolicyDecision, "_system:service:policy-kernel"),
        (RecordType::PolicyDecision, "_system:service:policy-kernel"),
        (RecordType::ExecutionCompleted, "human:finance/operator-2"),
    ];
    for (rt, subj) in seeds {
        client.append(append(*rt, subj)).await.expect("append rpc");
    }

    // ── operator-1@ops ──
    // Visible:
    //   - 3 own ActionReceived
    //   - 0 from operator-2 (different group, not FOREVER under non-recovery
    //     mode and ActionReceived isn't FOREVER)
    //   - 0 from ai-1 (group-share would admit but ai is in different
    //     subject-kind; wait — same group "ops". Group rule admits ai-1's
    //     records to a non-AI operator IF they share group. Let's keep
    //     consistent with module rules: rule 4 admits when group matches.
    //     So 2 ai-1 records visible.
    //   - 2 PolicyDecision (constitutionally public)
    //   - 1 operator-2 ExecutionCompleted (FOREVER; under recovery? NO,
    //     here recovery=false. So NOT visible.)
    // Total visible: 3 + 2 + 2 = 7. Suppressed: 3 (2x finance Action +
    //   1x finance ExecutionCompleted).
    let response = client
        .query(query_for("human:ops/operator-1", "ops", false, false))
        .await
        .expect("query rpc");
    let suppressed = response
        .metadata()
        .get(SUPPRESSED_COUNT_TRAILER)
        .expect("trailer")
        .to_str()
        .expect("ascii");
    assert_eq!(suppressed, "3", "operator-1 suppressed count");
    let visible: Vec<_> = response.into_inner().collect::<Vec<_>>().await;
    assert_eq!(visible.len(), 7, "operator-1 visible count");

    // ── operator-2@finance ──
    // Visible:
    //   - 0 ops operator-1 records (cross-group)
    //   - 0 ops ai-1 records (cross-group)
    //   - 2 own ActionReceived
    //   - 1 own ExecutionCompleted
    //   - 2 PolicyDecision (public)
    // Total: 5. Suppressed: 5 (3 ops/op-1 + 2 ops/ai-1).
    let response = client
        .query(query_for(
            "human:finance/operator-2",
            "finance",
            false,
            false,
        ))
        .await
        .expect("query rpc");
    let suppressed = response
        .metadata()
        .get(SUPPRESSED_COUNT_TRAILER)
        .expect("trailer")
        .to_str()
        .expect("ascii");
    assert_eq!(suppressed, "5", "operator-2 suppressed count");
    let visible: Vec<_> = response.into_inner().collect::<Vec<_>>().await;
    assert_eq!(visible.len(), 5, "operator-2 visible count");

    // ── ai-1@ops ──
    // AI is isolated: sees only own records + public-about-system.
    //   - 2 own ModelCall
    //   - 2 PolicyDecision (_system subject; public + AI Rule 5b)
    //   - 0 from operator-1 (human, AI cannot see)
    //   - 0 from operator-2 (cross-actor)
    // Total: 4. Suppressed: 6.
    let response = client
        .query(query_for("ai:ops/ai-1", "ops", true, false))
        .await
        .expect("query rpc");
    let suppressed = response
        .metadata()
        .get(SUPPRESSED_COUNT_TRAILER)
        .expect("trailer")
        .to_str()
        .expect("ascii");
    assert_eq!(suppressed, "6", "ai-1 suppressed count");
    let visible: Vec<_> = response.into_inner().collect::<Vec<_>>().await;
    assert_eq!(visible.len(), 4, "ai-1 visible count");

    // ── operator-1@ops + recovery_mode ──
    // Recovery broadens FOREVER records cross-group. Adds the 1 operator-2
    // ExecutionCompleted (FOREVER). Now suppressed drops from 3 to 2.
    let response = client
        .query(query_for("human:ops/operator-1", "ops", false, true))
        .await
        .expect("query rpc");
    let suppressed = response
        .metadata()
        .get(SUPPRESSED_COUNT_TRAILER)
        .expect("trailer")
        .to_str()
        .expect("ascii");
    assert_eq!(suppressed, "2", "operator-1 recovery suppressed count");
    let visible: Vec<_> = response.into_inner().collect::<Vec<_>>().await;
    assert_eq!(visible.len(), 8, "operator-1 recovery visible count");

    drop(shutdown);
}

#[tokio::test]
async fn privacy_ceiling_subscribe_filters_per_subscriber() {
    let (addr, shutdown) = spawn_server().await;
    let mut client = connect(addr).await;

    // Seed two private records and one public record.
    client
        .append(append(RecordType::ActionReceived, "human:ops/operator-1"))
        .await
        .expect("seed1");
    client
        .append(append(
            RecordType::ActionReceived,
            "human:finance/operator-2",
        ))
        .await
        .expect("seed2");
    client
        .append(append(
            RecordType::PolicyDecision,
            "_system:service:policy-kernel",
        ))
        .await
        .expect("seed3");

    // Subscribe as operator-1@ops with no bookmark — replay should yield
    // 2: own record + public PolicyDecision; operator-2 is filtered.
    let stream = client
        .subscribe(SubscribeRequest {
            record_types_filter: vec![],
            subject_filter: String::new(),
            correlation_id_filter: String::new(),
            resume_from_receipt_id: String::new(),
            max_buffered: 0,
            caller_subject: "human:ops/operator-1".to_owned(),
            caller_primary_group: "ops".to_owned(),
            caller_is_ai: false,
            caller_is_recovery_mode: false,
        })
        .await
        .expect("subscribe rpc")
        .into_inner();

    // Read 2 replay events with a deadline, then drop.
    let collected = tokio::time::timeout(std::time::Duration::from_millis(500), async move {
        let mut s = stream;
        let mut out = Vec::new();
        for _ in 0..2 {
            if let Some(item) = s.next().await {
                out.push(item.expect("ok"));
            }
        }
        out
    })
    .await
    .expect("timeout");

    assert_eq!(collected.len(), 2);
    for w in &collected {
        // Must be either own record or the public PolicyDecision.
        assert!(
            w.subject == "human:ops/operator-1" || w.subject == "_system:service:policy-kernel",
            "operator-1 should not see {}",
            w.subject
        );
    }

    drop(shutdown);
}
