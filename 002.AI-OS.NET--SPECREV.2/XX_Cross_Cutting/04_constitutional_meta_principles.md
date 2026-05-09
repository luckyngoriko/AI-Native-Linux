# Constitutional Meta-Principles (Rev.2)

| Field          | Value                                                                              |
| -------------- | ---------------------------------------------------------------------------------- |
| Status         | `CONTRACT` (initial; written 2026-05-11)                                           |
| Phase tag      | S0.4                                                                               |
| Layer          | XX Cross-Cutting                                                                   |
| Schema package | n/a (this contract is meta-narrative; it documents principles, not service shapes) |
| Consumes       | nothing (meta-level reflection)                                                    |
| Produces       | three constitutional patterns that the rest of the spec implicitly relies on       |

## §1 Purpose

The AIOS specification has matured to ~30 contract-grade sub-specs spanning all 11 layers. Reading the spec from the bottom up reveals **three structural patterns that recur throughout but are never named**. This contract names them. Each pattern is constitutional in spirit — the spec depends on it being true — and naming it makes audits possible.

The three patterns:

1. **Constitutional asymmetry between spec construction and spec runtime** — AIOS-runtime is bounded AI agency; the meta-process building AIOS is unbounded AI assistance. The two regimes are structurally different and the difference is intentional.
2. **Recursive self-application** — AIOS reasons about AIOS using AIOS. The same governance machinery that gates everything else gates the building of AIOS itself.
3. **The INV-002 enforcement map** — "AI proposes, never executes" appears to be a single rule but is mechanically enforced at six distinct sites across the layer model. Naming the sites lets auditors verify each independently.

## §2 Pattern 1 — Constitutional asymmetry between spec construction and spec runtime

### §2.1 Statement

AIOS specifies bounded AI agency at runtime: every AI proposal flows through the typed-action pipeline, every execution decision is operator-approved, every secret use is vault-brokered, every external network call is routed through L8.1's vault-brokered pattern. INV-002 — "AI proposes, never executes" — is the headline constitutional rule.

The construction of the AIOS specification itself, however, has been an unbounded AI assistance process. Background agents have written ~30 sub-specs totalling tens of thousands of lines of contract-grade prose. Each agent received a charter and produced output; the project owner reviewed and approved at integration boundaries. There is no INV-002-equivalent on the meta-process; the meta-process is human-supervised AI agency without the constitutional discipline that the resulting artefact specifies for runtime AI.

This asymmetry is **not a contradiction** — specification and runtime are different regimes — but it is worth naming explicitly so that future readers can interpret the spec's authorship correctly.

### §2.2 Why the asymmetry is acceptable

- **Specification is reversible.** Any sub-spec can be re-written; the `git log` is the audit trail. AIOS runtime actions, in contrast, can have irreversible physical-world effects (deleted data, deployed services, opened firewall ports, signed certificates). Bounded AI agency at runtime is a defense against irreversibility; bounded AI agency at spec time would be over-engineering.
- **Specification has a single human approver.** Every commit lands at the project owner's discretion; the integration boundary IS the approval point. Runtime, in contrast, may have many users with conflicting interests; the bounded-AI rule is what makes it safe to deploy at scale.
- **Specification produces a public artefact.** The spec is `git`-committed, signed off, and reviewable; readers can audit every claim. Runtime AI, in contrast, may produce private outputs that are never reviewable; the bounded-AI rule is what makes operator-private AI safe.
- **The asymmetry inverts at deployment.** When AIOS is implemented and an operator runs it, the operator becomes the human-approver-of-AI-proposals (the runtime regime); the spec-construction regime ends. The asymmetry is a transient property of the construction phase, not a permanent feature of AIOS.

### §2.3 What this means for readers

A reader auditing the spec should:

- **Trust the artefact, not the process.** The spec is what it says it is, regardless of how it was authored. Every claim is checkable against the spec text and against the cited source specs.
- **Apply runtime rules to runtime AI, not to spec construction.** A reader should not point at INV-002 and say "but Claude wrote the spec, so AI executed". The execution INV-002 forbids is _runtime_ execution of _typed actions_; spec authorship is a different category of activity governed by `git` history and review process, not by INV-002.
- **Verify the asymmetry inverts on deployment.** When AIOS ships, the runtime regime takes over; spec-construction-regime artefacts are frozen as `001.AI-OS.NET--SPECREV.1/` and `002.AI-OS.NET--SPECREV.2/`. Future spec revisions follow the same human-supervised-AI process under the same review boundary.

### §2.4 Constitutional standing

This asymmetry does NOT need a new INV. It is a meta-level observation about the spec's authorship, not a runtime rule. Naming it here is sufficient; auditors can cite §2 of this file when asked "who wrote this spec?".

## §3 Pattern 2 — Recursive self-application

### §3.1 Statement

The AIOS specification specifies AIOS, but in five distinct places it does so in a **recursively self-applying** way: the machinery that gates everything else is also the machinery that gates that machinery's own operation. This is intentional — every "but who watches the watchers?" question gets the answer "the same governance plane", not "an out-of-band exception".

### §3.2 The five sites of recursive self-application

| Site                              | What recurses                                                                                                                                                                                                                                                                                                 | Citation                                                                                                                                                       |
| --------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Build the kernel through AIOS** | `kernel.build` is a typed AIOS action (S10.1 dispatched through Capability Runtime, ISOLATED_SANDBOX dispatch, signed by `_system:service:kernel-builder`); the build of the kernel that runs AIOS goes through the same governance plane as everything else                                                  | [S9.3 Element 5](../L1_Kernel_Bootstrap_Recovery/03_dedicated_kernel_pipeline.md), [S10.1](../L3_AIOS_SGR_Service_Graph_Runtime/03_capability_runtime_grpc.md) |
| **Kernel as evidence subject**    | Every boot records the running kernel's BLAKE3 image hash via L9.1 Evidence Log. Drift detection checks the kernel itself against expected hash. The kernel hosts the evidence log that includes a record of the kernel itself.                                                                               | [S9.3 Element 7](../L1_Kernel_Bootstrap_Recovery/03_dedicated_kernel_pipeline.md), [S3.1](../L9_Observability_Admin_Operations/01_evidence_log.md)             |
| **Vault is a vault user**         | The vault broker's own master key is held in vault material under `vault://aios/system/root_signing` (ON_REVEAL_ONLY); the vault uses the vault to protect the vault. Bootstrap uses TPM unseal at first boot; runtime use is mediated by the same use-without-reveal contract that mediates everything else. | [S5.2](../L4_Policy_Identity_Vault/02_vault_broker.md), [S9.2](../L1_Kernel_Bootstrap_Recovery/02_first_boot_flow.md)                                          |
| **Evidence about evidence**       | Evidence chain integrity verification produces evidence (TAMPER_DETECTED, RECEIPT_INTEGRITY_QUARANTINED, RECEIPT_LINEAGE_CYCLE_DETECTED). The evidence log records its own integrity audits in itself.                                                                                                        | [S6.3 §G](../L0_Governance_Evidence_Safety/03_evidence_receipt_schema.md), [S3.1](../L9_Observability_Admin_Operations/01_evidence_log.md)                     |
| **Agents reasoning about agents** | Inter-agent coordination is itself a typed AIOS action (`agent.coordinate.send` through L3 Capability Runtime); agents talk to agents via the same machinery that agents use to talk to the runtime. There is no privileged inter-agent channel.                                                              | [S13.1 §8](../L5_Cognitive_Core/01_cognitive_core_model.md), [S10.1](../L3_AIOS_SGR_Service_Graph_Runtime/03_capability_runtime_grpc.md)                       |

### §3.3 Why recursive self-application is constructive, not problematic

A naive reader might worry that recursive self-application creates infinite-regress or inconsistency. The spec avoids these failure modes through three structural choices:

- **Bootstrap is mechanical, not cognitive.** First-boot (S9.2) and recovery (S9.1) initialise the recursion's base case via signed bundles loaded from media or factory image — not via L5 cognition. The recursion has a finite, mechanical bottom.
- **Hash-chain rooted at signed bundles.** The L0 invariant bundle (`invbundle_<hex>`), policy bundle (`polb_<hex>`), identity bundle (`idbundle_<hex>`), and capability catalog are all signed at first-boot by AIOS root. The recursion roots in cryptographic anchors that the recursion itself does not generate.
- **Failure modes are detectable, not paradoxical.** When a recursive site fails (kernel image drift, vault unavailability, evidence integrity break), the failure is detected by the same machinery; the system enters L9.3-defined degradation OR drops into recovery. There is no "the watchers fail and we don't know" outcome — failure of the recursion produces evidence of its own failure.

### §3.4 What this means for implementers

- The recursive sites are not optional optimisations — they are constitutional. An implementation that bypasses them (e.g. a special "untyped" path for bootstrap) violates the principle and weakens every other guarantee.
- The recursive pattern has a measurable cost (every bootstrap step incurs typed-action overhead) but the cost is paid against the gain of uniform governance.
- When proposing extensions, the question "could this be made recursive?" is the smart-vs-stupid filter. Smart extensions reuse the existing machinery; stupid extensions add out-of-band paths.

### §3.5 Constitutional standing

Recursive self-application does NOT need a new INV either. It is an architectural pattern realised by the existing INVs (especially INV-002, INV-005, INV-007, INV-014). Naming it here lets future contributors recognise the pattern when they see it; auditors can cite §3 of this file when asked "why does X go through the typed-action pipeline?".

## §4 Pattern 3 — The INV-002 enforcement map

### §4.1 Statement

INV-002 — "AI proposes, never executes" — appears in L0.4 as a single rule. In practice, **every layer in the stack enforces a piece of it** with mechanical, closed-vocabulary discipline. Without seeing all six enforcement sites named together, an auditor might mistake INV-002 for a single-line constitutional aspiration. Listing the sites makes its mechanical depth visible.

### §4.2 The six enforcement sites

| #   | Site                                 | Mechanical enforcement                                                                                                                                                                                                                                                                                                                                                                                                                                       | Closed enum / fail-closed code                                                              | Citation                                                                                                                                                                                                       |
| --- | ------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | **Vault hard-deny SECRET_GET**       | AI subjects rejected at request entry from `SECRET_GET` capability class regardless of capability state — capability is rejected, not revoked, since it could be valid for a HUMAN_USER                                                                                                                                                                                                                                                                      | `SUBJECT_KIND_REJECTED_FOR_VAULT` FOREVER                                                   | [S5.2](../L4_Policy_Identity_Vault/02_vault_broker.md)                                                                                                                                                         |
| 2   | **Package install gate**             | AI subjects emitting `package.install` / `package.uninstall.execute` / `app.install` / `app.uninstall.execute` directly are hard-denied at S2.3 §26.2.4 by `AIInstallInitiationBlocked` (added Wave 9). AI subjects may emit the proposing-variant `package.install.request` which the policy kernel returns as REQUIRE_APPROVAL with `approver_subject_filter = HUMAN_USER`; the legitimate path uses approval, the bypass path is closed-enum hard-denied. | `APP_AI_DIRECT_INSTALL_ATTEMPTED_BLOCKED` FOREVER                                           | [S2.3 §26.2.4](../L4_Policy_Identity_Vault/01_policy_kernel.md), [S11.1](../L10_Distribution_Ecosystem_Marketplace/01_repository_model.md), [S12.1](../L6_Apps_Packages_Compatibility/01_app_runtime_model.md) |
| 3   | **Network AI_VAULT_BROKERED_ONLY**   | AI subjects NEVER granted ALLOW_INTERNET / ALLOW_LIST_ONLY for arbitrary destinations — network access flows only through L4.2 vault-brokered capability handles                                                                                                                                                                                                                                                                                             | `AI_DIRECT_INTERNET_DENIED` FOREVER                                                         | [S8.1 §5.7](../L8_Network_Hardware_Devices/02_network_policy.md)                                                                                                                                               |
| 4   | **Capability Runtime queue cap**     | AI-origin actions cannot exceed 50% of total queue capacity (default fairness floor); INTERACTIVE queue class on AI subject silently downgrades to AGENT_PROPOSAL                                                                                                                                                                                                                                                                                            | `AI_INTERACTIVE_QUEUE_DOWNGRADE` STANDARD_24M (telemetry record)                            | [S10.1 §11](../L3_AIOS_SGR_Service_Graph_Runtime/03_capability_runtime_grpc.md)                                                                                                                                |
| 5   | **Cognitive proposing pipeline FSM** | The proposing pipeline (intent → planning → action_proposal_drafting → S0.1 envelope → L3) has NO execute path. Agent transitions BLOCKED_AWAITING_APPROVAL after emission and only resumes on result. The structure of the FSM IS the enforcement.                                                                                                                                                                                                          | `AGENT_DIRECT_FS_WRITE_BLOCKED` FOREVER (when agent attempts to bypass via direct FS write) | [S13.1 §6](../L5_Cognitive_Core/01_cognitive_core_model.md)                                                                                                                                                    |
| 6   | **Self-grading prohibition**         | `agent.grade.attempt` is REJECTED CLOSED if `subject.is_ai = true AND subject_canonical_id == graded_capability.author_subject_id` (binds INV-016 too — AI cannot grade its own outputs); rule name: `AgentSelfGradingBlocked` (mirrors S2.3 site 2 `AIInstallInitiationBlocked` discipline)                                                                                                                                                                 | `AGENT_SELF_GRADING_BLOCKED` FOREVER                                                        | [S13.1 §10.1](../L5_Cognitive_Core/01_cognitive_core_model.md), [S6.2 §10.6](../L0_Governance_Evidence_Safety/02_evidence_grades.md)                                                                           |

### §4.3 Why six sites and not one

A single check at one site would be **brittle**. If the AI agent could escape the check at any layer, the constitutional rule would collapse. By distributing enforcement across six layers (L4.2 vault, L4.3 + L10 install, L8.1 network, L3 runtime, L5 cognition, L0 grading), AIOS makes INV-002 a **defense-in-depth invariant** rather than a single gate.

Each site has its own closed-enum reject code and FOREVER evidence record; an auditor can verify each site independently. Catching an INV-002 violation at any of the six sites produces permanent forensic evidence — there is no quiet bypass.

### §4.4 The "bypass attempts produce evidence" principle

For every site, the failure mode is symmetric:

- AI subject does the constitutional thing → action proceeds normally
- AI subject attempts bypass → blocked + FOREVER evidence emitted

The FOREVER evidence is the constitutional anchor. Even if a hypothetical AI subject (compromised cognition, malicious adapter, etc.) attempted to bypass, the attempt itself becomes permanent forensic record. This converts "bounded AI agency" from a hope into a measurable property of the runtime — operators can audit the FOREVER logs and verify zero bypass attempts succeeded.

### §4.5 What this means for new contracts

When a new sub-spec introduces a capability or surface that AI subjects might interact with, the author should **explicitly think about which of the six INV-002 sites apply** and which (if any) need extension. The six-site list above is the design check.

If the new capability does not bind to any of the six, that is itself a finding — either the capability is not AI-reachable (binding NOT NEEDED) or there is a missing seventh site (a new constitutional gap).

### §4.6 Constitutional standing

INV-002 itself stays as L0 INV-002 in the catalog. This §4 is not a new INV — it is the _enforcement map_ of an existing INV. Promotion of any of the six sites into a separate INV would be a deliberate constitutional act per the discipline established in DEC-025/DEC-026; this contract does not propose any such promotion.

**Wave 9 update (2026-05-09):** §4.2 site 2 (package install gate) was previously a narrative-only enforcement claim cited against S11.1 / S12.1; the actual mechanical hard-deny was missing from S2.3 because `AISystemAdminBlocked` requires `target.scope = SYSTEM` and therefore did not fire for user-scope installs. Wave 9 adds the constitutional rule `AIInstallInitiationBlocked` (S2.3 §26.2.4) which hard-denies `subject.is_ai = true AND request.action IN { "package.install", "package.uninstall.execute", "app.install", "app.uninstall.execute" }` regardless of target scope, and emits the existing `APP_AI_DIRECT_INSTALL_ATTEMPTED_BLOCKED` FOREVER record. Site 2 is now mechanically real, not just narratively cited; the constitutional standing of the §4 enforcement map is correspondingly stronger. No new INV is introduced (the rule binds to existing INV-002); the discipline of DEC-025/DEC-026 is preserved.

## §5 Cross-spec dependencies

| Reference                      | Relationship                                             |
| ------------------------------ | -------------------------------------------------------- |
| L0.4 INV catalog               | This contract observes, does not create new INVs         |
| S6.3 evidence receipt          | §3.4 cites the evidence-about-evidence recursion site    |
| S9.3 dedicated kernel pipeline | §3.2 site 1 + 2 cited for kernel build + drift detection |
| S5.2 vault broker              | §3.2 site 3 + §4.2 site 1 cited                          |
| S13.1 cognitive core           | §3.2 site 5 + §4.2 sites 5 + 6 cited                     |
| S10.1 capability runtime       | §3.2 sites 1 + 5 + §4.2 site 4 cited                     |
| S11.1 repository model         | §4.2 site 2 cited (package install gate)                 |
| S12.1 app runtime model        | §4.2 site 2 cited                                        |
| S8.1 network policy            | §4.2 site 3 cited (AI_VAULT_BROKERED_ONLY)               |
| S6.2 evidence grades           | §4.2 site 6 cited (INV-016 self-grading)                 |

## §6 Use cases for this contract

- **Onboarding**: a new contributor reads §2–§4 and understands three patterns that recur throughout the spec.
- **Audit phase (Tier 5)**: action-path simulations cite §4 to verify INV-002 enforcement at every site; meta-audits cite §3 to verify recursive self-application is intact at every site; reader-trust questions cite §2 to clarify spec construction context.
- **Future architectural waves**: when a new capability or surface lands, the §4.5 design check catches missing INV-002 sites; the §3.4 design check catches missing recursive sites.
- **External communication**: the spec asks readers to trust an AI-authored artefact; §2 makes the asymmetry explicit so trust is calibrated correctly.

## §7 Open issues

This contract is meta-narrative about the rest of the spec. It has no open issues of its own. If the rest of the spec gains new INV-002 enforcement sites (a seventh, eighth), §4 should be extended in a new revision. If new recursive self-application sites land, §3 should be extended. If the asymmetry between spec construction and runtime evolves (e.g. AIOS itself starts authoring AIOS revisions through the runtime regime), §2 should be revisited.

## §8 Status & evidence

`Status: REAL`

`Evidence: E1` (file exists; structural meta-narrative complete; cross-references resolved; §2/§3/§4 each contain at least one explicit citation map; the six INV-002 sites are testable against their cited sub-specs)

## See also

- [L0.4 Constitutional Invariants](../L0_Governance_Evidence_Safety/04_invariants.md)
- [S0.1 Action Envelope and Lifecycle](01_action_envelope_lifecycle.md)
- [S0.3 MVP Golden Path Contract](03_mvp_golden_path.md)
- [Rev.2 Master Index](../00_MASTER_INDEX.md)
