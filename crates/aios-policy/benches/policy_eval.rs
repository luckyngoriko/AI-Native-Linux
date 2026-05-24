#![allow(
    missing_docs,
    clippy::panic,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::result_large_err
)]
//! Criterion microbench for the S2.3 §18.1 perf-budget probe (T-024).
//!
//! Measures three things:
//!
//! 1. **Fresh evaluation latency** — no cache attached; every iteration
//!    runs the full pipeline. Spec target: p95 < 5 ms (no enrichment).
//! 2. **Cached evaluation latency** — cache pre-warmed with a known
//!    decision; every iteration is a cache hit. Spec target: p95 < 1 ms
//!    (no enrichment).
//! 3. **Cache hit-rate workload** — 1 000 mixed envelopes, ~80% repeat;
//!    reports the effective hit rate.
//!
//! ## Informational only
//!
//! Criterion reports landed in `target/criterion/...`; `cargo bench` runs
//! them. The bench compiles under `cargo bench -p aios-policy --no-run`
//! to keep the M3 gate honest, but the **numeric thresholds are NOT
//! enforced in `cargo test`** — production CI integration of the perf
//! gate lands in T-025+ once the §15 telemetry-collection wiring stabilises.

use std::time::Duration;

use criterion::{black_box, criterion_group, criterion_main, Criterion};

use aios_action::{ActionEnvelope, Identity, Request, Trace};
use aios_policy::cache::{CacheKey, SharedDecisionCache};
use aios_policy::{
    AdapterEnrichment, EnrichmentSnapshot, HydratedSubject, InMemoryPolicyKernel, ObjectEnrichment,
    PolicyContext, PolicyKernel, SubjectType,
};

fn make_envelope(action: &str, tag: u32) -> ActionEnvelope {
    let identity = Identity::new("agent:dev", true);
    let request = Request::new(
        action,
        serde_json::json!({"tag": tag, "risk": {"destructive": false}}),
    );
    let trace_id = format!("{tag:032x}");
    let span_id = format!("{tag:016x}");
    let trace = Trace::new(&trace_id, &span_id, None);
    ActionEnvelope::new(identity, request, trace)
}

fn make_subject() -> HydratedSubject {
    HydratedSubject {
        canonical_subject_id: "agent:dev".into(),
        subject_type: SubjectType::Agent,
        groups: Vec::new(),
        capabilities: Vec::new(),
        session_class: "INTERNAL".into(),
        recovery_mode: false,
        is_ai: true,
    }
}

fn make_context() -> PolicyContext {
    let snapshot =
        EnrichmentSnapshot::with_fields(ObjectEnrichment::default(), AdapterEnrichment::default())
            .unwrap_or_default();
    PolicyContext::new(make_subject(), snapshot, "polb_bench_v1", "code_bench")
}

fn bench_fresh_evaluation(c: &mut Criterion) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap_or_else(|_| tokio::runtime::Runtime::new().unwrap_or_else(|_| panic!("rt")));
    let kernel = InMemoryPolicyKernel::new();
    let ctx = make_context();
    let env = make_envelope("service.status", 1);
    c.bench_function("policy_eval_fresh_no_cache", |b| {
        b.iter(|| {
            let d = rt
                .block_on(kernel.evaluate_policy(black_box(&env), black_box(&ctx)))
                .unwrap_or_else(|_| panic!("eval fresh"));
            black_box(d);
        });
    });
}

fn bench_cached_evaluation(c: &mut Criterion) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap_or_else(|_| tokio::runtime::Runtime::new().unwrap_or_else(|_| panic!("rt")));
    let cache = SharedDecisionCache::with_capacity(64);
    let kernel = InMemoryPolicyKernel::new_with_cache(cache.clone());
    let ctx = make_context();
    let env = make_envelope("service.status", 2);
    // Warm the cache with one evaluation.
    let _ = rt.block_on(kernel.evaluate_policy(&env, &ctx));
    c.bench_function("policy_eval_cached_hit", |b| {
        b.iter(|| {
            let d = rt
                .block_on(kernel.evaluate_policy(black_box(&env), black_box(&ctx)))
                .unwrap_or_else(|_| panic!("eval cached"));
            black_box(d);
        });
    });
    // Sanity report
    let key = CacheKey::new(
        env.request.request_hash().unwrap_or_default(),
        ctx.bundle_version.clone(),
    );
    let _ = cache.get(&key);
}

fn bench_mixed_workload_hit_rate(c: &mut Criterion) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap_or_else(|_| tokio::runtime::Runtime::new().unwrap_or_else(|_| panic!("rt")));
    let cache = SharedDecisionCache::with_capacity(256);
    let kernel = InMemoryPolicyKernel::new_with_cache(cache);
    let ctx = make_context();
    // Build a small population of envelopes; ~80% of accesses repeat.
    let envelopes: Vec<ActionEnvelope> = (0..50)
        .map(|i| make_envelope("service.status", i))
        .collect();
    c.bench_function("policy_eval_mixed_workload_1000", |b| {
        b.iter(|| {
            for i in 0..1000_u32 {
                let idx = if i % 5 == 0 {
                    (i % 50) as usize
                } else {
                    (i % 10) as usize
                };
                let env = &envelopes[idx];
                let d = rt
                    .block_on(kernel.evaluate_policy(black_box(env), black_box(&ctx)))
                    .unwrap_or_else(|_| panic!("eval mixed"));
                black_box(d);
            }
        });
    });
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .sample_size(10)
        .warm_up_time(Duration::from_millis(200))
        .measurement_time(Duration::from_secs(1));
    targets = bench_fresh_evaluation, bench_cached_evaluation, bench_mixed_workload_hit_rate
}
criterion_main!(benches);
