# AIOS — Specification Revision 2 (Active)

Layered rewrite of the AIOS specification. Each layer has its own folder following the L0–L10 model from [Rev.1 §6](../001.AI-OS.NET--SPECREV.1/02_SPECIFICATION.md). Cross-layer contracts live in `XX_Cross_Cutting/`.

## Top-level documents

| File                                                       | Purpose                                 |
| ---------------------------------------------------------- | --------------------------------------- |
| [01_executive_summary.md](01_executive_summary.md)         | What rev.2 changes vs rev.1             |
| [02_design_decisions.md](02_design_decisions.md)           | Decision log (ADR style)                |
| [03_architecture_overview.md](03_architecture_overview.md) | System diagram and layer dependency map |

## Layers

Dependency rule: a layer may depend on its own layer and lower-numbered layers. A layer must not require a higher-numbered layer for correctness.

> **Implementation status (Rev.2 FULL-REAL, 2026-05-29):** all 11 layers are implemented to `REAL` (E2+) across the 19-crate `crates/` workspace (4475 tests, 0 failed; see [`MILESTONES.md`](../MILESTONES.md)). The `REAL` statuses below reflect each layer's closed milestone scope. **M20 discharged the formerly-deferred surfaces:** the L5 Cognitive Core agent/plan/memory gRPC methods (`aios-cognitive`) and the 22 Tier-3 cross-layer verification primitives (`aios-verification`) are now REAL and tested.

| Layer | Folder                                                                            | Responsibility                                               | Status |
| ----- | --------------------------------------------------------------------------------- | ------------------------------------------------------------ | ------ |
| L0    | [L0_Governance_Evidence_Safety](L0_Governance_Evidence_Safety/)                   | status taxonomy, evidence grades, gates, invariants          | `REAL` |
| L1    | [L1_Kernel_Bootstrap_Recovery](L1_Kernel_Bootstrap_Recovery/)                     | Linux substrate, recovery path, dedicated kernel candidate   | `REAL` |
| L2    | [L2_AIOS_FS](L2_AIOS_FS/)                                                         | semantic object filesystem, `/aios`, versions, views         | `REAL` |
| L3    | [L3_AIOS_SGR_Service_Graph_Runtime](L3_AIOS_SGR_Service_Graph_Runtime/)           | desired-state service graph, runtime transitions             | `REAL` |
| L4    | [L4_Policy_Identity_Vault](L4_Policy_Identity_Vault/)                             | subjects, capabilities, approvals, secrets, policy packages  | `REAL` |
| L5    | [L5_Cognitive_Core](L5_Cognitive_Core/)                                           | intent, planning, memory, model routing, agent coordination  | `REAL` |
| L6    | [L6_Apps_Packages_Compatibility](L6_Apps_Packages_Compatibility/)                 | AIOS packages, apps, Windows/Android/Linux compatibility     | `REAL` |
| L7    | [L7_Interaction_Renderers](L7_Interaction_Renderers/)                             | KDE, Web, CLI, Voice, Mobile, shared UI schema               | `REAL` |
| L8    | [L8_Network_Hardware_Devices](L8_Network_Hardware_Devices/)                       | network policy, hardware graph, drivers, firmware            | `REAL` |
| L9    | [L9_Observability_Admin_Operations](L9_Observability_Admin_Operations/)           | health, logs, metrics, evidence viewer, recovery operations  | `REAL` |
| L10   | [L10_Distribution_Ecosystem_Marketplace](L10_Distribution_Ecosystem_Marketplace/) | publishing, repositories, marketplace, external integrations | `REAL` |

## Cross-cutting contracts

Contracts shared by multiple layers live here.

| Contract                    | Document                                                                                             | Consumed by        | Status                                       |
| --------------------------- | ---------------------------------------------------------------------------------------------------- | ------------------ | -------------------------------------------- |
| Action Envelope + Lifecycle | [XX_Cross_Cutting/01_action_envelope_lifecycle.md](XX_Cross_Cutting/01_action_envelope_lifecycle.md) | L3, L4, L5, L9     | `CONTRACT` (S0.1 design approved 2026-05-07) |
| ProxGuard Reference Model   | [XX_Cross_Cutting/02_proxguard_reference_model.md](XX_Cross_Cutting/02_proxguard_reference_model.md) | L3, L4, L6, L8, L9 | `CONTRACT` reference note                    |

## Status legend (L0 taxonomy)

`REAL` · `PARTIAL` · `SHELL` · `CONTRACT` · `DEFERRED` · `BLOCKED` · `UNKNOWN` · `RETIRED`

## Sub-spec roadmap

Phase 0 (foundation):

- **S0.1** — Action Envelope + Lifecycle → `XX_Cross_Cutting/01_action_envelope_lifecycle.md` (`CONTRACT`)
- **S0.2** — Self-evolving backend resolution → applied to rev.1 README (commit `be318da`)
- **S0.3** — MVP Golden Path Contract → `XX_Cross_Cutting/03_mvp_golden_path.md` (`CONTRACT`)
- **S0.4** — Constitutional Meta-Principles → `XX_Cross_Cutting/04_constitutional_meta_principles.md` (`CONTRACT`)

Phase 1 (the three killers):

- **S1.1** — Capability Translator → `L5_Cognitive_Core/02_capability_translator.md` (`CONTRACT`)
- **S1.2** — Latency tiering → `L5_Cognitive_Core/03_latency_tiering.md` (`CONTRACT`)
- **S1.3** — AIOS-FS object model + conflict resolution → `L2_AIOS_FS/01_object_model.md` + `L2_AIOS_FS/03_conflict_resolution.md` (`CONTRACT`)

Phase 2 (subsystems):

- **S2.1** — AIOS-FS query/view language → `L2_AIOS_FS/02_query_view_language.md` (`CONTRACT`)
- **S2.2** — AIOS-FS implementation space → `L2_AIOS_FS/04_implementation_space.md` (`CONTRACT`)
- **S2.3** — Policy Kernel implementation → `L4_Policy_Identity_Vault/01_policy_kernel.md` (`CONTRACT`)
- **S2.4** — Verification grammar → `L9_Observability_Admin_Operations/02_verification_grammar.md` (`CONTRACT`)

Phase 3 (operational):

- **S3.1** — Evidence log architecture → `L9_Observability_Admin_Operations/01_evidence_log.md` (`CONTRACT`)
- **S3.2** — Sandbox composition language → `L6_Apps_Packages_Compatibility/04_sandbox_composition.md` (`CONTRACT`)

Phase 4 (namespace):

- **S4.1** — AIOS-FS namespace layout → `L2_AIOS_FS/05_namespace_layout.md` (`CONTRACT`)

Phase 5 (L4 identity, vault, approval, override):

- **S5.1** — Identity Model → `L4_Policy_Identity_Vault/03_identity_model.md` (`CONTRACT`)
- **S5.2** — Vault Broker → `L4_Policy_Identity_Vault/02_vault_broker.md` (`CONTRACT`)
- **S5.3** — Approval Mechanics → `L4_Policy_Identity_Vault/04_approval_mechanics.md` (`CONTRACT`)
- **S5.4** — Emergency Override → `L4_Policy_Identity_Vault/05_emergency_override.md` (`CONTRACT`)

Phase 6 (L0 governance):

- **S6.1** — Status taxonomy → `L0_Governance_Evidence_Safety/01_status_taxonomy.md` (`CONTRACT`)
- **S6.2** — Evidence grades → `L0_Governance_Evidence_Safety/02_evidence_grades.md` (`CONTRACT`)
- **S6.3** — Evidence Receipt Schema → `L0_Governance_Evidence_Safety/03_evidence_receipt_schema.md` (`CONTRACT`)
- **S6.4** — Constitutional invariants (INV-001..INV-024) → `L0_Governance_Evidence_Safety/04_invariants.md` (`CONTRACT`)

Phase 7 (renderer architecture):

- **S7.1** — Surface + Composition Model → `L7_Interaction_Renderers/01_surface_composition.md` (`CONTRACT`)
- **S7.2** — Shared UI Schema → `L7_Interaction_Renderers/02_shared_ui_schema.md` (`CONTRACT`)
- **S7.3** — Visual Language → `L7_Interaction_Renderers/03_visual_language.md` (`CONTRACT`)
- **S7.4** — KDE Plasma Renderer → `L7_Interaction_Renderers/04_kde_renderer.md` (`CONTRACT`)
- **S7.5** — Web Renderer → `L7_Interaction_Renderers/05_web_renderer.md` (`CONTRACT`)
- **S7.6** — CLI Renderer → `L7_Interaction_Renderers/06_cli_renderer.md` (`CONTRACT`)

Phase 8 (L8 hardware/network):

- **S8.1** — Network Policy → `L8_Network_Hardware_Devices/02_network_policy.md` (`CONTRACT`)
- **S8.2** — GPU Resource Model → `L8_Network_Hardware_Devices/05_gpu_resource_model.md` (`CONTRACT`)
- **S8.3** — Hardware Graph → `L8_Network_Hardware_Devices/01_hardware_graph.md` (`CONTRACT`)
- **S8.4** — DNS / VPN Management → `L8_Network_Hardware_Devices/03_dns_vpn_management.md` (`CONTRACT`)
- **S8.5** — Firmware Trust → `L8_Network_Hardware_Devices/04_firmware_trust.md` (`CONTRACT`)

Phase 9 (L1 substrate):

- **S9.1** — Recovery Boundary → `L1_Kernel_Bootstrap_Recovery/01_recovery_boundary.md` (`CONTRACT`)
- **S9.2** — First-Boot Flow → `L1_Kernel_Bootstrap_Recovery/02_first_boot_flow.md` (`CONTRACT`)
- **S9.3** — Dedicated Kernel Pipeline (smart mechanism) → `L1_Kernel_Bootstrap_Recovery/03_dedicated_kernel_pipeline.md` (`CONTRACT`)

Phase 10 (L3 runtime):

- **S10.1** — Capability Runtime gRPC → `L3_AIOS_SGR_Service_Graph_Runtime/03_capability_runtime_grpc.md` (`CONTRACT`)

Phase 11 (L10 distribution):

- **S11.1** — Repository Model + Trust Roots → `L10_Distribution_Ecosystem_Marketplace/01_repository_model.md` (`CONTRACT`)
- **S11.2** — Marketplace UX (publisher onboarding + capability review) → `L10_Distribution_Ecosystem_Marketplace/02_marketplace.md` (`CONTRACT`)
- **S11.3** — External Integrations (Flathub/OCI/distro bridges) → `L10_Distribution_Ecosystem_Marketplace/03_external_integrations.md` (`CONTRACT`)

Phase 12 (L6 app runtime + compatibility):

- **S12.1** — App Runtime Model + Cross-Ecosystem Compatibility → `L6_Apps_Packages_Compatibility/01_app_runtime_model.md` (`CONTRACT`)
- **S12.2** — Package Object Model → `L6_Apps_Packages_Compatibility/02_package_model.md` (`CONTRACT`)
- **S12.3** — Compatibility Runtime Orchestration → `L6_Apps_Packages_Compatibility/03_compatibility_runtime.md` (`CONTRACT`)
- **S12.4** — Compatibility Knowledge (per-app profile database) → `L6_Apps_Packages_Compatibility/05_compatibility_knowledge.md` (`CONTRACT`)
- **S6.5** — Session Container Model (per-group containerized KDE sessions streamed to browser) → `L6_Apps_Packages_Compatibility/06_session_container_model.md` (`CONTRACT`)

Phase 13 (L5 cognitive core depth):

- **S13.1** — Cognitive Core Model (intent + planning + memory + agent coordination) → `L5_Cognitive_Core/01_cognitive_core_model.md` (`CONTRACT`)
- **S13.2** — Model Router (cognitive backend adapter discipline) → `L5_Cognitive_Core/05_model_router.md` (`CONTRACT`)

Phase 14 (L9 failure handling + telemetry):

- **S14.1** — Failure Handling and Degradation → `L9_Observability_Admin_Operations/03_failure_handling.md` (`CONTRACT`)
- **S14.2** — Telemetry Pipeline → `L9_Observability_Admin_Operations/04_telemetry_pipeline.md` (`CONTRACT`)

Phase 15 (L3 SGR depth):

- **S15.1** — Unit Manifest → `L3_AIOS_SGR_Service_Graph_Runtime/01_unit_manifest.md` (`CONTRACT`)
- **S15.2** — State Transitions (graph evaluation + A/B promotion) → `L3_AIOS_SGR_Service_Graph_Runtime/02_state_transitions.md` (`CONTRACT`)
- **S15.3** — Adapter Model (deepens S10.1 manifest discipline) → `L3_AIOS_SGR_Service_Graph_Runtime/04_adapter_model.md` (`CONTRACT`)

Reference donors:

- **R1** — ProxGuard control-plane patterns and optional AIOS system app → `XX_Cross_Cutting/02_proxguard_reference_model.md` (`CONTRACT` reference note; E1 artifact inspection only)
