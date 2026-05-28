# S11.4 — Integration Process (aios.integration.v1alpha1)

| Field               | Value                                                                                                                       |
| ------------------- | --------------------------------------------------------------------------------------------------------------------------- |
| **Status**          | SHELL                                                                                                                       |
| **Phase**           | S11.4                                                                                                                       |
| **Layer**           | L10 — Distribution / Ecosystem / Marketplace                                                                                |
| **Schema package**  | `aios.integration.v1alpha1`                                                                                                 |
| **Crate**           | `crates/aios-integration` (v0.0.1, M18 open)                                                                                |
| **Depends on**      | L0 (constitutional truth), L3 (SGR), L4 (Policy), L5 (Cognition), L7 (Renderers), L8 (Hardware/Network), L9 (Observability) |
| **Sub-spec status** | STUB — full specification lands at T-186 (M18 closer)                                                                       |
| **Created**         | 2026-05-28 (T-175, M18 opening task)                                                                                        |
| **Owner**           | claude-ds (DeepSeek-V4-Pro) via Governor workflow                                                                           |

---

## 1. Purpose

The Integration Process layer (S11.4) defines the typed contracts that bind
the M1…M17 crates — from `aios-action` through `aios-hardware` — into one
cohesive AIOS distribution. It provides:

1. A **lifecycle FSM** for every external integration (vendor contracts,
   standards subscriptions, CVE feeds, composed subsystems) with exactly 6
   states: Proposed → Evaluated → Piloted → Production → Deprecated → Retired.

2. A **vendor registry** (typed `VendorIntegrationContract`) with signed
   Ed25519 attestations, trust classes, rotation cadences, and breach
   playbook URLs.

3. A **standards subscription discipline** covering 11 regulator frameworks
   (NIST 800-53, 800-218 SSDF, 800-207 Zero Trust, 800-193 Firmware, DISA
   STIG, CIS Controls v8, FIPS 140-3, GDPR, HIPAA, ISO 27001, SOC 2) with
   timed review windows and responsible-party pinning.

4. A **CVE feed framework** that binds upstream vulnerability data to
   composed service graphs, with typed severity (Low/Medium/High/Critical)
   and remediation status (Open → UnderReview → Patched → Quarantined →
   NotApplicable).

5. A **system composition graph** (`ServiceComposition`) that records which
   services depend on which, their binding endpoints, and a topological
   ordering that the orchestrator binary enforces at boot and at recovery.

6. An **orchestrator binary** (future, T-182) that consumes the composition
   graph, boots services in order, monitors health, and fails safe when the
   graph is broken.

The layer is the **integration architecture backbone** of AIOS — without it,
the 17 crates are a loose collection of typed libraries rather than a
coherent, verifiable, policy-governed distribution.

---

## 2. Core invariants

These invariants are placeholder contracts until T-186 finalises the full spec.
Each invariant maps to a verification gate that the M18 closer must produce.

### I1 — Lifecycle FSM

Every integration resource (vendor contract, standard subscription, CVE binding,
composed system) MUST obey the 6-state lifecycle FSM defined in
`crates/aios-integration/src/lifecycle.rs`. Transitions between non-adjacent
states SHALL be denied with `IntegrationErrorCode::LifecycleInvalidTransition`.

- `Proposed` → `Evaluated` → `Piloted` → `Production` → `Deprecated` → `Retired`
- `Deprecated` MAY revert to `Production` (sunset cancelled).
- `Retired` is terminal; no forward transition is possible.
- `Proposed` MAY be rejected (transition to `Retired` with `reason` set).

### I2 — Signed vendor contracts

Every `VendorIntegrationContract` admitted into the registry MUST carry an
Ed25519 signature over the canonical JSON representation of the contract
metadata (canonicalisation per RFC 8785 JCS). Signature verification is the
sole gate for contract admission; trust classification (`AiosCertifiedPartner`,
`CommunityVerified`, `OperatorAuthorised`, `BlacklistedDoNotAdmit`) is a
policy-layer operation that may override or augment the signature check.
Implementation lands in T-176.

### I3 — Standards subscription discipline

Every `StandardSubscription` MUST have a timed review window (`next_review_due_at`)
and a pinned responsible party (`responsible_canonical_id`). Expired subscriptions
SHALL emit `IntegrationErrorCode::StandardSubscriptionExpired` and the
subsystem consuming the subscription SHALL degrade its compliance posture
accordingly (the degradation policy is defined per-standard, not in this layer).

### I4 — CVE feed framework

CVE severity is typed as an ordered closed enum (`Low < Medium < High < Critical`).
CVE status is an ordered closed enum (`Open → UnderReview → Patched`; or
`Open → Quarantined`; or `Open → NotApplicable`). The `PackageCveBinding`
(struct linking a `CveId` to a package in the composition graph) lands in T-178;
T-175 only defines the closed enums and the `CveId` newtype.

### I5 — System composition graph

The `ServiceComposition` struct defines a directed acyclic graph of services,
their dependencies, and a topological order. Cyclic dependencies are a hard
error (`IntegrationErrorCode::CompositionCycleDetected`). The topological
order is computed at verification time (T-181), not at construction time — the
in-memory struct carries an unverified `topological_order: Vec<String>` field
that the verification pass must validate against `services` and `dependencies`.

### I6 — Orchestrator binary

The orchestrator binary (future, T-182) MUST boot services in the topological
order defined by the verified `ServiceComposition`. It MUST fail safe —
`IntegrationErrorCode::OrchestratorBootFailed` — if any service fails to start
or if the composition graph is broken. No AIOS subsystem SHALL be started
outside the orchestrator's control once in production.

---

## 3. Milestone decomposition (M18)

| Task  | Refs       | What                                                                | Depends on          |
| ----- | ---------- | ------------------------------------------------------------------- | ------------------- |
| T-175 | NOW        | Typed skeleton (FSM, enums, newtypes, error catalogue) + S11.4 stub | —                   |
| T-176 | Next       | `VendorIntegrationContract` registry, Ed25519-signed admission      | T-175               |
| T-177 |            | `StandardSubscription` registry, review-window scheduler            | T-175               |
| T-178 |            | `PackageCveBinding`, CVE feed polling scaffold                      | T-175               |
| T-179 |            | External bridges (repository mirror, OCI sync, metrics exporter)    | T-176, T-177, T-178 |
| T-180 |            | Integration control map (gRPC surface for L9 observability)         | T-179               |
| T-181 |            | `ServiceComposition` topological-sort verifier                      | T-175               |
| T-182 |            | Orchestrator binary (`aios-orchestrator`)                           | T-181               |
| T-183 |            | gRPC service layer (typed RPCs for L9/L10 interop)                  | T-180               |
| T-184 |            | Evidence bridge (S11.4 ↔ S3.1 evidence log)                         | T-183               |
| T-185 |            | System integration tests (end-to-end composition + lifecycle)       | T-182, T-184        |
| T-186 | M18 CLOSER | Final spec + closure/acceptance gates                               | T-185               |

### Task assignment note

The Governor assigns T-175 through T-186 sequentially. T-175 is the M18
opening task; T-186 is the M18 closer. The Governor may inject plan-revision
tasks between T-175 and T-186 if the spec evolves during M18 implementation.

---

## 4. Compliance mapping (forward reference)

| Standard                     | What this layer enforces                    | Where                                                         |
| ---------------------------- | ------------------------------------------- | ------------------------------------------------------------- |
| NIST 800-53 Rev.5            | CM-8 (system component inventory)           | `ServiceComposition` (I5)                                     |
| NIST SP 800-218 (SSDF)       | PS.1.1 (secure design principles)           | Lifecycle FSM (I1)                                            |
| NIST SP 800-207 (Zero Trust) | Policy decision point                       | Policy Kernel (L4); this layer registers the integration      |
| NIST SP 800-193 (Firmware)   | Platform firmware resiliency                | `StandardSubscription` to firmware standards (I3)             |
| DISA STIG                    | Hardening checklists                        | `StandardSubscription` to STIG (I3)                           |
| CIS Controls v8              | Control 1 (inventory), Control 2 (software) | `ServiceComposition` (I5) + `VendorIntegrationContract` (I2)  |
| FIPS 140-3                   | Cryptographic module validation             | `VendorIntegrationContract.signer_fingerprint` (I2)           |
| GDPR Art. 30                 | Records of processing                       | `VendorIntegrationContract` (I2, data processor registration) |
| HIPAA §164.308               | Administrative safeguards                   | `StandardSubscription` to HIPAA (I3)                          |
| ISO 27001 A.8                | Asset management                            | `ServiceComposition` (I5)                                     |
| SOC 2 CC6.1                  | Logical and physical access controls        | `VendorTrustClass` + `VendorIntegrationContract` (I2)         |

---

## TODO — Filled at T-186 closure

- [ ] **§5** — Detailed lifecycle FSM: transition table, guard conditions, rollback semantics.
- [ ] **§6** — Vendor contract schema: canonical JSON shape, Ed25519 envelope, revocation list.
- [ ] **§7** — Standards subscription registry: review-window cron, auto-escalation on expiry.
- [ ] **§8** — CVE feed binding: poller architecture, severity-to-action mapping, NVD / OSV schema mapping.
- [ ] **§9** — System composition: DAG verification algorithm, boot order, health-check protocol.
- [ ] **§10** — Orchestrator binary: CLI, systemd unit, graceful shutdown, recovery integration.
- [ ] **§11** — gRPC surface: `IntegrationService` protobuf contract, interop with L9 observability.
- [ ] **§12** — Evidence bridge: how integration-layer events map to S3.1 evidence log entries.
- [ ] **§13** — Security considerations: supply-chain attack surface, vendor key compromise, CVE propagation delay.
- [ ] **§14** — Performance budget: integration-layer overhead (boot time, CVE poll latency, signature verification).
- [ ] **§15** — Acceptance gates: end-to-end test plan for M18 closure.
- [ ] **§16** — Migration path: how existing AIOS crates register with the integration layer.
