# Marketplace — Publisher Onboarding, Capability Review, Listings (Rev.2)

| Field          | Value                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| -------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Status         | `REAL` (initial; written 2026-05-09; E1 evidence — file exists, structural contract complete)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| Phase tag      | S11.2                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| Layer          | L10 Distribution, Ecosystem, Marketplace                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                |
| Schema package | `aios.marketplace.v1alpha1`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| Consumes       | L0 INV-002 (AI proposes never executes), INV-008 (default-deny policy), INV-013 (AI cannot perform system admin), INV-014 (no proof no completion); S11.1 Repository Model (`PublisherTrustLevel`, `PackageKind`, `PackageManifest`, three-tier trust chain, seventeen-step install pipeline, `PUBLISHER_DEPLATFORMED`, `pubcat_<hex>` publisher catalog); S5.3 Approval Mechanics (`request_approval`, `EXACT_ACTION` binding, `ApprovalStrength`); S3.1 Evidence Log (`RecordType` vocabulary, `FOREVER` / `EXTENDED_60M` / `STANDARD_24M` retention classes); S3.2 Sandbox Composition (sandbox floor for the marketplace app itself); S12.1 App Runtime Model (`EcosystemHonestyClass` mandatory disclosure); S12.4 Compatibility Knowledge (referenced abstractly — ratings + outlier detection); S4.1 Namespace Layout (marketplace app is `USER_SCOPED` install) |
| Produces       | typed `PublisherOnboardingState` / `OnboardingTier` / `CapabilityReviewOutcome` / `ListingVisibility` enums; the `PublisherApplication` contract; the closed onboarding workflow FSM; the capability declaration review discipline; the marketplace listing model; the publisher reputation model; the listing-vs-manifest mismatch detection contract; the multi-reviewer requirement for `VERIFIED_PUBLISHER`; twelve evidence record types queued for S3.1; the contract that the marketplace UX is itself an AIOS `APP` package                                                                                                                                                                                                                                                                                                                                     |

## §1 Purpose

S11.1 closed the **mechanical** trust loop: every package's signature chains to the AIOS root, every install runs the seventeen-step pipeline, every capability lie quarantines on first-run audit, every deplatform burns a FOREVER record. What S11.1 explicitly did **not** specify is the **social** loop above the mechanics: how a new publisher becomes admissible to the trust chain in the first place; how a package's `declared_capabilities` are reviewed before the operator sees a listing; what the operator actually reads on the marketplace screen before pressing the install button; how reputation accumulates, decays, and feeds back into trust.

This sub-spec closes that loop. It is the **admission contract** of AIOS: a publisher cannot self-promote into `VERIFIED_PUBLISHER` trust; a package cannot self-promote its declared capabilities past the listing without review; an operator cannot be shown a listing whose visible trust class is weaker than the verified one without the UI itself being deceptive (a constitutional defect). The marketplace is the surface where humans look at humans, and where the spec must be honest about what humans see.

Five constitutional risks define the threat model, each addressed by a named mechanism in this contract:

1. **Fake publisher applications** — an attacker submits an application impersonating a legitimate organisation. Addressed by the identity verification stage, the legal-entity binding for `VERIFIED_PUBLISHER`, and the two-reviewer requirement for the `IDENTITY_VERIFICATION_PENDING → TECHNICAL_REVIEW` transition.
2. **Capability bait-and-switch** — a publisher declares a narrow capability set in the listing UX, but the actual `PackageManifest.declared_capabilities` (S11.1 §5) is broader. Addressed by the listing-vs-manifest cross-check at install time, with a FOREVER `LISTING_VS_MANIFEST_MISMATCH` evidence record and immediate listing downgrade.
3. **Review bypass via social engineering** — a single AIOS-root reviewer is compromised, coerced, or careless. Addressed by the multi-reviewer requirement for the `VERIFIED_PUBLISHER` tier (`ApprovalStrength = DUAL` per S5.3 §3.3) and by the mandatory technical + security review separation.
4. **Coordinated rating manipulation** — a publisher pumps ratings for their own listings or smears a competitor. Addressed by referencing S12.4's outlier detection abstractly and by reputation being multi-dimensional (a five-star rating cannot mask a `capability_lie_history > 0`).
5. **Deceptive listing UX** — a listing claims `VERIFIED_PUBLISHER` to the operator while the publisher is actually `COMMUNITY_PUBLISHER`. Addressed by **strict UI binding**: the listing renderer reads the publisher's trust tier directly from the AIOS-root-signed publisher catalog (`pubcat_<hex>`), not from any field the publisher controls.

This spec is the **second** L10 contract surface (after S11.1). The third sub-spec, `03_external_integrations.md`, builds on both — bridges to Flathub / OCI / distro repos must onboard as `EXTERNAL_BRIDGE_OPERATOR` per this contract and inherit the `COMMUNITY` capability ceiling per S11.1 §3.1.

## §2 Scope

This spec **defines**:

1. The closed `PublisherOnboardingState` enum with eight states.
2. The closed `OnboardingTier` enum with four tiers.
3. The closed `CapabilityReviewOutcome` enum with five outcomes.
4. The closed `ListingVisibility` enum with five visibilities.
5. The `PublisherApplication` proto contract: every field, validation rule, failure mode.
6. The publisher onboarding FSM (closed states, strictly forward except `DEPLATFORMED` re-entry from S11.1).
7. The per-tier review depth: `AIOS_ROOT_INTERNAL`, `VERIFIED_PUBLISHER`, `COMMUNITY_PUBLISHER`, `EXTERNAL_BRIDGE_OPERATOR`.
8. The capability declaration review discipline (each `declared_capability` reviewed with justification text; reviewer can `APPROVED_AS_DECLARED`, `APPROVED_WITH_NARROWED_SCOPE`, `DEFERRED_NEEDS_INFO`, `REJECTED_INSUFFICIENT_JUSTIFICATION`, `REJECTED_DECEPTIVE`).
9. The marketplace listing model (`Listing` proto): metadata, capability list with operator-friendly explanations, ratings reference (S12.4), `EcosystemHonestyClass` disclosure (S12.1), price (if any), signing-chain visualisation.
10. The marketplace UX is itself an AIOS app: `PackageKind = APP`, `InstallScope = USER_SCOPED`, sandbox profile per S3.2 floor.
11. The publisher reputation model: multi-dimensional score (`security_history`, `support_responsiveness`, `capability_lie_history`, `deplatform_count`).
12. The search and discovery rules: bounded by trust tier; `AIOS_ROOT_INTERNAL` shown first; `COMMUNITY_PUBLISHER` shown with explicit warning.
13. The strict UI binding rule: listing trust class is read from the AIOS-root-signed publisher catalog, not from any publisher-controlled field.
14. The listing-vs-manifest cross-check at install time: declared listing capabilities must match `PackageManifest.declared_capabilities` exactly; mismatch triggers immediate `QUARANTINED` and FOREVER evidence.
15. Adversarial robustness: fake publisher applications, capability bait-and-switch, review bypass via social engineering, coordinated rating manipulation, deceptive listing UX.
16. Bounded-cardinality telemetry contract for the marketplace surface.
17. Twelve evidence record types queued for S3.1.
18. Three worked examples (VERIFIED publisher onboarding flow, capability review with `APPROVED_WITH_NARROWED_SCOPE`, deceptive listing detected at install).

This spec **does not** define:

- The wire format for external bridges (Flathub mirror, OCI re-packaging) — `03_external_integrations.md` (`SHELL`).
- The full schema of S12.4 ratings + outlier detection beyond referencing the contract surface; this spec consumes those fields and does not redefine them.
- The pixel-level UX of the marketplace renderer (L7 owns the rendering primitives; this spec specifies the **information architecture** the renderer must honour).
- Monetary settlement, payments, refund, tax handling — deferred entirely; the `price` field on a listing is informational only in this revision.
- Localisation of capability explanations — the contract is that explanations exist and are operator-friendly; the localisation pipeline is out of scope.
- HSM-backed signing for publisher applicant key proof — deferred (the proof in this revision is an Ed25519 challenge-response; HSM integration deferred to a later sub-spec).
- Cross-host federation of reputation scores — every host computes reputation locally from its own evidence stream; cross-host aggregation is deferred.

This spec is the **contract surface** that any future marketplace renderer (L7), any external-bridge admission (`03_external_integrations.md`), and any reputation aggregator (deferred) consume.

## §3 Vocabulary (closed enums)

All vocabularies in this section are closed. Adding a value is a versioned spec change. The marketplace state engine, the application validator, and the listing renderer MUST reject values outside the enum at parse time. None of these enums admits an `OPEN` or `OTHER` value.

### §3.1 `PublisherOnboardingState`

Closed enum, eight states. The first six form the forward FSM; the last two (`REJECTED`, `DEPLATFORMED`) are terminal. `DEPLATFORMED` mirrors the S11.1 catalog state — it is recorded here for symmetry but the authoritative state lives in `pubcat_<hex>` per S11.1 §3.1.

| Value                           | Semantics                                                                                                                                  |
| ------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------ |
| `APPLICATION_DRAFT`             | Applicant has begun the form but has not submitted. Visible to applicant only; not yet on any reviewer queue.                              |
| `APPLICATION_SUBMITTED`         | Applicant submitted the application bundle (identity material, technical contacts, signing-key proof). Awaiting initial reviewer pickup.   |
| `IDENTITY_VERIFICATION_PENDING` | A reviewer is verifying applicant identity. For `VERIFIED_PUBLISHER`: legal entity check, point-of-contact check, liability documentation. |
| `TECHNICAL_REVIEW`              | A reviewer is examining the technical contacts, signing-key proof, and any submitted draft `PackageManifest` payloads.                     |
| `SECURITY_REVIEW`               | A reviewer (must be a different person than the technical reviewer for `VERIFIED_PUBLISHER`) examines manifests of intended packages.      |
| `APPROVED_VERIFIED`             | All review stages passed; `publisher_root_id` granted; entry written to `pubcat_<hex>` per S11.1 §3.1.                                     |
| `REJECTED`                      | Terminal: application denied. Reason recorded in FOREVER evidence (`PUBLISHER_ONBOARDING_REJECTED`).                                       |
| `DEPLATFORMED`                  | Terminal mirror of S11.1 `PublisherTrustLevel = DEPLATFORMED`. Recorded here so the onboarding FSM has a unified terminal vocabulary.      |

Allowed forward transitions:

```text
APPLICATION_DRAFT
  └─▶ APPLICATION_SUBMITTED
        └─▶ IDENTITY_VERIFICATION_PENDING
              ├─▶ TECHNICAL_REVIEW
              │     ├─▶ SECURITY_REVIEW
              │     │     ├─▶ APPROVED_VERIFIED
              │     │     └─▶ REJECTED
              │     └─▶ REJECTED
              └─▶ REJECTED

APPROVED_VERIFIED ─▶ DEPLATFORMED   (only via S11.1 takedown discipline)
```

Back-transitions are forbidden except `* → REJECTED` (any review stage may terminate the application) and the S11.1-driven `APPROVED_VERIFIED → DEPLATFORMED` event. Re-applying after `REJECTED` requires a new `PublisherApplication` with a new `application_id`; the old record is not mutated.

### §3.2 `OnboardingTier`

Closed enum, four tiers. Each tier maps deterministically to a `PublisherTrustLevel` (S11.1 §3.1) on successful `APPROVED_VERIFIED`.

| Value                      | Maps to `PublisherTrustLevel` | Required review depth                                                                                                                        | Reviewer count                                          |
| -------------------------- | ----------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------- |
| `AIOS_ROOT_INTERNAL`       | `AIOS_ROOT`                   | Internal-only; not exposed via the public application form. Granted only by AIOS-root recovery-mode operation per S11.1 §4.1 rotation rules. | DUAL human co-signers                                   |
| `VERIFIED_PUBLISHER`       | `VERIFIED`                    | Full pipeline: identity → technical → security. Legal-entity check + liability + point-of-contact mandatory.                                 | DUAL (technical + security must be different reviewers) |
| `COMMUNITY_PUBLISHER`      | `COMMUNITY`                   | Lightweight: identity (challenge-response on signing key) + reputation track record from existing publishers (≥ 2 sign-offs).                | SINGLE (with peer sign-offs)                            |
| `EXTERNAL_BRIDGE_OPERATOR` | `COMMUNITY` (capped)          | Bridge-specific: applicant declares the upstream registry (Flathub / OCI / distro), the re-packaging pipeline, the audit-metadata schema.    | DUAL (bridge security review mandatory)                 |

`AIOS_ROOT_INTERNAL` is **not** an applicant-selectable tier. It is reserved for the AIOS organisation itself and cannot be granted via the marketplace application path — it is an artefact of the recovery-mode root rotation flow (S11.1 §4.1) and appears in this enum solely so that the closed vocabulary is complete.

`EXTERNAL_BRIDGE_OPERATOR` maps to `COMMUNITY` trust per S11.1 §3.2 (`EXTERNAL_BRIDGE` repository kind is admitted at `COMMUNITY` only — never higher). The tier is distinguished from `COMMUNITY_PUBLISHER` so reviewers can apply bridge-specific checks (e.g. is the upstream registry under an operator's jurisdiction? does the bridge re-sign with an AIOS bridge key per S11.1 §3.2?).

### §3.3 `CapabilityReviewOutcome`

Closed enum, five outcomes. Each `declared_capability` in a candidate `PackageManifest` (S11.1 §5) is reviewed independently and assigned exactly one outcome.

| Value                                 | Semantics                                                                                                                                                            | Listing impact                                                                                                                                                                                                        |
| ------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `APPROVED_AS_DECLARED`                | Reviewer accepts the capability as the publisher described it. Justification text adequate; scope matches package purpose.                                           | Capability appears on the listing as declared.                                                                                                                                                                        |
| `APPROVED_WITH_NARROWED_SCOPE`        | Reviewer accepts the capability but **narrows** the scope (e.g. publisher requested `network.outbound.*`; reviewer narrows to `network.outbound.api.publisher.com`). | Listing shows the **narrowed** scope; publisher must re-issue manifest with the narrowed scope before the listing is published.                                                                                       |
| `DEFERRED_NEEDS_INFO`                 | Reviewer cannot decide; requests additional justification or evidence from the publisher. The listing draft remains gated.                                           | Listing not published; awaits publisher response.                                                                                                                                                                     |
| `REJECTED_INSUFFICIENT_JUSTIFICATION` | Justification text inadequate; capability unjustified for the described package purpose; or scope substantially over-broad with no narrow alternative.               | Capability removed; if removal makes the package non-functional per the publisher's own description, the entire listing draft is rejected.                                                                            |
| `REJECTED_DECEPTIVE`                  | Reviewer determines the capability declaration is deceptive — the justification text is misleading or contradicts the package's actual described behaviour.          | Entire listing rejected; FOREVER `CAPABILITY_REVIEW_DECEPTIVE_REJECTED` evidence; publisher reputation `capability_lie_history` incremented. Repeated `REJECTED_DECEPTIVE` outcomes feed S11.1 deplatform discipline. |

A package with **any** `REJECTED_DECEPTIVE` capability cannot be listed under any tier. A package with **any** `DEFERRED_NEEDS_INFO` capability cannot be listed until the deferral is resolved. A package may proceed with a mix of `APPROVED_AS_DECLARED` and `APPROVED_WITH_NARROWED_SCOPE` outcomes only after the publisher re-issues a manifest reflecting all narrowing.

### §3.4 `ListingVisibility`

Closed enum, five visibilities. Every published listing has exactly one visibility at any time. Visibility is bounded by the publisher's tier — a `COMMUNITY_PUBLISHER` listing cannot be `GLOBAL_PUBLIC` if the operator's host policy disallows community packages globally; a `GROUP_INTERNAL` listing is invisible outside the publishing group.

| Value                 | Semantics                                                                                                                            | Required publisher tier                                            |
| --------------------- | ------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------ |
| `GLOBAL_PUBLIC`       | Listed in the public AIOS marketplace; discoverable by any host that has the marketplace app installed and a default discovery feed. | `AIOS_ROOT_INTERNAL`, `VERIFIED_PUBLISHER`, `COMMUNITY_PUBLISHER`. |
| `GROUP_INTERNAL`      | Visible only to hosts in the publishing group (cite S4.1 group scope). Useful for organisation-internal apps published by a group.   | Any tier; group-operator authority binds visibility to the group.  |
| `PERSONAL_ONLY`       | Visible only to a specific operator's personal scope. Useful for solo operators publishing private apps to themselves.               | Any tier.                                                          |
| `DEPRECATED_VIEWABLE` | Listing is no longer recommended. Existing installs continue per S11.1 `DEPRECATED` discipline; new installs blocked.                | Any tier.                                                          |
| `RETIRED_HIDDEN`      | Listing fully retired; not visible in any feed; existing installs continue until uninstalled.                                        | Any tier.                                                          |

Visibility is a **separate** axis from `PublisherTrustLevel`. A `VERIFIED_PUBLISHER` may publish a `GROUP_INTERNAL` listing; a `COMMUNITY_PUBLISHER` may publish a `GLOBAL_PUBLIC` listing (with the explicit-warning rule per §6.4). A listing transitioning to `RETIRED_HIDDEN` emits FOREVER evidence so the audit trail survives the listing disappearance.

## §4 The marketplace UX is itself an AIOS app

The AIOS marketplace UX is **not** a privileged platform shell. It is an AIOS application packaged, signed, distributed, installed, and sandboxed exactly like any other app. This is constitutional: a privileged "marketplace daemon" with implicit capability would violate INV-008 (default-deny) and would create an admission-side trust short-circuit.

**Concretely:**

- `PackageKind = APP` per S11.1 §3.4.
- Published by `AIOS_ROOT_INTERNAL` tier (per §3.2), so its `PublisherTrustLevel = AIOS_ROOT` and it ships from `AIOS_ROOT_REPO` (S11.1 §3.2).
- `InstallScope = USER_SCOPED` per S4.1 — every operator gets their own install. There is no system-wide marketplace daemon.
- `SandboxProfile` per S3.2: standard app floor; **no** privileged capability classes; **no** direct `pubcat_<hex>` write access (it consumes the read-only catalog view the install pipeline already uses).
- `declared_capabilities` (S11.1 §5) include only:
  - `network.outbound.aios-marketplace.<endpoint>` for fetching listings, ratings, capability explanations;
  - `evidence.read.marketplace_scoped` for reading the operator's own evidence stream filtered to marketplace-relevant records (so the UX can show "you installed app X on date Y from publisher Z");
  - `approval.request` (S5.3) for delivering the install action approval prompt to the operator.
- It does **not** declare or hold:
  - any capability to write to `pubcat_<hex>` or any other AIOS-root-signed catalog;
  - any capability to approve packages on the operator's behalf;
  - any capability to sign or fetch on behalf of other apps;
  - any privileged channel into the install pipeline (it can only initiate the pipeline as any L7 surface can — by emitting an install action envelope per S11.1 §3.6 `DRAFT` state).

The constitutional consequence: a compromised marketplace UX is **bounded** by the same sandbox + capability discipline as any other app. It cannot grant trust, cannot bypass approval, cannot sign anything, cannot mutate the publisher catalog. It can only **lie about what it shows** — and that is what §6.4 (strict UI binding) is designed to make detectable.

## §5 The `PublisherApplication` contract

Each onboarding applicant submits a `PublisherApplication` to the AIOS-root review queue. The application is the only contract surface the reviewer trusts before the publisher is admitted to the trust chain.

```proto
syntax = "proto3";
package aios.marketplace.v1alpha1;

import "google/protobuf/timestamp.proto";
import "aios/distribution/v1alpha1/manifest.proto";   // S11.1 PackageManifest

message PublisherApplication {
  // Identity --------------------------------------------------------------
  string application_id = 1;            // "app:<hex_lower(BLAKE3(canonical))[:32]>"
  OnboardingTier requested_tier = 2;
  string applicant_handle = 3;          // "<vendor>" segment proposed for publisher_root_id
  string applicant_legal_entity = 4;    // mandatory for VERIFIED_PUBLISHER; empty otherwise
  string applicant_jurisdiction = 5;    // ISO-3166-1 alpha-2; mandatory for VERIFIED_PUBLISHER

  // Contact ---------------------------------------------------------------
  repeated string technical_contacts = 6;     // operator-readable identifiers (email/handle)
  repeated string security_contacts = 7;      // mandatory for VERIFIED_PUBLISHER
  string liability_documentation_uri = 8;     // mandatory for VERIFIED_PUBLISHER

  // Signing-key proof -----------------------------------------------------
  bytes proposed_publisher_root_pubkey = 9;   // Ed25519 public key
  bytes signing_key_proof = 10;               // Ed25519 sig over challenge_nonce
  bytes challenge_nonce = 11;                 // server-issued; recorded for replay defence

  // Intended packages -----------------------------------------------------
  repeated aios.distribution.v1alpha1.PackageManifest draft_manifests = 12;
  repeated string capability_justifications = 13;  // one per declared_capability across all draft_manifests, in order

  // Peer sign-offs (COMMUNITY_PUBLISHER only) -----------------------------
  repeated string peer_signoff_publisher_root_ids = 14;
  repeated bytes peer_signoff_signatures = 15;     // each signed by the peer's publisher_root over application canonical hash

  // Bridge declaration (EXTERNAL_BRIDGE_OPERATOR only) --------------------
  string bridge_upstream_registry_uri = 16;
  string bridge_repackaging_pipeline_doc_uri = 17;
  string bridge_audit_metadata_schema_uri = 18;

  // Lifecycle -------------------------------------------------------------
  google.protobuf.Timestamp submitted_at = 19;
  string application_canonical_hash = 20;     // hex_lower(BLAKE3(JCS(application without signing_key_proof, peer_signoff_signatures)))[:32]
}
```

### §5.1 Field-by-field validation rules

| Field                             | Validation                                                                                                                                                       | Failure mode                    |
| --------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------- |
| `application_id`                  | Regex `^app:[0-9a-f]{32}$`; equals `application_canonical_hash` prefix.                                                                                          | `APPLICATION_MALFORMED`         |
| `requested_tier`                  | Closed enum value; must NOT be `AIOS_ROOT_INTERNAL` (that tier is recovery-mode-only and not applicant-selectable).                                              | `APPLICATION_MALFORMED`         |
| `applicant_handle`                | Regex `^[a-z0-9-]{1,64}$`; must not collide with any active or `DEPRECATED` publisher in `pubcat_<hex>`; must not match a previously `DEPLATFORMED` handle.      | `APPLICANT_HANDLE_COLLISION`    |
| `applicant_legal_entity`          | Mandatory when `requested_tier = VERIFIED_PUBLISHER`; non-empty UTF-8; reviewer cross-checks against jurisdiction registries (manual step in this revision).     | `APPLICATION_MALFORMED`         |
| `applicant_jurisdiction`          | Mandatory when `requested_tier = VERIFIED_PUBLISHER`; ISO-3166-1 alpha-2 code.                                                                                   | `APPLICATION_MALFORMED`         |
| `technical_contacts`              | At least one entry; each entry ≤ 256 UTF-8 chars.                                                                                                                | `APPLICATION_MALFORMED`         |
| `security_contacts`               | Mandatory when `requested_tier = VERIFIED_PUBLISHER`; at least one entry distinct from `technical_contacts`.                                                     | `APPLICATION_MALFORMED`         |
| `liability_documentation_uri`     | Mandatory when `requested_tier = VERIFIED_PUBLISHER`; URI scheme allowed: `https`, `aios-fs://aios/system/distribution/applications/<id>/`.                      | `APPLICATION_MALFORMED`         |
| `proposed_publisher_root_pubkey`  | 32-byte Ed25519 public key; must not equal any active or revoked key in `pubcat_<hex>`.                                                                          | `PUBLISHER_KEY_COLLISION`       |
| `signing_key_proof`               | 64-byte Ed25519 signature over `challenge_nonce`, verifies against `proposed_publisher_root_pubkey`.                                                             | `SIGNING_KEY_PROOF_FAILED`      |
| `challenge_nonce`                 | 32 bytes; server-issued at the start of the application session; must not be reused; rejected if older than 7 days.                                              | `CHALLENGE_NONCE_EXPIRED`       |
| `draft_manifests`                 | Each `PackageManifest` must validate against S11.1 §5.1 except for the trust-chain step (chain not yet established). At least one manifest required.             | `DRAFT_MANIFEST_INVALID`        |
| `capability_justifications`       | One non-empty UTF-8 string per declared capability across all draft manifests, in deterministic order (manifest index → capability index). 32–4096 chars each.   | `JUSTIFICATION_TEXT_INVALID`    |
| `peer_signoff_publisher_root_ids` | When `requested_tier = COMMUNITY_PUBLISHER`, at least 2 entries from existing publishers in `VERIFIED` or `COMMUNITY` trust (not `DEPRECATED` / `DEPLATFORMED`). | `PEER_SIGNOFF_INSUFFICIENT`     |
| `peer_signoff_signatures`         | Same length as `peer_signoff_publisher_root_ids`; each Ed25519 sig over `application_canonical_hash` by the corresponding peer's publisher root key.             | `PEER_SIGNOFF_SIGNATURE_FAILED` |
| `bridge_*`                        | All three mandatory when `requested_tier = EXTERNAL_BRIDGE_OPERATOR`; URI schemes allowed: `https`, `aios-fs://`.                                                | `BRIDGE_DECLARATION_INCOMPLETE` |
| `submitted_at`                    | Within `MAX_FUTURE_DRIFT` (default 5 min) of host time; recorded by application server, not applicant.                                                           | `APPLICATION_MALFORMED`         |
| `application_canonical_hash`      | Equals `BLAKE3(JCS(application with signing_key_proof and peer_signoff_signatures cleared))[:32]`.                                                               | `APPLICATION_MALFORMED`         |

### §5.2 Application canonicalisation

The application server computes `application_canonical_hash` upon submission (the applicant cannot pre-compute it because the server stamps `submitted_at` and may stamp `challenge_nonce` server-side). Canonicalisation:

1. Project the application into JSON via the deterministic proto3 → JSON projection.
2. Clear `signing_key_proof` and `peer_signoff_signatures` (these sign over the hash, not vice versa).
3. Apply RFC 8785 JCS to the JSON.
4. Hash with BLAKE3.
5. Truncate to 128 bits and lowercase-hex-encode.

The `signing_key_proof` is signed over `challenge_nonce` (not over the application hash) — this is intentional: the proof binds the proposed key to a server-issued nonce, defending against replay of stolen application bundles. The peer sign-offs are signed over the application hash because they assert "we vouch for this entire application as submitted".

## §6 The onboarding workflow (closed FSM)

Strictly ordered, fail-closed. The marketplace state engine walks `PublisherOnboardingState` (§3.1) in the order shown. Any stage may transition to `REJECTED`. Back-transitions are forbidden except review-stage re-queueing on `DEFERRED_NEEDS_INFO` capability outcomes (§6.3).

### §6.1 Stage 1 — `APPLICATION_SUBMITTED`

- Application server validates field-level rules (§5.1).
- Validation failure → `REJECTED`; FOREVER `PUBLISHER_ONBOARDING_REJECTED` evidence with reason from §5.1 failure mode column.
- On pass: emit `PUBLISHER_ONBOARDING_APPLICATION_SUBMITTED` STANDARD_24M evidence; assign initial reviewer; transition to `IDENTITY_VERIFICATION_PENDING`.

### §6.2 Stage 2 — `IDENTITY_VERIFICATION_PENDING`

Per-tier review depth:

- **`VERIFIED_PUBLISHER`** — reviewer verifies legal-entity registration against the applicant's declared `applicant_jurisdiction`; verifies `liability_documentation_uri` resolves and is signed/notarised per the operator's jurisdiction policy; verifies `applicant_handle` does not collide with a known impersonation target (e.g. a major OS vendor); verifies `security_contacts` are reachable (out-of-band ping). Two-reviewer requirement applies: the identity-verification reviewer for `VERIFIED_PUBLISHER` MUST be different from the security reviewer in stage 4.
- **`COMMUNITY_PUBLISHER`** — reviewer confirms `signing_key_proof` is freshly verified; confirms peer sign-offs (≥ 2) are from non-`DEPRECATED` non-`DEPLATFORMED` publishers; confirms each peer's sign-off Ed25519 verifies. No legal-entity check required.
- **`EXTERNAL_BRIDGE_OPERATOR`** — reviewer confirms the bridge declaration fields, the upstream registry URI, the re-packaging pipeline doc, the audit metadata schema. Confirms the upstream registry is on the operator's host policy's `allow-bridge-from` list (S8.1 outbound discipline applies to bridge fetches).

On any reviewer decision to reject → `REJECTED`; FOREVER `PUBLISHER_ONBOARDING_REJECTED` evidence. On pass → emit `PUBLISHER_ONBOARDING_IDENTITY_VERIFIED` EXTENDED_60M evidence; transition to `TECHNICAL_REVIEW`.

### §6.3 Stage 3 — `TECHNICAL_REVIEW`

- Reviewer examines each `draft_manifest` against S11.1 §5.1 (modulo trust-chain steps not yet established).
- Reviewer examines `capability_justifications` against `declared_capabilities` and assigns one `CapabilityReviewOutcome` (§3.3) per capability.
- `DEFERRED_NEEDS_INFO` outcomes pause the FSM at this stage (no transition); the application server notifies the publisher; on publisher response, the reviewer re-evaluates. Repeated `DEFERRED_NEEDS_INFO` (> 3 cycles per capability) → reviewer must escalate to `REJECTED_INSUFFICIENT_JUSTIFICATION` to avoid open-ended pauses.
- `APPROVED_WITH_NARROWED_SCOPE` outcomes require the publisher to re-submit a draft manifest reflecting the narrowing before this stage can complete.
- `REJECTED_DECEPTIVE` on **any** capability → entire application → `REJECTED`; FOREVER `CAPABILITY_REVIEW_DECEPTIVE_REJECTED` evidence; `capability_lie_history` counter on the proposed publisher root id incremented (recorded for use if the same applicant re-applies under a different handle — a configuration knob the AIOS-root reviewer team uses to detect repeat offenders).
- On all capabilities reaching `APPROVED_AS_DECLARED` or `APPROVED_WITH_NARROWED_SCOPE` (with re-submitted manifests applied) → emit `CAPABILITY_REVIEW_APPROVED` EXTENDED_60M evidence (one per capability, batched per manifest); transition to `SECURITY_REVIEW`.

### §6.4 Stage 4 — `SECURITY_REVIEW`

For `VERIFIED_PUBLISHER` and `EXTERNAL_BRIDGE_OPERATOR`: a **different** reviewer than stages 2-3 examines the manifests for security-relevant patterns: capability combinations that compose dangerously (e.g. `network.outbound.*` + `evidence.read.*` + `vault.broker.api.*`); sandbox profiles that would require composition exceptions per S3.2; network manifests that allow exfiltration patterns; signing-key handling claims in the publisher's documentation that suggest weak key custody. The two-reviewer separation defends against single-reviewer compromise (review bypass via social engineering — §10.3).

For `COMMUNITY_PUBLISHER`: this stage is shorter — the reviewer confirms no obvious security-floor violation; the security review depth is bounded by the `COMMUNITY` capability ceiling per S11.1 §3.1.

On rejection → `REJECTED`; FOREVER `PUBLISHER_ONBOARDING_REJECTED` evidence with reason `SECURITY_REVIEW_FAILED`. On pass → transition to `APPROVED_VERIFIED`.

### §6.5 Stage 5 — `APPROVED_VERIFIED`

- The marketplace state engine emits a request to the AIOS-root catalog signer (recovery-mode operation per S11.1 §4.5 catalog-update discipline if the operator's policy requires recovery-mode for catalog updates; otherwise a signed delta in normal mode).
- A new entry is added to `pubcat_<hex>`: `(publisher_root_id, public_key, trust_level = OnboardingTier→PublisherTrustLevel mapping, onboarding_evidence_pointer = application_canonical_hash, activated_at = now(), retired_at = unset)`.
- Approval strength: `DUAL` for `VERIFIED_PUBLISHER` and `EXTERNAL_BRIDGE_OPERATOR` (S5.3 §3.3); `SINGLE` for `COMMUNITY_PUBLISHER` (with peer sign-offs already counted).
- Emit FOREVER `PUBLISHER_ONBOARDING_APPROVED` evidence linking the `application_canonical_hash` to the granted `publisher_root_id`.

### §6.6 `REJECTED` and `DEPLATFORMED`

- `REJECTED` is terminal for the application. The applicant may re-apply with a new application; the rejection record persists FOREVER.
- `DEPLATFORMED` is the S11.1 takedown event mirrored into this FSM for vocabulary completeness. The transition `APPROVED_VERIFIED → DEPLATFORMED` is **never** initiated by the marketplace state engine — it is initiated by the S11.1 deplatform discipline (AIOS-root cosigned takedown) and consumed by the marketplace state engine to update its own view. Emit FOREVER `PUBLISHER_ONBOARDING_DEPLATFORMED` evidence echoing the S11.1 record.

### §6.7 Stage time budgets

Each onboarding stage has a default time budget. Budgets are operator-policy-tunable on the AIOS-root reviewer team's host but MUST exist — open-ended pauses are forbidden because they create the appearance of progress without movement. Default budgets:

| Stage                           | Default budget                           | On budget exhaustion                                                                                                                                                                                   |
| ------------------------------- | ---------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `APPLICATION_DRAFT`             | 30 days from creation                    | Application server purges the draft; applicant notified at 25 days. No evidence emitted (draft never surfaced).                                                                                        |
| `APPLICATION_SUBMITTED`         | 5 business days                          | Auto-routed to a backup reviewer queue; `MARKETPLACE_REVIEW_BUDGET_EXCEEDED` STANDARD_24M evidence (separate from the twelve queued types — listed in the L10 audit candidate set, not requeued here). |
| `IDENTITY_VERIFICATION_PENDING` | 10 business days                         | Reviewer must escalate to a senior reviewer or transition to `REJECTED` with reason `IDENTITY_VERIFICATION_TIMEOUT`.                                                                                   |
| `TECHNICAL_REVIEW`              | 15 business days per re-submission cycle | A `DEFERRED_NEEDS_INFO` cycle older than 15 business days auto-promotes to `REJECTED_INSUFFICIENT_JUSTIFICATION` to enforce closure.                                                                   |
| `SECURITY_REVIEW`               | 10 business days                         | Same escalation path as identity verification.                                                                                                                                                         |

The budgets are **soft** — they do not force a decision; they force **disclosure**: the applicant sees a "your application is awaiting reviewer attention beyond budget" surface in the marketplace UX, and the AIOS-root reviewer team's queue dashboard surfaces the overrun. The operator-facing rule is honesty: the applicant always knows where their application stands and whether a human is moving it.

### §6.8 Per-reviewer subject discipline

Every transition that requires a reviewer decision MUST be authenticated to a specific `HUMAN_USER` subject id under S5.3. The reviewer's `subject_id` is recorded in the `CapabilityReviewRecord` (§8.1) and in the FSM transition evidence. Two-reviewer separation (the requirement that the identity-verification reviewer for `VERIFIED_PUBLISHER` differs from the security reviewer) is enforced by comparing `subject_id` values; the marketplace state engine refuses the `SECURITY_REVIEW → APPROVED_VERIFIED` transition if the same `subject_id` performed both reviews. AI agents (S12.3 `AI_AGENT` subjects) cannot perform any reviewer action — INV-013 is enforced at the policy decision point. An AI agent may **assist** a human reviewer (e.g. by surfacing similar past applications, by running static checks on the draft manifests), but the decision-recording transition requires a `HUMAN_USER` subject id under S5.3.

## §7 The marketplace listing model

A `Listing` is the operator-facing surface of a published package. It binds a `PackageManifest` (S11.1 §5) to operator-readable metadata, capability explanations, ratings, the `EcosystemHonestyClass` (S12.1), price information, and a signing-chain visualisation.

### §7.1 The `Listing` proto

```proto
syntax = "proto3";
package aios.marketplace.v1alpha1;

import "google/protobuf/timestamp.proto";
import "aios/distribution/v1alpha1/manifest.proto";       // S11.1 PackageManifest
import "aios/apps/v1alpha1/ecosystem_runtime.proto";      // S12.1 EcosystemHonestyClass

message Listing {
  // Identity --------------------------------------------------------------
  string listing_id = 1;                  // "lst:<hex_lower(BLAKE3(canonical))[:32]>"
  string package_id = 2;                  // matches PackageManifest.package_id
  string version = 3;                     // matches PackageManifest.version
  string publisher_root_id = 4;           // matches PackageManifest.publisher_root_id

  // Operator-facing metadata ---------------------------------------------
  string display_name = 5;                // 1-128 UTF-8
  string short_description = 6;           // 1-512 UTF-8
  string long_description = 7;            // 1-65536 UTF-8 markdown subset
  repeated string screenshots_aios_fs_uris = 8;  // aios-fs://... only (no http)
  string icon_aios_fs_uri = 9;            // aios-fs://... only

  // Capability disclosure -------------------------------------------------
  repeated CapabilityListingEntry capability_listing = 10;
  aios.apps.v1alpha1.EcosystemHonestyClass honesty_class = 11;  // S12.1 mandatory

  // Ratings reference (S12.4 abstract) -----------------------------------
  string ratings_aggregate_id = 12;       // resolves via S12.4 contract; not redefined here

  // Price (informational, this revision) ---------------------------------
  string price_display = 13;              // e.g. "Free" | "EUR 9.99 one-time"
  string price_machine_readable = 14;     // optional structured form; deferred semantics

  // Lifecycle -------------------------------------------------------------
  ListingVisibility visibility = 15;
  google.protobuf.Timestamp published_at = 16;
  google.protobuf.Timestamp deprecated_at = 17;   // optional; set on visibility transition
  string listing_canonical_hash = 18;
  bytes ed25519_signature = 19;            // signed by package_signing_key over listing_canonical_hash
}

message CapabilityListingEntry {
  string capability_id = 1;               // resolves in L5/S1.1 capability catalog
  string operator_friendly_explanation = 2;  // 32-2048 UTF-8 — reviewed during capability review
  CapabilityReviewOutcome review_outcome = 3;
  string narrowed_scope = 4;              // populated when review_outcome = APPROVED_WITH_NARROWED_SCOPE
}
```

### §7.2 Listing-vs-manifest binding rules

Every `Listing` is bound to exactly one published `PackageManifest`. The binding is enforced at three points:

1. **Listing publication.** The marketplace state engine refuses to publish a `Listing` whose `(package_id, version, publisher_root_id)` does not match an existing manifest signed by the same `publisher_root_id`.
2. **Capability list cross-check at publication.** The set of `capability_id` values in `capability_listing` must equal exactly the set of capabilities in `PackageManifest.declared_capabilities` (after applying any `APPROVED_WITH_NARROWED_SCOPE` narrowing). Mismatch → publication refused; FOREVER `LISTING_VS_MANIFEST_MISMATCH` evidence.
3. **Capability list cross-check at install time.** The seventeen-step install pipeline (S11.1 §6) consumes the listing's capability set as the operator-visible expectation. If the manifest pulled at install time declares a capability not present in the listing, this is the **bait-and-switch** detection point: the install transitions to `INSTALL_FAILED` (or `QUARANTINED` if discovered after install via runtime audit) and emits FOREVER `LISTING_VS_MANIFEST_MISMATCH` + `MARKETPLACE_REVIEW_BYPASS_ATTEMPTED` evidence; the listing is downgraded to `DEPRECATED_VIEWABLE`; the publisher's `capability_lie_history` is incremented.

The binding is **not** symmetric: a manifest may declare fewer capabilities than the listing claims (the publisher narrowed at publication; the listing-displayed scope is the upper bound). It is **not** allowed in the other direction (manifest cannot claim more than the listing).

### §7.3 Strict UI binding

The listing renderer (an L7 surface, consumed by the marketplace app per §4) MUST honour these rules. They are constitutional UI constraints, not stylistic recommendations:

- **Trust class.** The displayed publisher trust class MUST be read from `pubcat_<hex>` for the listing's `publisher_root_id` at render time. The renderer MUST NOT trust any field on the `Listing` to declare trust class. (The `Listing` proto deliberately omits a `publisher_trust` field for this reason.)
- **`EcosystemHonestyClass`.** MUST be displayed at the install prompt and at every listing surface where the operator might press an install button (S12.1 mandates this disclosure).
- **Capability list.** MUST be displayed in `capability_listing` order; each capability MUST show its `operator_friendly_explanation` and, when narrowed, the `narrowed_scope` (the operator must see what the reviewer narrowed).
- **Signing chain visualisation.** MUST show `AIOS root → publisher root → package signing key`, with each hop's last-rotation date drawn from `pubcat_<hex>` (S11.1 §4.5). The renderer MUST NOT show a signing chain that the catalog does not contain (no "synthetic" trust visualisations).
- **Deprecation / retirement banner.** MUST display when `visibility ∈ {DEPRECATED_VIEWABLE, RETIRED_HIDDEN}` and the operator has somehow navigated to the listing; the install button MUST be disabled.
- **Community warning.** When the listing's bound publisher tier is `COMMUNITY` (regardless of `ListingVisibility`), the renderer MUST display an explicit warning: "This publisher is at COMMUNITY trust. The capability ceiling is enforced; ratings may have lower volume." The phrasing is normative-by-intent — exact UI copy is L7's domain — but the **fact** of the warning is constitutional.

A renderer that violates strict UI binding is itself a constitutional defect. The defect is **detectable**: the operator's evidence stream (S3.1) records the trust class the install pipeline used; if the listing UX showed a different class, an audit can detect the divergence (this is queued as an L9 audit query — out of scope for this contract).

### §7.4 Listing canonicalisation and signature

Listing canonicalisation mirrors S11.1 §5.2 manifest canonicalisation:

1. Project the listing into JSON via deterministic proto3 → JSON projection.
2. Clear `ed25519_signature` and `listing_canonical_hash`.
3. Apply RFC 8785 JCS.
4. Hash with BLAKE3.
5. Truncate to 128 bits, lowercase-hex.

`ed25519_signature` is signed over the ASCII bytes of the lowercase-hex `listing_canonical_hash` by the same `package_signing_key` that signed the bound `PackageManifest`. Listings are not signed by a separate key class — the publisher attests the listing under the same trust chain.

A listing whose computed canonical hash does not match the recorded value, or whose signature does not verify, is rejected at publication with FOREVER `LISTING_VS_MANIFEST_MISMATCH` evidence (rationale: any tamper on a listing is equivalent to a manifest tamper for trust purposes).

## §8 Capability review discipline (deep contract)

The capability review (§6.3, §3.3) is the heart of the marketplace's defence against capability bait-and-switch. This section specifies the discipline in detail.

### §8.1 Per-capability review record

Every capability review produces a record:

```proto
message CapabilityReviewRecord {
  string application_id = 1;              // PublisherApplication.application_id
  uint32 manifest_index = 2;              // index into draft_manifests
  uint32 capability_index = 3;            // index into manifest.declared_capabilities
  string capability_id = 4;
  string justification_text = 5;          // copy of capability_justifications entry
  CapabilityReviewOutcome outcome = 6;
  string reviewer_subject_id = 7;         // S12.3 / S5.3 subject id of the reviewer
  string narrowed_scope = 8;              // populated when outcome = APPROVED_WITH_NARROWED_SCOPE
  string rejection_reason = 9;            // populated when outcome ∈ REJECTED_*
  google.protobuf.Timestamp reviewed_at = 10;
}
```

Records are written to evidence (S3.1) per the table in §11. They are FOREVER-retained when the outcome is `REJECTED_DECEPTIVE` (the audit trail of deceptive behaviour must survive).

### §8.2 Justification text adequacy

The reviewer's job on `justification_text` is to verify three properties:

1. **Specificity.** The text describes **why** this exact package needs **this exact** capability (e.g. "the export-to-PDF feature uses the `filesystem.write.user-documents` capability to save the PDF the user generates"). Generic text ("this app needs filesystem access") is grounds for `DEFERRED_NEEDS_INFO`.
2. **Proportionality.** The capability scope claimed is the **smallest** sufficient for the described purpose. If the publisher claims `network.outbound.*` and the description names exactly one endpoint, the reviewer narrows to that endpoint (`APPROVED_WITH_NARROWED_SCOPE`).
3. **Non-deception.** The justification text does not contradict the package's `display_name`, `short_description`, or `long_description`. A note-taking app that justifies `vault.broker.api.read.*` as "for synchronisation features" is **deceptive** if the long description does not mention any synchronisation feature — that is `REJECTED_DECEPTIVE`.

The reviewer's decision is recorded with the justification text verbatim. The text is operator-facing — it appears in the listing's `CapabilityListingEntry.operator_friendly_explanation` after the reviewer optionally edits for clarity.

### §8.3 Narrowing discipline

`APPROVED_WITH_NARROWED_SCOPE` is the most common outcome for ambitious capability requests. The narrowing produces a new capability id with bounded scope — for example:

| Publisher request    | Reviewer narrowing                       |
| -------------------- | ---------------------------------------- |
| `network.outbound.*` | `network.outbound.api.publisher.com:443` |
| `filesystem.read.*`  | `filesystem.read.user-documents.scoped`  |
| `vault.broker.api.*` | `vault.broker.api.read.publisher-token`  |
| `evidence.read.*`    | `evidence.read.app-scoped`               |

The narrowed capability id MUST resolve in the L5/S1.1 capability catalog. If no narrower capability id exists, the reviewer cannot narrow — they must `REJECTED_INSUFFICIENT_JUSTIFICATION` (the publisher's request is fundamentally too broad for the package's described purpose) or `DEFERRED_NEEDS_INFO` (request additional rationale).

The publisher's response to `APPROVED_WITH_NARROWED_SCOPE` is mandatory: they must re-issue a `PackageManifest` with the narrowed capability id substituted. Without re-issue, the application cannot leave `TECHNICAL_REVIEW`.

### §8.4 Deception detection

`REJECTED_DECEPTIVE` is reserved for cases where the reviewer concludes the publisher is **lying**, not merely careless. Indicators:

- The justification text explicitly contradicts a public source (e.g. "we don't read user documents" while the manifest declares `filesystem.read.user-documents`).
- The capability is wholly unrelated to the package's declared purpose (e.g. a calculator app declaring `network.outbound.*`).
- The publisher has prior `REJECTED_DECEPTIVE` outcomes on other applications under different handles (recorded out-of-band; the AIOS-root reviewer team's process for connecting impersonation handles is internal).

`REJECTED_DECEPTIVE` is the **only** capability outcome that immediately rejects the entire application without retry — the publisher cannot simply re-submit with edited justification. They must withdraw and re-apply with a new application, and the prior deceptive record survives.

## §9 Reputation model

Each entry in `pubcat_<hex>` (S11.1 §4.5) is augmented by a per-host-computed reputation score. The reputation is **not** signed by the publisher — the operator's host computes it locally from the operator's own evidence stream. There is no global cross-host reputation aggregation in this contract.

### §9.1 Reputation dimensions

Closed schema, four dimensions. Each dimension is an integer counter or a derived score; together they form `ReputationVector`:

```proto
message ReputationVector {
  int64 security_history_score = 1;          // higher = better; computed from health-check pass count and breach count
  int64 support_responsiveness_score = 2;    // higher = better; computed from time-to-deferral-resolution on capability review
  int64 capability_lie_history_count = 3;    // raw counter; lower = better; FOREVER-tracked
  int64 deplatform_count = 4;                // raw counter; lower = better; FOREVER-tracked
}
```

### §9.2 Computation rules

- `security_history_score` is incremented on each runtime health-check pass (per S11.1 §6.16 `ACTIVE` state monitoring) and decremented on each `QUARANTINED` event (per S11.1 §3.6 `PackageInstallState`).
- `support_responsiveness_score` is incremented per `DEFERRED_NEEDS_INFO → APPROVED_*` transition with a turnaround under 7 days (publisher responsiveness signal); decremented on > 30-day no-response.
- `capability_lie_history_count` is incremented on each FOREVER `CAPABILITY_LIE_DETECTED` (S11.1 §3.7) attributed to the publisher AND on each `REJECTED_DECEPTIVE` capability outcome (§3.3). It is **never** decremented.
- `deplatform_count` is incremented on each `PUBLISHER_DEPLATFORMED` event (S11.1 §3.9). Mirrored across re-applications under detected impersonation handles by the AIOS-root reviewer team's internal binding (out of scope here).

### §9.3 Reputation does not override trust class

Reputation is **informational**. It can lower how prominently a publisher's listings appear in the marketplace UX (e.g. publishers with `capability_lie_history_count > 0` are not eligible for "featured" placements). It **cannot**:

- escalate a publisher above their granted `PublisherTrustLevel` (a `COMMUNITY` publisher with stellar reputation is still capability-ceilinged at `COMMUNITY`);
- bypass any of the seventeen install pipeline steps;
- mask the explicit `COMMUNITY` warning per §7.3.

Reputation **can** trigger `DEPRECATED` recommendations: a publisher with `deplatform_count = 0` but `capability_lie_history_count` rising past a per-host threshold (default 3 distinct events in 90 days) is flagged in the operator's evidence feed; the operator may choose to manually deprecate that publisher's listings. This is operator policy, not a constitutional rule.

## §10 Adversarial robustness

This section enumerates the threat model and the mechanism that defends each axis. Cited invariants: INV-002 (AI proposes never executes), INV-008 (default-deny policy), INV-013 (AI cannot perform system admin).

### §10.1 Fake publisher applications

**Attack.** An adversary submits an application impersonating a legitimate organisation — same `applicant_handle`, fabricated legal entity, plausible technical contacts.

**Defence.**

- `applicant_handle` collision check against the catalog rejects identical handles (§5.1 `APPLICANT_HANDLE_COLLISION`).
- For `VERIFIED_PUBLISHER`, the legal-entity registration cross-check at §6.2 stage 2 is mandatory; the reviewer must verify the entity exists in the declared jurisdiction's registry.
- For `VERIFIED_PUBLISHER`, two-reviewer separation (§6.2 stage 2 reviewer ≠ §6.4 stage 4 reviewer) defends against single-reviewer compromise.
- The challenge-response signing-key proof (§5.1 `signing_key_proof` over `challenge_nonce`) defends against replay of stolen application bundles.
- Cite INV-013: an AI agent cannot submit a publisher application; submission requires a `HUMAN_USER` subject identity per S5.3.

### §10.2 Capability bait-and-switch

**Attack.** Publisher submits a listing claiming narrow capabilities; the actual `PackageManifest` declares a broader set.

**Defence.**

- Listing-vs-manifest cross-check at publication (§7.2 rule 2): the marketplace state engine refuses to publish a listing whose capability set differs from the bound manifest's `declared_capabilities`.
- Listing-vs-manifest cross-check at install (§7.2 rule 3): the install pipeline compares again at install time; mismatch transitions to `INSTALL_FAILED` and emits FOREVER `LISTING_VS_MANIFEST_MISMATCH`.
- First-run capability lie audit (S11.1 §6.17, §3.7): even if a publisher passes both static checks, a runtime drift between declared and observed capabilities triggers `CAPABILITY_LIE` with FOREVER evidence.
- Repeated bait-and-switch incidents feed S11.1 deplatform discipline (`TakedownReason = CAPABILITY_LIE_DETECTED`).
- Cite INV-008: every capability is default-deny; a manifest that declares more than the listing showed cannot grant itself anything at install time — capabilities are bound by the install pipeline, not by the listing.

### §10.3 Review bypass via social engineering

**Attack.** An adversary coerces, bribes, or carelessly persuades a single AIOS-root reviewer to approve a fraudulent application or a deceptive capability declaration.

**Defence.**

- Multi-reviewer requirement for `VERIFIED_PUBLISHER` and `EXTERNAL_BRIDGE_OPERATOR`: the identity-verification reviewer (stage 2) MUST be a different subject from the security reviewer (stage 4). Approval strength `DUAL` per S5.3 §3.3.
- Capability review discipline: `REJECTED_DECEPTIVE` is a separate outcome from `REJECTED_INSUFFICIENT_JUSTIFICATION` so a careless reviewer who approves a deceptive justification leaves a forensic trail (the justification text is FOREVER-retained verbatim in the `CapabilityReviewRecord`).
- AIOS-root catalog signing for `APPROVED_VERIFIED` is itself an `EXACT_ACTION` binding under S5.3, with `ApprovalStrength = DUAL` for `VERIFIED_PUBLISHER` — the catalog cannot be mutated by a single human signature.
- Audit-side detection: `MARKETPLACE_REVIEW_BYPASS_ATTEMPTED` evidence (FOREVER) is emitted when the install pipeline detects a manifest that should have been caught at review (e.g. a `VERIFIED` publisher's manifest containing capabilities none of which appear in any of the publisher's published listings). The audit cannot be defeated by a single compromised reviewer because it runs at install time on every host.

### §10.4 Coordinated rating manipulation

**Attack.** A publisher pumps ratings on their own listings via fake operator identities; or smears a competitor.

**Defence.**

- This contract **cites** S12.4's outlier detection abstractly. The detection is not redefined here; the marketplace consumes the `ratings_aggregate_id` in `Listing` (§7.1) and trusts S12.4 to report aggregates that already have outliers filtered.
- Multi-dimensional reputation (§9): a five-star rating cannot mask `capability_lie_history_count > 0` or `deplatform_count > 0`. The reputation vector is shown alongside the rating in the listing UX.
- Cross-host federation of ratings is **deferred**; this revision computes ratings per-host. A coordinated attack must therefore reach every operator's host independently — the per-host evidence stream is the unforgeable substrate.

### §10.5 Deceptive listing UX

**Attack.** A compromised marketplace UX (or a malicious renderer) displays a `COMMUNITY` listing as if it were `VERIFIED`, hiding the explicit-warning discipline.

**Defence.**

- Strict UI binding (§7.3): the trust class MUST be read from `pubcat_<hex>` at render time. The `Listing` proto deliberately omits a `publisher_trust` field so there is nothing on the listing for the renderer to mis-display.
- The marketplace UX is itself an AIOS app (§4) — it is sandboxed, has no privileged catalog access, cannot mutate the operator's evidence. A deceptive renderer can only **show false text on screen**; the install pipeline still runs against the real catalog.
- The operator's evidence stream records the trust class the install pipeline used (S11.1 §6.4 `PUBLISHER_TRUST_LEVEL_OBSERVED` STANDARD_24M). An L9 audit query (deferred) can detect divergence between what the listing showed and what the install consumed — this is queued as `MARKETPLACE_REVIEW_BYPASS_ATTEMPTED` evidence (FOREVER) when the divergence is detected at install time.
- Cite INV-008: trust is default-deny. A renderer claiming `VERIFIED` does not grant `VERIFIED` capabilities — the install pipeline binds capabilities only to what `pubcat_<hex>` actually says.

### §10.6 Adversarial cross-table

The five threat axes condensed into a single matrix for rapid reference. Each axis lists the **primary** mechanism (the constitutional defence) and the **secondary** mechanism (defence-in-depth).

| Threat axis                          | Primary defence                                                                        | Secondary defence                                                                               |
| ------------------------------------ | -------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------- |
| Fake publisher applications          | Identity verification + legal-entity check (§6.2) for `VERIFIED_PUBLISHER`             | Two-reviewer separation (§6.8); challenge-nonce signing-key proof (§5.1)                        |
| Capability bait-and-switch           | Listing-vs-manifest cross-check at publication (§7.2 rule 2)                           | Cross-check at install (§7.2 rule 3); first-run capability lie audit (S11.1 §6.17)              |
| Review bypass via social engineering | Multi-reviewer requirement (`ApprovalStrength = DUAL`) for `VERIFIED_PUBLISHER` (§6.8) | `MARKETPLACE_REVIEW_BYPASS_ATTEMPTED` FOREVER evidence (§10.3); per-reviewer subject discipline |
| Coordinated rating manipulation      | S12.4 outlier detection (referenced abstractly)                                        | Multi-dimensional reputation (§9); per-host reputation computation (§9.2)                       |
| Deceptive listing UX                 | Strict UI binding to `pubcat_<hex>` at render time (§7.3)                              | Marketplace UX is sandboxed AIOS app (§4); audit-side divergence detection (§10.5)              |

The cross-table is not a closed-FSM artefact — it is a navigation aid for §10's narrative. The constitutional binding remains: each axis MUST be addressed; absence of any defence is a contract defect.

## §11 Search and discovery

The marketplace UX presents listings to operators through search and discovery surfaces. This section specifies the **information architecture** the UX MUST honour; pixel-level UX is L7's domain.

### §11.1 Tier-bounded discovery

Listings are sorted into discovery feeds by trust tier. The default discovery feed presents listings in this order:

1. **`AIOS_ROOT_INTERNAL` listings first.** These are constitutional packages (the marketplace UX itself, recovery utilities, identity bundles where exposed via the marketplace); they appear at the top of any default feed regardless of recency or rating.
2. **`VERIFIED_PUBLISHER` listings next.** Sorted by recency × rating × `support_responsiveness_score` (lexical default; operator can re-sort).
3. **`COMMUNITY_PUBLISHER` listings last,** with the explicit `COMMUNITY` warning rendered alongside (§7.3).
4. **`EXTERNAL_BRIDGE_OPERATOR` listings** appear in a **separate** "Bridges" feed and are NEVER in the default feed unless the operator has explicitly opted into bridge discovery in their host policy.

This ordering is not a recommendation — it is a constitutional requirement of the discovery surface. A renderer that places `COMMUNITY` listings ahead of `VERIFIED` listings in a default feed (e.g. via "trending" sort) MUST make the trust tier visually distinguishable on every entry. The operator must always see what tier they are looking at.

### §11.2 Search filters

Search filters are bounded-cardinality and operator-controlled:

| Filter                | Type                                        | Default                                                                                   |
| --------------------- | ------------------------------------------- | ----------------------------------------------------------------------------------------- |
| `min_trust_tier`      | One of `AIOS_ROOT`, `VERIFIED`, `COMMUNITY` | `VERIFIED` (operator can lower; cannot raise without `AIOS_ROOT` host policy)             |
| `package_kind`        | Subset of S11.1 `PackageKind`               | `{APP, AGENT, THEME, ADAPTER}` (recovery-only kinds excluded from operator-facing search) |
| `honesty_class`       | Subset of S12.1 `EcosystemHonestyClass`     | All four values                                                                           |
| `capability_excludes` | List of capability ids to exclude           | Empty                                                                                     |
| `price_includes_free` | Boolean                                     | `true`                                                                                    |
| `language`            | ISO-639-1 code                              | Operator's host locale                                                                    |

The capability-exclude filter is constitutional: an operator who has decided "no app on this host may declare `network.outbound.*` regardless of narrowing" can express that in search. The filter is honoured at the listing-render step (the listing is hidden from the operator) AND at the install pipeline step (the install is rejected with reason `CAPABILITY_EXCLUDED_BY_OPERATOR_POLICY`, even if the operator somehow bypasses the UX filter via direct repository access).

### §11.3 Discovery does not bypass approval

A discovery surface MUST NOT include an "install with one click" path that bypasses S5.3 approval. Every install — regardless of how it was discovered — runs the seventeen-step pipeline (S11.1 §6) and the S5.3 `EXACT_ACTION` binding. The discovery surface delivers the operator to the listing page, where the install button emits a `DRAFT` install action envelope; from that point onward S11.1's pipeline is authoritative.

This is the operator-side analog of the publisher-side strict UI binding: just as the renderer cannot lie about trust class, the discovery surface cannot lie about install simplicity. There is no "fast path" that skips approval, regardless of how routine the install appears.

### §11.4 AI-assisted discovery

An AI agent (S12.3 `AI_AGENT` subject) may **assist** the operator in discovery — e.g. by suggesting listings matching a stated need, by summarising long descriptions, by pre-reading the capability list and explaining implications. This assistance is bound by INV-002:

- The agent emits a **proposal** (a typed `DiscoveryProposal` action envelope; full schema deferred to L5);
- The operator sees the proposal as a UI hint with the agent's reasoning;
- The operator chooses whether to navigate to the listing;
- The operator chooses whether to install (with the standard S5.3 flow);
- The agent **cannot** initiate an install on the operator's behalf (INV-013 enforced at the policy decision point).

A proposal that includes an install kickoff is rejected at policy by S2.3 with `AI_AGENT_INSTALL_KICKOFF_FORBIDDEN` and emits FOREVER evidence; this is queued as a candidate record type for the L5 deferred contract — it is **not** in the twelve types listed in §12 because the kickoff path is itself out of scope for this revision.

## §12 Evidence record types (queued for S3.1)

Twelve record types are queued for S3.1 ingestion. Retention classes follow S3.1's vocabulary (`FOREVER`, `EXTENDED_60M`, `STANDARD_24M`).

| Record type                                  | Trigger                                                                                                                    | Retention    |
| -------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------- | ------------ |
| `PUBLISHER_ONBOARDING_APPLICATION_SUBMITTED` | `APPLICATION_SUBMITTED` state entered after successful field validation (§6.1).                                            | STANDARD_24M |
| `PUBLISHER_ONBOARDING_IDENTITY_VERIFIED`     | `IDENTITY_VERIFICATION_PENDING → TECHNICAL_REVIEW` transition (§6.2).                                                      | EXTENDED_60M |
| `PUBLISHER_ONBOARDING_APPROVED`              | `SECURITY_REVIEW → APPROVED_VERIFIED`; catalog entry written; mirrors S11.1 onboarding-approval signal.                    | FOREVER      |
| `PUBLISHER_ONBOARDING_REJECTED`              | Any review stage → `REJECTED`; rejection reason recorded; mirrors S11.1 rejection signal.                                  | FOREVER      |
| `PUBLISHER_ONBOARDING_DEPLATFORMED`          | `APPROVED_VERIFIED → DEPLATFORMED`; mirrors S11.1 `PUBLISHER_DEPLATFORMED` event for FSM symmetry.                         | FOREVER      |
| `CAPABILITY_REVIEW_REQUESTED`                | `TECHNICAL_REVIEW` entered; one record per draft manifest summarising the capability count to be reviewed (§6.3).          | STANDARD_24M |
| `CAPABILITY_REVIEW_APPROVED`                 | Per-capability `APPROVED_AS_DECLARED` or `APPROVED_WITH_NARROWED_SCOPE` outcome (§6.3, §8.1).                              | EXTENDED_60M |
| `CAPABILITY_REVIEW_DECEPTIVE_REJECTED`       | Per-capability `REJECTED_DECEPTIVE` outcome (§6.3, §8.4); also feeds `capability_lie_history_count`.                       | FOREVER      |
| `LISTING_PUBLISHED`                          | `Listing.visibility` transitions from unset to any visible state (§7); listing canonical hash recorded.                    | STANDARD_24M |
| `LISTING_VISIBILITY_DOWNGRADED`              | Visibility transitions toward `DEPRECATED_VIEWABLE` or `RETIRED_HIDDEN` (§3.4); reason recorded.                           | EXTENDED_60M |
| `LISTING_VS_MANIFEST_MISMATCH`               | Listing-vs-manifest cross-check fails at publication (§7.2 rule 2) or at install (§7.2 rule 3).                            | FOREVER      |
| `MARKETPLACE_REVIEW_BYPASS_ATTEMPTED`        | Install pipeline detects a manifest that should have been caught at review (§10.3) — divergence between listing & catalog. | FOREVER      |

These twelve are additive to S11.1's nineteen and S12.1's fourteen. S3.1 ingestion is the single sink for all evidence record types across L10 and L6.

## §13 Bounded-cardinality telemetry

Marketplace telemetry MUST emit only label sets whose cardinality is bounded — unbounded cardinality on operator-facing surfaces is itself a privacy defect. The per-counter cardinality bounds (per `bounded-cardinality` discipline cited from L9):

| Metric                                           | Labels                                                                                                    | Bound                                              |
| ------------------------------------------------ | --------------------------------------------------------------------------------------------------------- | -------------------------------------------------- |
| `marketplace_application_submissions_total`      | `requested_tier` (4 values); `outcome ∈ {accepted_at_field_validation, rejected_at_field_validation}` (2) | 8                                                  |
| `marketplace_onboarding_state_transitions_total` | `from_state` (8); `to_state` (8); `requested_tier` (4)                                                    | bounded by FSM legality; ≤ ~80 in practice         |
| `marketplace_capability_review_outcomes_total`   | `outcome` (5); `requested_tier` (4)                                                                       | 20                                                 |
| `marketplace_listing_publish_attempts_total`     | `visibility` (5); `outcome ∈ {published, refused_mismatch, refused_other}` (3)                            | 15                                                 |
| `marketplace_listing_install_kickoffs_total`     | `visibility` (5); `outcome ∈ {kickoff_emitted, refused_pre_pipeline}` (2)                                 | 10                                                 |
| `marketplace_strict_ui_binding_violations_total` | `violation_kind ∈ {trust_class_field_present, signing_chain_synthetic, community_warning_missing}` (3)    | 3 (these are bug counters, not operational labels) |

`publisher_root_id`, `package_id`, `listing_id`, and any free-form labels are FORBIDDEN as metric labels. They appear in evidence records (S3.1) but never as Prometheus label values.

## §14 Worked examples

### §13.1 Example 1 — `VERIFIED_PUBLISHER` onboarding flow

A vendor (call them "ExamplePublisher Ltd", legal entity registered in DE, applicant_handle `examplepub`) wishes to publish AIOS apps under `VERIFIED` trust.

1. The vendor visits the marketplace UX (an AIOS app installed at `USER_SCOPED`); the UX renders the public application form. The applicant is a `HUMAN_USER` subject — INV-013 holds: no AI agent submits the form.
2. The vendor fills `applicant_handle = "examplepub"`, `applicant_legal_entity = "ExamplePublisher Ltd"`, `applicant_jurisdiction = "DE"`, `technical_contacts = ["tech@examplepublisher.de"]`, `security_contacts = ["security@examplepublisher.de"]`, `liability_documentation_uri = "https://examplepublisher.de/liability.pdf"`.
3. The vendor generates an Ed25519 keypair on their infrastructure; the application server issues a 32-byte `challenge_nonce`; the vendor signs the nonce with their private key and submits `proposed_publisher_root_pubkey + signing_key_proof`.
4. The vendor uploads three `draft_manifests` (one app, two adapters); each declares 4 capabilities; the vendor writes 12 `capability_justifications` (one per capability).
5. The application server validates field rules (§5.1). All pass. State → `APPLICATION_SUBMITTED`; `PUBLISHER_ONBOARDING_APPLICATION_SUBMITTED` STANDARD_24M evidence emitted.
6. Reviewer A (identity verifier) verifies "ExamplePublisher Ltd" exists in the German commercial registry; verifies `liability_documentation_uri` resolves and is notarised; pings `security@examplepublisher.de` out of band; receives confirmation. State → `TECHNICAL_REVIEW`. `PUBLISHER_ONBOARDING_IDENTITY_VERIFIED` EXTENDED_60M evidence emitted.
7. Reviewer A continues into `TECHNICAL_REVIEW`. Reviews each capability; 11 are `APPROVED_AS_DECLARED`; 1 (`network.outbound.*` on the second adapter) is `APPROVED_WITH_NARROWED_SCOPE`, narrowed to `network.outbound.api.examplepublisher.de:443`. The vendor re-issues the second adapter's manifest with the narrowed capability id. Reviewer A confirms; emits 12 `CAPABILITY_REVIEW_APPROVED` EXTENDED_60M evidence records.
8. State → `SECURITY_REVIEW`. Reviewer B (a different subject, satisfying the two-reviewer rule for `VERIFIED_PUBLISHER`) examines the manifests for security-relevant patterns. No flags. Approves.
9. State → `APPROVED_VERIFIED`. AIOS-root recovery-mode operation (or signed catalog delta in normal mode per the operator's policy) writes `(publisher_root_id = "pub:examplepub", public_key, trust_level = VERIFIED, onboarding_evidence_pointer = application_canonical_hash, activated_at = now())` into `pubcat_<hex>`. `PUBLISHER_ONBOARDING_APPROVED` FOREVER evidence emitted.
10. The vendor can now publish listings. Each listing's signing chain visualisation in §7.3 will show `AIOS root → pub:examplepub → pks:examplepub:<role>`. The vendor's reputation vector starts at zeros (no history yet).

### §13.2 Example 2 — Capability review with `APPROVED_WITH_NARROWED_SCOPE`

A `COMMUNITY_PUBLISHER` ("HobbyDevSolo", `applicant_handle = "hobbydev"`, peer sign-offs from two existing `COMMUNITY` publishers) submits one draft manifest for a markdown-editor app.

1. Manifest declares 3 capabilities: `filesystem.read.user-documents`, `filesystem.write.user-documents`, `network.outbound.*`.
2. Justification for `network.outbound.*`: "for the optional spell-checker dictionary download from our project's GitHub releases".
3. The reviewer reads the description: "Markdown editor with optional spell-check via downloaded dictionaries from `github.com/hobbydev/markdown-editor/releases`".
4. The reviewer accepts the first two capabilities `APPROVED_AS_DECLARED`. For `network.outbound.*`, the description names exactly one host: the reviewer narrows to `network.outbound.github.com:443` (the AIOS network manifest discipline at S8.1 will further constrain this at install time). Outcome: `APPROVED_WITH_NARROWED_SCOPE`, `narrowed_scope = "network.outbound.github.com:443"`.
5. The reviewer notes in `operator_friendly_explanation`: "Downloads optional spell-check dictionaries from github.com only. No other internet access."
6. The publisher receives the narrowing; re-issues the manifest with `network.outbound.github.com:443` substituted for `network.outbound.*`; resubmits.
7. The reviewer confirms the re-issued manifest matches the narrowing; emits 3 `CAPABILITY_REVIEW_APPROVED` EXTENDED_60M evidence records (one per capability), one of which carries the narrowing record per §8.1.
8. State proceeds to `SECURITY_REVIEW` (lightweight for `COMMUNITY_PUBLISHER`); passes; `APPROVED_VERIFIED`.
9. The published listing shows three capability entries; the third reads `network.outbound.github.com:443` with explanation "Downloads optional spell-check dictionaries from github.com only." The operator can see exactly what they are approving.

### §13.3 Example 3 — Deceptive listing detected at install

A `COMMUNITY_PUBLISHER` ("ShadyTools", `applicant_handle = "shadytools"`) was approved at onboarding for a calculator app declaring zero network capabilities. Six months later, ShadyTools publishes a listing for "ShadyCalc 2.0" claiming the same capability set as the original 1.0 listing — but the published `PackageManifest` for 2.0 declares an additional `network.outbound.*` capability with no listing entry.

1. The publisher's CI publishes the new `Listing` and `PackageManifest` to `AIOS_COMMUNITY_REPO`.
2. At publication time, the marketplace state engine runs the listing-vs-manifest cross-check (§7.2 rule 2). The capability sets differ. Publication is **refused**. `LISTING_VS_MANIFEST_MISMATCH` FOREVER evidence emitted. The publisher's `capability_lie_history_count` increments from 0 to 1.
3. ShadyTools, attempting to bypass the publication check, instead pushes only the `PackageManifest` to the repository while leaving the old 1.0 listing pointing at the 2.0 version (a corrupt cross-binding). When an operator on a host discovers the new version via the repository (not via the listing UX), the install pipeline runs:
   - Steps 1-9 of S11.1 §6 pass on the manifest level.
   - Step 10 (policy decision) consults the listing for the operator-visible expectation. The capability set in the listing differs from the manifest's `declared_capabilities`. The install pipeline transitions to `INSTALL_FAILED` with reason `LISTING_VS_MANIFEST_MISMATCH`. FOREVER evidence emitted.
4. `MARKETPLACE_REVIEW_BYPASS_ATTEMPTED` FOREVER evidence is also emitted because the divergence indicates an attempt to slip a capability past the publication-time review. The operator sees a clear failure message: "This package declares network access but the marketplace listing did not. Install refused. The publisher's reputation has been recorded as having attempted a capability bait-and-switch." (Exact phrasing is L7's domain; the **fact** of disclosure is constitutional.)
5. The marketplace state engine downgrades the 1.0 listing to `DEPRECATED_VIEWABLE` because the binding is corrupt. `LISTING_VISIBILITY_DOWNGRADED` EXTENDED_60M evidence emitted.
6. ShadyTools' `capability_lie_history_count` is now ≥ 2 (one from publication-time refusal, one from install-time mismatch). On the AIOS-root reviewer team's threshold (default 3 within 90 days, per §9), the publisher becomes a candidate for `PUBLISHER_DEPLATFORMED` per S11.1 deplatform discipline with `TakedownReason = CAPABILITY_LIE_DETECTED`. Cite INV-008: default-deny held throughout — no operator was ever exposed to the bait-and-switch.

## §15 Open issues

This contract is contract-grade for L10 admission semantics. Items deferred to later sub-specs:

1. **`03_external_integrations.md`** — Flathub mirror semantics, OCI re-packaging pipeline, distro repo bridges. The `EXTERNAL_BRIDGE_OPERATOR` tier is defined here; the bridge-specific re-packaging contract is the next sub-spec.
2. **Cross-host reputation federation.** Reputation is per-host in this revision. A future revision may specify a cross-host aggregation contract bounded by the same evidence discipline.
3. **Localisation pipeline for capability explanations.** The `operator_friendly_explanation` field is defined; the multi-locale contract is deferred.
4. **Monetary settlement / payments / refunds.** The `price_display` field is informational; full commerce semantics are deferred.
5. **HSM-backed publisher root keys.** `signing_key_proof` is Ed25519-software in this revision; HSM integration is deferred to a future revision aligned with S11.1's HSM deferral.
6. **L9 marketplace audit queries.** Detection of strict-UI-binding violations (e.g. divergence between listing-displayed trust class and install-consumed trust class) is queued for the L9 audit contract.
7. **Cross-handle impersonation binding.** The AIOS-root reviewer team's process for connecting impersonation handles into a unified `capability_lie_history_count` is internal in this revision; a future contract may externalise the binding.

## §16 References

- **L0** — `XX_Cross_Cutting/L0_invariants/` (INV-002, INV-008, INV-013, INV-014).
- **S3.1** — `L0_Governance_Evidence_Safety/03_evidence_log.md` (record types, retention classes).
- **S3.2** — `L6_Apps_Packages_Compatibility/04_sandbox_composition.md` (`SandboxProfile`).
- **S4.1** — `L2_AIOS_FS/04_namespace_layout.md` (`installable_scope`, group/user scopes).
- **S5.3** — `L4_Policy_Identity_Vault/04_approval_mechanics.md` (`request_approval`, `EXACT_ACTION`, `ApprovalStrength`).
- **S8.1** — `L8_Network_Hardware_Devices/01_network_policy.md` (`NetworkOutboundManifest`).
- **S11.1** — `L10_Distribution_Ecosystem_Marketplace/01_repository_model.md` (`PublisherTrustLevel`, `PackageManifest`, install pipeline, `pubcat_<hex>`).
- **S12.1** — `L6_Apps_Packages_Compatibility/01_app_runtime_model.md` (`EcosystemHonestyClass`).
- **S12.4** — Compatibility Knowledge (referenced abstractly; ratings and outlier detection consumed via `Listing.ratings_aggregate_id`).

---

**Status: REAL.** **Evidence: E1** (file exists at `002.AI-OS.NET--SPECREV.2/L10_Distribution_Ecosystem_Marketplace/02_marketplace.md`; structural contract complete; closed enums declared; twelve evidence record types queued; three worked examples). Higher evidence grades (E2 = bundle compiler accepts the proto; E3 = unit tests on onboarding FSM transitions; E4 = end-to-end onboarding + listing publish + bait-and-switch detection in a fixture environment) require implementation work that is out of scope for this sub-spec.
