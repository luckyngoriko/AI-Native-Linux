#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    clippy::doc_markdown,
    clippy::significant_drop_tightening,
    clippy::wildcard_imports,
    clippy::similar_names,
    clippy::cast_possible_wrap,
    clippy::too_many_lines,
    clippy::needless_collect,
    clippy::format_collect,
    clippy::missing_const_for_fn,
    missing_docs,
    reason = "test code; panic-on-failure is the idiomatic test signal"
)]

use aios_integration::*;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn svc(id: &str) -> ComposedService {
    ComposedService {
        service_id: id.into(),
        crate_name: id.into(),
        binding_endpoint: format!("unix:/run/aios/{id}.sock"),
        depends_on: vec![],
    }
}

fn dep(from: &str, to: &str) -> ServiceDependency {
    ServiceDependency {
        from_service: from.into(),
        to_service: to.into(),
        required: true,
    }
}

fn make_composition(
    id: &str,
    services: Vec<ComposedService>,
    deps: Vec<ServiceDependency>,
) -> ServiceComposition {
    ServiceComposition {
        composition_id: ComposedSystemId(id.to_string()),
        services,
        dependencies: deps,
        topological_order: vec![],
    }
}

// ---------------------------------------------------------------------------
// Engine: register + get
// ---------------------------------------------------------------------------

#[tokio::test]
async fn register_and_get_returns_topological_order() {
    let engine = CompositionEngine::new();
    let id = ComposedSystemId("reg-get".to_string());
    engine
        .register(make_composition(
            "reg-get",
            vec![svc("a"), svc("b"), svc("c")],
            vec![dep("b", "a"), dep("c", "b")],
        ))
        .await
        .unwrap();
    let got = engine.get(&id).await.unwrap();
    assert_eq!(got.topological_order, vec!["a", "b", "c"]);
}

// ---------------------------------------------------------------------------
// Engine: register missing dependency
// ---------------------------------------------------------------------------

#[tokio::test]
async fn register_missing_dep_target_returns_error() {
    let engine = CompositionEngine::new();
    let err = engine
        .register(make_composition(
            "missing-dep",
            vec![svc("a"), svc("b")],
            vec![dep("b", "missing-svc")],
        ))
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        IntegrationError::ComposedServiceMissing { .. }
    ));
}

// ---------------------------------------------------------------------------
// Engine: register missing from_service
// ---------------------------------------------------------------------------

#[tokio::test]
async fn register_missing_from_service_returns_error() {
    let engine = CompositionEngine::new();
    let err = engine
        .register(make_composition(
            "missing-from",
            vec![svc("a"), svc("b")],
            vec![dep("missing-svc", "a")],
        ))
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        IntegrationError::ComposedServiceMissing { .. }
    ));
}

// ---------------------------------------------------------------------------
// Engine: register cycle
// ---------------------------------------------------------------------------

#[tokio::test]
async fn register_cycle_returns_composition_cycle_detected() {
    let engine = CompositionEngine::new();
    let err = engine
        .register(make_composition(
            "has-cycle",
            vec![svc("a"), svc("b"), svc("c")],
            vec![dep("b", "a"), dep("a", "c"), dep("c", "b")],
        ))
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        IntegrationError::CompositionCycleDetected { .. }
    ));
}

// ---------------------------------------------------------------------------
// Engine: get nonexistent
// ---------------------------------------------------------------------------

#[tokio::test]
async fn get_nonexistent_returns_none() {
    let engine = CompositionEngine::new();
    assert!(engine
        .get(&ComposedSystemId("no-such".into()))
        .await
        .is_none());
}

// ---------------------------------------------------------------------------
// Engine: list
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_after_three_registrations_returns_three() {
    let engine = CompositionEngine::new();
    for i in 0..3 {
        engine
            .register(make_composition(
                &format!("list-{i}"),
                vec![svc("a"), svc("b")],
                vec![dep("b", "a")],
            ))
            .await
            .unwrap();
    }
    assert_eq!(engine.list().await.len(), 3);
}

// ---------------------------------------------------------------------------
// Engine: validate does not store
// ---------------------------------------------------------------------------

#[tokio::test]
async fn validate_does_not_store() {
    let engine = CompositionEngine::new();
    let comp = make_composition(
        "val-only",
        vec![svc("a"), svc("b"), svc("c")],
        vec![dep("b", "a"), dep("c", "b")],
    );
    let order = engine.validate(&comp).await.unwrap();
    assert_eq!(order, vec!["a", "b", "c"]);
    assert!(engine.list().await.is_empty());
}

// ---------------------------------------------------------------------------
// Engine: validate cycle
// ---------------------------------------------------------------------------

#[tokio::test]
async fn validate_cycle_returns_error() {
    let engine = CompositionEngine::new();
    let comp = make_composition(
        "val-cycle",
        vec![svc("a"), svc("b")],
        vec![dep("a", "b"), dep("b", "a")],
    );
    let err = engine.validate(&comp).await.unwrap_err();
    assert!(matches!(
        err,
        IntegrationError::CompositionCycleDetected { .. }
    ));
    assert!(engine.list().await.is_empty());
}

// ---------------------------------------------------------------------------
// Engine: validate missing service
// ---------------------------------------------------------------------------

#[tokio::test]
async fn validate_missing_service_returns_error() {
    let engine = CompositionEngine::new();
    let comp = make_composition("val-missing", vec![svc("a")], vec![dep("a", "missing")]);
    let err = engine.validate(&comp).await.unwrap_err();
    assert!(matches!(
        err,
        IntegrationError::ComposedServiceMissing { .. }
    ));
}

// ---------------------------------------------------------------------------
// compute_topological_order
// ---------------------------------------------------------------------------

#[test]
fn topological_order_empty_returns_empty() {
    assert!(compute_topological_order(&[], &[]).unwrap().is_empty());
}

#[test]
fn topological_order_single_service() {
    let order = compute_topological_order(&[svc("a")], &[]).unwrap();
    assert_eq!(order, vec!["a"]);
}

#[test]
fn topological_order_linear_chain() {
    let services = vec![svc("a"), svc("b"), svc("c")];
    let deps = vec![dep("b", "a"), dep("c", "b")];
    let order = compute_topological_order(&services, &deps).unwrap();
    assert_eq!(order, vec!["a", "b", "c"]);
}

#[test]
fn topological_order_diamond() {
    let services = vec![svc("a"), svc("b"), svc("c"), svc("d")];
    let deps = vec![dep("b", "a"), dep("c", "a"), dep("d", "b"), dep("d", "c")];
    let order = compute_topological_order(&services, &deps).unwrap();
    assert_eq!(order[0], "a");
    assert_eq!(order[3], "d");
    assert!(order.contains(&"b".to_string()));
    assert!(order.contains(&"c".to_string()));
}

#[test]
fn topological_order_cycle_detected() {
    let services = vec![svc("a"), svc("b"), svc("c")];
    let deps = vec![dep("b", "a"), dep("a", "c"), dep("c", "b")];
    let err = compute_topological_order(&services, &deps).unwrap_err();
    assert!(matches!(
        err,
        IntegrationError::CompositionCycleDetected { .. }
    ));
}

#[test]
fn topological_order_self_cycle_detected() {
    let deps = vec![dep("a", "a")];
    let err = compute_topological_order(&[svc("a")], &deps).unwrap_err();
    assert!(matches!(
        err,
        IntegrationError::CompositionCycleDetected { .. }
    ));
}

#[test]
fn topological_order_extraneous_deps_ignored() {
    let services = vec![svc("a"), svc("b")];
    let deps = vec![dep("b", "a"), dep("b", "missing")];
    let order = compute_topological_order(&services, &deps).unwrap();
    assert_eq!(order, vec!["a", "b"]);
}

// ---------------------------------------------------------------------------
// default_aios_composition
// ---------------------------------------------------------------------------

#[test]
fn default_composition_has_17_services() {
    let comp = default_aios_composition();
    assert_eq!(comp.services.len(), 17);
}

#[test]
fn default_composition_is_acyclic() {
    let comp = default_aios_composition();
    assert_eq!(
        compute_topological_order(&comp.services, &comp.dependencies)
            .unwrap()
            .len(),
        17
    );
}

#[test]
fn default_composition_starts_with_action() {
    let comp = default_aios_composition();
    assert_eq!(comp.topological_order[0], "aios-action");
}

#[test]
fn default_composition_ends_with_hardware() {
    let comp = default_aios_composition();
    assert_eq!(comp.topological_order.last().unwrap(), "aios-hardware");
}

#[test]
fn default_composition_every_dependency_respected() {
    let comp = default_aios_composition();
    let order = &comp.topological_order;
    let pos: std::collections::HashMap<&str, usize> = order
        .iter()
        .enumerate()
        .map(|(i, s)| (s.as_str(), i))
        .collect();
    for dep in &comp.dependencies {
        let to_pos = pos[dep.to_service.as_str()];
        let from_pos = pos[dep.from_service.as_str()];
        assert!(
            to_pos < from_pos,
            "dependency {dep:?} violates topological order"
        );
    }
}

// ---------------------------------------------------------------------------
// Concurrent registration
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_register_three_distinct_no_panic() {
    use std::sync::Arc;
    let engine = Arc::new(CompositionEngine::new());
    let mut handles = Vec::new();
    for i in 0..3 {
        let e = Arc::clone(&engine);
        handles.push(tokio::spawn(async move {
            e.register(make_composition(
                &format!("concurrent-{i}"),
                vec![svc("a"), svc("b")],
                vec![dep("b", "a")],
            ))
            .await
        }));
    }
    for h in handles {
        h.await.unwrap().unwrap();
    }
    assert_eq!(engine.list().await.len(), 3);
}

// ---------------------------------------------------------------------------
// Integration-level: register default composition into engine
// ---------------------------------------------------------------------------

#[tokio::test]
async fn engine_registers_default_composition() {
    let engine = CompositionEngine::new();
    let comp = default_aios_composition();
    engine.register(comp).await.unwrap();
    let got = engine
        .get(&ComposedSystemId("aios-default".into()))
        .await
        .unwrap();
    assert_eq!(got.services.len(), 17);
    assert_eq!(got.topological_order.len(), 17);
}

// ---------------------------------------------------------------------------
// Overwrite: registering same ID twice is allowed (last write wins)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn register_same_id_twice_overwrites() {
    let engine = CompositionEngine::new();
    engine
        .register(make_composition(
            "overwrite-me",
            vec![svc("a"), svc("b")],
            vec![dep("b", "a")],
        ))
        .await
        .unwrap();
    engine
        .register(make_composition(
            "overwrite-me",
            vec![svc("x"), svc("y")],
            vec![dep("y", "x")],
        ))
        .await
        .unwrap();
    let got = engine
        .get(&ComposedSystemId("overwrite-me".into()))
        .await
        .unwrap();
    assert_eq!(got.services.len(), 2);
    assert_eq!(got.services[0].service_id, "x");
}
