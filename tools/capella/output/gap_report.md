# AIOS Capella model — gap report

Source model: `tools/capella/output/aios-rev2/`

## Summary

- Total capabilities: 77
  - Invariants (OA): 24
  - Sub-specs (SA/LA/PA): 53
- Consumes edges: 33
- INV realization links: 331

## Gaps detected

| Gap category | Count |
| --- | ---: |
| Orphan INVs (zero realizing sub-specs) | 0 |
| Orphan sub-specs (zero realized INVs) | 10 |
| Layer inversions (INV-007 candidates) | 14 |
| Consumes-graph cycles | 0 |

## Orphan sub-specs

Sub-spec capabilities with zero INV realization links. Some are legitimately structural (no constitutional binding needed); others may be missing their INV citations.

- S6.2 — Evidence Grades (Rev.2)
- S6.1 — Status Taxonomy (Rev.2)
- Rev.2 reference donor / system app candidate — ProxGuard Reference Model Notes (Rev.2)
- S1.3 — AIOS-FS Object Model (Rev.2)
- S2.2 — AIOS-FS Implementation Space (Rev.2)
- S0.1 — Action Envelope + Lifecycle (Rev.2)
- S1.1 — Capability Translator (Rev.2)
- S1.2 — Latency Tiering (Rev.2)
- S5.1 — Identity Model (Rev.2)
- S1.3 — AIOS-FS Conflict Resolution (Rev.2)

## Layer inversions (INV-007 candidates)

Consumes edges where the producer's layer is numerically higher than the consumer's. Per the W11-A discipline (DEC-049), `imports-vocabulary-from` is allowed upward; only `requires-for-correctness` is forbidden. Verify each by reading the source sub-spec's `Consumes` header.

- **S9.2 — First-Boot Flow (Rev.2)** (L1) → **S3.1 — Evidence Log Architecture (Rev.2)** (L9)
- **S9.2 — First-Boot Flow (Rev.2)** (L1) → **S4.1 — AIOS-FS Namespace Layout (Rev.2)** (L2)
- **S9.2 — First-Boot Flow (Rev.2)** (L1) → **S5.1 — Identity Model (Rev.2)** (L4)
- **S9.2 — First-Boot Flow (Rev.2)** (L1) → **S5.2 — Vault Broker (Rev.2)** (L4)
- **S9.1 — Recovery Boundary (Rev.2)** (L1) → **S2.3 — Policy Kernel (Rev.2)** (L4)
- **S9.1 — Recovery Boundary (Rev.2)** (L1) → **S5.4 — Emergency Override (Rev.2)** (L4)
- **S9.3 — Dedicated Kernel Pipeline (Rev.2)** (L1) → **S10.1 — Capability Runtime gRPC (Rev.2)** (L3)
- **S9.3 — Dedicated Kernel Pipeline (Rev.2)** (L1) → **S11.1 — Repository Model + Trust Roots (Rev.2)** (L10)
- **S9.3 — Dedicated Kernel Pipeline (Rev.2)** (L1) → **S3.2 — Sandbox Composition Language (Rev.2)** (L6)
- **S9.3 — Dedicated Kernel Pipeline (Rev.2)** (L1) → **S8.2 — GPU Resource Model (Rev.2)** (L8)
- **S6.5 — Session Container Model — per-group containerized KDE Plasma sessions streamed to browser (Rev.2)** (L6) → **S7.4 — KDE Plasma Renderer (Rev.2)** (L7)
- **S6.5 — Session Container Model — per-group containerized KDE Plasma sessions streamed to browser (Rev.2)** (L6) → **S7.5 — Web Renderer (Rev.2)** (L7)
- **S6.5 — Session Container Model — per-group containerized KDE Plasma sessions streamed to browser (Rev.2)** (L6) → **S8.1 — Network Policy (Rev.2)** (L8)
- **S8.4 — DNS / VPN Management (Rev.2)** (L8) → **S2.4 — Verification Grammar (Rev.2)** (L9)

## Hot spots

Sub-specs with highest fan-out / fan-in on the Consumes graph.

### Top consumers (most outgoing Consumes)

- S9.2 — First-Boot Flow (Rev.2) — consumes from 6 sub-specs
- S9.3 — Dedicated Kernel Pipeline (Rev.2) — consumes from 4 sub-specs
- S6.5 — Session Container Model — per-group containerized KDE Plasma sessions streamed to browser (Rev.2) — consumes from 3 sub-specs
- S6.3 — Evidence Receipt Schema (Rev.2) — consumes from 2 sub-specs
- S3.1 — Evidence Log Architecture (Rev.2) — consumes from 2 sub-specs

### Top producers (most incoming Consumes)

- S6.2 — Evidence Grades (Rev.2) — consumed by 1 sub-specs
- S6.4 — Constitutional Invariants (Rev.2) — consumed by 1 sub-specs
- S6.1 — Status Taxonomy (Rev.2) — consumed by 1 sub-specs
- S15.1 — AIOS-SGR Unit Manifest (Rev.2) — consumed by 1 sub-specs
- S14.1 — Failure Handling and Degradation (Rev.2) — consumed by 1 sub-specs

## Distributions

### Per-INV realizing sub-spec count

`{15: 3, 32: 1, 6: 2, 12: 2, 11: 2, 10: 1, 17: 2, 7: 2, 13: 1, 16: 2, 20: 1, 22: 1, 14: 3, 9: 1}`

Interpretation: e.g. `{1: 3, 5: 8}` means 3 INVs have exactly 1 realizing sub-spec and 8 INVs have 5 realizing sub-specs. A bucket at 0 = orphan INVs (also enumerated above).

### Per-sub-spec realized INV count

`{7: 4, 0: 10, 24: 1, 20: 1, 5: 5, 3: 2, 10: 3, 8: 5, 6: 4, 11: 1, 4: 5, 14: 1, 12: 2, 1: 3, 9: 5, 17: 1}`
