# AIOS Capella model — gap report

Source model: `tools/capella/output/aios-rev2/`

## Summary

- Total capabilities: 77
  - Invariants (OA): 24
  - Sub-specs (SA/LA/PA): 53
- Consumes edges: 238
- INV realization links: 331

## Gaps detected

| Gap category | Count |
| --- | ---: |
| Orphan INVs (zero realizing sub-specs) | 0 |
| Orphan sub-specs (zero realized INVs) | 10 |
| Layer inversions (INV-007 candidates) | 65 |
| Consumes-graph cycles | 1 |

## Orphan sub-specs

Sub-spec capabilities with zero INV realization links. Some are legitimately structural (no constitutional binding needed); others may be missing their INV citations.

- S6.1 — Status Taxonomy (Rev.2)
- S6.2 — Evidence Grades (Rev.2)
- S0.1 — Action Envelope + Lifecycle (Rev.2)
- Rev.2 reference donor / system app candidate — ProxGuard Reference Model Notes (Rev.2)
- S1.3 — AIOS-FS Object Model (Rev.2)
- S1.3 — AIOS-FS Conflict Resolution (Rev.2)
- S2.2 — AIOS-FS Implementation Space (Rev.2)
- S5.1 — Identity Model (Rev.2)
- S1.1 — Capability Translator (Rev.2)
- S1.2 — Latency Tiering (Rev.2)

## Layer inversions (INV-007 candidates)

Consumes edges where the producer's layer is numerically higher than the consumer's. Per the W11-A discipline (DEC-049), `imports-vocabulary-from` is allowed upward; only `requires-for-correctness` is forbidden. Verify each by reading the source sub-spec's `Consumes` header.

- **S1.3 — AIOS-FS Conflict Resolution (Rev.2)** (L2) → **S1.2 — Latency Tiering (Rev.2)** (L5)
- **S10.1 — Capability Runtime gRPC (Rev.2)** (L3) → **S2.3 — Policy Kernel (Rev.2)** (L4)
- **S10.1 — Capability Runtime gRPC (Rev.2)** (L3) → **S2.4 — Verification Grammar (Rev.2)** (L9)
- **S10.1 — Capability Runtime gRPC (Rev.2)** (L3) → **S3.1 — Evidence Log Architecture (Rev.2)** (L9)
- **S10.1 — Capability Runtime gRPC (Rev.2)** (L3) → **S3.2 — Sandbox Composition Language (Rev.2)** (L6)
- **S10.1 — Capability Runtime gRPC (Rev.2)** (L3) → **S5.3 — Approval Mechanics (Rev.2)** (L4)
- **S10.1 — Capability Runtime gRPC (Rev.2)** (L3) → **S5.4 — Emergency Override (Rev.2)** (L4)
- **S12.1 — App Runtime Model + Cross-Ecosystem Compatibility (Rev.2)** (L6) → **S11.1 — Repository Model + Trust Roots (Rev.2)** (L10)
- **S12.1 — App Runtime Model + Cross-Ecosystem Compatibility (Rev.2)** (L6) → **S8.1 — Network Policy (Rev.2)** (L8)
- **S12.2 — Package Object Model — On-Disk Layout, Versioning, Update, Rollback (Rev.2)** (L6) → **S2.4 — Verification Grammar (Rev.2)** (L9)
- **S12.2 — Package Object Model — On-Disk Layout, Versioning, Update, Rollback (Rev.2)** (L6) → **S3.1 — Evidence Log Architecture (Rev.2)** (L9)
- **S12.3 — Compatibility Runtime — Orchestration of EcosystemRuntime Adapters (Rev.2)** (L6) → **S11.1 — Repository Model + Trust Roots (Rev.2)** (L10)
- **S12.3 — Compatibility Runtime — Orchestration of EcosystemRuntime Adapters (Rev.2)** (L6) → **S8.2 — GPU Resource Model (Rev.2)** (L8)
- **S12.4 — Compatibility Knowledge — Per-App Profile Database (Rev.2)** (L6) → **S11.1 — Repository Model + Trust Roots (Rev.2)** (L10)
- **S12.4 — Compatibility Knowledge — Per-App Profile Database (Rev.2)** (L6) → **S3.1 — Evidence Log Architecture (Rev.2)** (L9)
- **S13.1 — Cognitive Core Model (Rev.2)** (L5) → **S3.1 — Evidence Log Architecture (Rev.2)** (L9)
- **S13.1 — Cognitive Core Model (Rev.2)** (L5) → **S8.1 — Network Policy (Rev.2)** (L8)
- **S13.2 — Model Router (Rev.2)** (L5) → **S11.1 — Repository Model + Trust Roots (Rev.2)** (L10)
- **S13.2 — Model Router (Rev.2)** (L5) → **S14.1 — Failure Handling and Degradation (Rev.2)** (L9)
- **S13.2 — Model Router (Rev.2)** (L5) → **S8.1 — Network Policy (Rev.2)** (L8)
- **S15.1 — AIOS-SGR Unit Manifest (Rev.2)** (L3) → **S2.4 — Verification Grammar (Rev.2)** (L9)
- **S15.1 — AIOS-SGR Unit Manifest (Rev.2)** (L3) → **S3.1 — Evidence Log Architecture (Rev.2)** (L9)
- **S15.1 — AIOS-SGR Unit Manifest (Rev.2)** (L3) → **S3.2 — Sandbox Composition Language (Rev.2)** (L6)
- **S15.2 — AIOS-SGR State Transitions and Graph Evaluation (Rev.2)** (L3) → **S2.4 — Verification Grammar (Rev.2)** (L9)
- **S15.3 — Adapter Model — Manifest, Registration, Capability Declaration, Fail-Closed Semantics (Rev.2)** (L3) → **S11.1 — Repository Model + Trust Roots (Rev.2)** (L10)
- **S15.3 — Adapter Model — Manifest, Registration, Capability Declaration, Fail-Closed Semantics (Rev.2)** (L3) → **S2.3 — Policy Kernel (Rev.2)** (L4)
- **S15.3 — Adapter Model — Manifest, Registration, Capability Declaration, Fail-Closed Semantics (Rev.2)** (L3) → **S2.4 — Verification Grammar (Rev.2)** (L9)
- **S15.3 — Adapter Model — Manifest, Registration, Capability Declaration, Fail-Closed Semantics (Rev.2)** (L3) → **S3.2 — Sandbox Composition Language (Rev.2)** (L6)
- **S2.1 — AIOS-FS Query and View Language (Rev.2)** (L2) → **S1.2 — Latency Tiering (Rev.2)** (L5)
- **S4.1 — AIOS-FS Namespace Layout (Rev.2)** (L2) → **S2.3 — Policy Kernel (Rev.2)** (L4)
- **S4.1 — AIOS-FS Namespace Layout (Rev.2)** (L2) → **S2.4 — Verification Grammar (Rev.2)** (L9)
- **S4.1 — AIOS-FS Namespace Layout (Rev.2)** (L2) → **S3.1 — Evidence Log Architecture (Rev.2)** (L9)
- **S4.1 — AIOS-FS Namespace Layout (Rev.2)** (L2) → **S3.2 — Sandbox Composition Language (Rev.2)** (L6)
- **S5.3 — Approval Mechanics (Rev.2)** (L4) → **S7.1 — Surface + Composition Model (Rev.2)** (L7)
- **S5.3 — Approval Mechanics (Rev.2)** (L4) → **S7.2 — Shared UI Schema (Rev.2)** (L7)
- **S6.3 — Evidence Receipt Schema (Rev.2)** (L0) → **S3.1 — Evidence Log Architecture (Rev.2)** (L9)
- **S6.3 — Evidence Receipt Schema (Rev.2)** (L0) → **S4.1 — AIOS-FS Namespace Layout (Rev.2)** (L2)
- **S6.3 — Evidence Receipt Schema (Rev.2)** (L0) → **S5.1 — Identity Model (Rev.2)** (L4)
- **S6.5 — Session Container Model — per-group containerized KDE Plasma sessions streamed to browser (Rev.2)** (L6) → **S7.1 — Surface + Composition Model (Rev.2)** (L7)
- **S6.5 — Session Container Model — per-group containerized KDE Plasma sessions streamed to browser (Rev.2)** (L6) → **S7.4 — KDE Plasma Renderer (Rev.2)** (L7)
- **S6.5 — Session Container Model — per-group containerized KDE Plasma sessions streamed to browser (Rev.2)** (L6) → **S7.5 — Web Renderer (Rev.2)** (L7)
- **S6.5 — Session Container Model — per-group containerized KDE Plasma sessions streamed to browser (Rev.2)** (L6) → **S8.1 — Network Policy (Rev.2)** (L8)
- **S7.1 — Surface + Composition Model (Rev.2)** (L7) → **S3.1 — Evidence Log Architecture (Rev.2)** (L9)
- **S7.1 — Surface + Composition Model (Rev.2)** (L7) → **S8.2 — GPU Resource Model (Rev.2)** (L8)
- **S7.2 — Shared UI Schema (Rev.2)** (L7) → **S3.1 — Evidence Log Architecture (Rev.2)** (L9)
- **S7.4 — KDE Plasma Renderer (Rev.2)** (L7) → **S8.2 — GPU Resource Model (Rev.2)** (L8)
- **S7.5 — Web Renderer (Rev.2)** (L7) → **S8.2 — GPU Resource Model (Rev.2)** (L8)
- **S8.1 — Network Policy (Rev.2)** (L8) → **S2.4 — Verification Grammar (Rev.2)** (L9)
- **S8.1 — Network Policy (Rev.2)** (L8) → **S3.1 — Evidence Log Architecture (Rev.2)** (L9)
- **S8.3 — Hardware Graph (Rev.2)** (L8) → **S3.1 — Evidence Log Architecture (Rev.2)** (L9)
- **S8.4 — DNS / VPN Management (Rev.2)** (L8) → **S2.4 — Verification Grammar (Rev.2)** (L9)
- **S8.4 — DNS / VPN Management (Rev.2)** (L8) → **S3.1 — Evidence Log Architecture (Rev.2)** (L9)
- **S8.5 — Firmware Trust + Signed Update Paths (Rev.2)** (L8) → **S3.1 — Evidence Log Architecture (Rev.2)** (L9)
- **S9.1 — Recovery Boundary (Rev.2)** (L1) → **S2.3 — Policy Kernel (Rev.2)** (L4)
- **S9.1 — Recovery Boundary (Rev.2)** (L1) → **S4.1 — AIOS-FS Namespace Layout (Rev.2)** (L2)
- **S9.1 — Recovery Boundary (Rev.2)** (L1) → **S5.1 — Identity Model (Rev.2)** (L4)
- **S9.1 — Recovery Boundary (Rev.2)** (L1) → **S5.4 — Emergency Override (Rev.2)** (L4)
- **S9.2 — First-Boot Flow (Rev.2)** (L1) → **S3.1 — Evidence Log Architecture (Rev.2)** (L9)
- **S9.2 — First-Boot Flow (Rev.2)** (L1) → **S4.1 — AIOS-FS Namespace Layout (Rev.2)** (L2)
- **S9.2 — First-Boot Flow (Rev.2)** (L1) → **S5.1 — Identity Model (Rev.2)** (L4)
- **S9.2 — First-Boot Flow (Rev.2)** (L1) → **S5.2 — Vault Broker (Rev.2)** (L4)
- **S9.3 — Dedicated Kernel Pipeline (Rev.2)** (L1) → **S10.1 — Capability Runtime gRPC (Rev.2)** (L3)
- **S9.3 — Dedicated Kernel Pipeline (Rev.2)** (L1) → **S11.1 — Repository Model + Trust Roots (Rev.2)** (L10)
- **S9.3 — Dedicated Kernel Pipeline (Rev.2)** (L1) → **S3.2 — Sandbox Composition Language (Rev.2)** (L6)
- **S9.3 — Dedicated Kernel Pipeline (Rev.2)** (L1) → **S8.2 — GPU Resource Model (Rev.2)** (L8)

## Consumes-graph cycles

Architectural cycles — sub-spec A imports from B which imports from A (directly or transitively).

- `S7.1 — Surface + Composition Model (Rev.2) → S8.2 — GPU Resource Model (Rev.2) → S7.1 — Surface + Composition Model (Rev.2)`

## Hot spots

Sub-specs with highest fan-out / fan-in on the Consumes graph.

### Top consumers (most outgoing Consumes)

- S0.3 — MVP Golden Path Contract (Rev.2) — consumes from 19 sub-specs
- S8.4 — DNS / VPN Management (Rev.2) — consumes from 9 sub-specs
- S13.1 — Cognitive Core Model (Rev.2) — consumes from 8 sub-specs
- S13.2 — Model Router (Rev.2) — consumes from 8 sub-specs
- S7.1 — Surface + Composition Model (Rev.2) — consumes from 8 sub-specs

### Top producers (most incoming Consumes)

- S0.1 — Action Envelope + Lifecycle (Rev.2) — consumed by 31 sub-specs
- S2.3 — Policy Kernel (Rev.2) — consumed by 21 sub-specs
- S3.1 — Evidence Log Architecture (Rev.2) — consumed by 17 sub-specs
- S3.2 — Sandbox Composition Language (Rev.2) — consumed by 17 sub-specs
- S5.1 — Identity Model (Rev.2) — consumed by 17 sub-specs

## Distributions

### Per-INV realizing sub-spec count

`{15: 3, 32: 1, 6: 2, 12: 2, 11: 2, 10: 1, 17: 2, 7: 2, 13: 1, 16: 2, 20: 1, 22: 1, 14: 3, 9: 1}`

Interpretation: e.g. `{1: 3, 5: 8}` means 3 INVs have exactly 1 realizing sub-spec and 8 INVs have 5 realizing sub-specs. A bucket at 0 = orphan INVs (also enumerated above).

### Per-sub-spec realized INV count

`{0: 10, 7: 4, 24: 1, 20: 1, 5: 5, 1: 3, 10: 3, 3: 2, 8: 5, 12: 2, 11: 1, 17: 1, 6: 4, 4: 5, 9: 5, 14: 1}`
