# Constitutional Invariants (Rev.2)

| Field          | Value                                                                                      |
| -------------- | ------------------------------------------------------------------------------------------ |
| Status         | `CONTRACT` (initial; written 2026-05-09)                                                   |
| Phase tag      | S6.4                                                                                       |
| Layer          | L0 Governance, Evidence, Safety                                                            |
| Schema package | `aios.governance.v1alpha1`                                                                 |
| Consumes       | nothing (L0 is the bottom of the dependency stack)                                         |
| Produces       | the closed list of constitutional invariants and the binding rules every spec must respect |

## 1. Purpose

A constitutional invariant is a system-wide truth that **cannot be loosened by any policy bundle, identity decision, sandbox composition, or operator override**. The invariants in this spec are the AIOS constitution — the rules every other spec is built on top of and every gate must respect. Without a fixed list, "constitutional" becomes opinion; with this list, it becomes a verifiable property.

This spec catalogs the invariants, names where each is enforced, names how each is verified, and defines the bundle format that snapshots the active invariant set for release gating.

## 2. Core invariants of this spec itself

- **I1 — Closed list.** The invariant catalog is a closed enum. Adding an invariant is a versioned spec change. Removing an invariant is a recovery-mode operation.
- **I2 — Each invariant has an enforcer and a verifier.** No invariant is "in spirit only". The spec names the layer/component that enforces it AND the S2.4 verification primitive or property that audits it.
- **I3 — Invariants are signed.** The active invariant bundle (`invbundle_<hex>`) is signed by AIOS root. Unsigned or signature-failing bundles put the governance service in degraded mode (only invariants `INV-001` and `INV-002` active).
- **I4 — Invariants cannot self-promote.** No invariant in this list is added or modified by an action whose subject is `is_ai = true`. Mutation of the invariant list is a recovery-mode operation by a `HUMAN_USER` subject only.
- **I5 — Invariant violations are FOREVER-retained evidence.** Every detection of an invariant violation produces a `TAMPER_DETECTED` evidence record (S3.1) with `invariant_id` populated.

## 3. The invariant catalog

```proto
enum InvariantId {
  INVARIANT_ID_UNSPECIFIED = 0;
  INV_001_RECOVERY_INDEPENDENT_OF_L5 = 1;
  INV_002_AI_PROPOSES_NEVER_EXECUTES = 2;
  INV_003_SECRETS_ARE_CAPABILITIES = 3;
  INV_004_RECOVERY_BOUNDARY = 4;
  INV_005_EVIDENCE_APPEND_ONLY = 5;
  INV_006_WEB_UI_LOCALHOST_DEFAULT = 6;
  INV_007_LAYER_DOWNWARD_DEPENDENCY = 7;
  INV_008_DEFAULT_DENY_POLICY = 8;
  INV_009_APPROVAL_BOUND_AND_EXPIRING = 9;
  INV_010_AI_SELF_APPROVAL_BLOCKED = 10;
  INV_011_CROSS_GROUP_ACCESS_FORBIDDEN = 11;
  INV_012_RECOVERY_REQUIRED_FOR_SYSTEM_MUTATION = 12;
  INV_013_AI_SYSTEM_ADMIN_BLOCKED = 13;
  INV_014_NO_PROOF_NO_COMPLETION = 14;
  INV_015_EVIDENCE_NEVER_CONTAINS_SECRETS = 15;
  INV_016_AI_CANNOT_SELF_GRADE = 16;
  INV_017_SANDBOX_FLOOR_CONSTITUTIONAL = 17;
  INV_018_VAULT_NO_RAW_SECRET_LEAK = 18;
  INV_019_VISUAL_IDENTITY_PRESERVED = 19;
  INV_020_TRUST_INDICATORS_VISIBLE = 20;
  INV_021_AI_HUMAN_VISUAL_DISTINCT = 21;
  INV_022_RECOVERY_AESTHETIC_DISTINCT = 22;
  INV_023_CHROME_ZONE_RESERVED = 23;
  INV_024_GPU_COMPUTE_GATED = 24;
}
```

### INV-001 — Recovery is independent of L5

**Statement:** The system boots into a usable state and recovers from failures without invoking any L5 cognitive component (LLM, planner, agent runtime). The recovery path uses only the L1 substrate, L2 AIOS-FS recovery objects, L4 identity in degraded mode, and operator credentials.

**Why:** an OS that requires AI to recover is not an OS — it is an application running on top of one. Recovery must remain mechanical so that AI failures, model corruption, or LLM provider outages cannot brick the machine.

**Enforced by:** L1 recovery path; L4 identity service degraded mode (only `_system` subjects available without bundle); L9 recovery operations sub-spec.

**Verified by:** S2.4 property `RECOVERY_PATH_INDEPENDENT_OF_L5` (in the existing S2.4 closed `PropertyType` enum). The property is a scheduled audit that ensures no recovery-path code references any L5 module.

**Cannot be loosened by:** any policy bundle, identity bundle, or operator override. Loosening requires recovery-mode invariant-bundle update by a human.

### INV-002 — AI proposes, never executes

**Statement:** AI-classified subjects (`is_ai = true`) emit typed action envelopes; they never execute side-effecting operations directly. Execution is mediated by the Capability Runtime gated by the Policy Kernel; AI can fill envelope fields and submit, but it cannot pass `policy_pending → executing` on its own actions without human or constitutional approval.

**Why:** the value proposition of AIOS is that AI agency is bounded and auditable. Direct execution by AI removes the audit window.

**Enforced by:** S2.3 §26.2.4 `AIInstallInitiationBlocked` (added Wave 9; AI subjects emitting install actions are hard-denied); S2.3 hard-deny `AISystemAdminBlocked`; S2.3 §17 AI self-approval prevention; S6.1 §7 AI cannot self-promote `REAL`; S6.2 §10.6 AI cannot self-grade.

**Verified by:** S2.4 property `POLICY_AI_SELF_APPROVAL_BLOCKED` (existing closed `PropertyType` value).

**Cannot be loosened by:** any policy bundle. The hard-coded constitutional check rejects bundle attempts that would loosen it.

### INV-003 — Secrets are capabilities, not values

**Statement:** Secret material is held by the Vault Broker. AI subjects can request operations on secrets ("sign this with key X", "decrypt this blob with key Y") but cannot retrieve secret bytes. The vault broker performs the operation and returns the result.

**Why:** capability-based secret access prevents prompt-injection and other adversarial flows from exfiltrating secrets through model outputs.

**Enforced by:** L4 vault broker (deferred sub-spec); S2.3 hard-denies on raw-secret-read for AI subjects.

**Verified by:** S2.4 property `VAULT_NO_RAW_SECRET_LEAK` (existing closed `PropertyType` value); see also INV-018.

**Cannot be loosened by:** any policy bundle. Vault broker rejects raw-read requests for AI subjects regardless of capability grants.

### INV-004 — Recovery boundary is preserved

**Statement:** The host filesystem is partitioned into:

- `/` immutable + recovery-safe: read-only at runtime; mutable only by recovery-mode operations.
- `/root` operator island: human operator's home; reachable in recovery; never AI-readable.
- `/aios` AI-native root: the AIOS-FS projection where AI-readable/writeable objects live.

These three roots are constitutional. AIOS-FS objects, agents, and apps live under `/aios/...` per S4.1; `/root` is operator-private; `/` is immutable post-install.

**Why:** an AI cannot corrupt `/`. An operator's recovery surface (`/root`) is not visible to AI. The boundary is mechanical.

**Enforced by:** L1 substrate (mount table); S4.1 namespace catalog (no `/aios/` path resolves into `/root` or vice versa); S3.2 sandbox (filesystem policy denies cross-root access).

**Verified by:** S2.4 property `FILESYSTEM_BOUNDARY_INTACT` (promoted in S2.4 Wave 14 §23.1.1; ID 33 in closed `PropertyType` enum). Composes the existing `aiosfs_path_in_namespace` primitive (S2.4 Wave 4 §17.1) for path-scope assertions over canary paths with the new `filesystem_root_intact(root)` primitive (S2.4 Wave 14 §23.3) and an `evidence_exists` scan for absence of normal-mode `SYSTEM_TREE_MUTATION` outside the recovery window.

**Cannot be loosened by:** policy bundle. Boundary loosening requires mount-table change which is a recovery-mode operation.

### INV-005 — Evidence is append-only

**Statement:** The evidence log is monotonically growing. Records cannot be deleted, modified, or reordered. Compaction (per S3.1) drops old segments according to retention class but never rewrites history.

**Why:** the evidence trail is the audit witness. Tamperable evidence is no evidence.

**Enforced by:** S3.1 evidence log (`Append`-only RPC; hash chain; per-segment Ed25519 signature; `VerifyChain` detects tamper).

**Verified by:** S2.4 properties `EVIDENCE_LOG_APPEND_ONLY` and `EVIDENCE_HASH_CHAIN_INTACT` (existing closed `PropertyType` values).

**Cannot be loosened by:** anything. Tamper detection emits `TAMPER_DETECTED` evidence (FOREVER retention) and triggers operator alert.

### INV-006 — Web UI is localhost-only by default

**Statement:** Web renderer ports listen on `127.0.0.1` and `::1` by default. LAN or remote exposure requires an explicit policy approval and an evidence record (`WEB_EXPOSURE_GRANTED`, FOREVER retention).

**Why:** an AI-mediated UI exposed to the LAN is a remote-control surface. Default-deny on exposure forces explicit operator decision.

**Enforced by:** L7 web renderer config; L8 network policy; S2.3 policy bundle constraints.

**Verified by:** S2.4 property `WEB_UI_LOCALHOST_BOUND` (promoted in S2.4 Wave 14 §23.1.2; ID 34 in closed `PropertyType` enum). Composes the existing `web_renderer_bound_to(host, port)` primitive (S2.4 Wave 5 §18.2) for the positive loopback bind assertion, the existing `port.closed(host="0.0.0.0", port, protocol)` primitive (S2.4 §4) for the external-bind absence assertion (and the `::` IPv6 variant), and the existing `evidence_exists(receipt_id)` primitive for the FOREVER exposure-grant record assertion when a non-loopback bind is intentional.

**Cannot be loosened by:** policy bundle alone — exposure requires both a policy approval and an explicit operator action recorded as `WEB_EXPOSURE_GRANTED` evidence.

### INV-007 — Layers depend downward only

**Statement:** Layer L_n may depend on L_n itself or any L_m with m < n. Dependencies on layers L_n+1 through L_10 are forbidden for correctness. (Optional information flow upward — e.g., L9 telemetry observing L1 — is allowed; correctness must not require it.)

**Why:** upward dependencies invert the dependency stack and make recovery and bring-up impossible.

**Enforced by:** every spec's "Consumes" header table (must list only same-or-lower layers).

**Verified by:** S2.4 property `LAYER_DOWNWARD_DEPENDENCY_HOLDS` (promoted in S2.4 Wave 14 §23.2.1; ID 35 in closed `PropertyType` enum). Composes the new `spec_consumes_table(spec_id)` primitive (S2.4 Wave 14 §23.3) which returns the closed-enum Consumes-row representation for a named sub-spec; a closed predicate over the rows asserts that every `dependency_direction = REQUIRES_FOR_CORRECTNESS` row points at a layer with `target_layer_index ≤ source_layer_index` per the discipline refined in `03_architecture_overview.md` Wave 11. Runs as a scheduled audit over the closed sub-spec list per `00_MASTER_INDEX.md` and as a build-time gate before any spec-tree release tag — converts INV-007 verification from the previous manual architectural audit step into a closed-vocabulary mechanical check (per DEC-049 W11-A intent, DEC-052 W14 closure).

**Cannot be loosened by:** anything. A higher-layer dependency is an architectural defect to be fixed.

### INV-008 — Default deny in policy

**Statement:** If no policy rule matches an action, the decision is `DENY`. Absence of an allow rule is not implicit allow.

**Why:** explicit-allow semantics is the only secure default. AIOS handles too many subject types and too many resources for blocklists to scale.

**Enforced by:** S2.3 Policy Kernel (default decision is `DENY`).

**Verified by:** S2.4 property `POLICY_DEFAULT_DENY_HOLDS` (existing closed `PropertyType` value); the property runs scheduled audit by submitting random unmatched actions and confirming `DENY`.

**Cannot be loosened by:** any policy bundle. The default is hard-coded.

### INV-009 — Approvals bind to one request and expire

**Statement:** An approval is bound to one exact `request_hash` (S0.1 §4) and one approver. Approvals expire (default TTL 5 minutes for INTERACTIVE, 24 hours for batch). Reusing an approval for a different request is rejected.

**Why:** generic "alice approves anything" is a TOCTOU disaster. Per-request binding makes intent explicit.

**Enforced by:** L4 approval mechanics (deferred sub-spec); S0.1 envelope schema; S2.3 policy decision references the approval id.

**Verified by:** S2.4 property `APPROVAL_BOUND_AND_EXPIRING` (promoted in S2.4 Wave 14 §23.1.3; ID 36 in closed `PropertyType` enum). Composes the existing `evidence_exists(receipt_id)` primitive (S2.4 §4) to fetch each approval receipt with the new `approval_binding_state(approval_id)` primitive (S2.4 Wave 14 §23.3) which returns the closed-enum binding shape (`request_hash`, `approver_subject_id`, `issued_at`, `expires_at`, `consumed_at_or_null`, `channel_kind`); closed-enum predicates assert per-request binding, single-approver shape, channel-bounded TTL, and consumption-before-expiration.

**Cannot be loosened by:** any policy bundle.

### INV-010 — AI cannot self-approve

**Statement:** An AI subject cannot serve as the approver for an action the same AI subject submitted. Combined with INV-002 and the hard-coded S2.3 §17 invariant, this means AI actions touching anything risk-flagged require a human approver.

**Why:** approval discipline collapses if the actor is the approver.

**Enforced by:** S2.3 §17 (existing constitutional invariant).

**Verified by:** S2.4 property `POLICY_AI_SELF_APPROVAL_BLOCKED` (existing).

**Cannot be loosened by:** any policy bundle, capability binding, or override.

### INV-011 — Cross-group access forbidden by default

**Statement:** A subject in group A cannot read, list, or write paths under `/aios/groups/<B>/...` for any `B ≠ A`. Single exception: `_system` scope subject + recovery mode + `system_audit_read` capability + human approver.

**Why:** group is the trust boundary. Default-deny across groups prevents lateral compromise.

**Enforced by:** S2.3 hard-deny `CrossGroupAccessForbidden` (per S4.1 §12.4 / S2.3 §26.2.1).

**Verified by:** S2.4 property `NAMESPACE_NO_CROSS_GROUP_POINTERS` (per S4.1 §12.5 / S2.4 §17.2).

**Cannot be loosened by:** any policy bundle.

### INV-012 — Recovery required for system mutation

**Statement:** Mutations to `/aios/system/policy/`, `/aios/system/capabilities/`, `/aios/system/vault/`, `/aios/system/recovery/` require `is_recovery_mode = true` on the subject + a human approver + a `RECOVERY_EVENT` evidence record (FOREVER retention).

**Why:** the constitutional layer of AIOS itself must not be edited from a normal-mode action.

**Enforced by:** S2.3 hard-deny `RecoveryRequiredForSystemMutation` (per S4.1 §12.4 / S2.3 §26.2.2).

**Verified by:** S2.4 property `RECOVERY_GATED_SYSTEM_MUTATIONS` (promoted in S2.4 Wave 14 §23.1.4; ID 37 in closed `PropertyType` enum). Composes the existing `policy.decision(policy_decision_id, expected_decision)` primitive (S2.4 §4) for the `RecoveryRequiredForSystemMutation` admission marker assertion (escape clause via `is_recovery_mode = true` OR the W9-B `is_first_boot = true` first-boot exception) with the existing `evidence_exists(receipt_id)` primitive for the paired FOREVER `RECOVERY_EVENT` (or `FIRST_BOOT_OPERATION` under the W9 escape) record assertion. Runs on every action receipt whose `target.path` matches the closed system-reserved namespace set.

**Cannot be loosened by:** any policy bundle.

### INV-013 — AI cannot perform system admin operations

**Statement:** AI subjects (`is_ai = true`) cannot mutate `/aios/system/apps/` or `/aios/system/agents/` even when holding the `system_admin` capability. The capability is human-only authorization.

**Why:** system mutations are the constitutional layer; AI must remain on the propose-not-execute side of every system-scope action.

**Enforced by:** S2.3 hard-deny `AISystemAdminBlocked` (per S4.1 §12.4 / S2.3 §26.2.3).

**Verified by:** S2.4 property `POLICY_AI_SELF_APPROVAL_BLOCKED` (promoted in S2.4 Wave 10 §21.1.1; ID 23) for INV-002 + INV-010 joint coverage, plus S2.4 property `AI_NEVER_SYSTEM_ADMIN` (promoted in S2.4 Wave 14 §23.1.5; ID 38 in closed `PropertyType` enum) for the system-admin-subtree audit specifically. The latter composes the existing `evidence_exists(receipt_id)` primitive to scan recent action receipts under `/aios/system/apps/...` and `/aios/system/agents/...` with the existing `policy.decision` primitive to inspect each decision trace for `AISystemAdminBlocked` reject codes on AI-subject attempts; any `succeeded` lifecycle phase by an AI subject under either subtree fails the property.

**Cannot be loosened by:** any capability binding or policy bundle.

### INV-014 — No proof, no completion

**Statement:** A capability cannot claim status `REAL` without evidence at the required grade per S6.1 §10 and S6.2 §6. A status claim that exceeds the actual evidence is detected and emits `TAMPER_DETECTED`.

**Why:** the constitutional principle of AIOS development. Without it, reports inflate and trust erodes.

**Enforced by:** S6.1 status taxonomy gates G2..G6, G10; S6.2 grade computation function.

**Verified by:** S2.4 property `STATUS_GRADE_CONSISTENT` (per S6.1 §9.3).

**Cannot be loosened by:** any operator. Even a project lead cannot mark `REAL` without evidence.

### INV-015 — Evidence never contains secrets

**Statement:** Evidence records (per S3.1) never carry secret values, even partially. Secret-bearing operations log capability ids and broker references, not material.

**Why:** evidence is queried by audit subjects, replicated across instances, and sometimes shared. Embedding secrets in evidence makes the log a high-value target.

**Enforced by:** L4 vault broker (S5.2); S3.1 record schema validation; S2.4 Wave 14 `EVIDENCE_NO_SECRET_LEAK` runtime audit. Schema validation in S3.1 catches structurally-typed secret fields; runtime detection of embedded-secret-in-text-payload patterns is now mechanical via the S2.4 property (no longer narrative-deferred).

**Verified by:** S2.4 property `EVIDENCE_NO_SECRET_LEAK` (promoted in S2.4 Wave 14 §23.1.6; ID 39 in closed `PropertyType` enum). Composes the existing `evidence_exists(receipt_id)` primitive (S2.4 §4) for record retrieval with the new `secret_pattern_match(record_id, pattern_catalog_id)` primitive (S2.4 Wave 14 §23.3) which runs a signed closed-enum pattern catalog (`spcat_<hex>`, PEM blocks / password regex / API key prefixes) against the record's canonical JCS payload; hit emits `TAMPER_DETECTED` with `invariant_id = INV_015_EVIDENCE_NO_SECRET_LEAK`. Sampling-based rolling-window audit; the probe inspects only sealed payloads (no raw secret request).

**Cannot be loosened by:** any policy bundle.

### INV-016 — AI cannot grade its own work

**Statement:** AI-authored code cannot be graded by an AI-emitted evidence receipt. Receipts of types `BUILD_PASSED`, `TEST_PASSED`, `E2E_PASSED`, `RECOVERY_REHEARSAL_PASSED`, `RELEASE_GATE_PASSED`, `OPERATIONAL_HEALTHY` naming an AI as producer for an artifact authored by the same AI are rejected with `AgentSelfGradingBlocked` (renamed from `ProducerCannotSelfGrade` in Wave 12 to mirror INV-002 site-2's PascalCase rule + record-stem-form discipline; the FOREVER record name `AGENT_SELF_GRADING_BLOCKED` is unchanged).

**Why:** this is the L0 mirror of INV-002 / INV-010 in the grade axis. Without it, AI could self-promote its own work to `REAL` via emitted evidence.

**Enforced by:** S6.2 §10.6 grade-receipt producer check.

**Verified by:** scheduled audit of evidence records by S2.4 (no specific property; cross-reference of `producer = AI subject` against `authorship` of cited artifact).

**Cannot be loosened by:** any policy bundle or capability binding.

### INV-017 — Sandbox floor is constitutional

**Statement:** The runtime safety floor of S3.2 is part of the constitutional layer. It is signed (`sigfloor_<hex>`) and cannot be loosened by any composition source. Per-class floors (human / AI / recovery) cannot be merged into a single permissive default.

**Why:** the sandbox is the last-line enforcement boundary for action execution. A loosenable floor is no floor.

**Enforced by:** S3.2 §5.4 floor enforcement step (after merge).

**Verified by:** S2.4 property `SANDBOX_PROFILE_MOST_RESTRICTIVE` (existing closed `PropertyType` value).

**Cannot be loosened by:** any policy bundle, app manifest, user request, or adapter default.

### INV-018 — Vault never leaks raw secrets

**Statement:** The Vault Broker performs operations on secrets without exposing the secret material to the requester. Capabilities like "decrypt this blob with key K" return only the operation's result; the requester never sees K.

**Why:** see INV-003. This invariant restates and tightens the rule for the broker side: even a HUMAN_USER subject with a vault capability does not receive raw bytes by default; only a tightly scoped "reveal-to-human" capability (rare, requires recovery + human approver) returns material, and that operation is logged with FOREVER retention.

**Enforced by:** L4 vault broker (deferred sub-spec).

**Verified by:** S2.4 property `VAULT_NO_RAW_SECRET_LEAK` (existing closed `PropertyType` value).

**Cannot be loosened by:** any policy bundle. The reveal-to-human path is itself an INV-003-aware exception.

### INV-019 — Visual identity preserved across renderers

**Statement:** AIOS visual language is constitutional. The KDE Plasma renderer (S7.4), Web renderer (S7.5), CLI renderer (S7.6), Voice renderer, and Mobile renderer cannot invent their own chrome or trust UX. Cross-renderer visual identity must be recognizable as AIOS — an operator using KDE and an operator using Web must instantly know they are using the same OS.

**Why:** without this invariant, each renderer drifts toward its host platform's defaults; AIOS looks like a generic app on each surface; the trust model degrades visually because security indicators look like generic notifications instead of constitutional chrome.

**Enforced by:** L7 renderer specs binding to the shared visual language contract (S7.3 Visual Language); S7.1 Surface + Composition Model defining composition zones and chrome rules; S7.2 Shared UI Schema constraining per-renderer surface schemas to the same vocabulary; renderer build gate rejects chrome divergence.

**Verified by:** S2.4 property `RENDERER_VISUAL_IDENTITY_PRESERVED` (added to the closed `PropertyType` enum). The property is a scheduled audit that checks each renderer's chrome rendering against the visual language contract; scheduled audit will be wired in S2.4 at consolidation.

**Cannot be loosened by:** any policy bundle, theme override, accessibility profile, or operator action. Visual identity is constitutional.

### INV-020 — Trust indicators cannot be hidden by app surfaces

**Statement:** The security indicator showing subject `is_ai` (per L4 identity §10), `action_id` (per S0.1), and the evidence link (per S3.1) is always visible. App surfaces (per S7.1's `SurfaceKind = APP_SURFACE`) cannot overdraw, obscure, or fake the AIOS chrome zone. The CHROME zone is always on top in S7.1's composition model.

**Why:** trust indicators are how the operator knows who acted (AI or human), what they did (action id), and where to verify (evidence link). An app surface that can hide them becomes a phishing vector inside the OS itself.

**Enforced by:** S7.1 composition zones — the CHROME zone is always on top; renderer rejects app-surface attempts to write into the CHROME zone; KWin layer-shell protocol enforces the top layer on KDE (S7.4); DOM `z-index` plus shadow root on Web (S7.5); CLI (S7.6) reserves a status row no app stream can overwrite.

**Verified by:** S2.4 property `TRUST_INDICATORS_ALWAYS_VISIBLE` (added to the closed `PropertyType` enum). The audit confirms that the chrome zone is rendered above all app surfaces in every renderer; scheduled audit will be wired in S2.4 at consolidation.

**Cannot be loosened by:** app manifest, user request, capability binding, or fullscreen mode. Fullscreen apps still see AIOS chrome.

### INV-021 — AI vs human action visually distinct

**Statement:** Every UI representation of an action — in evidence views, audit trails, approval queues, and action streams — visually distinguishes whether an AI subject (`is_ai = true` per L4 identity §10) or a human subject performed it. The distinct treatments are part of the constitutional visual language; the same treatment cannot be reused across AI and human actions.

**Why:** the AIOS trust model rests on the operator knowing at a glance whether they are looking at AI output or a human action. Visual ambiguity erases the bounded-AI-agency property that the rest of the constitution depends on.

**Enforced by:** S7.3 Visual Language spec defining `color.action.ai` and `color.action.human` as separate semantic tokens, with iconography and possibly typography distinctions; S7.2 Shared UI Schema binds those tokens to subject-axis renderer hints; renderer implementations (S7.4 KDE, S7.5 Web, S7.6 CLI) must bind to those tokens; renderer conformance tests reject token reuse across the AI/human axis.

**Verified by:** S2.4 property `AI_HUMAN_VISUAL_DISTINCTION` (added to the closed `PropertyType` enum). The audit confirms there is no overlap between AI and human visual treatments in rendered output; scheduled audit will be wired in S2.4 at consolidation.

**Cannot be loosened by:** theme override (themes can change colors but cannot collapse the distinction), accessibility profile (a11y modes use different distinctions like patterns or icons but the distinction must remain), or app skinning.

### INV-022 — Recovery mode aesthetically distinct from normal mode

**Statement:** The recovery shell — entered via the L1 recovery boot path, identity per L4 §7 `_system` scope subjects — is visually unmistakable from normal AIOS. Recovery uses different chrome treatment, a different accent palette, and stricter composition rules (no app surfaces in recovery mode per S7.1's recovery-mode composition rules). The operator must instantly recognize that they are in recovery.

**Why:** a recovery operator typing destructive commands into what they think is normal mode is a catastrophe. Visual distinction is the last line of defense before that mistake. Combined with INV-012 (recovery required for system mutation), this prevents the "I thought I was rehearsing" failure mode.

**Enforced by:** S7.1 separate surface stack for recovery (no `APP_SURFACE` allowed); S7.3 Visual Language defining a recovery-only theme that cannot be matched by any normal-mode theme; recovery accent tokens are gated to `is_recovery_mode = true` rendering paths in S7.4 (KDE), S7.5 (Web), and S7.6 (CLI).

**Verified by:** S2.4 property `RECOVERY_AESTHETIC_DISTINCT` (added to the closed `PropertyType` enum). The audit confirms that recovery rendering uses recovery-only tokens not present in any normal-mode theme; scheduled audit will be wired in S2.4 at consolidation.

**Cannot be loosened by:** theme override, accessibility profile, or any operator-changeable setting. The recovery aesthetic is locked at boot time and cannot be changed mid-session.

### INV-023 — CHROME composition zone is reserved for trust surfaces

**Statement:** The `CHROME` composition zone (per S7.1 closed `CompositionZone` enum) is reserved exclusively for renderer-owned trust surfaces authored by the AIOS system identity. AI subjects (`is_ai = true` per L4 §10) cannot author CHROME-zone content under any circumstance. `APP_SURFACE`-kind and `STREAM_SURFACE`-kind surfaces cannot be promoted into the CHROME zone, regardless of subject. Any authorship or promotion attempt fails closed.

**Why:** the CHROME zone hosts the operator's last-mile trust indicators — approval prompts, evidence links, security badges, identity chips, recovery-mode markers (per INV-020). If any other zone or any AI subject could compose into CHROME, the trust path collapses: an AI agent could synthesise a "Granted" badge over a denied action, an app surface could overlay the recovery-mode banner, or a marketplace skin could repaint the action-origin chip. The integrity of CHROME is the integrity of operator consent.

**Enforced by:** S7.1 Surface + Composition runtime (rejects any non-system surface targeting `zone = CHROME` and rejects subject-id mismatch on CHROME nodes); L4.1 Policy Kernel constitutional hard-deny `CompositionZoneForbidden` (§27.2.1) which fires before bundle rules; renderer conformance tests (S7.4 KDE, S7.5 Web, S7.6 CLI) reject CHROME-zone authorship by any subject other than the renderer's `aios_chrome` system identity.

**Verified by:** S2.4 property `CHROME_ZONE_RESERVED` (added to the closed `PropertyType` enum). The audit walks every live surface, confirms `surface_kind ∈ {AIOS_SURFACE}` for all entries with `zone = CHROME`, and confirms no AI subject appears as author for any CHROME node. Scheduled audit will be wired in S2.4 at consolidation.

**Cannot be loosened by:** theme override (themes can change CHROME tokens but cannot change CHROME authorship), accessibility profile, fullscreen mode, kiosk mode, app manifest declaration, capability binding, or any policy bundle. The CHROME zone is constitutional.

### INV-024 — GPU compute access is capability-gated

**Statement:** Access to `GPU_COMPUTE_HEAVY` (per L8.2 closed `GpuCapabilityClass` enum) requires an explicit `gpu.compute_heavy` capability grant tracked by the L4 capability catalog. The default capability set does not include this grant. Generic adapter capability negotiation cannot synthesise this access. Workloads that need GPGPU compute must request and receive the capability before any compute submission is dispatched to the device.

**Why:** GPGPU compute is the highest-cost, highest-risk renderer-adjacent resource. Unbounded GPU compute is the canonical vector for cross-group side-channels (residual memory probing, timing leaks across VkDevice partitions), sustained thermal and energy abuse, and exfiltration via shader timing. Default-deny with explicit capability grant ensures an operator (or, in recovery mode, an emergency override per S5.4) has authorised the workload class. Combined with the L8.2 per-group VkDevice partitioning, this invariant bounds GPU compute to declared-and-approved usage.

**Enforced by:** L8.2 GPU resource model — capability negotiation rejects compute submission without the active grant; L4.1 Policy Kernel constitutional hard-deny `GpuComputeOutsideAuthorisedClass` (§27.2.2) which fires before bundle rules; L4.3 capability binding records the grant with TTL, scope, and grant-evidence pointer; L9 telemetry (`gpu_compute_class_total`) reports per-class submission counts so out-of-class usage is visible operationally.

**Verified by:** S2.4 property `GPU_COMPUTE_GATED` (added to the closed `PropertyType` enum). The audit walks active GPU compute submissions and confirms every one has a live `gpu.compute_heavy` capability binding for the submitting subject; absence triggers `TAMPER_DETECTED` evidence with `invariant_id = INV_024_GPU_COMPUTE_GATED`. Scheduled audit will be wired in S2.4 at consolidation.

**Cannot be loosened by:** app manifest, capability auto-grant on install, generic adapter capability negotiation, fullscreen privilege, or any policy bundle. The capability is granted only through the L4 grant flow, which itself requires an approval (S5.3) — and outside recovery mode, hard-denied attempts can only be unblocked by an emergency override (S5.4) which records FOREVER evidence.

## 4. Invariant bundle (`invbundle_<hex>`)

The active invariant set is loaded from a signed bundle:

```proto
message InvariantBundle {
  string version = 1;            // invbundle_<hex_lower(BLAKE3(jcs(this)))[:32]>
  google.protobuf.Timestamp issued_at = 2;
  string issuer = 3;             // "aios-root"
  bytes ed25519_signature = 4;
  repeated InvariantEntry entries = 10;
}

message InvariantEntry {
  InvariantId id = 1;
  string title = 2;
  string statement = 3;
  string enforced_by = 4;        // free text reference
  string verified_by = 5;        // S2.4 property or primitive id
  bool active = 6;               // false only for retired (post-mutation) invariants
  google.protobuf.Timestamp activated_at = 7;
  google.protobuf.Timestamp retired_at = 8;
}
```

The bundle is loaded at startup. Signature failure puts the governance service in degraded mode: only INV-001 (recovery independence) and INV-002 (AI proposes never executes) remain active, blocking all higher-layer operation until a valid bundle is loaded.

Bundle update is a recovery-mode operation by a `HUMAN_USER` subject. The transition emits `INVARIANT_BUNDLE_LOADED` evidence (FOREVER retention).

## 5. Adversarial robustness

### 5.1 Bundle tampering

Any tampered bundle fails Ed25519 verification at load. The governance service does not run unsigned bundles; it falls back to degraded mode.

### 5.2 Invariant bypass attempts

Each invariant has a verifier (S2.4 property or primitive). Scheduled property audits run continuously. A detected bypass produces `TAMPER_DETECTED` evidence with `invariant_id` populated and triggers operator alert.

### 5.3 Layered loosening

Some invariants reference policy bundles, sandbox floors, or capability bindings. The "cannot be loosened by" rules are checked at the relevant enforcer. Attempting to load a policy bundle that loosens an invariant fails the bundle's signature check at S2.3 (the bundle compiler rejects invariant-loosening rules).

### 5.4 Invariant retirement

Removing an invariant from active list requires:

1. Recovery-mode operation by `HUMAN_USER`.
2. The `InvariantEntry.active = false` AND `retired_at` set.
3. `INVARIANT_RETIRED` evidence (FOREVER retention) with the operator's canonical id.

A retired invariant cannot be re-activated; activating again requires a new InvariantId with a separate audit trail.

## 6. Cross-spec dependencies

| Spec | Direction  | What this spec contributes                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| ---- | ---------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| S2.4 | producer   | twelve constitutional verification properties promoted: six visual-identity properties landed in S2.4 Wave 5 §18.1 (`RENDERER_VISUAL_IDENTITY_PRESERVED`, `TRUST_INDICATORS_ALWAYS_VISIBLE`, `AI_HUMAN_VISUAL_DISTINCTION`, `RECOVERY_AESTHETIC_DISTINCT`, `CHROME_ZONE_RESERVED`, `GPU_COMPUTE_GATED`); the remaining six constitutional verifiers landed in S2.4 Wave 14 §23.1 (`FILESYSTEM_BOUNDARY_INTACT` §23.1.1, `WEB_UI_LOCALHOST_BOUND` §23.1.2, `APPROVAL_BOUND_AND_EXPIRING` §23.1.3, `RECOVERY_GATED_SYSTEM_MUTATIONS` §23.1.4, `AI_NEVER_SYSTEM_ADMIN` §23.1.5, `EVIDENCE_NO_SECRET_LEAK` §23.1.6). |
| S3.1 | producer   | new record types `INVARIANT_BUNDLE_LOADED` FOREVER, `INVARIANT_RETIRED` FOREVER, `WEB_EXPOSURE_GRANTED` FOREVER                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| S6.1 | constraint | gate G6 (acceptance passing) checks invariant compliance for the capability                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| S6.2 | constraint | grade `E4` for any capability impacting `INV_001..INV_024` requires invariant verification                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| L7   | constraint | renderer specs (S7.1 Surface + Composition Model, S7.2 Shared UI Schema, S7.3 Visual Language, S7.4 KDE Plasma, S7.5 Web, S7.6 CLI) must bind to invariants `INV_019..INV_023` and surface conformance evidence for each renderer                                                                                                                                                                                                                                                                                                                                                                                |
| L8   | constraint | L8.2 GPU resource model is the enforcer of `INV_024_GPU_COMPUTE_GATED`; capability negotiation must reject compute submission without an active `gpu.compute_heavy` grant                                                                                                                                                                                                                                                                                                                                                                                                                                        |

## 7. Golden fixtures

### Fixture 1 — Bundle signature failure puts service in degraded mode

```text
Startup: load invbundle_<hash> → Ed25519 verify fails.
Result: governance service degraded mode. Only INV-001 + INV-002 active.
Effect: every action mutating policy/capabilities/vault/recovery rejected (INV-012-equivalent).
        Every AI execution attempt rejected (INV-002).
        Operator alert raised; recovery operation required.
```

### Fixture 2 — Detected invariant violation

```text
Scheduled audit: S2.4 property RECOVERY_PATH_INDEPENDENT_OF_L5 fails — recovery code references an L5 module.
Result: TAMPER_DETECTED evidence (FOREVER) with invariant_id = INV_001.
       Operator alert.
       Capability(ies) named in the audit demoted to UNKNOWN per S6.1 §5.3.
```

### Fixture 3 — Bundle update via recovery

```text
Boot into recovery mode.
Subject: _system:local:operator-247 (HUMAN_USER).
Action: load new invbundle_<new_hash>.
Result: bundle signature verified; entries diffed against previous; transition emits INVARIANT_BUNDLE_LOADED evidence FOREVER.
```

### Fixture 4 — Policy bundle attempting to loosen an invariant

```text
Operator submits a policy bundle whose rules would override CrossGroupAccessForbidden.
S2.3 bundle compiler rejects with InvariantLooseningAttempted.
   → bundle is NOT loaded.
   → INVARIANT_LOOSENING_REJECTED evidence emitted (FOREVER retention).
```

### Fixture 5 — Invariant retirement

```text
Hypothetical: INV-006 (web UI localhost default) is replaced by a finer-grained spec.
Required steps:
  1. Boot into recovery.
  2. Operator submits invariant retirement action.
  3. Bundle update sets InvariantEntry.active = false, retired_at = now.
  4. INVARIANT_RETIRED evidence (FOREVER) emitted.
  5. New invariant introduced under a new InvariantId in the same bundle update.
```

## 8. Telemetry contract

| Metric                                           | Type    | Labels (closed)                                    |
| ------------------------------------------------ | ------- | -------------------------------------------------- |
| `governance_invariant_violation_total`           | counter | `invariant_id` (closed enum, 24 entries)           |
| `governance_invariant_bundle_load_total`         | counter | `result` (success/signature_failure/parse_failure) |
| `governance_invariant_loosening_rejection_total` | counter | `attempted_loosening_class` (closed enum)          |
| `governance_active_invariants`                   | gauge   | none                                               |
| `governance_degraded_mode`                       | gauge   | none (1 = degraded, 0 = normal)                    |

Cardinality budget: ≤ 30 active label tuples per metric.

## 9. Acceptance criteria

- [ ] `InvariantId` is a closed enum with 24 values (corresponding to the 24 invariants in §3).
- [ ] Each invariant in the catalog (§3) has an explicit Statement, Why, Enforced by, Verified by, and Cannot be loosened by section.
- [ ] All ten new S2.4 properties are added to the closed `PropertyType` enum (`FILESYSTEM_BOUNDARY_INTACT`, `WEB_UI_LOCALHOST_BOUND`, `APPROVAL_BOUND_AND_EXPIRING`, `RECOVERY_GATED_SYSTEM_MUTATIONS`, `AI_NEVER_SYSTEM_ADMIN`, `EVIDENCE_NO_SECRET_LEAK`, `RENDERER_VISUAL_IDENTITY_PRESERVED`, `TRUST_INDICATORS_ALWAYS_VISIBLE`, `AI_HUMAN_VISUAL_DISTINCTION`, `RECOVERY_AESTHETIC_DISTINCT`).
- [ ] Three new evidence record types added to S3.1 vocabulary (`INVARIANT_BUNDLE_LOADED` FOREVER, `INVARIANT_RETIRED` FOREVER, `WEB_EXPOSURE_GRANTED` FOREVER).
- [ ] Invariant bundle (`invbundle_<hex>`) is signed by AIOS root; signature failure puts governance service in degraded mode.
- [ ] Invariant retirement requires recovery mode + HUMAN_USER + FOREVER-retained evidence.
- [ ] Detected invariant violations emit `TAMPER_DETECTED` evidence with `invariant_id` populated.
- [ ] All five golden fixtures (§7) produce the specified outcomes.
- [ ] Telemetry conforms to §8 cardinality bounds.

## 10. Open deferrals

- **Cross-machine invariant federation** — when AIOS becomes multi-host, invariants must agree across hosts. Deferred.
- **Invariant evolution policy** — process for amending invariants, sunsetting, replacing. Currently informal; formalization deferred.
- **Operator drill scenarios** — simulated invariant violations to test detection. Deferred to L9 admin operations sub-spec.
- **Per-invariant verification SLO** (e.g., "INV-005 audited every 60 seconds") — deferred.

## See also

- [S6.1 — Status Taxonomy](01_status_taxonomy.md)
- [S6.2 — Evidence Grades](02_evidence_grades.md)
- [S2.4 — Verification Grammar](../L9_Observability_Admin_Operations/02_verification_grammar.md)
- [S3.1 — Evidence Log](../L9_Observability_Admin_Operations/01_evidence_log.md)
- [S0.1 — Action Envelope and Lifecycle](../XX_Cross_Cutting/01_action_envelope_lifecycle.md)
- [S2.3 — Policy Kernel](../L4_Policy_Identity_Vault/01_policy_kernel.md)
- [S3.2 — Sandbox Composition](../L6_Apps_Packages_Compatibility/04_sandbox_composition.md)
- [S4.1 — Namespace Layout](../L2_AIOS_FS/05_namespace_layout.md)
- [S5.1 — Identity Model](../L4_Policy_Identity_Vault/03_identity_model.md)
- [Rev.1 §6 / §7 — Layer rules and Governance](../../001.AI-OS.NET--SPECREV.1/02_SPECIFICATION.md)
- [L0 Overview](00_overview.md)
- [Rev.2 Master Index](../00_MASTER_INDEX.md)
