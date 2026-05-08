# Executive Summary — Rev.2

Status: `PARTIAL` (rev.2 contracts are landing incrementally)

## Purpose of rev.2

Rev.1 established the AIOS vision and the layer model. Rev.2 turns that vision into contract-grade specifications: schemas, state machines, error models, and verification rules that an implementer can build against without further interpretation.

## Rev.1 → Rev.2 delta

To be filled as sub-specs land. Currently tracking:

- **S0.2 (applied):** README's "Self-Evolving Backend" reframed as "Adaptive Backend" — AI proposes; humans approve; Policy Kernel, Evidence Log, Vault Broker, and recovery path are excluded from the proposal pipeline. See [DEC-001](02_design_decisions.md).
- **S0.1 (contract):** Action Envelope and Lifecycle contract. Scope: idempotency, causality, error envelope, lifecycle FSM, versioning, OpenTelemetry hooks, sandbox profile binding, dry-run mode.
- **S1.1 (contract):** Capability Translator. Refined 2026-05-08 with deltas D1–D9: explicit hash encoding (lowercase hex, BLAKE3-256, 32-char truncation aligned with S0.1 §8.5); adversarial-robustness section; statelessness contract; skinny performance contract bridging to S1.2; signed-bundle catalog distribution and trust chain; testable golden fixtures (including Bulgarian utterance and prompt-injection cases); appendix with complete proto IDL; tightened cross-spec field-level mapping; multi-language utterance handling. Refusal taxonomy extended with `TranslationTimeout`. Scope unchanged: capability catalog, RAG-over-capabilities, target binding, ambiguity handling, refusal rules, conservative risk hints, verification intent generation, and translation evidence.
- **S1.2 (contract):** Latency Tiering. Refined 2026-05-08 with deltas D1–D12: typed `LatencyRouter` service with proto IDL; quantitative per-tier latency budgets (T0–T4 p50/p95/p99 + hard timeouts); deterministic priority-ordered routing algorithm replacing the rev.1 condition table; closed `PrivacyClass` enum with per-class tier ceilings; tier upgrade/downgrade dynamics with single-upgrade cap and explicit timeout (no silent fallback); T0 caching semantics with cache key, TTL, and invalidation triggers; adversarial protection (per-subject rate limits, external-model budget guards, defense delegation); statelessness contract; user routing preferences as hints (never enforcement); telemetry contract with bounded label cardinality; full evidence chain (`routing_id` → `translation_id` → `action_id` → evidence receipts); golden routing fixtures including Bulgarian-utterance, secret-bearing, classified, and adversarial cases. Scope unchanged: deterministic direct path, local model path, powerful reasoning path, degradation, and routing evidence.
- **S1.3 (contract draft):** AIOS-FS object model and conflict resolution. Scope: immutable versions, content-addressed chunks, pointer promotion, optimistic concurrency, explicit conflict records, and staged AI merge proposals.
- **S2.1 (contract draft):** AIOS-FS query/view language. Scope: semantic views, constrained DSL, projections, and natural-language-to-query boundaries.
- **S2.2 (contract draft):** AIOS-FS implementation space. Decision: userspace authoritative object store with FUSE/portal projections first; kernel work deferred.
- **S2.3 (contract draft):** Policy Kernel. Scope: default deny, hard denies, request-hash-bound approvals, policy schema, simulation, and decision evidence.
- **S2.4 (contract draft):** Verification grammar. Scope: typed verification primitives, composition, result shape, timeout/skipped semantics, and property checks.
- **S3.1 (contract draft):** Evidence log architecture. Scope: append-only WAL, sealed segments, hash chain, indexes, compaction boundaries, and redaction.
- **S3.2 (contract draft):** Sandbox composition language. Scope: deterministic composition across policy/app/adapter/runtime constraints and Linux enforcement backends.

## Active sub-specs

See [00_MASTER_INDEX.md](00_MASTER_INDEX.md) for the full roadmap.

## Out of scope for rev.2

- Subject identity canonical format (deferred to L4 `03_identity_model.md`)
- Saga / batching composition of actions
- Approval delivery mechanics (deferred to L4 `04_approval_mechanics.md`)
- TTL / expiration policy on queued actions
- Resource budget hints

## See also

- [Rev.1 (frozen) — vision and canonical spec](../001.AI-OS.NET--SPECREV.1/00_MASTER_INDEX.md)
