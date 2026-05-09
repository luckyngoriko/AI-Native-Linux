# Sandbox Composition Language (Rev.2)

| Field          | Value                                                                                                              |
| -------------- | ------------------------------------------------------------------------------------------------------------------ |
| Status         | `CONTRACT` (refined 2026-05-08)                                                                                    |
| Phase tag      | S3.2                                                                                                               |
| Layer          | L6 Apps, Packages, Compatibility                                                                                   |
| Schema package | `aios.sandbox.v1alpha1`                                                                                            |
| Consumes       | S2.3 policy decisions, S0.1 action envelopes, S1.3 object metadata, L3 adapter manifests, L6 application manifests |
| Produces       | typed `SandboxProfile`, applied profile evidence, enforcement bindings                                             |

## 1. Purpose

AIOS executes typed actions through adapters. Each action runs inside a sandbox. The Sandbox Composition Language defines:

1. The typed surface of a sandbox profile (filesystem, network, process, resources, secrets, evidence).
2. The deterministic algorithm that merges five composition sources into a single profile.
3. The compilation path from a composed profile to Linux enforcement backends (namespaces, cgroups, seccomp, Landlock, SELinux/AppArmor, portals, containers, VM fallback).
4. The compatibility wrappers for foreign code (Wine/Proton, Waydroid, VM fallback).

The Sandbox Composer is the only component allowed to translate composed profiles into kernel-level enforcement. Adapters never call `seccomp_load`, `setns`, `cgroup_attach`, or `landlock_create_ruleset` directly.

## 2. Core invariants

The composed sandbox is the **most restrictive** combination of:

```text
1. adapter default      (action handler's baseline; least authoritative)
2. application manifest (what the app needs)
3. user/request hint    (what the caller asked for; weakest source)
4. policy required      (S2.3 constraints; strongest above runtime floor)
5. runtime safety floor (constitutional minimum; cannot be weakened)
```

The order is the **fall-through priority**: a stricter source overrides a weaker one. A stricter weaker source overrides a less strict stronger source only if the weaker source is the runtime safety floor.

Constitutional invariants enforced regardless of composition inputs:

- **I1 — Default deny.** Omitted fields resolve to the most restrictive value. A profile with no `network` block becomes `network.mode = DENY_ALL`, not "unset".
- **I2 — Policy cannot be loosened.** Caller hints, app manifests, and adapter defaults can only equal or tighten what policy requires.
- **I3 — Runtime safety floor is absolute.** No source can drop below the runtime safety floor. The floor for AI-initiated actions is stricter than for human-initiated actions.
- **I4 — Foreign code is wrapped.** Windows and Android applications must run inside their compatibility wrapper (Wine prefix, Waydroid container, VM fallback). Direct host execution of foreign binaries is rejected.
- **I5 — Required enforcement is mandatory.** If the host lacks a backend the profile requires (e.g., Landlock not available), composition fails closed. Degraded sandboxing requires explicit policy approval and emits dedicated evidence.
- **I6 — Profiles are immutable post-application.** Applied profile is content-addressed; revocation creates a new profile id. Mutation is not a transition.

## 3. Proto IDL

```proto
syntax = "proto3";
package aios.sandbox.v1alpha1;

import "google/protobuf/timestamp.proto";
import "google/protobuf/duration.proto";

// ============================================================================
// Service
// ============================================================================

service SandboxComposer {
  // Compose a sandbox profile from the five typed inputs. Deterministic given
  // the same input tuple. Does not apply enforcement.
  rpc ComposeProfile(ComposeProfileRequest) returns (ComposeProfileResponse);

  // Validate a profile against host capabilities. Returns missing-backend
  // diagnostics. Read-only; does not mutate kernel state.
  rpc ValidateProfile(ValidateProfileRequest) returns (ValidateProfileResponse);

  // Apply a previously composed and validated profile to a target process or
  // process group. Emits SANDBOX_PROFILE_APPLIED evidence.
  rpc ApplyProfile(ApplyProfileRequest) returns (ApplyProfileResponse);

  // Explain how each field was decided across the five sources. Returns the
  // winner trace and the alternatives considered.
  rpc ExplainComposition(ExplainCompositionRequest) returns (ExplainCompositionResponse);

  // Revoke a previously applied profile. Emits SANDBOX_PROFILE_REVOKED.
  rpc RevokeProfile(RevokeProfileRequest) returns (RevokeProfileResponse);

  // Engine info: backend capability snapshot, composer version, schema version.
  rpc GetComposerInfo(GetComposerInfoRequest) returns (GetComposerInfoResponse);
}

// ============================================================================
// Composition inputs (five typed sources)
// ============================================================================

message CompositionInputs {
  // Source 1: adapter default. Identified by adapter id + version.
  AdapterDefault adapter_default = 1;

  // Source 2: application manifest. Identified by manifest content hash.
  AppManifestRequirements app_manifest = 2;

  // Source 3: user/request hint. Best-effort; ignored where it would loosen.
  UserRequestHint user_request = 3;

  // Source 4: policy required. From S2.3 PolicyDecision.constraints.
  PolicyRequiredConstraints policy_required = 4;

  // Source 5: runtime safety floor. Loaded from signed runtime config bundle.
  RuntimeSafetyFloor runtime_safety_floor = 5;

  // Host capability snapshot. Composition is keyed against this for determinism.
  HostCapabilitySnapshot host_capabilities = 6;

  // Action envelope context (action_id, subject, target). For evidence linkage.
  ActionContext action_context = 7;
}

message AdapterDefault {
  string adapter_id = 1;
  string adapter_version = 2;
  string default_profile_id = 3;          // prof_<hex>; adapter publishes
  SandboxProfile profile = 4;
}

message AppManifestRequirements {
  string app_id = 1;
  string manifest_hash = 2;                // hex_lower(BLAKE3(jcs(manifest)))[:32]
  SandboxProfile required = 3;             // what the app needs to function
  SandboxProfile preferred = 4;             // what the app would like (optional)
}

message UserRequestHint {
  string request_id = 1;                    // links to S0.1 action.identity
  // Caller hints. Only restrictive hints are honored. Permissive hints are
  // silently dropped.
  SandboxProfile hint = 2;
}

message PolicyRequiredConstraints {
  string policy_decision_id = 1;            // links to S2.3 PolicyDecision.id
  string bundle_version = 2;                // polb_<hex>; from S2.3
  // Constraints from S2.3 §10 closed vocabulary projected onto sandbox shape.
  SandboxProfile required = 3;
  // True if policy allows a degraded sandbox when host capability is missing.
  bool allow_degraded = 4;
  // List of degraded fallbacks the policy explicitly approves.
  repeated DegradedFallback approved_fallbacks = 5;
}

message RuntimeSafetyFloor {
  string floor_id = 1;                      // sigfloor_<hex>; signed bundle
  string bundle_signature = 2;              // Ed25519 by AIOS root
  // Floor for human-initiated actions.
  SandboxProfile human_initiated_floor = 3;
  // Floor for AI-initiated actions (stricter).
  SandboxProfile ai_initiated_floor = 4;
  // Floor for recovery-mode actions (relaxed for human operator use).
  SandboxProfile recovery_mode_floor = 5;
}

message HostCapabilitySnapshot {
  string snapshot_id = 1;                   // hex_lower(BLAKE3(jcs(caps)))[:32]
  google.protobuf.Timestamp captured_at = 2;
  BackendCapabilities backends = 3;
  CompatibilityRuntimes runtimes = 4;
}

message ActionContext {
  string action_id = 1;                     // act_<ulid>
  string subject_canonical_id = 2;
  bool is_ai = 3;
  bool is_recovery_mode = 4;
}

// ============================================================================
// Sandbox profile (typed surface)
// ============================================================================

message SandboxProfile {
  string profile_id = 1;                    // prof_<hex>; content-addressed
  string schema_version = 2;                // "aios.sandbox.v1alpha1"
  string label = 3;                         // human-readable, no semantics

  FilesystemPolicy filesystem = 10;
  NetworkPolicy network = 11;
  ProcessPolicy process = 12;
  ResourcePolicy resources = 13;
  SecretsPolicy secrets = 14;
  EvidencePolicy evidence = 15;
  CompatibilityProfile compatibility = 16;  // optional; for foreign-code apps

  // Set by composer; not authored by hand.
  CompositionMetadata composition_metadata = 50;
}

message FilesystemPolicy {
  FilesystemMode root_mode = 1;             // closed enum below
  repeated PathRule allow_read = 2;
  repeated PathRule allow_write = 3;
  repeated PathRule deny = 4;                // wins over allow
  bool tmpfs_for_tmp = 5;                    // /tmp on private tmpfs
  bool home_isolation = 6;                   // private $HOME (xdg portal mediated)
}

enum FilesystemMode {
  FILESYSTEM_MODE_UNSPECIFIED = 0;
  READ_ONLY = 1;
  READ_WRITE_PRIVATE = 2;                    // private writable view, COW
  READ_ONLY_BROKERED = 3;                    // read via portal only
  NO_ACCESS = 4;
}

message PathRule {
  string path = 1;                           // absolute; supports {action_id} template
  bool recursive = 2;
  bool allow_create = 3;                     // for write rules
}

message NetworkPolicy {
  NetworkMode mode = 1;
  repeated NetworkAllow allow = 2;
  repeated string deny_hosts = 3;             // DNS-resolved, locked at compose time
  bool dns_brokered = 4;                      // DNS via broker, not host resolver
  bool block_metadata_endpoints = 5;          // 169.254.169.254, etc. (default true)
}

enum NetworkMode {
  NETWORK_MODE_UNSPECIFIED = 0;
  DENY_ALL = 1;
  LOOPBACK_ONLY = 2;
  HOST_LIMITED = 3;                           // only specific host endpoints
  EXPLICIT_ALLOWLIST = 4;
  FULL = 5;                                    // requires policy override
}

message NetworkAllow {
  string host = 1;                            // FQDN or CIDR
  repeated uint32 ports = 2;
  Protocol protocol = 3;
}

enum Protocol {
  PROTOCOL_UNSPECIFIED = 0;
  TCP = 1;
  UDP = 2;
  HTTPS = 3;                                  // TCP/443 with TLS pinning hint
}

message ProcessPolicy {
  string seccomp_profile_id = 1;              // closed catalog id
  bool no_new_privileges = 2;                 // default true
  bool drop_all_capabilities = 3;             // default true
  repeated LinuxCapability allowed_capabilities = 4;
  bool allow_ptrace = 5;                      // default false
  bool allow_user_namespace = 6;              // default false
  uint32 max_processes = 7;                   // 0 = unlimited
  uint32 max_open_files = 8;
}

enum LinuxCapability {
  LINUX_CAPABILITY_UNSPECIFIED = 0;
  CAP_NET_BIND_SERVICE = 1;
  CAP_DAC_READ_SEARCH = 2;
  CAP_CHOWN = 3;
  CAP_NET_ADMIN = 4;
  // Closed enum; expanding requires versioned spec change.
}

message ResourcePolicy {
  uint64 cpu_weight = 1;                      // cgroup cpu.weight (1..10000)
  google.protobuf.Duration cpu_max_per_period = 2;
  google.protobuf.Duration cpu_period = 3;
  uint64 memory_max_bytes = 4;
  uint64 memory_swap_max_bytes = 5;
  uint64 io_weight = 6;                       // cgroup io.weight
  uint64 pids_max = 7;
}

message SecretsPolicy {
  SecretsMode mode = 1;
  repeated string allowed_capabilities = 2;   // L4 vault capability ids
  bool brokered_only = 3;                     // alias for mode=BROKER_ONLY
}

enum SecretsMode {
  SECRETS_MODE_UNSPECIFIED = 0;
  NO_SECRET_ACCESS = 1;
  BROKER_ONLY = 2;                            // capabilities, never raw material
  BROKER_PLUS_RECOVERY = 3;                   // human recovery mode only
}

message EvidencePolicy {
  StreamCapture capture_stdout = 1;
  StreamCapture capture_stderr = 2;
  uint64 max_evidence_bytes = 3;
  bool emit_apply_event = 4;                  // default true
  bool emit_revoke_event = 5;                 // default true
}

enum StreamCapture {
  STREAM_CAPTURE_UNSPECIFIED = 0;
  NONE = 1;
  TRUNCATED = 2;                              // bounded bytes only
  REDACTED = 3;                               // PII/secrets stripped
  FULL = 4;                                   // requires policy override
}

message CompatibilityProfile {
  CompatibilityKind kind = 1;
  oneof runtime {
    WineProtonProfile wine_proton = 10;
    WaydroidProfile waydroid = 11;
    VmFallbackProfile vm_fallback = 12;
  }
}

enum CompatibilityKind {
  COMPATIBILITY_KIND_UNSPECIFIED = 0;
  NATIVE_LINUX = 1;                           // no wrapper
  WINE_PROTON = 2;
  WAYDROID = 3;
  VM_FALLBACK = 4;
}

message WineProtonProfile {
  string prefix_path = 1;                     // /aios/runtime/wine/{app_id}
  bool isolate_prefix = 2;                    // default true
  repeated string allowed_dlls = 3;
  bool block_host_home = 4;                   // default true
  bool portal_file_picker = 5;                // default true
}

message WaydroidProfile {
  string container_id = 1;
  bool per_app_data = 2;                      // default true
  bool clipboard_brokered = 3;                // default true
  bool file_portal_only = 4;                  // default true
}

message VmFallbackProfile {
  string vm_image_id = 1;
  uint64 memory_mib = 2;
  uint32 vcpus = 3;
  repeated VmStorageShare storage_shares = 4;
  bool evidence_bridge = 5;                   // default true; logs to host evidence log
}

message VmStorageShare {
  string host_path = 1;
  string guest_path = 2;
  bool read_only = 3;
}

message CompositionMetadata {
  string composition_id = 1;                  // comp_<ulid>
  google.protobuf.Timestamp composed_at = 2;
  string composer_version = 3;
  HostCapabilitySnapshot capability_snapshot = 4;
  repeated FieldDecision field_decisions = 5;
  bool degraded_mode = 6;                     // true if missing-backend fallback used
  repeated DegradedFallback applied_fallbacks = 7;
}

message FieldDecision {
  string field_path = 1;                       // e.g., "filesystem.root_mode"
  CompositionSource winner_source = 2;
  string winner_value = 3;                     // canonical encoding
  repeated SourceCandidate considered = 4;
}

enum CompositionSource {
  COMPOSITION_SOURCE_UNSPECIFIED = 0;
  ADAPTER_DEFAULT = 1;
  APP_MANIFEST = 2;
  USER_REQUEST = 3;
  POLICY_REQUIRED = 4;
  RUNTIME_SAFETY_FLOOR = 5;
  COMPOSER_DEFAULT = 6;                        // most-restrictive default for omitted fields
}

message SourceCandidate {
  CompositionSource source = 1;
  string value = 2;
  bool dropped = 3;
  string drop_reason = 4;                      // e.g., "loosens policy required"
}

// ============================================================================
// Backend capabilities
// ============================================================================

message BackendCapabilities {
  bool namespaces_user = 1;
  bool namespaces_mount = 2;
  bool namespaces_network = 3;
  bool namespaces_pid = 4;
  bool namespaces_uts = 5;
  bool namespaces_ipc = 6;
  bool seccomp_filter = 7;
  uint32 landlock_abi_version = 8;            // 0 = unavailable
  SecurityModule selinux = 9;
  SecurityModule apparmor = 10;
  CgroupVersion cgroup_version = 11;
  bool nftables_available = 12;
  bool xdg_portals_available = 13;
}

enum SecurityModule {
  SECURITY_MODULE_UNSPECIFIED = 0;
  ABSENT = 1;
  PERMISSIVE = 2;
  ENFORCING = 3;
}

enum CgroupVersion {
  CGROUP_VERSION_UNSPECIFIED = 0;
  CGROUP_V1 = 1;
  CGROUP_V2 = 2;
  HYBRID = 3;
}

message CompatibilityRuntimes {
  bool wine_available = 1;
  bool proton_available = 2;
  bool waydroid_available = 3;
  bool kvm_available = 4;
  bool qemu_system_available = 5;
}

message DegradedFallback {
  string field_path = 1;
  string missing_backend = 2;                  // e.g., "landlock"
  string fallback_strategy = 3;                // e.g., "namespace+seccomp only"
  string approval_evidence_id = 4;             // evr_<ulid> from S3.1
}

// ============================================================================
// RPC request/response
// ============================================================================

message ComposeProfileRequest {
  CompositionInputs inputs = 1;
}

message ComposeProfileResponse {
  oneof result {
    SandboxProfile profile = 1;
    CompositionError error = 2;
  }
}

message CompositionError {
  CompositionErrorCode code = 1;
  string message = 2;
  repeated string conflicting_fields = 3;
  repeated string missing_backends = 4;
}

enum CompositionErrorCode {
  COMPOSITION_ERROR_CODE_UNSPECIFIED = 0;
  POLICY_LOOSENED_BY_LOWER_SOURCE = 1;
  REQUIRED_BACKEND_UNAVAILABLE = 2;
  RUNTIME_FLOOR_VIOLATED = 3;
  CIRCULAR_REFERENCE = 4;
  HOST_CAPABILITY_LIE = 5;
  COMPOSITION_BUDGET_EXCEEDED = 6;
  COMPATIBILITY_KIND_UNSUPPORTED = 7;
  INVALID_INPUT_SCHEMA = 8;
}

message ValidateProfileRequest { SandboxProfile profile = 1; HostCapabilitySnapshot host = 2; }
message ValidateProfileResponse {
  bool valid = 1;
  repeated string warnings = 2;
  repeated string errors = 3;
}

message ApplyProfileRequest {
  SandboxProfile profile = 1;
  string action_id = 2;                        // links to S0.1 action envelope
  uint32 target_pid = 3;                       // 0 if process not yet started
}
message ApplyProfileResponse {
  ProfileLifecycle state = 1;
  string applied_profile_id = 2;               // prof_<hex>
  string evidence_receipt_id = 3;              // evr_<ulid>
  CompositionError error = 4;
}

message RevokeProfileRequest { string applied_profile_id = 1; string reason = 2; }
message RevokeProfileResponse { bool revoked = 1; string evidence_receipt_id = 2; }

message ExplainCompositionRequest { string composition_id = 1; }
message ExplainCompositionResponse { CompositionMetadata metadata = 1; }

message GetComposerInfoRequest {}
message GetComposerInfoResponse {
  string composer_version = 1;
  string schema_version = 2;
  HostCapabilitySnapshot capabilities = 3;
}

enum ProfileLifecycle {
  PROFILE_LIFECYCLE_UNSPECIFIED = 0;
  DRAFT = 1;
  VALIDATED = 2;
  APPLIED = 3;
  ENFORCING = 4;
  REVOKED = 5;
}
```

## 4. Closed vocabulary

All enums in §3 are **closed**. Adding a `FilesystemMode`, a `NetworkMode`, a `LinuxCapability`, or a `CompatibilityKind` is a versioned spec change — it requires a major version bump on `aios.sandbox.v1alpha1` (→ `v1alpha2` if pre-release, → `v1` if stable). Adapters, app manifests, and policy bundles MUST reject profiles containing values outside the enum at parse time.

Free-form strings are forbidden in:

- mode fields (`filesystem.root_mode`, `network.mode`, `secrets.mode`, `process.seccomp_profile_id` — must reference catalog id)
- enforcement field choices (no embedded shell, no embedded regex)
- compatibility kind selection

Extension points are explicit (e.g., `process.seccomp_profile_id` references a catalog of named seccomp profiles maintained alongside this spec; the catalog itself is versioned).

## 5. Composition algorithm

### 5.1 Inputs

```text
S1 = adapter_default.profile             (from registered adapter)
S2 = app_manifest.required               (from signed app manifest)
S3 = user_request.hint                    (from S0.1 action envelope)
S4 = policy_required.required             (from S2.3 PolicyDecision)
S5 = runtime_safety_floor (per-class)     (from signed runtime config)
SC = composer_default                      (most-restrictive defaults for omitted fields)
H  = host_capability_snapshot
```

### 5.2 Floor selection

```text
if action_context.is_recovery_mode:
    F = S5.recovery_mode_floor
elif action_context.is_ai:
    F = S5.ai_initiated_floor
else:
    F = S5.human_initiated_floor
```

### 5.3 Per-field merge

For each field path in `SandboxProfile`, the composer evaluates the candidates in order [SC, S1, S2, S3, S4, F] and produces a winner. The winner function depends on the field type:

| Field type                                                           | Composition rule                                       |
| -------------------------------------------------------------------- | ------------------------------------------------------ |
| boolean (restrictive=true means stricter, e.g., `no_new_privileges`) | OR across all candidates (any `true` wins)             |
| boolean (permissive=true means stricter, e.g., `tmpfs_for_tmp`)      | OR across all candidates (any `true` wins)             |
| enum mode (`FilesystemMode`, `NetworkMode`, `SecretsMode`)           | strictness-ranked; max strictness wins                 |
| `LinuxCapability` set                                                | intersection (only capabilities all sources agree on)  |
| `allow_read` / `allow_write` paths                                   | intersection unless policy says union (rare)           |
| `deny` path list                                                     | union                                                  |
| numeric resource limits                                              | min (lower limit wins)                                 |
| numeric resource floors (e.g., `cpu_weight`)                         | min (lower weight wins)                                |
| `network.allow` endpoint set                                         | intersection                                           |
| `network.deny_hosts`                                                 | union                                                  |
| `evidence.capture_stdout` (`StreamCapture`)                          | strictness-ranked; max strictness wins                 |
| `compatibility` (oneof runtime)                                      | only one source may set; conflicts → COMPOSITION_ERROR |

**Strictness ranks** (most → least):

```
FilesystemMode:  NO_ACCESS > READ_ONLY_BROKERED > READ_ONLY > READ_WRITE_PRIVATE
NetworkMode:     DENY_ALL > LOOPBACK_ONLY > HOST_LIMITED > EXPLICIT_ALLOWLIST > FULL
SecretsMode:     NO_SECRET_ACCESS > BROKER_ONLY > BROKER_PLUS_RECOVERY
StreamCapture:   NONE > REDACTED > TRUNCATED > FULL
```

### 5.4 Floor enforcement

After per-field merge, the composer compares the result against the runtime safety floor F at every field. If the merged value is less strict than F at any field, the floor wins **unconditionally**. This is the only point where a stricter source overrides a less strict one regardless of layering — the floor is constitutional.

### 5.5 Policy invariant check

After floor enforcement, the composer compares the result against the policy required values from S4. If any field is less strict than S4, composition emits `COMPOSITION_ERROR_CODE = POLICY_LOOSENED_BY_LOWER_SOURCE` with the conflicting field path. This case is impossible under correct merging but is checked as a defense-in-depth invariant.

### 5.6 Field decision recording

For every field, the composer records:

```proto
FieldDecision {
  field_path: "filesystem.root_mode",
  winner_source: POLICY_REQUIRED,
  winner_value: "READ_ONLY",
  considered: [
    { source: ADAPTER_DEFAULT, value: "READ_WRITE_PRIVATE", dropped: true, drop_reason: "loosens policy required" },
    { source: APP_MANIFEST, value: "READ_ONLY", dropped: false },
    { source: USER_REQUEST, value: "READ_WRITE_PRIVATE", dropped: true, drop_reason: "loosens policy required" },
    { source: POLICY_REQUIRED, value: "READ_ONLY", dropped: false },
    { source: RUNTIME_SAFETY_FLOOR, value: "READ_ONLY", dropped: false },
  ]
}
```

The complete trace is included in `CompositionMetadata.field_decisions`. `ExplainComposition` returns this directly.

### 5.7 Determinism

Given identical `(adapter_default.default_profile_id, app_manifest.manifest_hash, jcs_canonical(user_request.hint), policy_decision_id, runtime_safety_floor.floor_id, host_capabilities.snapshot_id)`, the algorithm produces an identical `SandboxProfile`. The output `profile_id = "prof_" + hex_lower(BLAKE3(jcs_canonical(profile_without_metadata)))[:32]`. This is the contract for the composition cache (§7.3).

## 6. Backend capability detection

### 6.1 Capability probe

On startup and on configuration change, the composer probes the host:

```text
namespaces_*       → /proc/self/ns/{user,mnt,net,pid,uts,ipc} probe
seccomp_filter      → prctl(PR_GET_SECCOMP)
landlock_abi_version → landlock_create_ruleset(NULL, 0, LANDLOCK_CREATE_RULESET_VERSION)
selinux              → /sys/fs/selinux/enforce read
apparmor             → /sys/kernel/security/apparmor/profiles read
cgroup_version       → /sys/fs/cgroup/cgroup.controllers (v2) vs /sys/fs/cgroup/<controller> (v1)
nftables             → nft list ruleset (if available)
xdg_portals          → DBus introspection of org.freedesktop.portal.*
wine_proton          → wine --version, proton presence
waydroid             → waydroid status
kvm                  → /dev/kvm presence + permissions
```

Results captured in `HostCapabilitySnapshot` with `snapshot_id = hex_lower(BLAKE3(jcs(capabilities)))[:32]`. Snapshot is recomputed when probe values change; old snapshots remain queryable for historical composition explanation.

### 6.2 Required-backend mapping

Each `SandboxProfile` field implies one or more required backends:

| Profile field                      | Required backend(s)                                    |
| ---------------------------------- | ------------------------------------------------------ |
| `filesystem.deny`                  | `landlock` OR `namespaces_mount` + bind-mount masking  |
| `filesystem.root_mode = READ_ONLY` | `namespaces_mount` + read-only bind                    |
| `network.mode != DENY_ALL`         | `namespaces_network` + `nftables` (or iptables)        |
| `network.dns_brokered`             | broker socket + namespace                              |
| `process.seccomp_profile_id`       | `seccomp_filter`                                       |
| `process.no_new_privileges`        | `prctl(PR_SET_NO_NEW_PRIVS)` (always available)        |
| `process.drop_all_capabilities`    | `cap_set_proc` (always available)                      |
| `resources.*`                      | `cgroup_v2` (preferred) or `cgroup_v1` (fallback)      |
| `secrets.mode = BROKER_ONLY`       | L4 Vault Broker socket reachable                       |
| `compatibility.wine_proton`        | `wine_available` + `proton_available` (Steam runtimes) |
| `compatibility.waydroid`           | `waydroid_available`                                   |
| `compatibility.vm_fallback`        | `kvm_available` + `qemu_system_available`              |

If any required backend is missing, composition checks `policy_required.allow_degraded`:

- **`allow_degraded = false`** → composition fails with `REQUIRED_BACKEND_UNAVAILABLE`.
- **`allow_degraded = true`** → composer attempts the policy-approved fallback strategy from `policy_required.approved_fallbacks`. If no fallback is approved for this missing backend, composition fails. If fallback succeeds, `CompositionMetadata.degraded_mode = true` and `applied_fallbacks` is populated. A `SANDBOX_DEGRADED_GRANTED` evidence record is emitted (see §11).

### 6.3 Capability lies

The composer re-probes the relevant backend at apply time. If the apply-time capability disagrees with the compose-time snapshot (e.g., Landlock said ABI 4 at compose, returns ABI 0 at apply), the composer fails closed with `HOST_CAPABILITY_LIE` and emits a `TAMPER_DETECTED` evidence record (S3.1).

## 7. Profile lifecycle and identity

### 7.1 States

```text
DRAFT         composer in progress; not persisted
VALIDATED     composer finished; ValidateProfile passed; not yet applied
APPLIED       ApplyProfile succeeded; enforcement loaded
ENFORCING     adapter has begun action execution; profile is in force
REVOKED       RevokeProfile called; enforcement torn down
```

Valid transitions: `DRAFT → VALIDATED → APPLIED → ENFORCING → REVOKED`. `APPLIED → REVOKED` and `VALIDATED → REVOKED` are also valid (early revocation). No backward transitions.

### 7.2 Immutability

Once a profile reaches `APPLIED`, its `profile_id` and content are immutable. To change enforcement, revoke and apply a new profile (with a new `profile_id`). Mutation of an applied profile is rejected by the composer.

### 7.3 Composition cache

Composition results are cacheable per the determinism tuple from §5.7. Cache TTL defaults to 5 minutes. Cache is invalidated when:

- `host_capabilities.snapshot_id` changes (probe result delta)
- `runtime_safety_floor.floor_id` changes (new signed bundle loaded)
- adapter publishes a new `default_profile_id`
- policy bundle version changes

Cache hit emits a `composer_cache_hit_total{result="hit"}` counter increment; cache miss recomputes and persists.

## 8. Performance contract

| Operation                                 | Budget       | Hard timeout |
| ----------------------------------------- | ------------ | ------------ |
| `ComposeProfile` (cached)                 | p95 < 1 ms   | 10 ms        |
| `ComposeProfile` (fresh)                  | p95 < 5 ms   | 50 ms        |
| `ValidateProfile`                         | p95 < 10 ms  | 50 ms        |
| `ApplyProfile` (native)                   | p95 < 50 ms  | 500 ms       |
| `ApplyProfile` (Wine prefix bring-up)     | p95 < 2 s    | 10 s         |
| `ApplyProfile` (Waydroid container start) | p95 < 5 s    | 30 s         |
| `ApplyProfile` (VM fallback boot)         | p95 < 30 s   | 120 s        |
| `RevokeProfile`                           | p95 < 100 ms | 1 s          |
| `ExplainComposition`                      | p95 < 10 ms  | 100 ms       |

Failure modes — all fail closed:

- `BackendUnavailable` → composition fails or, with policy approval, degraded mode.
- `CompositionConflict` → caller receives `CompositionError` with conflicting fields.
- `ProfileApplicationFailed` → action does not start; S0.1 envelope transitions to `FAILED` with `SandboxApplicationFailed` error code.
- `ComposerInternal` → engine fails closed; no profile returned; alert emitted.

Backpressure: when composition queue depth exceeds threshold (default 100 in flight), new requests are rejected with `COMPOSITION_BUDGET_EXCEEDED`.

## 9. Compatibility runtime profiles

### 9.1 Wine/Proton (`WINE_PROTON`)

Default profile per app:

```yaml
filesystem:
  root_mode: READ_ONLY
  allow_write:
    - /aios/runtime/wine/{app_id}/prefix # private prefix
  deny:
    - $HOME # block host home
    - /etc # block host config
network:
  mode: EXPLICIT_ALLOWLIST # game/app-specific
process:
  seccomp_profile_id: wine-default
  no_new_privileges: true
secrets:
  mode: NO_SECRET_ACCESS
compatibility:
  kind: WINE_PROTON
  wine_proton:
    prefix_path: /aios/runtime/wine/{app_id}
    isolate_prefix: true
    block_host_home: true
    portal_file_picker: true
```

Composer floor for Wine apps adds:

- `block_host_home = true` (cannot be loosened by app manifest or user request)
- File access outside the prefix only via XDG portals
- No direct DBus access; brokered DBus proxy only

### 9.2 Waydroid (`WAYDROID`)

Default profile:

```yaml
filesystem:
  root_mode: NO_ACCESS # Android container is its own FS
  allow_write:
    - /aios/runtime/waydroid/{app_id}/data
network:
  mode: EXPLICIT_ALLOWLIST
process:
  seccomp_profile_id: waydroid-default
secrets:
  mode: NO_SECRET_ACCESS
compatibility:
  kind: WAYDROID
  waydroid:
    container_id: waydroid-{app_id}
    per_app_data: true
    clipboard_brokered: true
    file_portal_only: true
```

Floor: per-app data isolation, brokered clipboard, brokered file picker.

### 9.3 VM fallback (`VM_FALLBACK`)

Used for hard-incompatible apps (kernel-level anti-cheat, hard DRM, drivers requiring direct hardware access). Profile:

```yaml
filesystem:
  root_mode: NO_ACCESS # VM has its own disk
network:
  mode: EXPLICIT_ALLOWLIST # bridged via host firewall
secrets:
  mode: NO_SECRET_ACCESS
evidence:
  capture_stdout: REDACTED
compatibility:
  kind: VM_FALLBACK
  vm_fallback:
    vm_image_id: vmimg_<hex>
    memory_mib: 4096
    vcpus: 2
    storage_shares:
      - host_path: /aios/runtime/vm/{app_id}/data
        guest_path: /vm/data
        read_only: false
    evidence_bridge: true
```

Floor: explicit storage shares only (no virt-manager-style auto-mounts), evidence bridge mandatory, no direct host networking, qemu-system invocation (not virt-manager defaults).

### 9.4 Selection

App manifest declares `compatibility.kind`. Composer rejects compositions where:

- `kind = NATIVE_LINUX` but app binary is PE/ELF-foreign → `COMPATIBILITY_KIND_UNSUPPORTED`.
- `kind = WINE_PROTON` but `runtimes.wine_available = false` → `REQUIRED_BACKEND_UNAVAILABLE`.
- Multiple sources set different `compatibility.kind` values → `COMPOSITION_CONFLICT`.

## 10. Determinism contract

```text
GIVEN
  adapter_default.default_profile_id      = A
  app_manifest.manifest_hash              = M
  jcs_canonical(user_request.hint)        = U
  policy_decision_id                      = P
  runtime_safety_floor.floor_id           = F
  host_capabilities.snapshot_id           = H
  composer_version                        = V

THEN
  ComposeProfile(inputs) ≡ ComposeProfile(inputs)
  for any inputs with identical (A, M, U, P, F, H, V).
```

Composer version is part of the key because algorithm changes between versions can produce different valid profiles. A composer upgrade invalidates the cache.

## 11. Evidence integration

The composer emits the following evidence record types into S3.1 (added to the closed `RecordType` vocabulary):

| Record type                  | Emitted on                         | Retention class                  |
| ---------------------------- | ---------------------------------- | -------------------------------- |
| `SANDBOX_PROFILE_COMPOSED`   | every `ComposeProfile` success     | STANDARD_24M                     |
| `SANDBOX_PROFILE_APPLIED`    | every `ApplyProfile` success       | STANDARD_24M                     |
| `SANDBOX_PROFILE_REVOKED`    | every `RevokeProfile`              | STANDARD_24M                     |
| `SANDBOX_DEGRADED_GRANTED`   | composition with degraded fallback | FOREVER                          |
| `SANDBOX_APPLICATION_FAILED` | `ApplyProfile` failure             | EXTENDED_60M                     |
| `SANDBOX_CAPABILITY_LIE`     | apply-time capability mismatch     | FOREVER (also `TAMPER_DETECTED`) |

`SANDBOX_DEGRADED_GRANTED` is `FOREVER`-retained because it represents a constitutional concession that operators must be able to audit indefinitely. Cross-spec dependency: S3.1 §6 record-type table is updated to include these six types.

Each record carries:

- `composition_id` (links to `ComposeProfile` result)
- `profile_id`
- `policy_decision_id` (links to S2.3)
- `action_id` (links to S0.1)
- `host_capability_snapshot_id`
- For degraded: `missing_backend`, `fallback_strategy`, `approval_evidence_id`

## 12. Adversarial robustness

### 12.1 Field omission

Omitted fields **never** mean "permissive default". The composer initializes any unset field with the most-restrictive default (the SC source). Examples:

- Missing `network` block → `network.mode = DENY_ALL`.
- Missing `filesystem` block → `filesystem.root_mode = NO_ACCESS`, no allow lists.
- Missing `process.seccomp_profile_id` → composer defaults to `aios.deny-all` seccomp catalog id.
- Missing `secrets` block → `secrets.mode = NO_SECRET_ACCESS`.

### 12.2 Caller hint sanitization

`UserRequestHint.hint` is parsed but only restrictive aspects are honored. Permissive caller hints (e.g., `network.mode = FULL` from a user request when policy says `EXPLICIT_ALLOWLIST`) are silently dropped from the candidate set, with `dropped: true` and `drop_reason: "loosens policy required"` recorded. Repeated permissive hints are not an error — they simply have no effect.

### 12.3 Reference cycles

Adapter defaults that reference runtime safety floors (e.g., a default profile that says "inherit floor X for AI subjects") are forbidden. The composer DAG-checks `(adapter_default → runtime_safety_floor)` at composer startup and `(adapter_default → app_manifest → adapter_default)` at compose time. Cycles fail with `CIRCULAR_REFERENCE`.

### 12.4 Composition budget

Per-evaluation budget:

- ≤ 100 fields composed (more means schema bloat or attack)
- ≤ 1000 entries in any allow list
- ≤ 1000 entries in any deny list
- ≤ 10 KB total profile size after canonical encoding

Exceeding budget fails with `COMPOSITION_BUDGET_EXCEEDED`.

### 12.5 Rate limits

Per-subject `ComposeProfile` rate limit: 10 000 / minute (default). `ApplyProfile`: 1000 / minute. Subject-class limits are enforced before any composition work begins.

### 12.6 Apply-time invariants

At apply, the composer verifies:

- `profile_id` matches the content hash of the supplied profile (no mid-flight tampering).
- Host capability snapshot used at compose time still matches the live host (re-probe; mismatch → `HOST_CAPABILITY_LIE`).
- Target process exists if `target_pid != 0`.
- The action_id has a `policy_decision` evidence record with status `ALLOW`.

Violations fail closed and emit `SANDBOX_APPLICATION_FAILED`.

## 13. Cross-spec dependencies

| Spec        | Dependency                                                                                                                                                  |
| ----------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------- |
| S0.1        | `action_id` from action envelope; sandbox profile id recorded in `execution.profile_id` field                                                               |
| S1.1        | Capability translator emits action whose adapter is the source of `adapter_default`                                                                         |
| S1.2        | Latency router does not directly consume sandbox profiles, but routing class influences floor selection (e.g., T0 deterministic actions get stricter floor) |
| S1.3        | App manifests stored as AIOS-FS objects; `manifest_hash` computed via S1.3 chunk discipline                                                                 |
| S2.1        | Profile audit queries (e.g., "all degraded compositions in last 24h") use S2.1 query language                                                               |
| S2.3        | `PolicyRequiredConstraints` translated from `PolicyDecision.constraints`; `allow_degraded` and `approved_fallbacks` provided by policy bundle               |
| S2.4        | `SANDBOX_PROFILE_APPLIED` is a verifiable property; verification primitives can probe `process.seccomp_profile_id` and `filesystem.root_mode` post-apply    |
| S3.1        | Evidence record types added (six types in §11); `SANDBOX_DEGRADED_GRANTED` is FOREVER-retained                                                              |
| L4 Vault    | `SecretsPolicy.allowed_capabilities` references L4 vault capability ids; broker socket reachability is a backend requirement                                |
| L4 Identity | `is_ai`, `is_recovery_mode`, `subject_canonical_id` from L4 identity service; floor selection depends on these                                              |

## 14. Golden fixtures

Implementations must produce these exact outputs (or equivalents under canonical encoding) for the corresponding inputs.

### Fixture 1 — Basic 5-source merge

```yaml
input:
  adapter_default.profile.filesystem.root_mode: READ_WRITE_PRIVATE
  app_manifest.required.filesystem.root_mode: READ_ONLY
  user_request.hint.filesystem.root_mode: READ_WRITE_PRIVATE
  policy_required.required.filesystem.root_mode: READ_ONLY
  runtime_safety_floor.ai_initiated_floor.filesystem.root_mode: READ_ONLY
  action_context.is_ai: true

expected:
  profile.filesystem.root_mode: READ_ONLY
  composition_metadata.field_decisions["filesystem.root_mode"].winner_source: POLICY_REQUIRED
  user_request candidate dropped with reason: "loosens policy required"
```

### Fixture 2 — Policy wins over permissive request

```yaml
input:
  user_request.hint.network.mode: FULL
  policy_required.required.network.mode: EXPLICIT_ALLOWLIST
  runtime_safety_floor.network.mode: DENY_ALL # but policy required is stricter? no
  # rerank: EXPLICIT_ALLOWLIST is less strict than DENY_ALL
expected:
  profile.network.mode: DENY_ALL # floor wins
  composition_metadata.field_decisions["network.mode"].winner_source: RUNTIME_SAFETY_FLOOR
```

### Fixture 3 — Allow-list intersection

```yaml
input:
  adapter_default.network.allow: [{ host: "api.example.com", ports: [443] }]
  app_manifest.network.allow:
    [
      { host: "api.example.com", ports: [443] },
      { host: "metrics.example.com", ports: [9090] },
    ]
  policy_required.network.allow: [{ host: "api.example.com", ports: [443] }]
expected:
  profile.network.allow: [{ host: "api.example.com", ports: [443] }]
  # metrics.example.com dropped (not in policy allow)
```

### Fixture 4 — Deny-list union

```yaml
input:
  adapter_default.filesystem.deny: ["/etc/shadow"]
  app_manifest.filesystem.deny: ["/etc/passwd"]
  runtime_safety_floor.filesystem.deny: ["/proc/sys"]
expected:
  profile.filesystem.deny: ["/etc/passwd", "/etc/shadow", "/proc/sys"] # union, sorted
```

### Fixture 5 — Resource floor wins

```yaml
input:
  adapter_default.resources.memory_max_bytes: 2147483648 # 2 GiB
  app_manifest.resources.memory_max_bytes: 1073741824 # 1 GiB
  policy_required.resources.memory_max_bytes: 536870912 # 512 MiB
expected:
  profile.resources.memory_max_bytes: 536870912 # 512 MiB; min wins
```

### Fixture 6 — Required backend missing, no degraded

```yaml
input:
  app_manifest.required.filesystem.deny: ["/etc"] # requires landlock
  host_capabilities.backends.landlock_abi_version: 0 # absent
  policy_required.allow_degraded: false
expected:
  result: CompositionError
  code: REQUIRED_BACKEND_UNAVAILABLE
  missing_backends: ["landlock"]
```

### Fixture 7 — Degraded mode allowed by policy

```yaml
input:
  app_manifest.required.filesystem.deny: ["/etc"]
  host_capabilities.backends.landlock_abi_version: 0
  policy_required.allow_degraded: true
  policy_required.approved_fallbacks:
    [
      {
        field_path: "filesystem.deny",
        missing_backend: "landlock",
        fallback_strategy: "namespace+bind-mount masking",
      },
    ]
expected:
  profile.composition_metadata.degraded_mode: true
  profile.composition_metadata.applied_fallbacks: [{ ... }]
  # SANDBOX_DEGRADED_GRANTED evidence emitted with policy_decision_id linkage
```

### Fixture 8 — Wine prefix wrapper composed

```yaml
input:
  app_manifest.compatibility.kind: WINE_PROTON
  app_manifest.compatibility.wine_proton.prefix_path: /aios/runtime/wine/notepad-plus-plus
  user_request.hint.filesystem.allow_write: [{ path: "$HOME" }] # permissive; will drop
  runtime_safety_floor.wine.block_host_home: true
expected:
  profile.compatibility.kind: WINE_PROTON
  profile.compatibility.wine_proton.block_host_home: true
  profile.filesystem.deny: ["$HOME"] # via floor
  user_request hint dropped with reason: "loosens runtime safety floor"
```

### Fixture 9 — Circular reference rejected

```yaml
input:
  adapter_default.adapter_id: "adapter-A"
  adapter_default.profile.references: ["adapter-A"] # cycle
expected:
  result: CompositionError
  code: CIRCULAR_REFERENCE
```

### Fixture 10 — Apply-time capability lie detected

```yaml
input:
  compose_time.host_capabilities.backends.landlock_abi_version: 4
  apply_time.host_capabilities.backends.landlock_abi_version: 0 # lie or real change
expected:
  ApplyProfile result: CompositionError
  code: HOST_CAPABILITY_LIE
  evidence emitted: TAMPER_DETECTED + SANDBOX_CAPABILITY_LIE (FOREVER retention)
```

## 15. Telemetry contract

All metrics MUST use bounded label cardinality. **Subject id, app id, action id, and profile id are NEVER labels**; they appear in evidence records, never as Prometheus labels.

| Metric                             | Type      | Labels (closed set)                                              |
| ---------------------------------- | --------- | ---------------------------------------------------------------- |
| `sandbox_compose_duration_seconds` | histogram | `result` (success/error), `cache` (hit/miss)                     |
| `sandbox_compose_total`            | counter   | `result`, `error_code` (closed enum)                             |
| `sandbox_apply_duration_seconds`   | histogram | `kind` (NATIVE_LINUX/WINE_PROTON/WAYDROID/VM_FALLBACK), `result` |
| `sandbox_apply_total`              | counter   | `kind`, `result`, `error_code`                                   |
| `sandbox_degraded_mode_total`      | counter   | `missing_backend` (closed enum)                                  |
| `sandbox_capability_lie_total`     | counter   | `backend` (closed enum)                                          |
| `sandbox_revoke_total`             | counter   | `reason_class` (planned/error/policy_revocation)                 |
| `sandbox_active_profiles`          | gauge     | `kind`                                                           |
| `sandbox_backend_available`        | gauge     | `backend` (closed enum)                                          |
| `sandbox_floor_class_in_use`       | counter   | `floor_class` (human/ai/recovery)                                |

Cardinality budget: ≤ 200 active label tuples per metric. Backend enum has fewer than 20 values; error code enum has 9 values; kind enum has 4 values.

## 16. Acceptance criteria

- [ ] Profiles are declarative, typed, and free of free-form strings outside catalog id references.
- [ ] Composition is deterministic given the §10 input tuple.
- [ ] Policy can only equal or tighten the merged profile relative to lower-priority sources.
- [ ] Runtime safety floor is constitutional and cannot be loosened by any source.
- [ ] Omitted fields resolve to the most restrictive defaults (no permissive defaults anywhere).
- [ ] Missing required enforcement fails closed unless policy explicitly approves a degraded fallback.
- [ ] Degraded mode emits `SANDBOX_DEGRADED_GRANTED` evidence with FOREVER retention.
- [ ] Applied profile id is content-addressed and recorded in S0.1 `execution.profile_id`.
- [ ] Foreign apps (Windows, Android) never run unwrapped on the host.
- [ ] Wine prefixes never have direct host home access without portal mediation (floor-enforced).
- [ ] Waydroid containers never have host docker socket or host clipboard without brokering.
- [ ] VM fallback always uses explicit storage shares; no virt-manager-style auto-mounts.
- [ ] Apply-time capability re-probe detects host lies; mismatch fails closed with `TAMPER_DETECTED`.
- [ ] All ten golden fixtures (§14) produce the specified outputs under the implementation.
- [ ] Telemetry conforms to §15 cardinality bounds; subject/app/action/profile ids never appear as labels.

## 17. Open deferrals

These are intentionally out of scope for S3.2 and tracked elsewhere:

- **VM fallback orchestration mechanics** (image build/distribution, snapshot, live migration) — deferred to `03_compatibility_runtime.md`.
- **Compatibility knowledge database** (per-app proven profiles, ProtonDB-equivalent governance) — deferred to `05_compatibility_knowledge.md`.
- **Profile A/B testing** for app updates with new sandbox requirements — deferred (post-Rev.2).
- **Multi-tenant profile namespacing** (per-tenant adapter defaults, per-tenant runtime floors) — deferred (post-Rev.2; requires multi-tenant identity model).
- **Hot-reload of runtime safety floor** mid-execution — deferred; current contract is "new floor applies to new compositions only".
- **Profile diffing UI** for operators inspecting why a composition produced a given result — deferred to L7 renderer specs.
- **Hardware-pinned secure enclaves** (TPM-bound profiles, TEE-isolated sandboxes) — deferred to L8 hardware integration.

## 18. Namespace integration (S4.1 cross-spec touch-up)

Applied 2026-05-09. Source: [S4.1 §12.7](../L2_AIOS_FS/05_namespace_layout.md).

### 18.1 New composition source — `GROUP_FLOOR`

The five-source composition algorithm (§5.1) becomes a six-source algorithm. The new source is inserted between `policy_required` and `runtime_safety_floor`:

```text
1. adapter_default
2. app_manifest
3. user_request
4. policy_required        (S2.3 PolicyDecision.constraints)
5. group_floor            (NEW — group's policy delta sandbox additions)
6. runtime_safety_floor   (constitutional minimum)
```

The `CompositionSource` enum (§3) gains `GROUP_FLOOR = 7`. Strictness ordering and per-field merge rules (§5.3) extend to this source unchanged: a stricter `group_floor` value wins over weaker upstream sources, and the runtime safety floor still wins over `group_floor` if `group_floor` is somehow weaker (defense-in-depth).

`group_floor` is loaded from `/aios/groups/<group_id>/policy/sandbox_floor.aios` if present; absent → empty `SandboxProfile` (no contribution).

### 18.2 New input field

`CompositionInputs` adds:

```proto
message CompositionInputs {
  // ... existing fields ...
  GroupFloor group_floor = 8;
}

message GroupFloor {
  string group_id = 1;
  string floor_id = 2;        // sigfloor_<hex>; signed if group has signing key
  SandboxProfile floor = 3;
}
```

### 18.3 Apply-time group ownership check

At `ApplyProfile`, the composer adds an invariant check:

```text
IF action.target.group_id != "" AND
   profile.compatibility.kind != COMPATIBILITY_KIND_UNSPECIFIED AND
   agent_owner.group_id != action.target.group_id
THEN fail with CompositionError {
  code = COMPATIBILITY_KIND_UNSUPPORTED,
  message = "agent group_owner does not match action target group"
}
```

This prevents an agent registered in group A from running an action whose target is in group B even if S2.3's `CrossGroupAccessForbidden` somehow allowed it (defense-in-depth).

### 18.4 Compatibility runtime path discipline

Wine prefix paths, Waydroid container paths, and VM fallback storage shares MUST live under the agent's group namespace:

| Runtime kind  | Required path prefix                                          |
| ------------- | ------------------------------------------------------------- |
| `WINE_PROTON` | `/aios/groups/<group_id>/agents/<agent_id>/runtime/wine/`     |
| `WAYDROID`    | `/aios/groups/<group_id>/agents/<agent_id>/runtime/waydroid/` |
| `VM_FALLBACK` | `/aios/groups/<group_id>/agents/<agent_id>/runtime/vm/`       |

System agents (under `/aios/system/agents/<agent_id>/`) use `/aios/system/runtime/<runtime_kind>/<agent_id>/` instead. Profiles whose paths violate this discipline are rejected with `CompositionError.code = INVALID_INPUT_SCHEMA` and a sub-reason `RuntimePathOutsideAgentScope`.

### 18.5 New evidence record on group-floor application

`SANDBOX_GROUP_FLOOR_APPLIED` (STANDARD_24M retention) is added to S3.1 RecordType vocabulary and emitted whenever `group_floor` strictness modified the composed profile (i.e., at least one `FieldDecision.winner_source = GROUP_FLOOR` in the composition trace).

## 19. Wave 5 cross-spec touch-up (S7.1+S8.2 + L0 INV-019..022 consolidation)

Applied 2026-05-10. Sources: [S7.1 §13](../L7_Interaction_Renderers/01_surface_composition.md), [S8.2 §11](../L8_Network_Hardware_Devices/05_gpu_resource_model.md). This section adds the queued S3.2 cross-spec touch-up requested by S7.1 and S8.2: the `SandboxProfile.gpu_policy` field and the apply-time defense-in-depth check that ties a sandbox application to the surface group_owner discipline.

### 19.1 New field — `SandboxProfile.gpu_policy`

`SandboxProfile` (§3) gains a typed `GpuPolicy` field. The composition algorithm (§5) treats it as a normal merge participant: the most-restrictive value across the six sources wins, and the runtime safety floor cannot be loosened by any earlier source.

```proto
message GpuPolicy {
  string gpu_capability_class = 1;          // closed enum reference to S8.2 GpuCapabilityClass
  uint64 vram_max_bytes = 2;                 // explicit cap; cannot exceed class default
  string queue_priority = 3;                  // closed enum reference to S8.2 QueuePriority
  uint32 frame_rate_cap_fps = 4;
  repeated string allowed_dmabuf_peers = 5;   // canonical_subject_ids permitted to receive dmabuf
  bool deny_compute_pipeline = 6;             // override class default; force compute-disabled
  bool deny_validation_layers = 7;            // recovery-mode default; can be set explicitly
}

// SandboxProfile gains:
//   GpuPolicy gpu_policy = N;
```

Per-field merge rules:

| Field                    | Merge rule                                                                                                                                                              |
| ------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `gpu_capability_class`   | The strictest class wins (closed-enum ordering: `GPU_NONE` > `GPU_DISPLAY_ONLY` > ... > `GPU_COMPUTE_HEAVY`); a stricter class downgrades a permissive upstream choice. |
| `vram_max_bytes`         | `min` across all sources; runtime safety floor cannot raise it.                                                                                                         |
| `queue_priority`         | The lowest priority wins (`BACKGROUND` < `INTERACTIVE` < `REALTIME`).                                                                                                   |
| `frame_rate_cap_fps`     | `min` across all sources; 0 = "no cap" is treated as the maximum.                                                                                                       |
| `allowed_dmabuf_peers`   | Set intersection across all sources; empty set = no peers allowed.                                                                                                      |
| `deny_compute_pipeline`  | OR across all sources (any `true` wins).                                                                                                                                |
| `deny_validation_layers` | OR across all sources, with one exception: under `recovery_mode = true` the runtime safety floor sets it to `true` regardless of upstream sources.                      |

`gpu_policy` is class-bounded: `vram_max_bytes` and `queue_priority` cannot exceed the per-class defaults declared in S8.2; an out-of-bounds composition is rejected with `CompositionError.code = INVALID_INPUT_SCHEMA` and sub-reason `GpuPolicyExceedsClass`.

### 19.2 Apply-time invariant — surface group ownership

The §11 evidence integration and the §12 adversarial robustness already require strict source provenance per composition. Wave 5 adds a defense-in-depth check at `ApplyProfile`:

```text
IF action.target.surface_id != "" THEN
   // resolve the surface from the S7.1 registry
   LET surface = SurfaceRegistry.get(action.target.surface_id);
   IF surface.group_owner != subject.primary_group_id
   THEN fail with CompositionError {
     code = INVALID_INPUT_SCHEMA,
     sub_reason = "SurfaceGroupOwnershipMismatch",
     message = "subject primary_group_id does not match surface group_owner"
   }
```

This complements:

- **S2.3 §26** `CrossGroupAccessForbidden` (policy-tier) and **S3.2 §18.3** the agent / action group-ownership check.
- The check is independent — even if the policy decision is `ALLOW`, the apply-time discipline rejects a mismatched binding. This is the specific defense-in-depth requested by [S7.1 §13](../L7_Interaction_Renderers/01_surface_composition.md).

The mismatch emits a `CROSS_GROUP_ACCESS_DENIED` evidence record (S3.1) with the offending `surface_id` and `group_owner` recorded in the redacted observation.

### 19.3 Source contribution allowance for `gpu_policy`

| Source                 | May contribute `gpu_policy`? | Notes                                                                          |
| ---------------------- | ---------------------------- | ------------------------------------------------------------------------------ |
| `adapter_default`      | yes                          | Adapter declares the minimum class it actually needs.                          |
| `app_manifest`         | yes                          | App manifest declares its requested class within its capability set.           |
| `user_request`         | yes (downgrade only)         | A user can only further restrict; cannot upgrade beyond the manifest request.  |
| `policy_required`      | yes                          | Policy can downgrade or restrict (e.g., disable compute outside grant window). |
| `group_floor`          | yes                          | Group-scoped baseline; can mandate `deny_compute_pipeline` for the group.      |
| `runtime_safety_floor` | yes                          | Constitutional minimum: `deny_validation_layers = true` under recovery mode.   |

The composition trace records each `FieldDecision` for every `gpu_policy` field per the §11.2 trace discipline.

### 19.4 New evidence integration

This touch-up does not introduce a new RecordType — the existing `GPU_CAPABILITY_DENIED` (added to S3.1 in §24) covers the apply-time GpuPolicy-derived denials, and the existing `CROSS_GROUP_ACCESS_DENIED` covers the §19.2 invariant failures. The composition trace `FieldDecision` entries for `gpu_policy.*` participate in the existing `SANDBOX_GROUP_FLOOR_APPLIED` lineage.

## See also

- [S0.1 — Action Envelope and Lifecycle](../XX_Cross_Cutting/01_action_envelope_lifecycle.md)
- [S2.3 — Policy Kernel](../L4_Policy_Identity_Vault/01_policy_kernel.md)
- [S2.4 — Verification Grammar](../L9_Observability_Admin_Operations/02_verification_grammar.md)
- [S3.1 — Evidence Log](../L9_Observability_Admin_Operations/01_evidence_log.md)
- [S4.1 — Namespace Layout](../L2_AIOS_FS/05_namespace_layout.md)
- [S7.1 Surface Composition](../L7_Interaction_Renderers/01_surface_composition.md)
- [S7.2 Shared UI Schema](../L7_Interaction_Renderers/02_shared_ui_schema.md)
- [S7.3 Visual Language](../L7_Interaction_Renderers/03_visual_language.md)
- [S7.4 KDE Renderer](../L7_Interaction_Renderers/04_kde_renderer.md)
- [S7.5 Web Renderer](../L7_Interaction_Renderers/05_web_renderer.md)
- [S8.2 GPU Resource Model](../L8_Network_Hardware_Devices/05_gpu_resource_model.md)
- [Rev.1 §17 — Application, Package, and Compatibility Model](../../001.AI-OS.NET--SPECREV.1/02_SPECIFICATION.md)
- [L6 Overview](00_overview.md)
- [Rev.2 Master Index](../00_MASTER_INDEX.md)
