#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    clippy::unwrap_in_result,
    clippy::doc_markdown,
    clippy::significant_drop_tightening,
    clippy::wildcard_imports,
    clippy::similar_names,
    clippy::cast_possible_wrap,
    clippy::too_many_lines,
    missing_docs,
    reason = "test code; panic-on-failure is the idiomatic test signal"
)]

use std::sync::Arc;

use aios_hardware::*;

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

fn dummy_gpu(gpu_id: &str, vram_mb: u64) -> GpuDevice {
    GpuDevice {
        gpu_id: GpuId(gpu_id.into()),
        vendor: GpuVendorKind::Nvidia,
        product_name: format!("TestGPU-{gpu_id}"),
        vram_total_bytes: vram_mb * 1024 * 1024,
        supported_classes: vec![
            GpuCapabilityClass::RenderOnly,
            GpuCapabilityClass::ComputeOnly,
            GpuCapabilityClass::RenderAndCompute,
        ],
        iommu_protected: true,
        host_canonical_id: format!("pci:0000:{gpu_id}"),
    }
}

fn demo_binding_request(gpu_id: &str, vram_mb: u64) -> BindingRequest {
    BindingRequest {
        gpu_id: GpuId(gpu_id.into()),
        group_id: "group-alpha".into(),
        subject_canonical_id: format!("subject-{gpu_id}"),
        capability_class: GpuCapabilityClass::ComputeOnly,
        vram_bytes: vram_mb * 1024 * 1024,
        ttl: None,
    }
}

// ---------------------------------------------------------------------------
// Device registration
// ---------------------------------------------------------------------------

#[tokio::test]
async fn register_device_then_list_returns_1() {
    let reg = GpuResourceRegistry::new();
    reg.register_device(dummy_gpu("00", 1024))
        .await
        .expect("register");
    let list = reg.list_devices().await;
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].gpu_id, GpuId("00".into()));
}

#[tokio::test]
async fn register_duplicate_gpu_id_returns_internal_error() {
    let reg = GpuResourceRegistry::new();
    reg.register_device(dummy_gpu("00", 1024))
        .await
        .expect("first register");
    let err = reg
        .register_device(dummy_gpu("00", 2048))
        .await
        .expect_err("duplicate must fail");
    assert_eq!(err.code(), HardwareErrorCode::Internal);
}

// ---------------------------------------------------------------------------
// VkDevicePartition – standalone unit
// ---------------------------------------------------------------------------

#[tokio::test]
async fn vkdevice_partition_authorize_then_is_authorized_returns_true() {
    let mut p = VkDevicePartition::new(GpuId("gpu0".into()), "group-a".into());
    assert!(!p.is_authorized("alice"));
    p.authorize_subject("alice".into()).expect("authorize");
    assert!(p.is_authorized("alice"));
}

#[tokio::test]
async fn vkdevice_partition_revoke_unknown_subject_returns_ok() {
    let mut p = VkDevicePartition::new(GpuId("gpu0".into()), "group-a".into());
    // Idempotent — revoking a non-existent subject should succeed (Ok)
    let result = p.revoke_subject("nobody");
    assert!(result.is_ok());
}

#[tokio::test]
async fn vkdevice_partition_authorize_duplicate_is_idempotent() {
    let mut p = VkDevicePartition::new(GpuId("gpu0".into()), "group-a".into());
    p.authorize_subject("alice".into()).expect("first");
    p.authorize_subject("alice".into())
        .expect("second (idempotent)");
    assert_eq!(p.authorized_subjects.len(), 1);
}

// ---------------------------------------------------------------------------
// Partition lifecycle via registry
// ---------------------------------------------------------------------------

#[tokio::test]
async fn ensure_partition_creates_new_for_first_group() {
    let reg = GpuResourceRegistry::new();
    reg.register_device(dummy_gpu("00", 1024))
        .await
        .expect("register");
    let p = reg
        .ensure_partition(&GpuId("00".into()), "group-alpha")
        .await
        .expect("ensure");
    assert_eq!(p.gpu_id, GpuId("00".into()));
    assert_eq!(p.group_id, "group-alpha");
    assert!(!p.partition_id.is_empty());
    assert!(p.authorized_subjects.is_empty());
}

#[tokio::test]
async fn ensure_partition_returns_existing_for_same_group() {
    let reg = GpuResourceRegistry::new();
    reg.register_device(dummy_gpu("00", 1024))
        .await
        .expect("register");
    let p1 = reg
        .ensure_partition(&GpuId("00".into()), "group-alpha")
        .await
        .expect("first");
    let p2 = reg
        .ensure_partition(&GpuId("00".into()), "group-alpha")
        .await
        .expect("second");
    assert_eq!(p1.partition_id, p2.partition_id);
}

#[tokio::test]
async fn list_partitions_for_group_returns_only_that_groups() {
    let reg = GpuResourceRegistry::new();
    reg.register_device(dummy_gpu("00", 1024))
        .await
        .expect("register");
    reg.register_device(dummy_gpu("01", 2048))
        .await
        .expect("register");
    reg.ensure_partition(&GpuId("00".into()), "group-alpha")
        .await
        .expect("p1");
    reg.ensure_partition(&GpuId("01".into()), "group-beta")
        .await
        .expect("p2");
    reg.ensure_partition(&GpuId("01".into()), "group-alpha")
        .await
        .expect("p3");

    let alpha = reg.list_partitions_for_group("group-alpha").await;
    assert_eq!(alpha.len(), 2);
    let beta = reg.list_partitions_for_group("group-beta").await;
    assert_eq!(beta.len(), 1);
    let empty = reg.list_partitions_for_group("group-nonexistent").await;
    assert!(empty.is_empty());
}

// ---------------------------------------------------------------------------
// Binding — capability class gating
// ---------------------------------------------------------------------------

#[tokio::test]
async fn request_binding_for_supported_capability_class_succeeds() {
    let reg = GpuResourceRegistry::new();
    reg.register_device(dummy_gpu("00", 1024))
        .await
        .expect("register");
    let req = demo_binding_request("00", 256);
    let binding = reg.request_binding(req).await.expect("binding");
    assert_eq!(binding.capability_class, GpuCapabilityClass::ComputeOnly);
    assert!(!binding.binding_id.is_empty());
}

#[tokio::test]
async fn request_binding_for_unsupported_capability_class_returns_gpu_binding_invalid() {
    let reg = GpuResourceRegistry::new();
    let mut gpu = dummy_gpu("00", 1024);
    gpu.supported_classes = vec![GpuCapabilityClass::RenderOnly];
    reg.register_device(gpu).await.expect("register");

    let req = BindingRequest {
        gpu_id: GpuId("00".into()),
        group_id: "g".into(),
        subject_canonical_id: "s".into(),
        capability_class: GpuCapabilityClass::VideoEncode, // not in supported set
        vram_bytes: 64 * 1024 * 1024,
        ttl: None,
    };
    let err = reg.request_binding(req).await.expect_err("must fail");
    assert_eq!(err.code(), HardwareErrorCode::GpuBindingInvalid);
}

#[tokio::test]
async fn request_binding_for_unknown_gpu_returns_gpu_binding_invalid() {
    let reg = GpuResourceRegistry::new();
    let req = demo_binding_request("99", 256);
    let err = reg.request_binding(req).await.expect_err("must fail");
    assert_eq!(err.code(), HardwareErrorCode::GpuBindingInvalid);
}

// ---------------------------------------------------------------------------
// Binding — VRAM budget enforcement
// ---------------------------------------------------------------------------

#[tokio::test]
async fn request_binding_within_vram_budget_succeeds() {
    let reg = GpuResourceRegistry::new();
    // GPU with 1 GiB VRAM, request 256 MiB — must succeed
    reg.register_device(dummy_gpu("00", 1024))
        .await
        .expect("register");
    let req = demo_binding_request("00", 256);
    let b = reg.request_binding(req).await.expect("must succeed");
    assert_eq!(b.vram_bytes_reserved, 256 * 1024 * 1024);
}

#[tokio::test]
async fn request_binding_exceeding_vram_returns_gpu_vram_exhausted_with_correct_available() {
    let reg = GpuResourceRegistry::new();
    // GPU with 256 MiB, request 512 MiB — must fail with available ≈ 256 MiB
    reg.register_device(dummy_gpu("00", 256))
        .await
        .expect("register");
    let req = demo_binding_request("00", 512);
    let err = reg.request_binding(req).await.expect_err("must exhaust");
    assert_eq!(err.code(), HardwareErrorCode::GpuVramExhausted);
    if let HardwareError::GpuVramExhausted {
        gpu: _,
        requested,
        available,
    } = &err
    {
        assert_eq!(*requested, 512 * 1024 * 1024);
        assert_eq!(*available, 256 * 1024 * 1024); // all 256 MiB still free
    } else {
        panic!("wrong variant");
    }
}

#[tokio::test]
async fn request_binding_partial_vram_consumption_reduces_available() {
    let reg = GpuResourceRegistry::new();
    reg.register_device(dummy_gpu("00", 1024))
        .await
        .expect("register");
    // Consume 768 MiB, then request another 512 MiB — only 256 MiB left → fail
    reg.request_binding(demo_binding_request("00", 768))
        .await
        .expect("first");
    let req2 = demo_binding_request("00", 512);
    let err = reg.request_binding(req2).await.expect_err("exhaust");
    if let HardwareError::GpuVramExhausted { available, .. } = &err {
        assert_eq!(*available, 256 * 1024 * 1024);
    } else {
        panic!("wrong variant");
    }
}

// ---------------------------------------------------------------------------
// Binding — partition-authorization side effects
// ---------------------------------------------------------------------------

#[tokio::test]
async fn request_binding_authorizes_subject_in_partition() {
    let reg = GpuResourceRegistry::new();
    reg.register_device(dummy_gpu("00", 1024))
        .await
        .expect("register");
    let req = demo_binding_request("00", 128);
    let _b = reg.request_binding(req).await.expect("binding");

    let partitions = reg.list_partitions_for_group("group-alpha").await;
    assert_eq!(partitions.len(), 1);
    assert!(partitions[0].is_authorized("subject-00"));
}

#[tokio::test]
async fn request_2_bindings_for_same_subject_on_same_partition_succeeds() {
    let reg = GpuResourceRegistry::new();
    reg.register_device(dummy_gpu("00", 1024))
        .await
        .expect("register");

    let req1 = BindingRequest {
        gpu_id: GpuId("00".into()),
        group_id: "group-alpha".into(),
        subject_canonical_id: "alice".into(),
        capability_class: GpuCapabilityClass::ComputeOnly,
        vram_bytes: 128 * 1024 * 1024,
        ttl: None,
    };
    let b1 = reg.request_binding(req1).await.expect("binding 1");

    let req2 = BindingRequest {
        gpu_id: GpuId("00".into()),
        group_id: "group-alpha".into(),
        subject_canonical_id: "alice".into(),
        capability_class: GpuCapabilityClass::RenderOnly,
        vram_bytes: 128 * 1024 * 1024,
        ttl: None,
    };
    let b2 = reg.request_binding(req2).await.expect("binding 2");

    assert_ne!(b1.binding_id, b2.binding_id);
    // Both share same partition
    assert_eq!(b1.vk_device_partition_id, b2.vk_device_partition_id);
}

// ---------------------------------------------------------------------------
// Binding — release
// ---------------------------------------------------------------------------

#[tokio::test]
async fn release_binding_known_id_succeeds() {
    let reg = GpuResourceRegistry::new();
    reg.register_device(dummy_gpu("00", 1024))
        .await
        .expect("register");
    let b = reg
        .request_binding(demo_binding_request("00", 256))
        .await
        .expect("bind");
    reg.release_binding(&b.binding_id).await.expect("release");
}

#[tokio::test]
async fn release_binding_decrements_bytes_reserved() {
    let reg = GpuResourceRegistry::new();
    reg.register_device(dummy_gpu("00", 1024))
        .await
        .expect("register");

    // Before release: total_vram_used = 256 MiB
    let b = reg
        .request_binding(demo_binding_request("00", 256))
        .await
        .expect("bind");
    assert_eq!(
        reg.total_vram_used(&GpuId("00".into())).await,
        256 * 1024 * 1024
    );

    reg.release_binding(&b.binding_id).await.expect("release");
    assert_eq!(reg.total_vram_used(&GpuId("00".into())).await, 0);
}

#[tokio::test]
async fn release_binding_unknown_id_returns_gpu_binding_invalid() {
    let reg = GpuResourceRegistry::new();
    let err = reg
        .release_binding("nonexistent-binding-id")
        .await
        .expect_err("unknown");
    assert_eq!(err.code(), HardwareErrorCode::GpuBindingInvalid);
}

#[tokio::test]
async fn release_binding_revokes_subject_when_no_other_bindings_remain() {
    let reg = GpuResourceRegistry::new();
    reg.register_device(dummy_gpu("00", 1024))
        .await
        .expect("register");

    let b = reg
        .request_binding(demo_binding_request("00", 128))
        .await
        .expect("bind");

    // Subject is authorized before release
    let parts = reg.list_partitions_for_group("group-alpha").await;
    assert!(parts[0].is_authorized("subject-00"));

    reg.release_binding(&b.binding_id).await.expect("release");

    // Subject should be revoked since no other bindings remain
    let parts = reg.list_partitions_for_group("group-alpha").await;
    assert!(!parts[0].is_authorized("subject-00"));
}

// ---------------------------------------------------------------------------
// Accounting queries
// ---------------------------------------------------------------------------

#[tokio::test]
async fn get_accounting_returns_per_subject_entries() {
    let reg = GpuResourceRegistry::new();
    reg.register_device(dummy_gpu("00", 1024))
        .await
        .expect("register");

    // Two subjects in the same group
    let req1 = BindingRequest {
        gpu_id: GpuId("00".into()),
        group_id: "group-alpha".into(),
        subject_canonical_id: "alice".into(),
        capability_class: GpuCapabilityClass::ComputeOnly,
        vram_bytes: 64 * 1024 * 1024,
        ttl: None,
    };
    let req2 = BindingRequest {
        gpu_id: GpuId("00".into()),
        group_id: "group-alpha".into(),
        subject_canonical_id: "bob".into(),
        capability_class: GpuCapabilityClass::RenderOnly,
        vram_bytes: 32 * 1024 * 1024,
        ttl: None,
    };
    reg.request_binding(req1).await.expect("alice");
    reg.request_binding(req2).await.expect("bob");

    let entries = reg.get_accounting(&GpuId("00".into()), "group-alpha").await;
    assert_eq!(entries.len(), 2);

    let alice_entry = entries
        .iter()
        .find(|e| e.subject_canonical_id == "alice")
        .expect("alice entry");
    assert_eq!(alice_entry.bytes_reserved, 64 * 1024 * 1024);
    assert_eq!(alice_entry.bytes_used, 0);
}

#[tokio::test]
async fn total_vram_used_sums_across_all_groups_and_subjects() {
    let reg = GpuResourceRegistry::new();
    reg.register_device(dummy_gpu("00", 4096))
        .await
        .expect("register");

    // Group alpha, alice 1 GiB
    reg.request_binding(BindingRequest {
        gpu_id: GpuId("00".into()),
        group_id: "alpha".into(),
        subject_canonical_id: "alice".into(),
        capability_class: GpuCapabilityClass::ComputeOnly,
        vram_bytes: 1024 * 1024 * 1024,
        ttl: None,
    })
    .await
    .expect("alice");

    // Group beta, bob 512 MiB
    reg.request_binding(BindingRequest {
        gpu_id: GpuId("00".into()),
        group_id: "beta".into(),
        subject_canonical_id: "bob".into(),
        capability_class: GpuCapabilityClass::RenderOnly,
        vram_bytes: 512 * 1024 * 1024,
        ttl: None,
    })
    .await
    .expect("bob");

    let total = reg.total_vram_used(&GpuId("00".into())).await;
    assert_eq!(total, (1024 + 512) * 1024 * 1024);
}

#[tokio::test]
async fn get_accounting_returns_empty_for_unknown_group() {
    let reg = GpuResourceRegistry::new();
    reg.register_device(dummy_gpu("00", 1024))
        .await
        .expect("register");
    let entries = reg
        .get_accounting(&GpuId("00".into()), "no-such-group")
        .await;
    assert!(entries.is_empty());
}

// ---------------------------------------------------------------------------
// Concurrency
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn concurrent_request_5_bindings_no_panic_and_vram_accounting_correct() {
    let reg = Arc::new(GpuResourceRegistry::new());
    let total_mb = 5120u64; // 5 GiB
    let per_binding_mb = 1024u64; // 1 GiB each

    reg.register_device(dummy_gpu("00", total_mb))
        .await
        .expect("register");

    let mut handles = Vec::new();
    for i in 0..5 {
        let reg = Arc::clone(&reg);
        handles.push(tokio::spawn(async move {
            let req = BindingRequest {
                gpu_id: GpuId("00".into()),
                group_id: "group-alpha".into(),
                subject_canonical_id: format!("subject-{i}"),
                capability_class: GpuCapabilityClass::ComputeOnly,
                vram_bytes: per_binding_mb * 1024 * 1024,
                ttl: None,
            };
            reg.request_binding(req).await
        }));
    }

    for h in handles {
        let result = h.await.expect("join");
        assert!(result.is_ok(), "binding must succeed: {result:?}");
    }

    let total = reg.total_vram_used(&GpuId("00".into())).await;
    assert_eq!(total, 5 * per_binding_mb * 1024 * 1024);
}
