# AIOS Rev.2 → ARCADIA modeling plan

How the AIOS markdown specification maps onto the four ARCADIA layers (Operational Analysis → System Analysis → Logical Architecture → Physical Architecture) and the cross-cutting traceability surfaces in Eclipse Capella 7.0.1.

This is the **planning document** the user follows when building the model interactively in the IDE. The extracted CSVs under `tools/capella/manifests/` provide the input data; Capella's import wizards + manual diagram authoring assemble the model.

## Layer 1 — Operational Analysis (OA)

**Goal:** capture what the system does for its stakeholders, independent of system structure.

### Operational Entities (4)

The stakeholder taxonomy comes from S5.1 (Identity Model) closed `SubjectKind` enum:

| Operational Entity | Maps to                            | Notes                                                                            |
| ------------------ | ---------------------------------- | -------------------------------------------------------------------------------- |
| `HUMAN_USER`       | S5.1 `SubjectKind::HUMAN_USER`     | The operator                                                                     |
| `AI_AGENT`         | S5.1 `SubjectKind::AI_AGENT`       | LLM-backed agent; constitutionally constrained per INV-002/010/013/016/018       |
| `SERVICE`          | S5.1 `SubjectKind::SERVICE`        | Constitutional system services (vault broker, evidence log, policy kernel, etc.) |
| `LOCAL_OPERATOR`   | S5.1 `SubjectKind::LOCAL_OPERATOR` | Recovery-console operator (physically present at first-boot)                     |

### Operational Capabilities (24)

The 24 invariants from `L0_Governance_Evidence_Safety/04_invariants.md` become Operational Capabilities. Each capability has:

- Description = INV statement
- Rationale = INV "Why" field
- Verifications link → INV "Verified by" sub-spec citation

**Import path:** `manifests/invariants.csv` → Capella `Project > Import > CSV` into Operational Capabilities folder.

### Operational Activities

The §22 MVP golden path activities:

1. Operator submits a typed action
2. Cognitive core proposes (AI subject) OR operator commits (human subject)
3. Policy kernel evaluates
4. Approval mechanics gate (if required)
5. Capability runtime dispatches via adapter
6. Adapter executes (sandboxed)
7. Verification grammar runs probes
8. Evidence log records full chain
9. Renderer surfaces result + chrome

### Operational Scenarios

Build 4 scenarios for live simulation:

| Scenario                      | Source                                    | What it exercises                  |
| ----------------------------- | ----------------------------------------- | ---------------------------------- |
| Golden path (happy)           | `XX_Cross_Cutting/03_mvp_golden_path.md`  | Lifecycle end-to-end               |
| AI install attempted (denied) | S2.3 §26.2.4 `AIInstallInitiationBlocked` | INV-002 + INV-013 enforcement      |
| First-boot provisioning       | S9.2 first-boot flow                      | Recovery-mode service subjects     |
| Tamper detected → recovery    | S3.1 §11.4 + S9.1 RecoveryEntryReason     | Constitutional anchor failure path |

## Layer 2 — System Analysis (SA)

**Goal:** define what the system must do, still mostly black-box.

### System Functions (~53)

Each sub-spec becomes one System Function (or a function group with sub-functions).

**Import path:** `manifests/sub_specs.csv` → Capella `System Functions` folder. Each row's `path` field links back to the markdown source for navigation.

System Functions are organised by their `layer` field — start with grouping by layer L0..L10 + XX (matches `manifests/layers.csv`).

### System Actors (external)

| Actor                                  | Sources from                                         |
| -------------------------------------- | ---------------------------------------------------- |
| External Browser                       | S7.5 web renderer + S6.5 session container streaming |
| Recovery Console                       | S9.1 + S9.2                                          |
| Hardware (TPM, GPU, etc.)              | S8.3 hardware graph + S8.2 GPU model                 |
| Network peers                          | S8.1 network policy                                  |
| External Apps (Wine/Waydroid runtimes) | S12.1 cross-ecosystem                                |
| Marketplace publisher                  | S11.2                                                |

### Functional Exchanges

Use the gRPC service surfaces declared in proto IDLs as functional exchanges:

| Service                                       | Exchange items                                 |
| --------------------------------------------- | ---------------------------------------------- |
| `aios.action.v1alpha1.ActionService`          | `ActionEnvelope` (S0.1)                        |
| `aios.evidence.v1alpha1.EvidenceLog`          | `EvidenceReceipt`, `SealedSegment` (S3.1)      |
| `aios.policy.v1alpha1.PolicyKernel`           | `PolicyDecision`, `ApprovalRequirement` (S2.3) |
| `aios.session.v1alpha1.SessionRuntimeAdapter` | `SessionContainerManifest` (S6.5)              |
| `aios.recovery.v1alpha1.*`                    | `RecoveryEntryReason`, `RecoveryStage` (S9.1)  |

## Layer 3 — Logical Architecture (LA)

**Goal:** how the system is internally structured (still implementation-neutral).

### Logical Components (12)

The 11 AIOS layers + XX cross-cutting from `manifests/layers.csv`. Each is a top-level Logical Component:

```
L0  Governance, Evidence, Safety
L1  Kernel, Bootstrap, Recovery
L2  AIOS-FS
L3  AIOS-SGR Service Graph Runtime
L4  Policy, Identity, Vault
L5  Cognitive Core
L6  Apps, Packages, Compatibility
L7  Interaction Renderers
L8  Network, Hardware, Devices
L9  Observability, Admin, Operations
L10 Distribution, Ecosystem, Marketplace
XX  Cross-Cutting (S0.1, S0.3, S0.4, ProxGuard)
```

### Logical Sub-components (~53)

Each sub-spec becomes a sub-component within its layer's Logical Component. Re-use `manifests/sub_specs.csv`.

### Logical Interfaces

Mirror the System Functional Exchanges from SA layer, refined with internal types. Each closed enum (RecordType, ActionPhase, RecoveryMode, etc.) becomes an Exchange Category.

### Logical Architecture Diagram — coverage check

After laying out the 12 Logical Components + ~53 sub-components, validate:

- [ ] Every sub-spec from `manifests/sub_specs.csv` has a Logical Sub-component
- [ ] Every `Consumes` edge from `manifests/trace_consumes.csv` is a Logical Interface
- [ ] No Logical Interface points from a higher-numbered to a lower-numbered layer (INV-007 visual check)

## Layer 4 — Physical Architecture (PA)

**Goal:** what actually runs (deployment shape).

### Physical Components (implementation crates)

Current implementation state (post-M2):

| Crate                     | Status        | Implements                              |
| ------------------------- | ------------- | --------------------------------------- |
| `aios-action`             | M1 closed     | S0.1 action envelope                    |
| `aios-evidence`           | M2 closing    | S3.1 evidence log + gRPC service        |
| `aios-policy`             | M3+ (planned) | S2.3 policy kernel                      |
| `aios-capability-runtime` | M3+ (planned) | S10.1 capability runtime                |
| `aios-fs`                 | M4+ (planned) | S1.3 AIOS-FS object model               |
| `aios-vault`              | M5+ (planned) | S5.2 vault broker                       |
| `aios-session`            | M6+ (planned) | S6.5 session container manager (Podman) |
| `aios-renderer-cli`       | M7+ (planned) | S7.6 CLI renderer                       |

Plus deployment-level components:

| Component                     | Hosts                       | Notes                                              |
| ----------------------------- | --------------------------- | -------------------------------------------------- |
| Podman                        | Session containers per S6.5 | Rootless, recovery-safe                            |
| KDE Plasma                    | KDE renderer per S7.4       | Native Wayland                                     |
| Browser                       | Web renderer per S7.5       | DOM + WebGPU + selkies-gstreamer streamed sessions |
| selkies-gstreamer             | KDE-in-browser streaming    | Per S6.5 design                                    |
| Linux kernel + recovery image | L1 substrate                | Generic kernel; recovery toolkit                   |

### Physical Interfaces

Concrete protocols:

- gRPC over TLS for service-to-service (S2.3, S3.1, S10.1, etc.)
- gRPC over loopback for in-process services (test/dev)
- WebSocket for browser ↔ selkies streaming
- Wayland for KDE compositor ↔ apps
- File-system / RocksDB for evidence log persistence (T-012)

## Cross-cutting: Traceability surfaces

### INV × Sub-spec matrix

Import `manifests/trace_inv_to_subspec.csv` as a traceability matrix:

- Rows: 24 INVs (Operational Capabilities)
- Columns: 53 sub-specs (System Functions)
- Cells: "cited" where the sub-spec mentions the INV

**Gap detection:**

- Empty INV row → no sub-spec cites that INV → either INV is dead-letter or enforcement story is missing
- Empty sub-spec column → that sub-spec doesn't cite any INV → either it's not constitutional or its INV citations are missing

### Consumes dependency graph

Import `manifests/trace_consumes.csv` as a graph view:

- Nodes: sub-specs
- Directed edges: consumer → producer

**Gap detection:**

- Sub-spec X consumes Y.foo but Y doesn't produce foo → dangling reference
- Cycle in the Consumes graph → architectural violation (INV-007 hint)

### RecordType emitter matrix

Build manually (extract.py does not capture emitter relationships yet — Wave N+1 enhancement):

- Rows: 427 RecordTypes
- Columns: sub-specs
- Cells: "emits" where sub-spec produces that record

**Gap detection:**

- Empty RecordType row → defined in S3.1 vocabulary but never emitted → vestigial enum entry

## Validation workflow

1. After every spec change: `python3 tools/capella/extract.py`
2. Refresh Capella model: `Project > Import > CSV` for each refreshed manifest
3. Re-render traceability matrices: Capella auto-updates
4. Scan for new gaps (empty rows/columns, dangling references, cycles)
5. Either fix the spec (preferred) or document the gap explicitly (rare exception)

## Acceptance for "AIOS Capella simulation complete"

- [ ] All 24 INVs imported as Operational Capabilities with statement + rationale
- [ ] All 53 sub-specs imported as System Functions, grouped by layer
- [ ] All 12 layers represented as Logical Components
- [ ] All 238 Consumes edges visualised as Logical Interfaces
- [ ] INV × sub-spec traceability matrix populated (331 cells)
- [ ] At least 4 Operational Scenarios authored (golden path + denied + first-boot + tamper)
- [ ] Architecture diagram per layer (12 total) with sub-spec components visible
- [ ] Layer-dependency view validates INV-007 (no upward `requires-for-correctness` arrows)
- [ ] Validation report exported: orphan INVs, dangling Consumes, orphan RecordTypes — fixed or documented
