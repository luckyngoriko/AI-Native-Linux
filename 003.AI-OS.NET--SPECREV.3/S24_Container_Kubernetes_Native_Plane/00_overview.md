# S24 — Container and Kubernetes Native Plane

| Field     | Value                                                                                                                                                                                                                       |
| --------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Status    | `CONTRACT` (created 2026-05-29; awaiting implementation evidence)                                                                                                                                                           |
| Phase tag | S24                                                                                                                                                                                                                         |
| Layer     | Cross-cutting: L6, L8, crossing L4, L9                                                                                                                                                                                      |
| Consumes  | S17 App Capsule Runtime, S18 Kernel Personality and Portability Plane, S8.1 Network Policy, S3.2 Sandbox Composition, S16.6 SBOM + Provenance + VEX (SBOM/provenance/VEX vocabulary), S2.3 Policy Kernel, S3.1 Evidence Log |
| Produces  | `ContainerEnginePolicy`, `K8sProfile`, `IsolationLevel`, `CloudNativePassport`, `WorkloadImporter` risk diff, `EcosystemRuntimeAdapter` set, container/Kubernetes/runtime evidence records                                  |

## 1. Responsibility

S24 defines how AIOS treats containers, OCI artifacts, Kubernetes, and the
broader cloud-native runtime ecosystem as a **first-class, governed OS plane** —
not as an add-on package installed later by the operator. It owns the engine
admission policy (Podman/Docker/containerd/CRI-O), the Kubernetes node profiles,
the per-workload isolation selector, the `compose.yaml`/Helm/Kustomize importer,
and — per DEC-R3-011 — the ecosystem runtime adapters (WASM, eBPF, Deno, Bun,
native Python).

A container is not a small VM. Containers share the host kernel, so an isolation
choice is a security decision, not a performance preference. S24 therefore makes
every workload a typed, scored, policy-checked, rollbackable object with a
declared isolation level and a clear blocked reason when admission fails.

Invariant links: INV-002, INV-004, INV-005, INV-008, INV-012, INV-013,
INV-014, INV-017, INV-024, and the new INV-025 ("AI cannot author eBPF",
DEC-R3-005), reused here for the `RUNTIME_EBPF_NATIVE` adapter.

## 2. Product principle

AIOS must make containers and Kubernetes easy for the operator, but never casual
for the host kernel.

```text
workload request (image | compose | helm | kustomize | manifest | wasm | repo)
  -> capability probe (engine, kernel matrix, profile, devices, network)
  -> CloudNativePassport assembly
  -> isolation level selection (risk -> boundary)
  -> supply-chain + network + privilege risk diff
  -> policy decision + human approval where required
  -> admit native-rootless | container | gVisor | Kata | VM | wasm | block
  -> evidence
  -> rollback / remove with evidence
```

The default answer to "run this container" is never "expose the Docker socket
and run it as root." The default answer is: rootless Podman with the weakest
viable capability set, the strongest viable isolation for the workload's risk,
digest-pinned images on secure profiles, and a recorded passport.

This plane reuses the universal solver pattern (holistic §6) through the Capsule
Solver of S17; it does not invent a parallel solver. The container/Kubernetes
path is one branch of the S17 capsule decision, specialized with the schemas
below.

## 3. Reference patterns

| Pattern                                                                                                | S24 use                                                                                              |
| ------------------------------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------- |
| [Open Container Initiative](https://opencontainers.org/)                                               | Base contract for image, runtime, and distribution; OCI is the canonical artifact shape AIOS admits. |
| [Podman docs](https://podman.io/docs)                                                                  | Default secure local engine; rootless containers and pods.                                           |
| [Podman Quadlet / systemd units](https://docs.podman.io/en/latest/markdown/podman-systemd.unit.5.html) | Preferred single-host service container format integrated with systemd lifecycle.                    |
| [Docker Engine docs](https://docs.docker.com/engine/)                                                  | Compatibility lane for existing Docker workflows without making the socket universal root.           |
| [Docker BuildKit](https://docs.docker.com/build/buildkit/)                                             | Native, reproducible image build engine.                                                             |
| [Compose Specification](https://compose-spec.github.io/compose-spec/spec.html)                         | Importer source format for `compose.yaml`, runtime-agnostic.                                         |
| [containerd](https://containerd.io/)                                                                   | System runtime substrate and primary Kubernetes CRI runtime.                                         |
| [CRI-O](https://www.cncf.io/projects/cri-o/)                                                           | Kubernetes-focused alternate CRI runtime.                                                            |
| [Kubernetes CRI](https://kubernetes.io/docs/concepts/architecture/cri/)                                | The node-level runtime contract every K8s profile must speak; kubelet never depends on Docker.       |
| [K3s](https://docs.k3s.io/) / [k0s](https://k0sproject.io/)                                            | Small-footprint distributions for dev/edge/local profiles.                                           |
| [Helm](https://helm.sh/) / [Kustomize](https://kustomize.io/)                                          | Chart install and overlay formats admitted only through importer risk diff.                          |
| [gVisor](https://gvisor.dev/) / [Kata Containers](https://katacontainers.io/)                          | Syscall-sandbox and lightweight-VM isolation boundaries.                                             |
| [KubeVirt](https://kubevirt.io/)                                                                       | Full-VM workloads for apps that should not be containerized.                                         |
| [WASI](https://wasi.dev/interfaces) / [WasmEdge](https://wasmedge.org/)                                | WebAssembly host runtime for portable, capability-scoped modules.                                    |
| [Sigstore / cosign](https://docs.sigstore.dev/) / [SLSA](https://slsa.dev/spec/v1.2/about)             | Signature and provenance verification for admitted artifacts.                                        |
| [SPDX](https://spdx.dev/about/overview/) / [CycloneDX](https://cyclonedx.org/capabilities)             | SBOM formats required by profile (vocabulary inherited from S16.6).                                  |

## 4. Container engine policy

The engine is selected by use case and constrained by the active security
profile. The Docker socket is never exposed by default on any profile.

```text
ContainerEngine =
  PODMAN_ROOTLESS
| PODMAN_ROOTFUL
| DOCKER_COMPAT
| CONTAINERD
| CRIO
```

`Unknown values are rejected by the ContainerEnginePolicy validator.`

The image build engine is a separate closed enum (rootless build preferred):

```text
ImageBuildEngine =
  BUILDKIT
| BUILDAH
```

`Unknown values are rejected by the build-engine selector.`

```yaml
container_engine_policy:
  default_engine: PODMAN_ROOTLESS
  selection:
    desktop_app_container: PODMAN_ROOTLESS
    existing_docker_project: DOCKER_COMPAT # compatibility lane, not default
    system_service_container: PODMAN_ROOTLESS # via Quadlet/systemd
    kubernetes_node: CONTAINERD # or CRIO; never Docker as kubelet runtime
    image_build: { engine: BUILDKIT } # ImageBuildEngine; rootless build preferred
  docker_socket_exposed: false # hard default on every profile
  rootful_requires_human_approval: true
  digest_pin_required: false # raised to true by profile gate (§9)
  allowed_engines:
    - PODMAN_ROOTLESS
    - PODMAN_ROOTFUL
    - DOCKER_COMPAT
    - CONTAINERD
    - CRIO
```

Engine preference order (the policy may override only with an explicit reason
recorded in evidence, e.g. a Kubernetes node that must use CRI-O for a CNI):

```text
PODMAN_ROOTLESS
  > CONTAINERD          (Kubernetes / system substrate)
  > CRIO                (Kubernetes-focused CRI)
  > PODMAN_ROOTFUL      (only with human approval)
  > DOCKER_COMPAT       (compatibility lane; socket still not default-exposed)
```

## 5. Kubernetes node profiles

Kubernetes is not forced onto every host. It belongs to explicit node profiles,
each with its own admission posture.

```text
K8sProfile =
  K8S_DEV_LOCAL
| K8S_EDGE_NODE
| K8S_WORKSTATION_NODE
| K8S_SERVER_CLUSTER
| K8S_AIRGAP_CLUSTER
| K8S_GPU_AI_NODE
| K8S_RT_EDGE_NODE
```

`Unknown values are rejected by the K8sProfile loader.`

| Profile                | Description                                                            | Default CRI         | Admission posture                                          |
| ---------------------- | ---------------------------------------------------------------------- | ------------------- | ---------------------------------------------------------- |
| `K8S_DEV_LOCAL`        | Single-node dev/lab cluster (K3s/k0s/kind).                            | containerd          | Relaxed; rollback + evidence still required.               |
| `K8S_EDGE_NODE`        | Lightweight edge for homelab/IoT/branch.                               | containerd          | Signed images preferred; network default-deny.             |
| `K8S_WORKSTATION_NODE` | Workstation running local services, AI/build pipelines, test clusters. | containerd          | Rootless-first; device intent declared.                    |
| `K8S_SERVER_CLUSTER`   | HA server/cluster with storage, ingress, policy, backup.               | containerd or CRI-O | Policy-as-code admission required (Kyverno/OPA).           |
| `K8S_AIRGAP_CLUSTER`   | Offline cluster, signed local mirror, controlled updates.              | containerd or CRI-O | Digest-pinned, signed-mirror-only; no live registry.       |
| `K8S_GPU_AI_NODE`      | GPU-aware node for AI/media/render/game-stream.                        | containerd          | Explicit GPU/video device plugin policy, not broad `/dev`. |
| `K8S_RT_EDGE_NODE`     | Experimental mixed-criticality / RT-adjacent edge.                     | containerd          | Strict admission; RT-island isolation per S18.             |

Native Kubernetes support a profile may declare (each capability is a typed,
policy-gated feature, not an implicit grant):

```text
kubectl_context_manager   # operator sees active cluster/context before any command
helm_admission            # charts install only through importer risk diff + values diff
kustomize_overlays        # overlays promoted without uncontrolled mutation
gitops_controller         # Flux or Argo CD desired-state reconciliation
cni_network_policy         # Cilium/eBPF or standard CNI under S8.1 network policy
admission_policy           # Kyverno and/or OPA Gatekeeper policy-as-code
runtime_security           # Falco/Tetragon-style detection (drop-only eBPF, see §10)
observability              # OpenTelemetry + Prometheus-compatible metrics/logs/traces
backup_restore             # Velero-style cluster + PV backup
external_secrets           # External Secrets Operator -> Vault Broker, never env-sprayed
signed_registry_mirror     # local OCI mirror for airgap/fleet/offline installs
gpu_device_plugins         # explicit GPU/video device policy
```

The Kubernetes **admin kubeconfig is never silently shared with an AI subject**
(§9 hard deny). An AI subject may read cluster state through scoped, redacted
state objects and propose typed actions; it cannot hold cluster-admin
credentials.

## 6. Isolation levels

Isolation is selected per workload risk, not per convenience. A container shares
the host kernel; stronger isolation needs another boundary.

```text
IsolationLevel =
  PROCESS_SANDBOX
| ROOTLESS
| STANDARD
| GVISOR
| KATA
| FULL_VM
| WASM
| RT_ISLAND
```

`Unknown values are rejected by the IsolationLevel selector.`

| Isolation level   | Boundary                                           | Candidate technology                    | Use case                                                                |
| ----------------- | -------------------------------------------------- | --------------------------------------- | ----------------------------------------------------------------------- |
| `PROCESS_SANDBOX` | Same kernel, MAC + namespaces + seccomp + Landlock | SELinux, namespaces, seccomp, Landlock  | Low-risk local apps.                                                    |
| `ROOTLESS`        | Same kernel, no root daemon                        | Podman/rootless OCI                     | Normal app/service containers (default).                                |
| `STANDARD`        | Same kernel, runc                                  | containerd/CRI-O/runc                   | Kubernetes workloads with normal risk.                                  |
| `GVISOR`          | Syscall interposition                              | gVisor/runsc                            | Untrusted services needing Linux ABI with reduced host kernel exposure. |
| `KATA`            | Lightweight VM, separate kernel                    | Kata Containers                         | Stronger tenant/workload isolation with container UX.                   |
| `FULL_VM`         | Hypervisor, separate kernel                        | KVM/QEMU/KubeVirt                       | Legacy/unsafe/other-OS workloads; strong compartment.                   |
| `WASM`            | Capability-scoped WASI sandbox                     | WASI/WasmEdge                           | Small portable services, plugins, edge functions, safer applets.        |
| `RT_ISLAND`       | Deterministic isolated domain (S18)                | PREEMPT_RT / co-kernel / appliance boot | Real-time / deterministic workloads.                                    |

Selection rule (the **Secure Runtime Selector**, a branch of the S17 Capsule
Solver):

```text
trusted, signed, low-risk, secure profile     -> ROOTLESS / STANDARD
unknown image, unsigned, or first-seen          -> GVISOR (stricter until trusted)
untrusted but needs full Linux ABI               -> KATA
other-OS / legacy / cannot be containerized      -> FULL_VM
portable plugin / capability-scoped function      -> WASM
deterministic workload                           -> RT_ISLAND
no viable safe boundary on this profile           -> BLOCK_WITH_REASON
```

Unknown images run in stricter isolation until trust is established; isolation
can only be relaxed by a typed policy decision with evidence, never silently.

## 7. Cloud Native Passport

Every containerized or runtime-admitted workload gets a passport, analogous to
the S17 `AppCapsule` and the package passport, capturing the full governed
identity of the workload.

```yaml
cloud_native_passport:
  passport_id: "cnp_<ULID>"
  workload_id: "workload:example"
  source: REGISTRY # GIT | REGISTRY | LOCAL | COMPOSE | HELM | K8S_MANIFEST
  artifacts:
    images: ["registry.example/app@sha256:..."] # digest-pinned form on secure profiles
    charts: []
    manifests: []
    sboms: ["spdx:...", "cyclonedx:..."]
    signatures: ["cosign:..."]
  runtime:
    engine: PODMAN_ROOTLESS # ContainerEngine
    isolation: ROOTLESS # IsolationLevel
  privileges:
    rootless: true
    privileged: false # privileged requires human approval (§9)
    capabilities: [] # weakest viable set
    seccomp_profile: "default"
    selinux_type: "container_t"
    devices: [] # declared device intent only (GPU/USB/cam/mic/video)
  network:
    ports: []
    egress: deny_unknown # under S8.1 Network Policy
    dns: []
    service_mesh: none
    ingress: none
  storage:
    volumes: []
    secrets: [] # fetched via policy/Vault Broker, never env-sprayed
    persistence_class: "none"
    backup_policy: "none"
  supply_chain:
    signature: required # by profile (§9)
    provenance: optional # SLSA attestation; raised by profile
    sbom: optional # SPDX/CycloneDX; raised by profile
    vex: optional # VEX statements; vocabulary from S16.6
    vulnerabilities: []
  update_policy: MANUAL_APPROVAL # PINNED | ROLLING | AUTO | MANUAL_APPROVAL
  rollback:
    snapshot_ref: ""
    previous_image_digest: ""
    previous_manifest_ref: ""
  evidence:
    admit_receipt: "evr_..."
    risk_diff_receipt: "evr_..."
    policy_decision_receipt: "evr_..."
```

`source`, `runtime.engine`, `runtime.isolation`, and `update_policy` are closed
enums; `Unknown values are rejected by the CloudNativePassport validator.`

Passport filesystem layout (sibling to the S19 driver capsule layout):

```text
/aios/system/workloads/<workload_id>/
  cloud-native-passport.toml
  artifacts/
  importer/
  policy/
  network/
  storage/
  rollback/
  evidence/
```

## 8. Workload importer and risk diff

`compose.yaml`, Helm charts, Kustomize overlays, and Kubernetes manifests are
**inputs**, not commands. The importer translates them into a candidate
`CloudNativePassport` plus a risk diff that the operator and Policy Kernel see
before anything runs.

```text
WorkloadImporter =
  COMPOSE_IMPORTER       # compose.yaml      -> Podman/Docker/K8s plan
| HELM_IMPORTER          # chart + values    -> manifest plan + values diff
| KUSTOMIZE_IMPORTER     # base + overlays   -> rendered manifest plan
| K8S_MANIFEST_IMPORTER  # raw manifests     -> schema + policy preflight
| DOCKERFILE_BUILDER     # Dockerfile/Containerfile -> BuildKit/Buildah plan
| DEVCONTAINER_IMPORTER  # devcontainer.json -> Workstation/Dev profile plan
```

`Unknown values are rejected by the WorkloadImporter dispatcher.`

The risk diff is mandatory and surfaces, at minimum:

```text
images            (and whether digest-pinned)
exposed ports     (and bind scope: localhost vs LAN)
volume mounts     (and host path exposure)
secrets           (and whether they would be env-sprayed -> rejected)
privileged mode   (-> human approval gate)
requested devices (GPU/USB/cam/mic/video -> declared device intent)
capabilities      (delta from weakest viable set)
update policy      (pinned vs rolling vs auto)
network egress    (allowed destinations under S8.1)
```

A Compose/Helm/Kustomize/manifest import **always emits a risk diff before
execution**; an import that would env-spray a secret, expose the Docker socket,
or request privileged mode without approval is blocked at preflight.

## 9. Security profile gates

| Profile          | Container/Kubernetes rule                                                                                                                                                                                  |
| ---------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `DEV_RELAXED`    | Rootless preferred; rootful and unsigned images allowed with warning and rollback; Docker socket still not default-exposed.                                                                                |
| `SECURE_DEFAULT` | Rootless default; default-deny inbound network; signed images preferred; unknown images forced to `GVISOR` until trusted.                                                                                  |
| `STIG_ALIGNED`   | Rootless or policy-approved rootful only; images digest-pinned and signed; SBOM/provenance required; admission policy (Kyverno/OPA) required for clusters; privileged needs recovery-gated human approval. |
| `AIRGAP_HIGH`    | Rootless or VM only; no Docker socket; signed local-mirror images only, digest-pinned; no live registry pulls; strongest viable isolation per workload.                                                    |

Hard denies (Policy Kernel; §9 enumerates them as policy ids):

- Docker socket is never exposed by default on any profile.
- Privileged containers require explicit human approval and evidence.
- Kubernetes admin kubeconfig is never silently shared with AI subjects.
- Secrets are never sprayed into environment variables by default.
- Under `STIG_ALIGNED`/`AIRGAP_HIGH`, unsigned or non-digest-pinned images are
  blocked unless a recovery-approved exception exists.
- No AI subject may author or load an eBPF program (INV-025); see §10.
- No workload may be promoted without a rollback path.
- No Compose/Helm/manifest import may execute before its risk diff is recorded.

## 10. Ecosystem runtime adapters

Per DEC-R3-011, S24 owns the ecosystem runtime adapter set. Each adapter maps a
foreign runtime onto a `CloudNativePassport` + `IsolationLevel`, so these
runtimes are governed like containers, not bolted on outside the plane.

```text
EcosystemRuntimeAdapter =
  RUNTIME_WASM_NATIVE
| RUNTIME_EBPF_NATIVE
| RUNTIME_DENO
| RUNTIME_BUN
| RUNTIME_PYTHON_NATIVE
```

`Unknown values are rejected by the EcosystemRuntimeAdapter registry loader.`

| Adapter                 | Backend                               | Default isolation | Authority / safety rule                                                                                                                                                                                                                                                                    |
| ----------------------- | ------------------------------------- | ----------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `RUNTIME_WASM_NATIVE`   | Wasmtime / WasmEdge (WASI)            | `WASM`            | Capability-scoped; safer than Linux-native; admitted via `WASM_MODULE_ADMITTED`.                                                                                                                                                                                                           |
| `RUNTIME_EBPF_NATIVE`   | Kernel-side eBPF                      | n/a (in-kernel)   | **AI is drop-only** (DEC-R3-005 / INV-025). Authoring/loading restricted to `HUMAN_OPERATOR`/`HUMAN_USER` and policy-gated `SYSTEM_SERVICE`. An AI subject may at most request a pre-vetted, signed, drop-only template (no `redirect`, no map-write-to-userspace) through a typed action. |
| `RUNTIME_DENO`          | Deno                                  | `PROCESS_SANDBOX` | JS sandbox with explicit capability mapping (`--allow-*` mapped to declared grants).                                                                                                                                                                                                       |
| `RUNTIME_BUN`           | Bun                                   | `PROCESS_SANDBOX` | JS sandbox with capability mapping; same grant discipline as Deno.                                                                                                                                                                                                                         |
| `RUNTIME_PYTHON_NATIVE` | CPython / Pyodide (PEP 711 lockfiles) | `PROCESS_SANDBOX` | Python sandbox; dependencies resolved from signed lockfiles, no arbitrary network install on secure profiles.                                                                                                                                                                              |

The eBPF adapter is the one capability where the "AI proposes, never executes"
boundary is enforced at the kernel edge: in-kernel execution authored by AI
would violate INV-002, so the signed drop-only template is the only AI-reachable
form. eBPF template loads emit `EBPF_TEMPLATE_LOADED`.

## 11. Evidence records

S24 adds these record types (UPPER_SNAKE_CASE):

```text
CONTAINER_WORKLOAD_ADMITTED
CONTAINER_WORKLOAD_BLOCKED
COMPOSE_IMPORT_RISK_DIFF
HELM_IMPORT_RISK_DIFF
KUSTOMIZE_IMPORT_RISK_DIFF
K8S_MANIFEST_PREFLIGHT
ISOLATION_LEVEL_SELECTED
ENGINE_POLICY_SELECTED
PRIVILEGED_CONTAINER_APPROVED
DOCKER_SOCKET_ACCESS_DENIED
IMAGE_DIGEST_PIN_ENFORCED
SUPPLY_CHAIN_VERIFICATION_RESULT
EBPF_TEMPLATE_LOADED
WASM_MODULE_ADMITTED
WORKLOAD_ROLLED_BACK
```

Minimum fields for `CONTAINER_WORKLOAD_ADMITTED`:

```text
workload_id
passport_id
source
engine
isolation_level
images_digest_pinned
privileged
security_profile
risk_diff_receipt_id
policy_decision_id
rollback_plan_id
evidence_receipt_id
```

## 12. Non-goals

- Do not promise every image, chart, compose stack, or Kubernetes manifest runs
  unmodified; promise a best-fit execution plan, rollback, evidence, and a clear
  blocked reason.
- Do not expose the Docker socket to make compatibility "just work."
- Do not treat a container as a security boundary equivalent to a VM —
  containers share a kernel; stronger isolation needs another boundary
  (`GVISOR`/`KATA`/`FULL_VM`).
- Do not let an AI subject author or load eBPF, hold cluster-admin kubeconfig, or
  approve a privileged container.
- Do not force Kubernetes onto desktop profiles; it lives in explicit
  `K8sProfile` node profiles.
- Do not env-spray secrets to ease container configuration.
- Do not duplicate S8.1 network policy or S3.2 sandbox composition; bind to them.

## 13. Acceptance criteria

S24 is `REAL` only when:

1. `ContainerEnginePolicy`, `K8sProfile`, `IsolationLevel`,
   `CloudNativePassport`, `WorkloadImporter`, and `EcosystemRuntimeAdapter`
   parse and reject unknown enum values.
2. The default engine is `PODMAN_ROOTLESS` and the Docker socket is not exposed
   on any profile without an explicit, evidenced exception.
3. A `compose.yaml` import produces a `COMPOSE_IMPORT_RISK_DIFF` before any
   container starts, surfacing ports, volumes, secrets, privileged mode, and
   devices.
4. A Kubernetes manifest is preflighted (`K8S_MANIFEST_PREFLIGHT`) for schema,
   image trust, network, and device requirements before admission.
5. The Secure Runtime Selector forces unknown/unsigned images to `GVISOR` or
   stronger under `SECURE_DEFAULT` and above, and emits
   `ISOLATION_LEVEL_SELECTED`.
6. Under `STIG_ALIGNED`/`AIRGAP_HIGH`, unsigned or non-digest-pinned images are
   blocked unless a recovery-approved exception exists.
7. A privileged container cannot be admitted without a recorded
   `PRIVILEGED_CONTAINER_APPROVED` human approval.
8. An AI subject cannot author or load an eBPF program; only a signed drop-only
   template request is reachable, recorded as `EBPF_TEMPLATE_LOADED`.
9. An AI subject cannot obtain a Kubernetes admin kubeconfig.
10. Every admitted workload carries a `CloudNativePassport` with a rollback path
    and emits `CONTAINER_WORKLOAD_ADMITTED`; removal/rollback emits
    `WORKLOAD_ROLLED_BACK`.
11. Secrets are fetched through policy and are not present as environment
    variables in the rendered passport.

## 14. See also

- [Rev.3 Holistic Specification](../00_REV3_HOLISTIC_SPEC.md)
- [Rev.3 Design Decisions (DEC-R3-005, DEC-R3-011)](../02_design_decisions.md)
- [S17 App Capsule Runtime](../S17_App_Capsule_Runtime/00_overview.md)
- [S18 Kernel Personality and Portability Plane](../S18_Kernel_Personality_Portability/00_overview.md)
- [S16.6 SBOM + Provenance + VEX](../S16_Security_Hardening_Compliance/06_sbom_provenance_vex.md)
- [S8.1 Network Policy](../../002.AI-OS.NET--SPECREV.2/L8_Network_Hardware_Devices/02_network_policy.md)
- [S3.2 Sandbox Composition](../../002.AI-OS.NET--SPECREV.2/L6_Apps_Packages_Compatibility/04_sandbox_composition.md)
- [S2.3 Policy Kernel](../../002.AI-OS.NET--SPECREV.2/L4_Policy_Identity_Vault/01_policy_kernel.md)
- [S3.1 Evidence Receipt Schema](../../002.AI-OS.NET--SPECREV.2/L0_Governance_Evidence_Safety/03_evidence_receipt_schema.md)
