# W11-A layer-inversion classification

Source: `tools/capella/output/gap_report.json` (run `analyze.py` to refresh).
Inversions classified: 65

## Per-class totals

| Classification | Count | W11-A verdict |
| --- | ---: | --- |
| vocabulary  | 63 | ALLOWED upward (DEC-049) |
| exception   | 2 | ALLOWED — runtime dep with bounded waiver (e.g. recovery degraded subset) |
| runtime     | 0 | FORBIDDEN upward (INV-007 violation) |
| uncertain   | 0 | needs manual reviewer decision |

## All edges

| Consumer | C-layer | Producer | P-layer | Class | Snippet | Keywords |
| --- | --- | --- | --- | --- | --- | --- |
| S9.1 | L1 | S2.3 | L4 | **exception** | . **Requires for correctness (degraded subset only)**: S2.3 Policy Kernel hard-deny `RecoveryRequiredForSystemMutation` (the recovery boot path requires the _degraded_ policy-kernel-with-recovery-bund... | degraded subset; degraded subset only; requires for correctness; subset only |
| S9.1 | L1 | S5.4 | L4 | **exception** | S5.4 Emergency Override `STRONG_SOLO` recovery-only (recovery boot requires the override stack present in degraded form) | degraded form; requires the over |
| S1.3 | L2 | S1.2 | L5 | **vocabulary** | S1.2 privacy class (type-level) | type-level |
| S10.1 | L3 | S2.3 | L4 | **vocabulary** | S2.3 Policy Kernel (decision shape + hard-deny vocabulary, type-level), S5.3 Approval Mechanics (approval-request shape, type-level), S5.4 Emergency Override (`NonOverridableClass` shape, type-level),... |  shape; approval-request shape; type-level; vocabulary |
| S10.1 | L3 | S2.4 | L9 | **vocabulary** | S2.4 Verification Engine (verification primitive vocabulary, type-level), S3.2 Sandbox Composition (`SandboxProfile` shape, type-level), S3.1 Evidence Log RecordType + retention vocabulary (type-level... |  shape; recordtype; type-level; vocabulary |
| S10.1 | L3 | S3.1 | L9 | **vocabulary** | S3.1 Evidence Log RecordType + retention vocabulary (type-level), S2.3 Policy Kernel (decision shape + hard-deny vocabulary, type-level), S5.3 Approval Mechanics (approval-request shape, type-level), ... |  shape; approval-request shape; recordtype; type-level; vocabulary |
| S10.1 | L3 | S3.2 | L6 | **vocabulary** | S3.2 Sandbox Composition (`SandboxProfile` shape, type-level), S3.1 Evidence Log RecordType + retention vocabulary (type-level), S2.3 Policy Kernel (decision shape + hard-deny vocabulary, type-level),... |  shape; approval-request shape; recordtype; type-level; vocabulary |
| S10.1 | L3 | S5.3 | L4 | **vocabulary** | S5.3 Approval Mechanics (approval-request shape, type-level), S5.4 Emergency Override (`NonOverridableClass` shape, type-level), L4.2 Vault Broker (capability-binding shape, type-level), L0 invariant ... |  shape; approval-request shape; type-level |
| S10.1 | L3 | S5.4 | L4 | **vocabulary** | S5.4 Emergency Override (`NonOverridableClass` shape, type-level), L4.2 Vault Broker (capability-binding shape, type-level), L0 invariant catalog (closed-id reference) |  shape; type-level |
| S12.1 | L6 | S11.1 | L10 | **vocabulary** | lifecycle FSM); S11.1 Repository Model (`PackageKind = ADAPTER`, `RepositoryKind = AIOS_COMMUNITY_REPO`, `PublisherTrustLevel`, capability-lie audit); S5.3 Approval Mechanics (`request_approval`, `EXA... | manifest |
| S12.1 | L6 | S8.1 | L8 | **vocabulary** | `EXACT_ACTION` binding); S8.1 Network Policy (`NetworkOutboundManifest`); S8.2 GPU Resource Model (per-group VkDevice, `GpuPolicy`); S3.1 Evidence Log (`RecordType` vocabulary, `FOREVER`/`STANDARD_24M... | manifest; recordtype; vocabulary |
| S12.2 | L6 | S2.4 | L9 | **vocabulary** | `RecoveryRequired`); S2.4 Verification Grammar (`PropertyType`, `aiosfs_path_in_namespace`, `SANDBOX_PROFILE_MOST_RESTRICTIVE`); S3.1 Evidence Log (`RecordType` vocabulary, retention classes `STANDARD... | recordtype; verification grammar; vocabulary |
| S12.2 | L6 | S3.1 | L9 | **vocabulary** | `SANDBOX_PROFILE_MOST_RESTRICTIVE`); S3.1 Evidence Log (`RecordType` vocabulary, retention classes `STANDARD_24M` / `EXTENDED_60M` / `FOREVER`); S3.2 Sandbox Composition (`SandboxProfile`, six-source ... | recordtype; vocabulary |
| S12.3 | L6 | S11.1 | L10 | **vocabulary** | §20 `ecosystem_runtime` field consolidation); S11.1 Repository Model (signed package manifest with `ecosystem_runtime` field, `PackageKind = ADAPTER`); S8.2 GPU Resource Model (per-group `VkDevice`, `... |  field; manifest |
| S12.3 | L6 | S8.2 | L8 | **vocabulary** | `PackageKind = ADAPTER`); S8.2 GPU Resource Model (per-group `VkDevice`, `GpuPolicy`); S0.1 Action Envelope (typed action `app.launch`, lifecycle FSM); S2.4 Verification Grammar (post-launch verificat... | recordtype; verification grammar |
| S12.4 | L6 | S11.1 | L10 | **vocabulary** | `ApprovalStrength`); S11.1 Repository Model (`AIOS_COMMUNITY_REPO`, `RepositoryKind`, `publisher_root_id`, `PublisherTrustLevel`, AIOS-root-signed publisher catalog); S12.1 App Runtime Model (`Ecosyst... | manifest |
| S12.4 | L6 | S3.1 | L9 | **vocabulary** | INV-017 (sandbox floor constitutional); S0.1 envelope FSM (AI cannot transition `policy_pending → executing`); S3.1 Evidence Log (`RecordType` vocabulary, `STANDARD_24M` / `EXTENDED_60M` / `FOREVER` r... | recordtype; vocabulary |
| S13.1 | L5 | S3.1 | L9 | **vocabulary** | S3.1 (evidence record vocabulary — type-level; L5 emits records that L9 absorbs) | evidence record; type-level; vocabulary |
| S13.1 | L5 | S8.1 | L8 | **vocabulary** | S8.1 (network policy — `AICrossOriginPosture = AI_VAULT_BROKERED_ONLY` closed enum, type-level; L5 emits envelopes that L8 enforces network-side) |  enum; closed enum; type-level |
| S13.2 | L5 | S11.1 | L10 | **vocabulary** | S11.1 (`PackageKind = ADAPTER`; `PublisherTrustLevel = AIOS_VERIFIED`) | backtick-group:`PackageKind = ADAPTER` |
| S13.2 | L5 | S14.1 | L9 | **vocabulary** | S14.1 (failure handling — circuit breaker discipline, `DegradationLevel`, anti-cascade rules) | circuit breaker; discipline |
| S13.2 | L5 | S8.1 | L8 | **vocabulary** | S8.1 (`AICrossOriginPosture`; vault-brokered external pattern §5.7; `EXTERNAL_MODEL_CALL_BROKERED` evidence record) | evidence record; vault-brokered |
| S15.1 | L3 | S2.4 | L9 | **vocabulary** | S2.4 Verification Grammar, S3.1 Evidence Log, S1.3 Object Model (versioning), | verification grammar |
| S15.1 | L3 | S3.1 | L9 | **vocabulary** | S3.1 Evidence Log, S1.3 Object Model (versioning), | bare-spec-ref-list |
| S15.1 | L3 | S3.2 | L6 | **vocabulary** | S3.2 Sandbox Composition, S2.4 Verification Grammar, S3.1 Evidence Log, S1.3 Object Model (versioning), | verification grammar |
| S15.2 | L3 | S2.4 | L9 | **vocabulary** | S2.4 Verification | bare-spec-ref-list |
| S15.3 | L3 | S11.1 | L10 | **vocabulary** | runtime safety floor); S11.1 Repository Model (publisher trust chain, `PackageKind = ADAPTER`, Ed25519 signature discipline); S2.4 Verification Engine; S2.3 Policy Kernel; L4.2 Vault Broker; L0 invari... | discipline; trust chain |
| S15.3 | L3 | S2.3 | L4 | **vocabulary** | Ed25519 signature discipline); S2.4 Verification Engine; S2.3 Policy Kernel; L4.2 Vault Broker; L0 invariant catalog (INV-002, INV-008, INV-013, INV-014, INV-015, INV-017) | discipline |
| S15.3 | L3 | S2.4 | L9 | **vocabulary** | Ed25519 signature discipline); S2.4 Verification Engine; S2.3 Policy Kernel; L4.2 Vault Broker; L0 invariant catalog (INV-002, INV-008, INV-013, INV-014, INV-015, INV-017) | discipline |
| S15.3 | L3 | S3.2 | L6 | **vocabulary** | `ExecutionFailureReason`); S0.1 Action Envelope + Lifecycle; S3.2 Sandbox Composition (`SandboxProfile`, runtime safety floor); S11.1 Repository Model (publisher trust chain, `PackageKind = ADAPTER`, ... | discipline; trust chain |
| S2.1 | L2 | S1.2 | L5 | **vocabulary** | S1.2 PrivacyClass enum |  enum |
| S4.1 | L2 | S2.3 | L4 | **vocabulary** | S2.3 (policy condition predicates — type-level) | type-level |
| S4.1 | L2 | S2.4 | L9 | **vocabulary** | S2.4 (verification path-property shape — type-level) |  shape; type-level |
| S4.1 | L2 | S3.1 | L9 | **vocabulary** | S3.1 (record-scoping shape — type-level) |  shape; type-level |
| S4.1 | L2 | S3.2 | L6 | **vocabulary** | S3.2 (sandbox boundary descriptor — type-level) | type-level |
| S5.3 | L4 | S7.1 | L7 | **vocabulary** | S7.1 surface composition, S7.2 shared UI schema | bare-spec-ref-list |
| S5.3 | L4 | S7.2 | L7 | **vocabulary** | S7.2 shared UI schema | bare-spec-ref-list |
| S6.3 | L0 | S3.1 | L9 | **vocabulary** | **Imports vocabulary from**: S3.1 (`RecordType` closed enum + `RetentionClass` enum + segment model + hash chain algorithm — type-level shape co-defined with L9; L0 receipt envelope embeds these witho... |  enum;  shape; closed enum; imports vocabulary; recordtype; shape co-defined; type-level; vocabulary |
| S6.3 | L0 | S4.1 | L2 | **vocabulary** | S4.1 (`(scope, group_id, user_id)` triple — type-level scope shape owned by L2) |  shape; type-level |
| S6.3 | L0 | S5.1 | L4 | **vocabulary** | S5.1 (Subject canonical id format — type-level string format owned by L4) | id format; string format; type-level |
| S6.5 | L6 | S7.1 | L7 | **vocabulary** | runtime safety floor); S7.1 Surface + Composition Model (`CompositionZone`, `SurfaceKind`); S7.4 KDE Plasma Renderer (Wayland session shape); S7.5 Web Renderer (subdomain per-group origins, recovery.l... |  shape |
| S6.5 | L6 | S7.4 | L7 | **vocabulary** | `SurfaceKind`); S7.4 KDE Plasma Renderer (Wayland session shape); S7.5 Web Renderer (subdomain per-group origins, recovery.localhost); S8.1 Network Policy (`OutboundDirective`, `InboundExposureClass`)... |  shape |
| S6.5 | L6 | S7.5 | L7 | **vocabulary** | `SurfaceKind`); S7.4 KDE Plasma Renderer (Wayland session shape); S7.5 Web Renderer (subdomain per-group origins, recovery.localhost); S8.1 Network Policy (`OutboundDirective`, `InboundExposureClass`)... |  shape |
| S6.5 | L6 | S8.1 | L8 | **vocabulary** | recovery.localhost); S8.1 Network Policy (`OutboundDirective`, `InboundExposureClass`); S8.2 GPU Resource Model (`GpuCapabilityClass`, per-group VkDevice); S5.1 Identity (subject canonical id); S5.3 A... | backtick-group:`OutboundDirective` |
| S7.1 | L7 | S3.1 | L9 | **vocabulary** | S3.1 evidence record schema (type-level), S3.2 sandbox profile (type-level), S4.1 namespace catalog (type-level), S5.1 identity (type-level), L0 invariants INV-019..INV-022 (closed-id reference), S8.2... | evidence record; type-level |
| S7.1 | L7 | S8.2 | L8 | **vocabulary** | S8.2 L8 GPU resource model (`gpu_capability_class` closed enum + per-group `VkDevice` brokering API — type-level shape co-defined with L8) |  enum;  shape; closed enum; shape co-defined; type-level |
| S7.2 | L7 | S3.1 | L9 | **vocabulary** | S3.1 (evidence refs) | bare-spec-ref-list |
| S7.4 | L7 | S8.2 | L8 | **vocabulary** | S8.2 GPU Resource Model (per-group `VkDevice` shape, dmabuf brokering capability schema, `gpu_capability_class` closed enum — type-level shape co-defined with L8; renderer requests a `GpuCapabilityBin... |  enum;  shape; closed enum; shape co-defined; type-level |
| S7.5 | L7 | S8.2 | L8 | **vocabulary** | S8.2 GPU Resource Model (per-origin `GPUAdapter` shape, group-bound origin scheme — type-level shape co-defined with L8), L0 INV-006 (web localhost default — closed-id reference), INV-019..INV-022 (cl... |  shape; shape co-defined; type-level |
| S8.1 | L8 | S2.4 | L9 | **vocabulary** | S2.4 Verification Grammar (network primitives queued), S3.1 Evidence Log (record append), S3.2 Sandbox Composition (`NetworkMode` floor binding), S5.1 Identity Model (subject `is_ai`, `recovery_mode`,... | verification grammar |
| S8.1 | L8 | S3.1 | L9 | **vocabulary** | S3.1 Evidence Log (record append), S3.2 Sandbox Composition (`NetworkMode` floor binding), S5.1 Identity Model (subject `is_ai`, `recovery_mode`, `primary_group_id`), S4.1 Namespace Layout (`/aios/gro... | backtick-group:`NetworkMode` |
| S8.3 | L8 | S3.1 | L9 | **vocabulary** | hard-deny vocabulary — type-level); S3.1 Evidence Log (record append shape, FOREVER retention class — type-level); S3.2 Sandbox Composition (sandbox `device_policy` floor schema — type-level); S4.1 Na... |  enum;  shape; closed enum; type-level; vocabulary |
| S8.4 | L8 | S2.4 | L9 | **vocabulary** | S2.4 Verification Grammar (DNS / VPN / mDNS primitives queued), S3.1 Evidence Log (record append), S3.2 Sandbox Composition (`NetworkMode` floor still binds — VPN does not loosen sandbox), S4.1 Namesp... | verification grammar |
| S8.4 | L8 | S3.1 | L9 | **vocabulary** | S3.1 Evidence Log (record append), S3.2 Sandbox Composition (`NetworkMode` floor still binds — VPN does not loosen sandbox), S4.1 Namespace Layout (`/aios/system/network/resolvers/`, `/aios/system/net... | backtick-group:`NetworkMode` |
| S8.5 | L8 | S3.1 | L9 | **vocabulary** | `SCOPE_TOO_BROAD` — type-level); S3.1 Evidence Log (`RecordType` vocabulary, retention classes `STANDARD_24M`/`EXTENDED_60M`/`FOREVER` — type-level); S2.3 Policy Kernel hard-deny vocabulary (type-leve... | recordtype; type-level; vocabulary |
| S9.1 | L1 | S4.1 | L2 | **vocabulary** | S4.1 (`/aios/system/...` recovery-only mutation classes — closed path enum; type-level) |  enum; type-level |
| S9.1 | L1 | S5.1 | L4 | **vocabulary** | **Imports vocabulary from**: S5.1 (`SessionClass.RECOVERY`, `_system` scope subjects — closed-enum identifier shape; type-level) |  shape; imports vocabulary; type-level; vocabulary |
| S9.2 | L1 | S3.1 | L9 | **vocabulary** | S3.1 (`RecordType` vocabulary + retention classes + append-only chain shape — type-level evidence-log schema) |  shape; recordtype; type-level; vocabulary |
| S9.2 | L1 | S4.1 | L2 | **vocabulary** | S4.1 (`/aios/system/...` namespace path enum — type-level) |  enum; type-level |
| S9.2 | L1 | S5.1 | L4 | **vocabulary** | **Imports vocabulary from**: S5.1 (`SubjectKind`, `_system` scope subjects, `SessionClass` — closed enums; type-level) |  enum; closed enum; imports vocabulary; type-level; vocabulary |
| S9.2 | L1 | S5.2 | L4 | **vocabulary** | S5.2 (master-key seal API surface — type-level capability schema co-defined with L4 vault) | type-level |
| S9.3 | L1 | S10.1 | L3 | **vocabulary** | **Imports vocabulary from**: S10.1 Capability Runtime (`ActionLifecycleState`, `ActionDispatchKind = ISOLATED_SANDBOX`, `AdapterIOMode = TYPED_PARAMETERS_ONLY`, `ExecutionFailureReason = EXECUTION_VER... |  shape; imports vocabulary; type-level; vocabulary |
| S9.3 | L1 | S11.1 | L10 | **vocabulary** | S11.1 Repository Model (`PackageKind = KERNEL_CANDIDATE`, `RepositoryKind = AIOS_RECOVERY_REPO`, `UpdateChannel = RECOVERY_CRITICAL` — type-level package vocabulary), S8.2 GPU Resource Model (hardware... | id format; type-level; vocabulary |
| S9.3 | L1 | S3.2 | L6 | **vocabulary** | S3.2 Sandbox Composition (`SandboxProfile`, runtime safety floor, AI-floor — type-level profile schema), S11.1 Repository Model (`PackageKind = KERNEL_CANDIDATE`, `RepositoryKind = AIOS_RECOVERY_REPO`... | type-level |
| S9.3 | L1 | S8.2 | L8 | **vocabulary** | S8.2 GPU Resource Model (hardware graph snapshot id format — type-level), L8 HDM (`HardwareGraphSnapshotId` shape — type-level), S3.1 Evidence Log (`RecordType` + retention classes + hash chain shape ... |  shape; id format; recordtype; type-level |