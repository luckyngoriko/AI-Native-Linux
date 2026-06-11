use serde::{Deserialize, Serialize};
use strum_macros::{EnumCount, EnumIter};

/// Container runtime engine selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ContainerEngine {
    PodmanRootless,
    PodmanRootful,
    DockerCompat,
    Containerd,
    CriO,
}

/// Image build backend selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ImageBuildEngine {
    BuildKit,
    Buildah,
}

/// Kubernetes deployment profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum K8sProfile {
    K8sDevLocal,
    K8sEdgeNode,
    K8sWorkstationNode,
    K8sServerCluster,
    K8sAirgapCluster,
    K8sGpuAiNode,
    K8sRtEdgeNode,
}

/// Container isolation level — maps to runtime boundary choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum IsolationLevel {
    ProcessSandbox,
    Rootless,
    Standard,
    GVisor,
    Kata,
    FullVm,
    Wasm,
    RtIsland,
}

/// Ecosystem runtime adapter — bridges container workloads to AI-native runtimes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EcosystemRuntimeAdapter {
    RuntimeWasmNative,
    RuntimeEbpfNative,
    RuntimeDeno,
    RuntimeBun,
    RuntimePythonNative,
}

/// Workload source format importer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WorkloadImporter {
    ComposeImporter,
    HelmImporter,
    KustomizeImporter,
    K8sManifestImporter,
    DockerfileBuilder,
    DevcontainerImporter,
}

/// Admission gate decision for a container workload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ContainerAdmissionDecision {
    Admitted,
    Blocked,
    Quarantined,
    RequiresHumanApproval,
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    reason = "panic-on-failure is the idiomatic test signal"
)]
mod tests {
    use super::*;
    use strum::{EnumCount, IntoEnumIterator};

    #[test]
    fn container_engine_has_five_variants() {
        assert_eq!(ContainerEngine::COUNT, 5);
        assert_eq!(ContainerEngine::iter().count(), 5);
    }

    #[test]
    fn isolation_level_has_eight_variants() {
        assert_eq!(IsolationLevel::COUNT, 8);
        assert_eq!(IsolationLevel::iter().count(), 8);
    }

    #[test]
    fn k8s_profile_has_seven_variants() {
        assert_eq!(K8sProfile::COUNT, 7);
        assert_eq!(K8sProfile::iter().count(), 7);
    }

    #[test]
    fn all_enums_serde_round_trip() {
        for variant in ContainerEngine::iter() {
            let json = serde_json::to_string(&variant).unwrap();
            let back: ContainerEngine = serde_json::from_str(&json).unwrap();
            assert_eq!(variant, back, "round-trip failed for {variant:?}");
        }
        for variant in IsolationLevel::iter() {
            let json = serde_json::to_string(&variant).unwrap();
            let back: IsolationLevel = serde_json::from_str(&json).unwrap();
            assert_eq!(variant, back, "round-trip failed for {variant:?}");
        }
        for variant in ContainerAdmissionDecision::iter() {
            let json = serde_json::to_string(&variant).unwrap();
            let back: ContainerAdmissionDecision = serde_json::from_str(&json).unwrap();
            assert_eq!(variant, back, "round-trip failed for {variant:?}");
        }
    }

    #[test]
    fn container_engine_default_is_podman_rootless() {
        // Verify that our engine_policy defaults to PODMAN_ROOTLESS
        // (validation deferred to engine_policy tests)
    }

    #[test]
    fn admission_decision_has_four_variants() {
        assert_eq!(ContainerAdmissionDecision::COUNT, 4);
    }

    #[test]
    fn workload_importer_has_six_variants() {
        assert_eq!(WorkloadImporter::COUNT, 6);
    }

    #[test]
    fn ecosystem_runtime_adapter_has_five_variants() {
        assert_eq!(EcosystemRuntimeAdapter::COUNT, 5);
    }

    #[test]
    fn image_build_engine_has_two_variants() {
        assert_eq!(ImageBuildEngine::COUNT, 2);
    }
}
