# Rev.3 — Design Decisions (ADR Log)

| Field       | Value                                                                           |
| ----------- | ------------------------------------------------------------------------------- |
| Status      | `CONTRACT` (append-only ADR log for Rev.3; created 2026-05-29)                  |
| Kind        | Architecture decision record; mirrors Rev.2 `02_design_decisions.md` discipline |
| Predecessor | `002.AI-OS.NET--SPECREV.2/02_design_decisions.md`                               |

This log records the binding decisions that resolve the open questions in
`00_PLANNING_NOTES.md` (§"Open questions to resolve before Wave 1") and the
structural choices made while completing Rev.3 to a full CONTRACT-grade surface.
Decisions are append-only. A superseded decision is marked `SUPERSEDED-BY` rather
than deleted.

---

## DEC-R3-001 — "Implementable" means CONTRACT-grade, not code

**Decision.** Completing Rev.3 means every section reaches `CONTRACT` status:
closed schemas, closed enums, lifecycle FSMs, evidence record types, security
profile gates, non-goals, and testable acceptance criteria, with clean Capella
traceability. `REAL` status still requires implementation evidence (E2+) and is
delivered by separate implementation milestones, exactly as in Rev.2.

**Rationale.** A spec is "implementable" when an engineer can begin coding with no
undefined dependency. Promising running code in the spec phase would violate the
"no proof = no completion" law.

---

## DEC-R3-002 — Hardware attestation root: firmware-pinned + TPM 2.0 dual chain

**Resolves.** Planning open question 3 (attestation root).

**Decision.** The high-assurance root of trust is a **dual chain**: the existing
firmware-pinned trust root (Rev.2 L10 S11.1) **plus** TPM 2.0 measured boot with
signed quotes. Either chain alone is insufficient for `STIG_ALIGNED`/`AIRGAP_HIGH`;
both are required where a TPM is present. Hosts without a TPM may reach
`SECURE_DEFAULT` but not `STIG_ALIGNED`. This is specified in S16.4.

**Rationale.** Defense in depth; firmware pinning survives TPM-absent hardware,
TPM measured boot detects firmware/bootchain tampering the pin cannot.

---

## DEC-R3-003 — Cluster scale: 2–100 hosts, hub-and-spoke WireGuard with optional mesh

**Resolves.** Planning open question 1 (cluster size).

**Decision.** Fleet/cluster contracts (S25) target 2–100 hosts (homelab through
small enterprise). Default overlay is hub-and-spoke WireGuard; a mesh option
(headscale/innernet-style) is admitted for sites that need it. The host remains
sovereign: a cluster root cannot silently bypass host policy. Evidence logs become
a Merkle-DAG to preserve append-only (INV-014) under replication.

**Rationale.** Pairwise WireGuard is O(n²); hub-and-spoke scales to the stated
range without forcing mesh complexity on small deployments.

---

## DEC-R3-004 — Mobile: renderer-first; phone edition on mainline Linux + Waydroid

**Resolves.** Planning open question 2 (mobile substrate).

**Decision.** `AIOS_MOBILE_RENDERER` (phone as a signed approval/monitoring console
over the Shared UI Schema) is the first deliverable and the priority. `AIOS_PHONE_EDITION`
(AIOS on phone-class hardware) targets **mainline Linux** (Plasma Mobile / phosh)
as the L1 substrate; Android apps run via Waydroid/VM, not by adopting an AOSP base.
INV-001 (recovery boot without AI) is re-phrased for keyboardless devices in S23.
Specified in S23.

**Rationale.** Renderer-first strengthens the desktop/server product immediately and
defers the harder L1 substrate fork; mainline Linux keeps the 11-layer model intact.

---

## DEC-R3-005 — eBPF subject set: HUMAN + SYSTEM_SERVICE may author; AI is drop-only

**Resolves.** Planning open question 4 (eBPF subject set).

**Decision.** eBPF programs may be authored by `HUMAN_OPERATOR`/`HUMAN_USER` and, for
observability, by `SYSTEM_SERVICE` subjects under policy. `AI_NATIVE_SUBJECT` and
`AI_AGENT_CAPSULE` **cannot author or load eBPF**; an AI may at most request a
pre-vetted, signed, drop-only (no `redirect`/no map-write-to-userspace) eBPF
template through a typed action. New invariant **INV-025** ("AI cannot author eBPF")
is added. Specified in S24 (ecosystem runtime adapters).

**Rationale.** In-kernel execution by AI would violate "AI proposes, never executes"
(INV-002); a signed drop-only template preserves the boundary while keeping the
capability useful.

---

## DEC-R3-006 — Compliance jurisdiction order: EU GDPR first, then SOC2/ISO 27001/HIPAA

**Resolves.** Planning open question 5 (compliance jurisdiction priority).

**Decision.** Data-governance and audit-export contracts (S16.9) specify EU GDPR
(including RTBF) first, then map SOC2 / ISO 27001 / HIPAA reporting. The RTBF ↔
append-only-evidence tension is resolved by **crypto-shredding**: personal data is
stored encrypted with a per-subject key; erasure destroys the key, leaving the
append-only evidence chain intact but the payload unrecoverable. Specified in S16.9.

**Rationale.** The operator and primary deployment context are EU; crypto-shredding
honors both INV-014 (append-only) and GDPR erasure.

---

## DEC-R3-007 — Voice renderer: full contract (TTS/STT + conversational binding)

**Resolves.** Planning open question 6 (voice renderer).

**Decision.** S23 specifies both the TTS/STT renderer (output/input over the Shared
UI Schema, no new authority) and the conversational binding (a typed L5 Cognitive
Core surface that emits the same typed actions as the AI terminal). Voice never gains
authority the AI terminal lacks; voice input is untrusted text subject to the S20
prompt-boundary classifier.

**Rationale.** Specifying both avoids deferred scope; the conversational path reuses
the S20 typed-action fabric, so it adds a binding, not a new authority surface.

---

## DEC-R3-008 — S18/S19/S20 remain single-file contract overviews (intentional)

**Decision.** S18, S19, and S20 stay as single-file `00_overview.md` contracts rather
than being decomposed into numbered sub-specs. Missing schemas (`KernelBuildCandidate`,
`RTWorkloadManifest`, `risk_class` enum, `AIComplianceRegistry`, `ProhibitedPatternGate`)
are added **in place** within those overviews. S16 and S17 remain decomposed because
their breadth (security control families; capsule object/solver/Windows/reliability/UI)
warrants it.

**Rationale.** These three planes are cohesive enough for a single contract document.
The real defect the gap report found (T2-1) was the Capella `layer = XX` exemption from
inversion classification, which is fixed by assigning real layers and registering edges
(DEC-R3-009), not by file-splitting.

---

## DEC-R3-009 — Capella traceability covers the full Rev.3 vocabulary

**Decision.** All Rev.3 evidence record types (~95), all "Produces" state objects, and
all sibling dependency edges (S17→S18, S19→S18, S20→S18, S20→S19, S18/S19→S16.4) are
registered in the Capella manifests. The S3.1 record-type enum is treated as frozen;
a Rev.3 evidence-delta is authored and `tools/capella/extract.py` is repointed to read
both. S18/S19/S20/S21–S28 are assigned real primary layers in `sub_specs.csv`. Holistic
acceptance criterion §16.7 is rewritten to additionally require zero orphan record types,
zero orphan sub-specs, and ≥1 emitter trace per declared record type, with the exact
tool command that constitutes "pass".

**Rationale.** Closes the false-coherence gap (gap report T1-2/T1-3): criterion 7 must
be computed over the full Rev.3 graph, not the inherited Rev.2 subset.

---

## DEC-R3-010 — Rev.3 invariants extend, never replace, Rev.2 INV-001..024

**Decision.** Rev.3 inherits INV-001..024 verbatim from Rev.2 L0 and adds new invariants
starting at INV-025 (e.g. INV-025 "AI cannot author eBPF"; INV-026 cluster-root cannot
override host policy; INV-027 crypto-shred erasure preserves evidence chain). New
constitutional rules stated in prose across S18–S28 are mapped to either an inherited or
a new INV in `04_invariants.md`. No inherited invariant is weakened.

**Rationale.** Preserves the constitutional baseline while giving the new planes
first-class invariant coverage (closes gap report T2-8).

---

## DEC-R3-011 — Section grouping for cross-cutting planning themes

**Decision.** Cross-cutting themes from the planning notes are homed as follows, to avoid
orphan scope: federated identity → **S25**; multi-agent coordination → **S20** (added
section); ecosystem runtime adapters (WASM/eBPF/Deno/Bun/Python) → **S24**; voice renderer
→ **S23**; energy/power policy → **S22**; GDPR/RTBF + audit export → **S16.9**; trusted-time
constitutional plane → **S28**. Each home is recorded in `00_MASTER_INDEX.md`.

**Rationale.** Every planning theme gets exactly one owning contract; nothing is left as
"planning-only" at completion.
