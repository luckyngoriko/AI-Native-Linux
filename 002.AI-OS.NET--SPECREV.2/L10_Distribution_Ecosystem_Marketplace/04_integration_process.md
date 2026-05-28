# S11.4 — Integration Process (aios.integration.v1alpha1)

| Field               | Value                                                                                                                       |
| ------------------- | --------------------------------------------------------------------------------------------------------------------------- |
| **Status**          | REAL                                                                                                                        |
| **Phase**           | S11.4                                                                                                                       |
| **Layer**           | L10 — Distribution / Ecosystem / Marketplace                                                                                |
| **Schema package**  | `aios.integration.v1alpha1`                                                                                                 |
| **Crate**           | `crates/aios-integration` (v0.1.0, M18 closed)                                                                              |
| **Depends on**      | L0 (constitutional truth), L3 (SGR), L4 (Policy), L5 (Cognition), L7 (Renderers), L8 (Hardware/Network), L9 (Observability) |
| **Sub-spec status** | REAL — full specification finalised at T-186 (M18 closer)                                                                   |
| **Created**         | 2026-05-28 (T-175, M18 opening task)                                                                                        |
| **Finalised**       | 2026-05-28 (T-186, M18 closer)                                                                                              |
| **Owner**           | claude-ds (DeepSeek-V4-Pro) via Governor workflow                                                                           |

---

## § 1. Purpose

The Integration Process layer (S11.4) defines the typed contracts that bind
the M1…M17 crates — from `aios-action` through `aios-hardware` — into one
cohesive, verifiable, policy-governed AIOS distribution. Without this layer,
the 17 crates are a loose collection of typed libraries; with it, they form an
auditable, boot-ordered, evidence-producing operating system.

S11.4 provides the following architectural pillars:

1. **Integration Lifecycle FSM** (§ 2 I1, § 3): every external integration —
   vendor contracts, standards subscriptions, CVE bindings, bridge contracts,
   composed subsystems — obeys a single 6-state lifecycle (Proposed →
   Evaluated → Piloted → Production → Deprecated → Retired) with typed guard
   conditions per transition. Retired is terminal and irreversible. Deprecated
   may revert to Production (sunset cancelled). Proposed may be rejected
   directly to Retired.

2. **Vendor Registry** (§ 2 I2, § 3): all third-party integrations
   (Flathub, OCI registries, NVD CVE feeds, compliance providers, distro
   package repos, identity providers, metrics exporters) are admitted via an
   Ed25519-signed `VendorIntegrationContract`. The registry enforces trust
   classification (`AiosCertifiedPartner`, `CommunityVerified`,
   `OperatorAuthorised`, `BlacklistedDoNotAdmit`), key rotation cadence,
   and blacklist admission guards.

3. **Standards Subscription Discipline** (§ 2 I3, § 3): 11 regulatory
   frameworks (NIST 800-53 Rev.5, 800-218 SSDF, 800-207 Zero Trust,
   800-193 Firmware, DISA STIG, CIS Controls v8, FIPS 140-3, GDPR, HIPAA,
   ISO 27001, SOC 2) are tracked with timed 90-day review windows and a
   30-day grace period after expiry. Each subscription pins a responsible
   canonical identity and a revision-versioned catalog URL.

4. **CVE Feed Framework** (§ 2 I4, § 3): upstream vulnerability data
   (NVD, GitHub Advisory, OSV) is ingested as typed `CveRecord` values with
   CVSS v3 scores. Package bindings (`PackageCveBinding`) link CVE identifiers
   to AIOS package identifiers, each with a typed remediation status
   (Open → UnderReview → Patched | Quarantined | NotApplicable). A 4-tier
   enforcement ladder (`cvss < 4` → MonitorOnly, `< 7` → OperatorNotify,
   `< 9` → QuarantineCandidate, `≥ 9` → AutoQuarantine) derives concrete
   action from severity.

5. **System Composition Graph** (§ 2 I5, § 3): a `ServiceComposition` DAG
   records every service in the stack, its inter-service dependencies, and a
   verified topological order. Cyclic dependencies are a hard error
   (`CompositionCycleDetected`). Missing dependencies are a hard error
   (`ComposedServiceMissing`). The topological order is computed by Kahn's
   algorithm at admission time, not at construction time.

6. **External Bridge Contracts** (§ 2 I7, § 3): five external package
   ecosystems (Flathub, OCI registries, apt repositories, dnf repositories,
   pacman repositories) are bridged via typed `BridgeContract` values, each
   carrying a `VendorIntegrationContract`, manifest translation rules, and
   capability extraction rules.

7. **Control Map + Drift Detection** (§ 2 I7, § 3): every AIOS
   constitutional invariant is mapped to a set of external control framework
   references (NIST 800-53 CM-8, CIS Controls v8 Control 1, etc.) via
   `ControlMapping` entries. `ComplianceBaseline` snapshots capture the
   mapping state at a point in time. `ControlDriftReport` detects differences
   between two snapshots (added, removed, modified, unchanged).

8. **Orchestrator Binary** (§ 2 I6, § 3): the `aios-system` binary
   (`crates/aios-integration/src/bin/aios_system.rs`) consumes the
   composition graph, produces a deterministic topological boot order, and
   exposes five clap subcommands (`boot`, `health`, `validate`,
   `evidence-chain`, `catalogue`) for operational control and
   compliance audit.

9. **IntegrationService gRPC** (§ 4): a ~22-RPC gRPC surface
   (`aios.integration.v1alpha1.IntegrationService`) exposes vendor
   admission/lookup/list/revoke/transition, standard subscribe/revise/status/
   list/unsubscribe, CVE ingest/bind/unbind/list/enforce, bridge register/
   list, composition validate/order, orchestrator boot/health/validate,
   control snapshot/drift, and evidence chain verification to L9 observability
   and external auditors.

10. **Evidence Bridge** (§ 5): eight `IntegrationRecordType` discriminators
    map integration-layer lifecycle events into S3.1 evidence log entries
    via a shared `InMemoryIntegrationEvidenceEmitter`. Every vendor proposal,
    standard update, CVE binding, lifecycle transition, vendor revocation,
    bridge admission, baseline snapshot, and control drift event produces
    a BLAKE3-chained, Ed25519-signed evidence receipt with deterministic
    retention classification (Standard24M or Forever).

11. **Unified Record Catalogue** (§ 5): a `UnifiedRecordCatalogue` indexes
    every `RecordType` the AIOS evidence system can emit (currently 64+
    entries covering action lifecycle, policy decisions, vault operations,
    FS object mutations, sandbox events, network posture changes, hardware
    trust attestations, and integration lifecycle events), keyed by wire
    name with ownership metadata (`RecordTypeOwnership`) tracing each entry
    back to its defining crate and sub-spec.

These 11 pillars transform the 17 independent crates into a single auditable
system: every third-party integration, every compliance subscription, every
CVE, every bridge, every service dependency, every boot order, and every
evidence chain is typed, signed, verified, and recorded.

---

## § 2. Core Invariants

These 8 invariants define the constitutional contracts of the integration
layer. Each invariant is enforced by at least one verification gate in
`tests/m18_closure.rs` and at least one acceptance test in
`tests/m18_acceptance.rs`.

### I1 — Lifecycle FSM

Every integration resource (vendor contract, standard subscription, CVE
binding, bridge contract, composed system) MUST obey the 6-state lifecycle
FSM defined in `crates/aios-integration/src/lifecycle.rs`. The FSM has
exactly 6 states:

```
Proposed → Evaluated → Piloted → Production → Deprecated → Retired
```

Legal transitions (enforced by `is_transition_allowed` in
`vendor_registry.rs`):

| From       | To         | Guard                                 |
| ---------- | ---------- | ------------------------------------- |
| Proposed   | Evaluated  | Always allowed                        |
| Proposed   | Retired    | Rejection shortcut; `reason` required |
| Evaluated  | Piloted    | `security_audit_passed == true`       |
| Evaluated  | Retired    | Audit-failed rejection                |
| Piloted    | Production | Always allowed                        |
| Piloted    | Deprecated | Always allowed                        |
| Piloted    | Retired    | Pilot-abandoned rejection             |
| Production | Deprecated | Always allowed                        |
| Production | Retired    | Direct retirement with `reason`       |
| Deprecated | Retired    | Always allowed                        |

Illegal transitions (e.g. Proposed → Production, Evaluated → Production,
Piloted → Proposed) SHALL be rejected with
`IntegrationErrorCode::LifecycleInvalidTransition`. Retired is a strict
terminal state — no transition out of Retired is ever permissible.

### I2 — Signed Vendor Contracts

Every `VendorIntegrationContract` admitted into the registry MUST carry an
Ed25519 signature over the canonical bytes of the contract metadata:

```
contract_id\n
vendor_name\n
vendor_kind_label\n
trust_class_label\n
contact_canonical_id\n
rotation_cadence_days\n
breach_playbook_url
```

The canonical byte sequence is defined in
`vendor_registry::canonical_contract_bytes`. Signature verification uses
`VerifyingKey::verify_strict` (ed25519-dalek 2.1 API). The signature
material and `signer_fingerprint` are NOT emitted in evidence payloads
(INV-015).

Admission gates (in order):

1. `trust_class == BlacklistedDoNotAdmit` → `VendorBlacklisted`
2. `vendor_name` in blacklist set → `VendorBlacklisted`
3. `signer_fingerprint` not a registered authority → `VendorContractSignatureInvalid`
4. `Signature::from_slice` fails → `VendorContractSignatureInvalid`
5. `verify_strict` returns Err → `VendorContractSignatureInvalid`
6. `contract_id` already admitted → `VendorContractSignatureInvalid`
7. Insert contract + set lifecycle to `Proposed` → success

Blacklisted contracts are permanently inadmissible. The blacklist is an
in-memory `HashSet<String>` of vendor names, modifiable via
`add_to_blacklist` / `remove_from_blacklist` with a mutable reference
to the registry (operator-action pattern, not a runtime automated gate).

### I3 — Standards Subscription Discipline

Every `StandardSubscription` MUST have:

- A timed review window (`next_review_due_at`) defaulting to 90 days from
  the most recent revision.
- A pinned responsible canonical identity (`responsible_canonical_id`).
- A revision-versioned catalog URL.

The subscription status is computed relative to `now`:

- `now <= next_review_due_at` → `SubscriptionStatus::Current { until }`
- `next_review_due_at < now <= next_review_due_at + 30 days` → `SubscriptionStatus::ReviewDue { since }`
- `now > next_review_due_at + 30 days` → `SubscriptionStatus::Expired { expired_at }`

Expired subscriptions do NOT automatically block any subsystem — the
degradation policy is defined per-consuming-subsystem, not in this layer.
The `list_expired` and `list_due_for_review` query methods provide the
signal; consumers decide the response.

### I4 — CVE Feed Framework

CVE severity is typed as an ordered closed enum with the natural ordering
`Low < Medium < High < Critical`. The `cvss_to_enforcement` function maps
CVSS v3 base scores to a 4-tier enforcement level:

| CVSS Score    | Enforcement Level   |
| ------------- | ------------------- |
| `< 4.0`       | MonitorOnly         |
| `4.0 ≤ < 7.0` | OperatorNotify      |
| `7.0 ≤ < 9.0` | QuarantineCandidate |
| `≥ 9.0`       | AutoQuarantine      |

CVE remediation status is a closed 5-variant enum: `Open → UnderReview`,
then either `UnderReview → Patched`, `UnderReview → Quarantined`, or
`Open → NotApplicable`.

CVE identifiers MUST match the pattern `CVE-YYYY-N+` (4-digit year, at
least 4-digit numeric suffix). The `is_valid_cve_id` function validates
this without regex, using only `str` primitives.

The `CveFeedShape` rejects invalid CVE ids and out-of-range CVSS scores
(0.0..=10.0) at ingestion time with `ConfigInvalid`. Every successful
`bind_to_package` call emits a `PACKAGE_HAS_KNOWN_CVE` evidence record.

### I5 — System Composition Graph

The `ServiceComposition` struct defines a directed acyclic graph of
`ComposedService` nodes and `ServiceDependency` edges. Every service
carries a unique `service_id`, a `crate_name`, a `binding_endpoint`
(URI), and a `depends_on` list of service ids.

The `CompositionEngine::validate` method performs two ordered checks:

1. For every `ServiceDependency`, both `from_service` and `to_service`
   MUST exist in `services`. Missing endpoint →
   `IntegrationError::ComposedServiceMissing`.
2. Kahn's topological sort is executed on the dependency graph. If fewer
   than `n` services are sorted, a directed cycle exists →
   `IntegrationError::CompositionCycleDetected` with one cycle path.

The topological order is stored in `ServiceComposition::topological_order`
as a `Vec<String>`. It is computed at validation time, not at construction
time — the `ServiceComposition` struct carries an empty or stale order
that `validate` overwrites.

### I6 — Orchestrator Binary

The `aios-system` orchestrator binary (`crates/aios-integration/src/bin/
aios_system.rs`) consumes the `Orchestrator` struct which wraps a
`ServiceComposition` and a `CompositionEngine`. The orchestrator exposes
five clap subcommands:

- `boot`: prints the deterministic topological boot order.
- `health`: prints a per-service health summary with `ServiceScaffoldStatus`.
- `validate <composition>`: validates an external composition JSON file.
- `evidence-chain <path>`: verifies the hash-chain integrity of an evidence log.
- `catalogue`: prints the unified record catalogue index.

The default composition (17 crates) is known-acyclic by construction.
`Orchestrator::from_default_composition()` panics (via `expect`) if
the default composition fails — this is a programmer error, not a
runtime condition.

### I7 — Evidence Emission + Unified Record Catalogue

Every integration-layer lifecycle event produces a BLAKE3-chained,
Ed25519-signed evidence receipt emitted through the
`IntegrationEvidenceEmitter` trait. Eight event types are defined
(`IntegrationRecordType`):

| Discriminator                        | Evidence RecordType             | Retention   |
| ------------------------------------ | ------------------------------- | ----------- |
| `INTEGRATION_PROPOSED`               | `StatusTransition`              | Standard24M |
| `STANDARD_UPDATE_AVAILABLE`          | `PolicyDecision`                | Standard24M |
| `PACKAGE_HAS_KNOWN_CVE`              | `FailureObserved`               | Standard24M |
| `INTEGRATION_LIFECYCLE_TRANSITIONED` | `StatusTransition`              | Standard24M |
| `VENDOR_CONTRACT_REVOKED`            | `StatusTransition`              | Forever     |
| `BRIDGE_ADMITTED`                    | `ExternalBridgePackageAdmitted` | Standard24M |
| `COMPLIANCE_BASELINE_SNAPSHOT`       | `ChainCheckpoint`               | Forever     |
| `CONTROL_MAP_DRIFT_DETECTED`         | `StatusTransition`              | Forever     |

INV-015: NO raw signature bytes, NO raw CVE feed payloads beyond the record
summary, and NO private key material are ever emitted in evidence payloads.
The `emit_integration_proposed` helper explicitly excludes the `signature`
field; `emit_package_has_known_cve` includes only the binding summary fields.

The `UnifiedRecordCatalogue` is pre-populated via `default_index_entries()`
which returns 64+ `CatalogueEntry` values, each keyed by wire name with
`RecordTypeOwnership` metadata tracing the entry to its defining crate and
sub-spec. The catalogue is read-only after construction.

### I8 — Cross-Crate Composition Integrity

The `default_aios_composition()` function returns a `ServiceComposition`
with exactly 17 services in a fixed canonical order:

```
aios-action → aios-evidence → aios-policy → aios-capability-runtime →
aios-fs → aios-vault → aios-verification → aios-recovery → aios-sgr →
aios-cognitive → aios-sandbox → aios-apps → aios-renderer-cli →
aios-renderer-kde → aios-renderer-web → aios-network → aios-hardware
```

Each service depends on all services that precede it in the canonical list.
This is the "layered dependency" model — lower-numbered layers are
prerequisites for higher-numbered layers per the L0…L10 layer model.

The topological order computed by Kahn's algorithm on this DAG matches the
canonical order exactly. `Orchestrator::boot_order()` returns this
deterministic order at runtime.

---

## § 3. Typed Surfaces

This section documents the closed Rust types that implement the invariants
from § 2. Every type listed here is `REAL` (E3 evidence: compilation +
integration tests) in `crates/aios-integration/src/`.

### 3.1 IntegrationLifecycleState (6-state FSM)

```rust
pub enum IntegrationLifecycleState {
    Proposed   { proposer: String, proposed_at: DateTime<Utc> },
    Evaluated  { evaluator: String, evaluated_at: DateTime<Utc>,
                 security_audit_passed: bool },
    Piloted    { since: DateTime<Utc>, profile: String },
    Production { since: DateTime<Utc> },
    Deprecated { since: DateTime<Utc>, sunset_due: Option<DateTime<Utc>> },
    Retired    { since: DateTime<Utc>, reason: String,
                 data_migration_completed: bool },
}
```

Each variant carries typed payload fields that downstream subsystems
(evidence emitter, audit log, renderer) can inspect without matching on
arbitrary string data. The companion `IntegrationLifecycleLabel` enum
provides a payload-free discriminant for transition-table dispatch.

Transition legality is enforced by `vendor_registry::is_transition_allowed`
which is a `const fn` for compile-time verification. The transition matrix
is the single source of truth — if a transition is not listed in the matrix
from § 2 I1, it is illegal.

### 3.2 VendorIntegrationContract

```rust
pub struct VendorIntegrationContract {
    pub contract_id: VendorContractId,
    pub vendor_name: String,
    pub vendor_kind: VendorKind,           // 8-variant closed enum
    pub trust_class: VendorTrustClass,     // 4-variant including Blacklisted
    pub contact_canonical_id: String,
    pub rotation_cadence_days: u32,
    pub breach_playbook_url: String,
    pub signer_fingerprint: String,
    pub signature: Vec<u8>,                // Ed25519, 64 bytes
    pub admitted_at: DateTime<Utc>,
}
```

`VendorKind` (8 variants): `PackageRepository`, `ApplicationStore`,
`OciRegistry`, `CveFeed`, `ComplianceProvider`, `MetricsExporter`,
`IdentityProvider`, `OtherCertified`.

`VendorTrustClass` (4 variants): `AiosCertifiedPartner`,
`CommunityVerified`, `OperatorAuthorised`, `BlacklistedDoNotAdmit`.

The `VendorContractId` is an opaque newtype over `String` with the
prefix convention `vc_<ULID>`.

### 3.3 StandardSubscription + StandardKind

```rust
pub struct StandardSubscription {
    pub subscription_id: StandardSubscriptionId,
    pub standard: StandardKind,            // 11-variant closed enum
    pub catalog_url: String,
    pub current_revision: String,
    pub last_reviewed_at: DateTime<Utc>,
    pub next_review_due_at: DateTime<Utc>,
    pub responsible_canonical_id: String,
}
```

`StandardKind` (11 variants): `Nist80053Rev5`, `NistSp800218Ssdf`,
`NistSp800207ZeroTrust`, `NistSp800193Firmware`, `DisaStig`,
`CisControlsV8`, `Fips1403`, `Gdpr`, `Hipaa`, `Iso27001`, `Soc2`.

`StandardSubscriptionId` is an opaque newtype with prefix `ss_<ULID>`.
The `standard_kind_to_canonical_url` const fn maps each variant to its
public catalog URL for automated revision checking (future M19+).

### 3.4 CveRecord, PackageCveBinding, CveEnforcementLevel

```rust
pub struct CveRecord {
    pub cve_id: CveId,
    pub published_at: DateTime<Utc>,
    pub last_modified_at: DateTime<Utc>,
    pub cvss_v3_score: f32,                // 0.0..=10.0
    pub severity: CveSeverity,             // Low | Medium | High | Critical
    pub summary: String,
    pub affected_cpe_uris: Vec<String>,
}

pub struct PackageCveBinding {
    pub binding_id: String,
    pub cve_id: CveId,
    pub package_id: String,
    pub status: CveStatus,                 // Open|UnderReview|Patched|Quarantined|NotApplicable
    pub bound_at: DateTime<Utc>,
    pub matched_via_cpe: Option<String>,
    pub mitigated_by: Option<String>,
}

pub enum CveEnforcementLevel {
    MonitorOnly,          // CVSS < 4.0
    OperatorNotify,       // 4.0 ≤ CVSS < 7.0
    QuarantineCandidate,  // 7.0 ≤ CVSS < 9.0
    AutoQuarantine,       // CVSS ≥ 9.0
}
```

`CveSeverity` derives `PartialOrd` and `Ord` so that `Low < Medium < High <
Critical` is enforced by the Rust type system. `CveStatus` is a closed
5-variant enum without ordering.

The `cvss_to_enforcement` function is `const fn` — enforcement level
derivation is a pure computation with no runtime overhead.

### 3.5 BridgeContract + ExternalBridgeRegistry

```rust
pub struct BridgeContract {
    pub bridge_id: String,
    pub kind: BridgeKind,                  // Flathub|Oci|Apt|Dnf|Pacman
    pub vendor_contract: VendorIntegrationContract,
    pub translation_rules: ManifestTranslationRules,
    pub capability_extractors: Vec<CapabilityExtractorRule>,
    pub admitted_at: DateTime<Utc>,
}
```

Five default bridge contracts are provided as `const`-compatible
constructors: `default_flathub_contract()`, `default_oci_contract()`,
`default_apt_contract()`, `default_dnf_contract()`,
`default_pacman_contract()`. Each is a prefabricated `BridgeContract`
with a stub `VendorIntegrationContract` — the vendor signature is empty
and the signer fingerprint is `"<default-contract-no-signature>"`.

### 3.6 ControlMapping + ComplianceBaseline

```rust
pub struct ControlMapping {
    pub mapping_id: String,
    pub aios_invariant: AiosInvariant,     // ref + description
    pub framework_refs: Vec<ControlFrameworkRef>,
    pub last_updated_at: DateTime<Utc>,
    pub is_automatically_verifiable: bool,
}

pub struct ComplianceBaseline {
    pub baseline_id: String,
    pub aios_version: String,
    pub mappings: Vec<ControlMapping>,
    pub snapshot_at: DateTime<Utc>,
    pub validator_canonical_id: String,
}

pub struct ControlDriftReport {
    pub prior_baseline_id: String,
    pub added: Vec<ControlMapping>,
    pub removed: Vec<ControlMapping>,
    pub modified: Vec<ControlMapping>,
    pub unchanged_count: usize,
}
```

`ControlFrameworkRef` carries a `framework: StandardKind` and a
`control_id: String` (e.g. `"CM-8"`, `"Control 1"`).

### 3.7 ServiceComposition + ComposedService

```rust
pub struct ServiceComposition {
    pub composition_id: ComposedSystemId,
    pub services: Vec<ComposedService>,
    pub dependencies: Vec<ServiceDependency>,
    pub topological_order: Vec<String>,
}

pub struct ComposedService {
    pub service_id: String,                // e.g. "aios-policy"
    pub crate_name: String,                // e.g. "aios-policy"
    pub binding_endpoint: String,          // e.g. "unix:/run/aios/policy.sock"
    pub depends_on: Vec<String>,           // service_ids
}

pub struct ServiceDependency {
    pub from_service: String,              // depender
    pub to_service: String,                // dependee (prerequisite)
    pub required: bool,                    // hard dependency?
}
```

### 3.8 ServiceHealthSummary

```rust
pub struct ServiceHealthSummary {
    pub service_id: String,
    pub crate_name: String,
    pub status: ServiceScaffoldStatus,     // ScaffoldReady|NotInComposition|ConfigMissing
    pub topological_index: usize,          // 0-based position in boot order
}
```

Currently all services report `ScaffoldReady` — real health probes (gRPC
health check, process liveness, socket reachability) are deferred to M19+
when the orchestrator binary spawns actual service processes.

---

## § 4. IntegrationService gRPC Surface

The gRPC surface is defined in `proto/aios.integration.v1alpha1.proto`
and implemented by `crates/aios-integration/src/service/server.rs`.
The service wraps `Arc<VendorIntegrationRegistry>`, `Arc<ExternalStandardRegistry>`,
`Arc<CveFeedShape>`, `Arc<ExternalBridgeRegistry>`, `Arc<ControlMapRegistry>`,
`Arc<CompositionEngine>`, `Arc<Orchestrator>`, `Arc<InMemoryIntegrationEvidenceEmitter>`,
and `Arc<UnifiedRecordCatalogue>`.

### 4.1 RPC Catalogue (~22 RPCs)

**Vendor Management (6 RPCs):**

- `AdmitVendor(VendorIntegrationContract) → AdmitVendorResponse`
- `GetVendor(VendorContractId) → VendorIntegrationContract`
- `ListVendors(ListVendorsRequest) → ListVendorsResponse`
- `RevokeVendor(VendorContractId) → RevokeVendorResponse`
- `TransitionVendor(VendorContractId, IntegrationLifecycleLabel) → TransitionVendorResponse`
- `AddToBlacklist(vendor_name) → AddToBlacklistResponse`

**Standards Subscription (5 RPCs):**

- `SubscribeStandard(StandardSubscription) → SubscribeStandardResponse`
- `ReviseStandard(StandardSubscriptionId, new_revision, reviewer, note) → ReviseStandardResponse`
- `GetStandardStatus(StandardSubscriptionId, now) → GetStandardStatusResponse`
- `ListStandards(ListStandardsRequest) → ListStandardsResponse`
- `UnsubscribeStandard(StandardSubscriptionId) → UnsubscribeStandardResponse`

**CVE Feed (5 RPCs):**

- `IngestCve(CveRecord) → IngestCveResponse`
- `BindCve(PackageCveBinding) → BindCveResponse`
- `UnbindCve(binding_id) → UnbindCveResponse`
- `ListCves(ListCvesRequest) → ListCvesResponse`
- `GetEnforcementLevel(CveId) → GetEnforcementLevelResponse`

**Bridge Management (2 RPCs):**

- `RegisterBridge(BridgeContract) → RegisterBridgeResponse`
- `ListBridges(ListBridgesRequest) → ListBridgesResponse`

**Composition + Orchestration (2 RPCs):**

- `ValidateComposition(ServiceComposition) → ValidateCompositionResponse`
- `BootOrder() → BootOrderResponse`

**Control Map (2 RPCs):**

- `SnapshotBaseline(aios_version, validator_canonical_id) → SnapshotBaselineResponse`
- `DetectDrift(prior_baseline_id) → DetectDriftResponse`

**Evidence (1 RPC):**

- `VerifyEvidenceChain() → VerifyEvidenceChainResponse`

Total: **22 RPCs** spanning all 9 integration subsystems.

### 4.2 Status Code Mapping

`IntegrationError` → `tonic::Status` mapping:

| IntegrationError               | tonic::Code        |
| ------------------------------ | ------------------ |
| LifecycleInvalidTransition     | FailedPrecondition |
| VendorContractSignatureInvalid | PermissionDenied   |
| VendorBlacklisted              | PermissionDenied   |
| StandardSubscriptionExpired    | FailedPrecondition |
| CveFeedUnreachable             | Unavailable        |
| CompositionCycleDetected       | FailedPrecondition |
| ComposedServiceMissing         | NotFound           |
| OrchestratorBootFailed         | Internal           |
| ConfigInvalid                  | InvalidArgument    |
| Internal                       | Internal           |

### 4.3 Service Implementation

The `IntegrationService` tonic server is implemented as a struct holding
`Arc` handles to each subsystem. Each RPC handler:

1. Acquires read or write locks as needed.
2. Delegates to the subsystem's async methods.
3. Maps `IntegrationError` to `tonic::Status` via the conversion table.
4. Returns the protobuf response type.

The server is tested via in-process tonic tests (`tests/grpc_integration.rs`)
that bind to `127.0.0.1:0`, drive requests through a `TcpListener`, and
assert response shapes.

---

## § 5. Evidence Record Types

### 5.1 IntegrationRecordType (8 discriminators)

The closed `IntegrationRecordType` enum defines 8 lifecycle event
discriminators, each mapping to an `aios_evidence::RecordType` variant
and a retention class:

| #   | Discriminator                      | Evidence RecordType                      | Retention   |
| --- | ---------------------------------- | ---------------------------------------- | ----------- |
| 1   | `IntegrationProposed`              | `StatusTransition` (ID 116)              | Standard24M |
| 2   | `StandardUpdateAvailable`          | `PolicyDecision` (ID 4)                  | Standard24M |
| 3   | `PackageHasKnownCve`               | `FailureObserved` (ID 130)               | Standard24M |
| 4   | `IntegrationLifecycleTransitioned` | `StatusTransition` (ID 116)              | Standard24M |
| 5   | `VendorContractRevoked`            | `StatusTransition` (ID 116)              | Forever     |
| 6   | `BridgeAdmitted`                   | `ExternalBridgePackageAdmitted` (ID 421) | Standard24M |
| 7   | `ComplianceBaselineSnapshot`       | `ChainCheckpoint` (ID 420)               | Forever     |
| 8   | `ControlMapDriftDetected`          | `StatusTransition` (ID 116)              | Forever     |

### 5.2 Emission Pattern

Every evidence-emitting operation follows this pattern:

1. Acquire subsystem locks to extract the payload data.
2. Construct a `serde_json::Value` payload with INV-015-compliant fields.
3. Call `seal_and_append(record_type, payload)` which:
   a. Creates a `ReceiptBuilder` with the mapped `RecordType`, `retention_class`, and subject.
   b. Calls `builder.seal(prev)` to compute the BLAKE3 content hash and link hash.
   c. Appends the receipt to the `ReceiptChain`.
4. Returns an `EvidenceReceipt` with `record_id`, `hash`, and `sequence`.

### 5.3 Unified Record Catalogue

The `UnifiedRecordCatalogue` is pre-populated with 64+ `CatalogueEntry` values
via `default_index_entries()`. Each entry carries:

- `wire_name: &'static str` (e.g. `"ACTION_RECEIVED"`)
- `wire_id: u32` (e.g. 1)
- `category: &'static str` (e.g. `"action_lifecycle"`)
- `ownership: RecordTypeOwnership` (crate, sub_spec, retention_class)

The catalogue is read-only after construction. It serves as the canonical
index for evidence log consumers (L9 observability, external auditors,
renderer UIs) to discover the full set of record types the system can emit.

---

## § 6. SystemComposition — Default 17-Crate Wiring

### 6.1 Canonical Service Order

The `default_aios_composition()` function constructs a `ServiceComposition`
with exactly 17 services in this dependency order:

```
 0: aios-action              (L3/L0 cross-cutting)
 1: aios-evidence            (L9)
 2: aios-policy              (L4)
 3: aios-capability-runtime  (L3)
 4: aios-fs                  (L2)
 5: aios-vault               (L4)
 6: aios-verification        (L9)
 7: aios-recovery            (L1)
 8: aios-sgr                 (L3)
 9: aios-cognitive           (L5)
10: aios-sandbox             (L6)
11: aios-apps                (L6)
12: aios-renderer-cli        (L7)
13: aios-renderer-kde        (L7)
14: aios-renderer-web        (L7)
15: aios-network             (L8)
16: aios-hardware            (L8)
```

### 6.2 Dependency Edges

Each service depends on all services that precede it in the canonical order.
This produces `16 + 15 + ... + 1 = 136` dependency edges, all of the form:

```rust
ServiceDependency {
    from_service: svc[i].service_id,
    to_service: svc[j].service_id,  // j < i
    required: true,
}
```

### 6.3 Topological Order Verification

The `CompositionEngine::validate` method computes the topological order via
Kahn's algorithm:

1. Build adjacency list and in-degree map from the dependency edges.
2. Semantically, `ServiceDependency { from, to }` means "from depends on to"
   — the dependee must come first in the topological order.
3. Start BFS from nodes with `in_degree == 0`.
4. If `sorted.len() < n`, a cycle exists — extract one cycle path from the
   remaining subgraph by following predecessors backwards.

Kahn's algorithm runs in O(V + E) time, where V = 17 and E = 136 for the
default composition. The validated topological order matches the canonical
order exactly.

### 6.4 Binding Endpoints

Each service in the default composition is assigned a Unix domain socket
binding endpoint of the form `unix:/run/aios/{service_id}.sock`. This is
a forward-looking convention — the orchestrator binary does not yet spawn
actual service processes (M19+), but the endpoint schema is defined here
so that the composition graph is complete and verifiable.

---

## § 7. Adversarial Robustness

### 7.1 Signed Contract Forgery

**Attack:** An adversary crafts a `VendorIntegrationContract` with a
forged Ed25519 signature.

**Defense:** The `admit_contract` method verifies the signature using
`VerifyingKey::verify_strict` over the canonical byte sequence. The
`canonical_contract_bytes` function produces a deterministic byte
sequence that includes every field the signer committed to. Any
modification to any field — vendor name, trust class, rotation cadence,
breach playbook URL — changes the canonical bytes and invalidates the
signature. The verification is atomic with contract insertion under a
single write lock.

**Test coverage:** `m18_closure.rs` INV reachability test:
`VendorIntegrationRegistry rejects invalid signature`.

### 7.2 Standards Subscription Expiry Attack

**Attack:** An operator neglects to review a standard subscription, causing
it to silently drift past its review window and grace period.

**Defense:** The `ExternalStandardRegistry` provides `list_due_for_review`
and `list_expired` query methods that any consumer (scheduler, operator
dashboard, compliance scanner) can poll. The 30-day grace window is
hardcoded in `status()` and cannot be disabled. The `STANDARD_UPDATE_AVAILABLE`
evidence record is emitted on every `revise` call, creating an audit trail.

**Test coverage:** `m18_closure.rs` INV reachability test:
`ExposureExternalStandard expires after grace period`.

### 7.3 CVE Feed Poisoning

**Attack:** An upstream CVE feed delivers a record with an invalid CVE id
format (`CVE-XXXX-YYYYY`) or an out-of-range CVSS score (11.0, -1.0).

**Defense:** `CveFeedShape::ingest_record` validates:

- `0.0 <= cvss_v3_score <= 10.0` (returns `ConfigInvalid` on violation).
- `is_valid_cve_id(&record.cve_id.0)` (returns `ConfigInvalid` on violation).

`bind_to_package` requires the CVE record to already be ingested (returns
`Internal("unknown CVE id")` on missing CVE), preventing orphan bindings.

**Test coverage:** `m18_closure.rs` INV reachability test:
`CveFeedShape rejects invalid CVE id format`.

### 7.4 Composition Cycle Injection

**Attack:** An adversary crafts a `ServiceComposition` with a directed
cycle in the dependency graph, attempting to create an unresolvable
boot order.

**Defense:** `CompositionEngine::validate` runs Kahn's topological sort.
If fewer than `n` services are sorted, a cycle is detected and a
`CompositionCycleDetected` error is returned with one cycle path.
The orchestrator binary refuses to boot from a cyclic composition.

**Test coverage:** `m18_closure.rs` INV reachability test:
`CompositionEngine rejects cyclic graph`.

---

## § 8. Cross-Spec Follow-Ups (Rev.3)

### 8.1 CVE Feed Live Wiring → Rev.3 Category 7

The current `CveFeedShape` is an in-memory store with push-based ingestion
(`ingest_record`). Rev.3 should add:

- A pull-based CVE feed poller that queries NVD API 2.0 and GitHub Advisory
  Database on a configurable interval (default: 6 hours).
- An `osv.dev` schema adapter for Google OSV-format feeds.
- A `CvePollerBackend` async trait with `InMemoryPoller` and
  `RocksDbPoller` implementations.
- Differential ingestion: only store records whose `last_modified_at` is
  newer than the stored version.

### 8.2 SIEM Bridges → Rev.3 Category 7

The current bridge contracts cover package ecosystems (Flathub, OCI, apt,
dnf, pacman). Rev.3 should add:

- A `SplunkHecBridge` implementing HEC (HTTP Event Collector) protocol.
- An `ElasticsearchBridge` for ELK stack integration.
- A `GenericSyslogBridge` for RFC 5424 structured syslog emission.
- Each bridge carries its own `VendorIntegrationContract` and is admitted
  through the same vendor registry gates.

### 8.3 STIG Control-Map Automation → Rev.3 Category 7

The current `ControlMapRegistry` is populated manually via `add_mapping`.
Rev.3 should add:

- A `StigXmlParser` that ingests DISA STIG XCCDF XML checklists.
- Automated `ControlMapping` generation from STIG rule → `AiosInvariant`
  crosswalk tables.
- A `ComplianceScanner` that walks the control map, checks each invariant
  against the current system state, and produces a `ComplianceReport`.

---

## § 9. Worked Examples

### 9.1 Example 1: Admit a Flathub Vendor Contract

```rust
use aios_integration::*;
use ed25519_dalek::{SigningKey, VerifyingKey};
use rand_core::OsRng;

// 1. Generate a keypair for the vendor authority.
let mut csprng = OsRng;
let signing_key = SigningKey::generate(&mut csprng);
let verifying_key = signing_key.verifying_key();

// 2. Build a registry and register the authority.
let mut registry = VendorIntegrationRegistry::new();
registry.register_authority("flathub-2026", verifying_key);

// 3. Build and sign a vendor contract.
let contract = VendorIntegrationContract {
    contract_id: VendorContractId("vc_01J...".into()),
    vendor_name: "Flathub".into(),
    vendor_kind: VendorKind::ApplicationStore,
    trust_class: VendorTrustClass::CommunityVerified,
    contact_canonical_id: "human:flathub-ops".into(),
    rotation_cadence_days: 90,
    breach_playbook_url: "https://docs.flathub.org/security".into(),
    signer_fingerprint: "flathub-2026".into(),
    signature: /* Ed25519 signature over canonical bytes */,
    admitted_at: Utc::now(),
};

// 4. Admit the contract.
registry.admit_contract(contract).await.unwrap();
// State is now Proposed; evidence record INTEGRATION_PROPOSED emitted.
```

### 9.2 Example 2: Subscribe to NIST 800-53 and Track Drift

```rust
// 1. Subscribe to NIST 800-53 Rev.5.
let reg = ExternalStandardRegistry::new();
reg.subscribe(StandardSubscription {
    subscription_id: StandardSubscriptionId("ss_01J...".into()),
    standard: StandardKind::Nist80053Rev5,
    catalog_url: standard_kind_to_canonical_url(StandardKind::Nist80053Rev5).into(),
    current_revision: "Rev.5 Update 1".into(),
    last_reviewed_at: Utc::now(),
    next_review_due_at: Utc::now() + chrono::Duration::days(90),
    responsible_canonical_id: "human:compliance-officer".into(),
}).await.unwrap();

// 2. Build a control map.
let mut cm = ControlMapRegistry::new();
cm.add_mapping(ControlMapping {
    mapping_id: "cm_001".into(),
    aios_invariant: AiosInvariant {
        ref_id: "INV-001".into(),
        description: "Evidence is append-only".into(),
    },
    framework_refs: vec![ControlFrameworkRef {
        framework: StandardKind::Nist80053Rev5,
        control_id: "AU-3".into(),
    }],
    last_updated_at: Utc::now(),
    is_automatically_verifiable: true,
}).await.unwrap();

// 3. Snapshot baseline.
let baseline = cm.snapshot("aios/0.1.0", "human:compliance-officer").await.unwrap();

// 4. Mutate the map, then detect drift.
cm.add_mapping(/* new mapping */).await.unwrap();
let drift = cm.detect_drift(&baseline.baseline_id).await.unwrap();
assert!(drift.added.len() == 1);
assert!(drift.unchanged_count == 1);
// Evidence records: COMPLIANCE_BASELINE_SNAPSHOT + CONTROL_MAP_DRIFT_DETECTED.
```

### 9.3 Example 3: 17-Crate Orchestrator Boot

```rust
// 1. Build the orchestrator from the default composition.
let orch = Orchestrator::from_default_composition().unwrap();

// 2. Get the deterministic boot order.
let order = orch.boot_order().await;
assert_eq!(order.len(), 17);
assert_eq!(order[0], "aios-action");
assert_eq!(order[16], "aios-hardware");

// 3. Validate a custom external composition.
let external = ServiceComposition { /* ... */ };
let validated_order = orch.validate_external_composition(&external).await.unwrap();

// 4. Check health summaries.
let summaries = orch.health_summary().await;
assert_eq!(summaries.len(), 17);
for s in &summaries {
    assert_eq!(s.status, ServiceScaffoldStatus::ScaffoldReady);
}
```

---

## § 10. Performance Budgets

| Operation                                   | Budget | Rationale                                                    |
| ------------------------------------------- | ------ | ------------------------------------------------------------ |
| `CompositionEngine::validate` (17-crate)    | < 50ms | Kahn's algorithm O(V+E), V=17, E=136. Should complete in μs. |
| `VendorIntegrationRegistry::admit_contract` | < 10ms | One Ed25519 verify + one HashMap insert.                     |
| `ExternalStandardRegistry::status`          | < 1ms  | One HashMap lookup + two DateTime comparisons.               |
| `CveFeedShape::ingest_record`               | < 1ms  | Two range checks + one HashMap insert.                       |
| `CveFeedShape::bind_to_package`             | < 5ms  | Two HashMap inserts + optional evidence emission.            |
| `ControlMapRegistry::snapshot`              | < 10ms | Cloning Vec<ControlMapping> for current state.               |
| `ControlMapRegistry::detect_drift`          | < 10ms | O(n) comparison of old vs. new mapping sets.                 |
| `Orchestrator::boot_order`                  | < 1ms  | Returning a pre-computed `Vec<String>` clone.                |
| `IntegrationEvidenceEmitter` seal+append    | < 5ms  | BLAKE3 hash + ReceiptChain append.                           |
| `UnifiedRecordCatalogue` lookup             | < 1ms  | HashMap<&str, CatalogueEntry> lookup.                        |

All budgets are measured on the in-memory backends. Persistent backends
(RocksDB, PostgreSQL) will have different budgets defined in Rev.3.

---

## § 11. Trust Model Summary

The integration layer operates on a **trust-but-verify** model:

1. **Vendor trust is cryptographic, not reputational:** Every
   `VendorIntegrationContract` is Ed25519-signed. The signature is
   verified at admission time and never stored in evidence payloads.
   Trust classification (`AiosCertifiedPartner` vs. `CommunityVerified`)
   is a policy overlay, not a cryptographic guarantee.

2. **Compliance is time-bounded:** Every `StandardSubscription` has a
   review window and a grace period. Expired subscriptions do not
   block operations, but they signal degraded compliance posture to
   consuming subsystems.

3. **Vulnerability data is validated at the boundary:** CVE records are
   validated for CVSS score range and CVE id format at ingestion time.
   Invalid data is rejected at the edge, not silently stored.

4. **Composition integrity is structural:** The `ServiceComposition` DAG
   is verified for acyclicity and completeness at admission time. The
   orchestrator refuses to boot from an unverified composition.

5. **Evidence is append-only and signed:** Every integration lifecycle
   event produces a BLAKE3-chained, Ed25519-signed evidence receipt.
   The chain is verifiable end-to-end. Retention classification ensures
   that compliance-baseline snapshots, vendor revocations, and control
   drift events are preserved forever.

6. **The orchestrator is the single boot authority:** No AIOS subsystem
   is started outside the orchestrator's control once in production.
   The boot order is deterministic and dependency-respecting.

7. **Cross-crate composition is canonical:** The 17-crate default
   composition is known-acyclic by construction and serves as the
   reference topology for all integration tests, acceptance fixtures,
   and M19+ distribution packaging.

---

## § 12. M18 Closure — Evidence Summary

M18 opened with T-175 (typed skeleton + S11.4 stub) and closes with T-186
(full spec + acceptance fixtures + v0.1.0 bump). The 12-task sequence:

| Task  | Status     | Artifact                                        |
| ----- | ---------- | ----------------------------------------------- |
| T-175 | ✓ complete | Typed skeleton (FSM, enums, error catalogue)    |
| T-176 | ✓ complete | `VendorIntegrationRegistry` with Ed25519        |
| T-177 | ✓ complete | `ExternalStandardRegistry` with review windows  |
| T-178 | ✓ complete | `CveFeedShape` + `PackageCveBinding`            |
| T-179 | ✓ complete | 5 external bridge contracts                     |
| T-180 | ✓ complete | `ControlMapRegistry` + drift detection          |
| T-181 | ✓ complete | `CompositionEngine` topological sort            |
| T-182 | ✓ complete | `Orchestrator` + `aios-system` binary           |
| T-183 | ✓ complete | `IntegrationService` gRPC (~22 RPCs)            |
| T-184 | ✓ complete | 8 evidence record types + unified catalogue     |
| T-185 | ✓ complete | `SystemIntegrationHarness` (9-subsystem wiring) |
| T-186 | ✓ complete | S11.4 spec finalised + closure/acceptance tests |

**M18 output:** `aios-integration v0.1.0` crate with 302+ tests, 22 gRPC
RPCs, 8 evidence record types, 64+ unified record catalogue entries, 5
external bridge contracts, 11 standard subscriptions, 4-tier CVE
enforcement ladder, 17-crate composition DAG, 5-subcommand orchestrator
binary, and a 9-subsystem system integration harness for E2E testing.

**Next milestone:** M19 (`aios-distribution`) — S11.1 repository model +
publisher trust chain. Consumes M18's integration framework for publisher
trust transitions, external bridge contracts (Flathub / OCI / .deb / .rpm),
and CVE-aware install pipeline.

---

_Specification finalised 2026-05-28 by claude-ds (DeepSeek-V4-Pro).
Status: REAL. Evidence level: E3 (compilation + integration tests).
All 5 cargo gates green._
