# Design Decisions — Rev.2

Decision log. Each entry follows ADR (Architecture Decision Record) discipline: context, decision, consequences, status. Entries are append-only; superseded decisions are marked as such, never deleted.

---

## DEC-001 — README "Self-Evolving Backend" reframed as "Adaptive Backend"

- **Context:** Rev.1 README contained a "Self-Evolving Backend" section that implied autonomous AI patching of backend code, in conflict with the spec discipline of typed actions, policy gating, and evidence-first execution. Rev.1 SPECIFICATION.md never sanctioned self-evolution.
- **Decision:** Reframe as "Adaptive Backend": AI may _propose_ backend patches, kernel adaptation adjustments, distro compatibility profiles, and new runtime adapters; production promotion always requires human approval. The Cognitive Core may not modify the Policy Kernel, the Evidence Log, the Vault Broker, or the recovery boot path through this pipeline.
- **Consequences:** README and SPECIFICATION are now consistent on the AI's bounded execution model. The pipeline (Observe → Propose → Sandbox simulation → Tests → Human review → Staged deployment → Monitor → Rollback) is fully explicit.
- **Status:** `REAL` (applied in initial commit `be318da`)
- **Phase tag:** S0.2

---

## DEC-002 — Repository layout: revision-per-folder, layer-per-subfolder

- **Context:** Rev.1 was a single `SPECIFICATION.md` file. As rev.2 grows, a single file becomes unwieldy. Comparable projects (NeuroCAD `000.000.Roadmap/NEUROCAD_UNIFIED_PLATFORM_SPEC_REV*`) use one folder per revision with topic-named files inside.
- **Decision:** For AIOS:
  - One folder per revision: `001.AI-OS.NET--SPECREV.1/`, `002.AI-OS.NET--SPECREV.2/`, …
  - Inside each rev.2+ folder, one folder per layer (L0–L10) plus a `XX_Cross_Cutting/` folder for contracts shared by multiple layers.
  - Inside each layer folder, files numbered `00_overview.md`, `01_<topic>.md`, `02_<topic>.md`, …
  - Rev.1 stays as a flat verbatim move (the original two files) since it pre-dates this convention.
- **Consequences:** Clean navigation, easy to grow (e.g., L4 will hold Policy Kernel, Vault Broker, and Identity Model as three separate sub-specs). Slightly more nesting than NeuroCAD's flat-files-per-revision approach, but justified by AIOS layers containing multiple distinct sub-systems each.
- **Status:** `REAL` (applied in this commit)
- **Phase tag:** infrastructure

---

## DEC-003 — Action Envelope + Lifecycle (S0.1) design choices

- **Context:** Rev.1 §13 sketched an action envelope and lifecycle but left major gaps: idempotency, causality, formal error model, lifecycle FSM precision, schema versioning, OpenTelemetry handling, sandbox profile binding, and dry-run modes. S0.1 closes these per scope option **B — Pragmatic+** (agreed at brainstorming).
- **Key decisions captured in [`XX_Cross_Cutting/01_action_envelope_lifecycle.md`](XX_Cross_Cutting/01_action_envelope_lifecycle.md):**
  - **Envelope style:** custom AIOS-native, proto-first, with `request`/`execution` field-level separation borrowed from Kubernetes API machinery. CloudEvents (event-style, not command-style) and full K8s API machinery (overkill for AIOS) were rejected.
  - **Top-level partition:** `schema_version`, `identity`, `request`, `execution`, `trace`. `identity` and `request` are caller-set and immutable after creation; `execution` is runtime-set and mutates over the lifecycle; `trace` is transport metadata.
  - **IDs:** ULID (Crockford base32, 26 chars), prefix-namespaced (`act_`, `intent_`, `plan_`, `corr_`, `evr_`, `polreq_`, `poldec_`, `appr_`). Chosen over UUID for chronological sortability and compactness.
  - **Idempotency:** Stripe-style separate `idempotency_key` field with 24h default TTL. Same key + same `hash(request)` deduplicates; same key + different hash returns `IdempotencyConflict`.
  - **Causality:** single-parent `parent_action_id` with cycle detection. Multi-parent saga deferred.
  - **Lifecycle:** five-phase FSM (`PENDING`, `RUNNING`, `SUCCEEDED`, `FAILED`, `ROLLED_BACK`) with K8s-style `conditions[]` for fine-grained facts. Six valid transitions; terminal phases are truly terminal (post-hoc rollback is a new envelope, not a transition).
  - **Result and Error:** two independent optional fields (not `oneof`), enabling `ROLLED_BACK` to populate both (cause + rollback summary). Population rules enforced by Capability Runtime.
  - **Error taxonomy:** ~30 canonical PascalCase codes grouped into validation/policy/auth/execution/verification/rollback/infrastructure. Recursive `cause` chain bounded at depth 8. `retryable` hint as a non-binding caller hint. English `message`; localization is a renderer concern.
  - **Versioning:** proto API style (`v1alpha1` → `v1betaN` → `v1` → `v2`), not SemVer. Stable allows additive changes only; breaking changes require major version bump with 12-month dual-version support.
  - **Canonical encoding:** proto3 deterministic serialization for the wire and idempotency hashes; JCS (RFC 8785) for JSON projection in evidence. **BLAKE3** (256-bit) for all canonical hashes.
  - **Trace context:** standard W3C Trace Context, no AIOS-specific extensions; child actions inherit parent `trace_id`.
  - **Sandbox profile:** resolved at execution start as `max_restriction(policy_required, caller_request, adapter_default)`. Fail-closed if all unset.
  - **Dry-run modes:** `LIVE` (default), `VALIDATE` (schema/idempotency only), `SIMULATE` (full path with simulated execution; sandbox still applied; evidence marked `simulated=true`). Adapters that handle destructive operations must declare a `simulate` capability.
  - **gRPC interface:** simplified from rev.1's nine RPCs to six. Single entry point `SubmitAction` (replaces `Validate`/`EvaluatePolicy`/`Execute`/`Verify`/`Rollback` orchestration). Streaming `WatchAction` replaces polling. New `GetCapabilityRuntimeInfo` for version negotiation.
- **Out of scope (intentionally deferred):** subject canonical format (L4 identity), saga composition, approval delivery mechanics (L4), TTL/expiration, resource budgets.
- **Consequences:** L3 (Capability Runtime), L4 (Policy Kernel client), L5 (Cognitive Core), L9 (Evidence) now have a stable contract to build against. Phase 1 sub-specs (Capability Translator S1.1, Latency Tiering S1.2, AIOS-FS object model S1.3) can proceed without re-litigating action contract questions.
- **Status:** `CONTRACT` (design approved 2026-05-07; awaits implementation evidence to escalate to `REAL`).
- **Phase tag:** S0.1
- **Schema version:** `aios.action.v1alpha1`
