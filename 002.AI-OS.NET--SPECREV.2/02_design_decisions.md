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

---

## DEC-004 — Capability Translator as catalog-bound compiler, not command generator

- **Context:** Rev.1 required that AI never directly execute shell commands and instead produce typed actions. S0.1 defined the action envelope, but L5 still needed a precise contract for translating natural-language intent and planner steps into those envelopes at large catalog scale.
- **Decision:** Define the Capability Translator as a catalog-bound compiler. It may use LLMs for parsing, extraction, explanation, and candidate suggestion, but every READY result must validate against a known capability manifest, the selected target schema, and the S0.1 envelope schema. Vector retrieval is allowed for candidate discovery but cannot be final authority. Ambiguity returns clarification; missing schema fields block translation; unknown actions and free-form shell execution are rejected.
- **Consequences:** AIOS gets a scalable path for thousands of typed capabilities without becoming a prompt-to-shell system. L3 adapter manifests become the source of truth for action availability. L4 remains the policy authority. L9 receives translation evidence for auditability. S1.2 can now specify latency routing between direct translation and cognitive translation without redefining translator semantics.
- **Status:** `CONTRACT` draft (awaiting review before design approval)
- **Phase tag:** S1.1

---

## DEC-005 — Complete Rev.2 roadmap as contract drafts before implementation planning

- **Context:** The project is still in specification mode. After S0.1, the remaining roadmap entries needed enough precision to preserve the architecture without producing implementation plans against absent code.
- **Decision:** Fill the active Rev.2 roadmap as contract drafts: Latency Tiering (S1.2), AIOS-FS Object Model and Conflict Resolution (S1.3), Query/View Language (S2.1), Implementation Space (S2.2), Policy Kernel (S2.3), Verification Grammar (S2.4), Evidence Log (S3.1), and Sandbox Composition (S3.2). These documents define boundaries, invariants, schemas, and acceptance criteria, but do not prescribe a build plan.
- **Consequences:** Rev.2 now reads as a coherent agent contract pack rather than isolated notes. The strongest architectural decisions are explicit: userspace-first AIOS-FS, catalog-bound translation, deterministic latency routing, default-deny policy, append-only evidence, typed verification, and most-restrictive sandbox composition.
- **Status:** `CONTRACT` draft
- **Phase tag:** Rev.2 completion pass

---

## DEC-010 — S2.1 + S2.2 AIOS-FS query language and implementation space refinement (deltas D1–D12)

- **Context:** S2.1 (107 lines) and S2.2 (75 lines) landed as thin drafts in commit `dfa3be5`. Both are L2 AIOS-FS concerns and tightly coupled (the query engine depends on what the storage layer can index efficiently); refining them in one cycle keeps consistency.
- **Decision:** Apply twelve combined deltas across the two files without scope expansion:
  - **D1 — Formal EBNF grammar (S2.1):** closed operator vocabulary with `from` / `where` / `group by` / `order by` / `limit` / `offset` / `as of` / `project`; sources are `objects`/`versions`/`pointers`/`transactions`/`conflicts`/`evidence`; `and` only (no `or`); aggregations bounded to `count/max/min/first/last`. No arbitrary expressions.
  - **D2 — Proto IDL + `AIOSFSQuery` gRPC service (S2.1):** `aios.fs.query.v1alpha1` package; `ExecuteQuery`/`ExplainQuery`/`CreateView`/`RebuildView`/`ListViews`/`DeleteView`/`GetQueryEngineInfo` RPCs. Appendix A holds full IDL.
  - **D3 — Materialization model (S2.1):** virtual vs materialized; refresh strategies `ON_DEMAND`/`ON_WRITE`/`SCHEDULED`/`MANUAL`; invalidation triggers; cost guidance for choosing materialized.
  - **D4 — Privacy class filter (S2.1):** results above the caller's ceiling are silently excluded; counts and aggregations exclude them; an evidence record reports the excluded count without leaking object IDs.
  - **D5 — Time-travel queries (S2.1):** `AS OF <version_id>` and `AS OF <timestamp>` evaluate against historical snapshots; bounded by 90-day default transaction-log retention; materialization forbidden for `as of` queries.
  - **D6 — Pagination, budget, timeout (S2.1):** OFFSET_LIMIT and CURSOR pagination; per-query wall-clock, memory, result-size, and scan-row budgets; cursor TTL 30 min; budget exhaustion fails closed (no partial results).
  - **D7 — NL→query bridge (S2.1):** explicit contract that the engine never accepts NL; S1.1 translator owns NL→DSL; canonical DSL is the only query input; evidence linkage required for queries touching `SECRET_BEARING`/`CLASSIFIED` sources.
  - **D8 — Golden fixtures + telemetry (S2.1):** nine fixtures (simple filter, time-travel, privacy filter, forbidden field, cursor pagination, materialized refresh, aggregation, `in` clause, budget exhausted); bounded-cardinality metrics; subject is never a metric label.
  - **D9 — Backing storage choice (S2.2):** RocksDB primary for chunks/objects/versions/pointers/transactions/WAL; SQLite WAL mode for metadata catalog; Tantivy for lexical/full-text; embedding store deferred to L5 vector sub-spec. Rationale per option recorded.
  - **D10 — Crash consistency, snapshot/backup, encryption (S2.2):** WAL fsync per `CommitTransaction` followed by atomic write batch on primary CFs; recovery replays from last checkpoint; ZFS/Btrfs snapshots preferred, LVM fallback, logical-export tarball for portability; encryption at rest delegated to LUKS/dm-crypt/ZFS native; per-object encryption deferred to L4 vault sub-spec.
  - **D11 — Performance targets and resource budgets (S2.2):** p95 budgets per operation on reference hardware (8-core 16 GB NVMe); memory budgets per subsystem (RocksDB block cache 1 GB, Tantivy 512 MB, SQLite cache 64 MB) with backpressure on exhaustion.
  - **D12 — Acceptance fixtures and migration (S2.2):** seven fixtures (WAL replay, atomic CAS, snapshot round-trip, POSIX import idempotency, no in-band encryption, FUSE rebuild, backpressure); `posix-to-aiosfs` import + lossy export; AIOS-FS-to-AIOS-FS migration via logical export.
- **Consequences:** S2.1 grows from 107 to 731 lines; S2.2 from 75 to 421 lines (combined 182 → 1152). The query engine and storage backend now have implementer-grade contracts. Storage choice is settled; future kernel/distributed work is deferred to follow-on sub-specs without blocking rev.2 implementation.
- **Status:** `REAL` (applied 2026-05-08).
- **Phase tag:** S2.1 + S2.2 refinement.

## DEC-009 — S1.3 AIOS-FS Object Model + Conflict Resolution refinement (deltas D1–D12)

- **Context:** S1.3 landed as two thin drafts in commit `dfa3be5`: object model (170 lines) and conflict resolution (94 lines). Architecturally correct (immutable versions, content-addressed chunks, optimistic concurrency, sibling versions, AI proposes-not-promotes) but missing typed surfaces, hash encoding precision, GC contracts, CRDT vocabulary, and operational mechanics.
- **Decision:** Apply twelve combined deltas without scope expansion:
  - **D1 — Hash encoding explicit:** chunk IDs use **full** BLAKE3-256 lowercase hex (64 hex chars, no truncation) — chunks need full collision resistance for persistent storage handles. Metadata digests follow S0.1 §8.5 truncation rule. Distinction recorded explicitly.
  - **D2 — Proto IDL + gRPC service:** `aios.fs.v1alpha1` package; `AIOSFSObjects` service with `BeginTransaction`/`WriteVersion`/`PromotePointer`/`CommitTransaction`/`AbortTransaction`/`ReadObject`/`ReadVersion`/`ReadChunk`/`EnumerateObjects`/`ListConflicts`/`ResolveConflict`/`RebuildIndexes`/`QuarantineVersion`/`ExitQuarantine`/`RetireObject`/`PurgeObject`. Appendix A in object_model holds full IDL; Appendix B in conflict_resolution adds conflict-specific records.
  - **D3 — Transaction model with multi-pointer atomicity:** transactions can write multiple versions and promote multiple pointers; all CAS succeed or all fail (two-phase commit fence within one AIOS-FS instance).
  - **D4 — Chunking strategy + GC contract:** FastCDC defaults (min=64KB, avg=256KB, max=1MB); fixed-size permitted as fallback for streaming workloads; orphan staging TTL 24h; reference counting via active version refs; GC passes are evidence-logged, never silent.
  - **D5 — Privacy class and object lifecycle:** `PrivacyClass` field on objects mirrors the S1.2 §5 enum (`PUBLIC`/`INTERNAL`/`SENSITIVE`/`SECRET_BEARING`/`CLASSIFIED`); class can be raised but not lowered. Object lifecycle adds `ACTIVE`/`RETIRED`/`PURGED` with default 90-day retention; shortening retention requires policy approval.
  - **D6 — Pointer move CAS protocol:** atomic compare-and-swap on `(pointer_id, expected_current_version_id)`; standardized `ConflictDetected` error gateway to the conflict resolution sub-spec; multi-pointer transactions emit one conflict per failed pointer.
  - **D7 — Read consistency model:** `SNAPSHOT` (default; consistent across pointers as of read time), `LINEARIZABLE` (latest committed; for synchronization), `EVENTUAL` (for views; may lag).
  - **D8 — Quarantine semantics:** triggers (validation/integrity/policy/external attestation/operator), effects (pointer rollback, restricted reads), exits (manual review, automated re-validation), 30-day quarantine TTL leading to `RETIRED_VERSION`, evidence trail.
  - **D9 — CRDT vocabulary:** closed set `G_COUNTER`/`PN_COUNTER`/`OR_SET`/`LWW_REGISTER`/`OR_MAP`/`RGA_TEXT`; per-object-kind merge policy declares which CRDT applies; new types require additive enum bump.
  - **D10 — Conflict timeout, notification, authority:** 30-day default TTL → `ABANDONED` status with evidence; push notifications via L9 evidence stream with debouncing (5s coalescing window); pull via `ListConflicts`; resolution authority is owner / collaborator / operator-with-override, evaluated by L4 Policy Kernel.
  - **D11 — AI merge proposal validation:** structural rejection rules in code (proposal must include base/current/candidate/resolution/explanation/verification/evidence; rejected on empty verification, missing evidence, secret-shaped explanation per S1.1 §17.2.6, or AI involvement at privacy class beyond merge policy allowance); auto-promote requires `PUBLIC`/`INTERNAL` class plus explicit policy opt-in.
  - **D12 — Golden fixtures and telemetry contracts:** seven fixtures for object_model (write-promote, CAS conflict, multi-pointer atomicity, quarantine, GC, privacy monotonicity, recovery enumerate, snapshot read); seven fixtures for conflict_resolution (default reject, CRDT auto-merge, AI redaction, multi-pointer conflicts, TTL abandon, unauthorized resolve, resolution race). Telemetry metrics with bounded label cardinality; subject never a metric label.
- **Consequences:** S1.3 grows from 264 to 1428 lines combined (object_model 913, conflict_resolution 515). Object model becomes the authoritative source for the L2 gRPC surface; conflict resolution operates on the object model's CAS protocol. Multi-pointer transactions are now contract-grade. AI auto-promote conditions are explicit and verifiable.
- **Status:** `REAL` (applied 2026-05-08).
- **Phase tag:** S1.3 refinement.

## DEC-008 — S1.2 Latency Tiering refinement (deltas D1–D12)

- **Context:** S1.2 landed as a 157-line draft in commit `dfa3be5`. Architectural shape was correct (5 tiers, T0/T1 must work without external AI, no tier bypasses safety) but the contract was tilt-heavy on bullet lists and missing typed surfaces, numbers, and adversarial defense.
- **Decision:** Apply twelve deltas without scope expansion:
  - **D1 — Proto IDL + `LatencyRouter` service:** typed `RoutingRequest`, `RoutingDecision`, `RoutingResponse`, `RoutingEvidence`, `RouterInfo`. Stable identifier `rt_<ULID>`. `Route()` and `GetRouterInfo()` RPCs. Full IDL in Appendix A.
  - **D2 — Quantitative per-tier budgets:** T0 p95 < 10 ms, T1 < 50 ms, T2 < 200 ms, T3 < 800 ms, T4 < 3 s; hard timeouts; cold start budgets. Aligned with S1.1 §19; this section is now authoritative.
  - **D3 — Deterministic routing algorithm:** priority-ordered guards (recovery mode → privacy class → policy egress → budget → rate limit) followed by tier selection (T0 cache → exact match → lexical → ambiguity → multi-step → default). Lowest tier wins on tie.
  - **D4 — `PrivacyClass` enum:** closed set `PUBLIC`, `INTERNAL`, `SENSITIVE`, `SECRET_BEARING`, `CLASSIFIED` with per-class tier ceiling table. Default `SENSITIVE`; secret detection raises to `SECRET_BEARING`; class can be raised but never lowered.
  - **D5 — Tier dynamics:** auto-upgrade rules (T1→T2 on miss, T2→T3 on low confidence + bind fail, T3→T4 on multi-step) capped at one upgrade per request; auto-downgrade on tier unavailable; hard timeout returns `TIMEOUT` outcome (never silent fallback).
  - **D6 — Evidence chain:** `routing_id → translation_id → action_id → evidence_receipt_ids`. `RoutingEvidence.translation_ids[]` filled when downstream translations produced. `correlation_id` flows through all four levels.
  - **D7 — Adversarial protection + cost model:** per-subject rate limits (60 routing/min, 20 T3/min, 10 T4/hour, 200 external/day defaults); external model budget guards with 80%/95%/100% threshold actions; defense delegation table.
  - **D8 — T0 caching semantics:** explicit cacheable / never-cacheable rules; cache key formula `rtc_<hex_lower(BLAKE3(JCS(...)))[:32]>`; TTL defaults (60 s status, 5 m views); invalidation triggers (catalog flip, evidence-event, session expiry, operator flush).
  - **D9 — Telemetry contract:** Prometheus-compatible metrics with bounded label cardinality (subject is never a label); histogram buckets for latency; required counters and gauges enumerated.
  - **D10 — Golden routing fixtures:** ten `{ input, expected, status }` triples covering exact, fuzzy, ambiguous, multi-step, recovery, secret-bearing, budget-exhausted, Bulgarian, adversarial-rate-limit, and classified-context cases.
  - **D11 — Statelessness contract:** router stateless across `Route()` calls (mirrors S1.1 §18 discipline); reproducibility input set documented.
  - **D12 — User preferences:** `RoutingPreference` with `prefer_local`, `prefer_speed`, `prefer_quality` as hints; policy and guards always override preferences; preferences are per-call, not persisted by router.
- **Consequences:** S1.2 grows from 157 to 933 lines. Becomes authoritative for latency budgets (S1.1 §19 was already deferring). New canonical refusal paths via `RoutingOutcome.REFUSED` and `RoutingOutcome.TIMEOUT`.
- **Status:** `REAL` (applied 2026-05-08).
- **Phase tag:** S1.2 refinement.

## DEC-007 — S1.1 Capability Translator refinement (deltas D1–D9)

- **Context:** S1.1 landed as a draft in commit `dfa3be5` together with the rest of the rev.2 roadmap (DEC-005). The draft was structurally sound but missing several contract-grade pieces.
- **Decision:** Apply nine deltas without scope expansion:
  - **D1 — Hash encoding explicit:** `catalog_version` and `idempotency_key` use `hex_lower(BLAKE3(...))[:32]` (lowercase hex, first 32 chars = 128 bits). Aligns with S0.1 §8.5 and removes encoding ambiguity across language runtimes.
  - **D2 — Adversarial robustness section:** explicit threat model (prompt injection, secret smuggling, ambiguity exploitation, action aliasing, embedded shell, social engineering of `reason`); structural defenses (catalog-only action names, schema-only target fields, no shell surface, deterministic identity, sanitized `reason`, secret-shaped redaction); behavioural defenses (ambiguity-loses, high-risk + underspecified ⇒ clarification, closed refusal vocabulary); defense delegation table.
  - **D3 — Statelessness contract:** translator is stateless across `TranslateIntent` calls; reproducibility from `(request, catalog_version, code_version)`; allowed local optimizations are observably indistinguishable from re-computing; state outside the translator is enumerated.
  - **D4 — Performance contract (skinny):** budget shape per path (direct < 50 ms, lexical < 200 ms, RAG < 800 ms, reasoning < 3 s); cold start and reload budgets; backpressure rules (direct path always available; reasoning shed first); resource limits.
  - **D5 — Catalog distribution and trust:** signed bundles; AIOS root → publisher → bundle trust chain; hot-reload semantics with grace period; explicit operator-only rollback; out-of-scope items routed to L4, L10, and a future adapter-distribution sub-spec.
  - **D6 — Testable golden fixtures:** §17 examples reformatted as `{ input, expected, status }` triples for an acceptance harness, including Bulgarian utterance, prompt-injection refusal, and secret-exfiltration refusal.
  - **D7 — Appendix A: complete proto IDL:** single concatenated proto file consolidating sections 5 and 14.
  - **D8 — Tightened cross-spec references:** field-level mapping to S0.1, S1.2, S1.3, S2.3, S2.4, S3.1.
  - **D9 — Multi-language utterances:** UTF-8 accepted; Intent Engine owns language detection and normalization; manifest `aliases_localized` extension; full i18n deferred to renderer-side sub-spec.
- **Consequences:** S1.1 grows from 877 to 1340 lines but covers the contract-grade gaps without re-litigating the architectural shape from DEC-004. New canonical refusal code `TranslationTimeout`. Acceptance criteria now reference golden fixtures (§21) and trust-chain rejection of unendorsed publishers.
- **Status:** `REAL` (applied 2026-05-08).
- **Phase tag:** S1.1 refinement.

## DEC-006 — ProxGuard used as prototype donor and optional AIOS app

- **Context:** Local ProxGuard materials contain a working control-plane architecture pattern: manifest-driven simulation, deterministic policy, runtime adapters, release packaging, inbox handoff, production apply, audit events, DNS provider abstraction, and golden path tests. These patterns overlap with AIOS L3 Capability Runtime, L4 Policy Kernel, L8 Network, and L9 Evidence requirements.
- **Decision:** Treat ProxGuard as a reference donor for concepts and acceptance-test shape, and as a candidate optional AIOS infrastructure app. AIOS may adapt the patterns into AIOS-native contracts: `CapabilityManifest`, typed action target schemas, dry-run execution, sealed approved action packages, isolated executor inbox, evidence receipts, deterministic policy reason codes, DNS capability adapters, and golden path proof scripts. As an app, ProxGuard may live under `/aios/apps/proxguard` and expose typed capabilities such as `proxguard.service.simulate`, `proxguard.service.deploy`, `proxguard.dns.plan`, `proxguard.dns.apply`, `proxguard.gateway.route`, and `proxguard.audit.read`.
- **Rejected:** Importing ProxGuard product UI, billing/Paddle flows, SaaS workspace assumptions, managed cloud provisioner scope, NGINX/OpenResty product scope, branding, or website content into AIOS Rev.2.
- **Consequences:** AIOS gets a practical starting shape for the first real Capability Runtime proof and a realistic first system app candidate while keeping its own layer model, Rust-owned execution runtime, action envelope, evidence log, and policy semantics. ProxGuard Python code remains a prototype/reference artifact until tests prove donor behavior and AIOS-native ports or package wrappers are written.
- **Status:** `CONTRACT` reference note; direct code reuse `DEFERRED`; donor runtime health `UNKNOWN`.
- **Evidence:** E1 local artifact inspection only; ProxGuard runtime tests were not run during this spec pass.
- **Phase tag:** R1 reference donor
- **Document:** [`XX_Cross_Cutting/02_proxguard_reference_model.md`](XX_Cross_Cutting/02_proxguard_reference_model.md)
