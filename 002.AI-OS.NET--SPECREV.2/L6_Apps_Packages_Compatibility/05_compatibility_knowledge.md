# Compatibility Knowledge — Per-App Profile Database (Rev.2)

| Field          | Value                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| -------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Status         | `REAL` (initial; written 2026-05-09; E1 evidence — file exists, structural contract complete)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| Phase tag      | S12.4                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| Layer          | L6 Apps, Packages, Compatibility                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| Schema package | `aios.compatknowledge.v1alpha1`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| Consumes       | L0 INV-002 (AI proposes never executes), INV-008 (default-deny policy), INV-014 (no proof, no completion), INV-015 (evidence never contains secrets), INV-017 (sandbox floor constitutional); S0.1 envelope FSM (AI cannot transition `policy_pending → executing`); S3.1 Evidence Log (`RecordType` vocabulary, `STANDARD_24M` / `EXTENDED_60M` / `FOREVER` retention classes, BLAKE3 + JCS canonicalisation); S5.3 Approval Mechanics (`request_approval`, `EXACT_ACTION` binding, `ApprovalStrength`); S11.1 Repository Model (`AIOS_COMMUNITY_REPO`, `RepositoryKind`, `publisher_root_id`, `PublisherTrustLevel`, AIOS-root-signed publisher catalog); S12.1 App Runtime Model (`EcosystemRuntime`, `EcosystemHonestyClass`, `ManifestTranslationStrategy`, `ObservedBehavior`, Phase A/B/C/D pipeline, `RecipeTrustClass`, AppRecipe content addressing, capability-lie audit); S12.3 Compatibility Runtime (orchestration; concurrent) |
| Produces       | typed `CompatibilityRating` / `RatingDimension` / `EvidenceLevel` / `ProfileVisibility` enums; the `CompatibilityProfile` object schema; the per-operator weighted reputation algorithm with outlier detection; the closed import-mapping contract for ProtonDB / WineHQ AppDB / Flathub / Snapcraft (one-shot translation, never federation); the privacy-preserving share-opt-in protocol for operator-contributed observations; eight evidence record types queued for S3.1 Wave 8 consolidation; bounded-cardinality telemetry contract; the `ProfileRetiredReason` closed enum                                                                                                                                                                                                                                                                                                                                                           |

## 1. Purpose

S12.1 names the per-app compatibility profile registry as the long-lived companion to the recipe registry: a recipe says **how** an app is installed and sandboxed; a profile says **how well** that recipe actually works for real operators on real hardware over time. Without a profile contract, two failure modes are open:

- the L7 marketplace surface ranks recipes by reputation but has no closed taxonomy for what "works well" means, leading to ad-hoc emoji-rating inflation;
- a single malicious operator (or a coordinated farm) can publish a `PLATINUM` or `BORKED` rating that distorts an app's standing without any structural defence.

This contract closes both. It defines:

1. the `CompatibilityProfile` object — keyed by `app_id`, scoped per ecosystem runtime, signed per contribution, hash-chained per aggregation;
2. four closed vocabularies that fix what a rating means (`CompatibilityRating`), along which axis (`RatingDimension`), with what corroboration (`EvidenceLevel`), and at what visibility (`ProfileVisibility`);
3. the per-operator weighted aggregation algorithm and its outlier detector;
4. one-shot import mappings from ProtonDB / WineHQ AppDB / Flathub / Snapcraft, with attribution preserved and **all** operational recommendations re-routed through the local Phase A pre-flight pipeline of S12.1;
5. the adversarial robustness story: profile poisoning, fake reputation farms, coordinated suppression of breakage reports, single-operator outlier whitewashing.

This contract is a **knowledge layer**. It does not install. It does not sandbox. It does not approve. It feeds Phase B / Phase D proposers (S12.1) and the L7 marketplace surface with structured reputation, while every actual install is still gated by Phase A observation, Phase B operator-approved manifest, S5.3 EXACT_ACTION approval, and Phase C first-run audit. INV-002 binds: an AI subject reading the profile database to inform a Phase B proposal cannot use the database to bypass approval. The profile is advisory data; the approval is operational truth.

## 2. Position in the system

```text
                  ┌──────────────────────────────────────────────────────────────┐
                  │                          OPERATOR                            │
                  │              (HUMAN_USER subject; per L4 identity)           │
                  └──────────────────────────────────────────────────────────────┘
                                              │
                                              │ install browse + approval
                                              ▼
                  ┌──────────────────────────────────────────────────────────────┐
                  │                  L7 MARKETPLACE SURFACE                      │
                  │  (renders per-app CompatibilityProfile next to AppRecipe)    │
                  └──────────────────────────────────────────────────────────────┘
                                              │
                                              │ profile lookup by app_id
                                              ▼
                  ┌──────────────────────────────────────────────────────────────┐
                  │            COMPATIBILITY PROFILE DATABASE  (THIS SPEC)       │
                  │   - per-app CompatibilityProfile objects                     │
                  │   - per-operator weighted aggregate ratings                  │
                  │   - outlier detector + reputation farm detector              │
                  │   - import bridges to ProtonDB / WineHQ / Flathub / Snap     │
                  └──────────────────────────────────────────────────────────────┘
                            ▲                         │
                            │ contributes (operator)  │ feeds Phase B / Phase D
                            │                         ▼
                  ┌─────────────────────────────────────────────────────────────┐
                  │  S12.1 PHASE A → PHASE B → S5.3 APPROVAL → PHASE C AUDIT    │
                  │  (local install pipeline; profile is advisory only)         │
                  └─────────────────────────────────────────────────────────────┘
                            │
                            │ post-install runtime evidence
                            ▼
                  ┌─────────────────────────────────────────────────────────────┐
                  │  S3.1 EVIDENCE LOG (PROFILE_CONTRIBUTED, _AGGREGATED, …)    │
                  └─────────────────────────────────────────────────────────────┘
```

The profile database sits **alongside** the Community Recipe Registry (S12.1 §6) and the Repository Model (S11.1). A recipe and a profile are different objects with different lifecycles:

- a recipe is the install/sandbox plan; its key is `recipe:<vendor>:<app_name>:<version>`; it is content-addressed and changes only when the plan changes;
- a profile is the running-quality reputation of the (app, runtime) pair; its key is `profile:<app_id>:<ecosystem_runtime>`; it is appended-to over time as operators run the app and contribute observations.

A single recipe may be referenced by many profiles (one profile per recipe-runtime combination per app). A single profile may aggregate contributions from hundreds of operators over months. The profile is **not** content-addressed by its full state — it is a versioned evolving object whose every aggregation step emits a `PROFILE_RATING_AGGREGATED` evidence record.

## 3. Vocabulary (closed enums)

All vocabularies in this section are closed. Adding a value is a versioned spec change. Manifest validators, the registry's profile-ingest path, the import bridges, and the outlier detector MUST reject values outside the enum at parse time. None of these enums admits an `OPEN` or `OTHER` value.

### 3.1 `CompatibilityRating`

Closed enum, five values, deliberately mirroring the ProtonDB convention so that import is structurally lossless on the rating axis. Values are ordered from best to worst on a five-point ordinal scale.

| Value      | Semantics                                                                                                                                                                                                |
| ---------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `PLATINUM` | App runs as well as a native first-class implementation across all rated dimensions. No operator-visible compatibility fault. Recommended without caveat.                                                |
| `GOLD`     | App runs well after one trivial tweak (a launch flag, a config toggle, a shader-cache warm-up). Caveat is documented in `recommended_manifest_delta` and is operator-comprehensible.                     |
| `SILVER`   | App runs with a minor, persistent compromise — slightly reduced visual fidelity, an unsupported optional feature (e.g., in-game video playback), occasional recoverable hitch. Core gameplay/use intact. |
| `BRONZE`   | App runs but with a significant compromise — major feature missing, frequent stutter, audio breakage requiring workaround, reduced save-state safety. Operator may proceed with caveats.                 |
| `BORKED`   | App does not run, crashes on launch, corrupts state, or trips DRM/anti-cheat in a way the runtime cannot honestly mediate. Operator should not install without explicit override.                        |

The five values are ordinal: aggregation arithmetic is permitted only via the explicit weighted-bucket method of §5.2; no hidden numeric mapping exists. The enum is closed; an aggregator emitting any other value fails with `InvalidRatingValue` and emits `RECEIPT_REDACTION_FAILED` is not the right vehicle — the failure is a domain error and emits `PROFILE_RATING_AGGREGATED` with `outcome = AGGREGATION_REJECTED` (see §11).

### 3.2 `RatingDimension`

Closed enum, eight dimensions. A `CompatibilityRating` is computed **per dimension** and the profile aggregates per-dimension ratings; the L7 surface presents the worst dimension to the operator alongside the headline rating, so that a `PLATINUM` headline with a `BRONZE` audio dimension is never silently averaged into `GOLD`.

| Value                    | Question the dimension answers                                                                                                                                                                                                                                                    |
| ------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `LAUNCH_RELIABILITY`     | Does the app reach an interactive state on first launch and on subsequent launches without manual intervention?                                                                                                                                                                   |
| `GAMEPLAY_STABILITY`     | Does the app stay running through a representative interactive session without crashes, freezes, or non-recoverable errors? "Gameplay" is the dimension's historical name (ProtonDB heritage); for non-game apps, it is reinterpreted as "primary-workflow stability".            |
| `VISUAL_QUALITY`         | Are graphical primitives rendered with the fidelity the app intends (no missing shaders, no missing textures, no flickering)? For non-graphical apps, the dimension reports `NOT_APPLICABLE` rather than being silently averaged.                                                 |
| `AUDIO_FUNCTIONALITY`    | Does the app produce audio output and (if applicable) capture audio input correctly through the AIOS audio capability surface, with no missing channels or persistent dropouts?                                                                                                   |
| `INPUT_HANDLING`         | Are mouse, keyboard, gamepad, touch, and stylus inputs delivered to the app correctly and at the latency the operator perceives as native?                                                                                                                                        |
| `NETWORK_BEHAVIOR`       | Does the app's network behaviour stay within the declared `NetworkOutboundManifest` (S12.1) and use online features as advertised? This dimension overlaps with capability-lie evidence (S12.1 §11) — overlap is the design, not duplication.                                     |
| `SAVE_STATE_CORRECTNESS` | Are saves, configuration files, and persistent state written, read, and migrated without corruption across restarts and across the `recommended_manifest_delta` evolutions?                                                                                                       |
| `DRM_BEHAVIOR`           | Does any DRM, licence-check, or anti-cheat system interact with the runtime in a way the operator can honestly resolve? A `BORKED` here is not a moral judgement; it is a factual observation that the runtime cannot honestly mediate the protection mechanism on this hardware. |

The eight dimensions are independent in interpretation: a ProtonDB import that gives one `PLATINUM` aggregate must be unfolded across the eight dimensions using the import bridge's documented heuristic (§7), with explicit `EvidenceLevel = SELF_REPORTED` on dimensions the upstream did not measure separately. Forging a per-dimension rating that the upstream did not provide is forbidden at import; the bridge MUST emit `EvidenceLevel = SELF_REPORTED` on under-measured dimensions and the L7 surface MUST display the level alongside the rating.

A dimension that is genuinely inapplicable (e.g., `VISUAL_QUALITY` for a CLI utility) is recorded with the closed sentinel `RATING_NOT_APPLICABLE` (a sixth value of the rating enum? — no, this is enforced via `is_applicable = false` on the per-dimension record; the rating field is then absent and the aggregator skips the dimension entirely). The closed `CompatibilityRating` retains its five values exactly.

### 3.3 `EvidenceLevel`

Closed enum, four levels, ordered from weakest to strongest. The level on a per-dimension rating governs how heavily the rating contributes to aggregation (§5.2) and how visibly the L7 surface flags the rating to the operator.

| Value                         | Semantics                                                                                                                                                                                                                                                                                                                                                                       |
| ----------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `SELF_REPORTED`               | A single operator's stated opinion, without runtime evidence attached. Lowest aggregation weight. Imports from upstream registries default to this level on dimensions the upstream did not separately measure.                                                                                                                                                                 |
| `SINGLE_OPERATOR_OBSERVED`    | A single operator's rating accompanied by attached, redaction-passing runtime evidence (Phase C audit pass + a representative session evidence summary). Medium weight; subject to outlier review.                                                                                                                                                                              |
| `MULTI_OPERATOR_CORROBORATED` | Three or more independent operator subjects, each with `SINGLE_OPERATOR_OBSERVED`-grade evidence, agree on the rating within one bucket on the rating ordinal. High weight. The concrete corroboration threshold is a parameter of §5.4 and defaults to `min_corroborators = 3`, `bucket_tolerance = 1`.                                                                        |
| `VERIFIED_PUBLISHER`          | The package's `publisher_root_id` (S11.1 §3.1) at trust level `AIOS_VERIFIED` (or the upstream's analogue, attributed in `upstream_attribution`) attests the rating. Highest weight. AIOS does **not** treat upstream publisher attestation as equivalent to AIOS-root attestation; an imported `VERIFIED_PUBLISHER` claim is reset to `MULTI_OPERATOR_CORROBORATED` at import. |

The level enum is closed. A rating contribution carries exactly one level; a contribution that claims `VERIFIED_PUBLISHER` whose signature does not chain to the AIOS-root publisher catalog is rejected at ingest with `EvidenceLevelClaimUnverifiable` and emits `PROFILE_REPUTATION_FARM_SUSPECTED` only if the same contributor has tried such an unverifiable upgrade more than `farm_suspect_threshold` times (default 3) within `farm_suspect_window` (default 30 days); otherwise the ingest is rejected with no extraordinary evidence.

### 3.4 `ProfileVisibility`

Closed enum, three values. The visibility of a per-operator contribution governs whether it is aggregated into the public profile, only into a group's internal profile, or only retained locally for the contributing operator's own consumption.

| Value            | Semantics                                                                                                                                                                                                                                                                                                               |
| ---------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `PUBLIC`         | The operator opted in to publish the contribution to `AIOS_COMMUNITY_REPO` (S11.1). The contribution is signed by the operator (or by an anonymous ephemeral key derived from the operator's vault per S12.1 §6.2), aggregated into the global per-app profile, and visible to every operator who queries the database. |
| `GROUP_INTERNAL` | The contribution is shared only within a defined AIOS group (per S4.1 namespace + INV-011 cross-group access forbidden). The group-internal aggregator runs on the group's own infrastructure; the public registry never sees the contribution.                                                                         |
| `PERSONAL_ONLY`  | The contribution is retained locally on the contributing operator's host. Phase B/D proposers may consult it as private prior knowledge; it is never aggregated, never shared, never imported by any other operator.                                                                                                    |

The default is `PERSONAL_ONLY`. An operator must explicitly opt in to upgrade visibility to `GROUP_INTERNAL` or `PUBLIC`, and the upgrade is itself a typed action `compat.contribute_profile_observation` (§8) that requires S5.3 EXACT_ACTION approval. Visibility cannot be downgraded silently by the registry; a downgrade is recorded with `PROFILE_VISIBILITY_DOWNGRADED` evidence (§11), happens only in the adversarial-robustness flow (§9), and never erases the contributor's local copy.

### 3.5 `ProfileRetiredReason`

Closed enum, six values. A profile may be retired by AIOS-root authority or by the contributing community when an app changes identity (renamed, refactored, or replaced) or when the underlying recipe class becomes obsolete.

| Value                              | Semantics                                                                                                                       |
| ---------------------------------- | ------------------------------------------------------------------------------------------------------------------------------- |
| `RETIRED_APP_DELISTED`             | The app's upstream publisher delisted or sunset the app; no further contributions are accepted.                                 |
| `RETIRED_RECIPE_REPLACED`          | The recipe class the profile was bound to has been superseded by a structurally different recipe class (different runtime).     |
| `RETIRED_QUARANTINED_BY_AIOS_ROOT` | AIOS-root governance retired the profile because of evidence-grade abuse (e.g., confirmed coordinated reputation farm).         |
| `RETIRED_PUBLISHER_DEPLATFORMED`   | The associated publisher was deplatformed (S11.1 §3.1 `DEPLATFORMED`) and the profile is severed from active recommendations.   |
| `RETIRED_OPERATOR_REQUEST`         | The single contributing operator (in `PERSONAL_ONLY` or sole-contributor `GROUP_INTERNAL` cases) requested retirement.          |
| `RETIRED_DUE_TO_HONESTY_VIOLATION` | The profile's recipe was found to claim an `EcosystemHonestyClass` it cannot honour (S12.1 §9.5); profile is retired alongside. |

A retired profile is not deleted: its history remains audit-visible (`PROFILE_RETIRED` evidence, EXTENDED_60M); it is removed from active surfaces and from aggregation. A subsequent re-publication of the app under a new content-addressed recipe creates a new profile with a fresh history.

## 4. The `CompatibilityProfile` object

The profile is a structured object stored in `AIOS_COMMUNITY_REPO` (when `PUBLIC`) or in the group's namespace (when `GROUP_INTERNAL`) or in the operator's own AIOS-FS scope (when `PERSONAL_ONLY`). Its canonical schema is the proto IDL below; every contribution and every aggregation step emits an evidence receipt with the canonical envelope of S0.1 / S3.1.

```proto
message CompatibilityProfile {
  string profile_id = 1;
    // Format: "profile:<app_id>:<ecosystem_runtime_lower>"
    // app_id: the canonical app identifier (S11.1 §3.1 publisher catalog
    //         resolves the publisher; the app_id is publisher-scoped).
    // ecosystem_runtime_lower: the EcosystemRuntime enum value lowercased
    //         and underscore-stripped (e.g., "windows_proton").

  string app_id = 2;
    // Cite S11.1 §3.1 / §3.2: AIOS_COMMUNITY_REPO scopes app ids by publisher.

  EcosystemRuntime ecosystem_runtime = 3;
    // Cite S12.1 §3.1; one of twelve closed runtimes.

  RecipeTrustClass current_recipe_trust_class = 4;
    // Cite S12.1 §3.5; the recipe currently rated by this profile.

  repeated DimensionRating dimension_ratings = 10;
    // Per-dimension aggregated rating; see §4.1.

  CompatibilityRating headline_rating = 11;
    // The worst non-NOT_APPLICABLE dimension on the standing aggregate
    // (§5.3). Never higher than the worst dimension.

  EvidenceLevel headline_evidence_level = 12;
    // The lowest evidence level among the dimensions used to compute
    // headline_rating. Honest disclosure: a PLATINUM headline whose
    // worst-evidence dimension is SELF_REPORTED is presented to the
    // operator with the level visible alongside.

  repeated KnownIssue known_issues = 20;
    // Closed-shape typed list; see §4.2.

  ManifestDelta recommended_manifest_delta = 21;
    // Cite S12.1 §5 (ManifestDeltaOutcome) and §3 (typed manifest delta
    // shape). The delta is advisory; Phase D still requires operator
    // approval per S12.1.

  repeated SignedContribution contributing_operators = 30;
    // Per-operator contributions, each signed (§4.3).

  ReputationAggregate reputation = 40;
    // Aggregated counters (§5.2 / §5.3).

  repeated UpstreamAttribution upstream_attribution = 50;
    // ProtonDB / WineHQ AppDB / Flathub / Snapcraft attributions, if
    // any. Multiple attributions allowed (e.g., ProtonDB + WineHQ).

  ProfileVisibility visibility = 60;
    // Closed enum from §3.4; defaults to PERSONAL_ONLY at construction.

  google.protobuf.Timestamp created_at = 70;
  google.protobuf.Timestamp last_aggregated_at = 71;

  ProfileRetiredReason retired_reason = 80;
    // Unset if active. PROFILE_RETIRED_REASON_UNSPECIFIED is rejected
    // when retired_at is set.
  google.protobuf.Timestamp retired_at = 81;
}
```

### 4.1 Per-dimension rating

```proto
message DimensionRating {
  RatingDimension dimension = 1;          // closed enum, §3.2
  bool is_applicable = 2;                 // false → rating field absent
  CompatibilityRating rating = 3;         // closed enum, §3.1
  EvidenceLevel evidence_level = 4;       // closed enum, §3.3
  uint32 contribution_count = 5;          // contributors used in aggregation
  uint32 outlier_count = 6;               // flagged outliers excluded
  string aggregation_evidence_id = 7;     // PROFILE_RATING_AGGREGATED receipt
}
```

The dimension rating is the aggregator's output, not a contributor's input. A contributor signs a `ContributedDimensionRating` (which has the same fields except `contribution_count` and `outlier_count`); the aggregator combines these into the `DimensionRating` of the profile.

### 4.2 Known issue

```proto
message KnownIssue {
  string issue_id = 1;                    // "issue_<ulid26>"
  RatingDimension affected_dimension = 2; // closed enum
  KnownIssueClass class = 3;              // closed enum, §4.2.1
  string operator_facing_summary = 4;     // ≤ 240 chars, redaction-passed
  string evidence_id = 5;                 // S3.1 evidence id of first sighting
  uint32 occurrence_count = 6;            // distinct operators reporting
  google.protobuf.Timestamp first_seen_at = 7;
  google.protobuf.Timestamp last_seen_at = 8;
  bool resolved = 9;                      // resolved by manifest delta or upstream patch
  string resolution_evidence_id = 10;     // if resolved
}

enum KnownIssueClass {
  KNOWN_ISSUE_CLASS_UNSPECIFIED = 0;
  ISSUE_CRASH_ON_LAUNCH = 1;
  ISSUE_CRASH_INTERMITTENT = 2;
  ISSUE_FEATURE_MISSING = 3;
  ISSUE_VISUAL_GLITCH = 4;
  ISSUE_AUDIO_GLITCH = 5;
  ISSUE_INPUT_LATENCY_HIGH = 6;
  ISSUE_NETWORK_FALLBACK = 7;
  ISSUE_SAVE_STATE_LOSS = 8;
  ISSUE_DRM_REJECTION = 9;
  ISSUE_ANTICHEAT_REJECTION = 10;
  ISSUE_HONESTY_CLASS_DRIFT = 11;
}
```

The `KnownIssueClass` is closed. Issues are reported by operators alongside their rating; the aggregator deduplicates by class + affected dimension within a configurable similarity radius (default: same class + same dimension within a 30-day rolling window collapse into a single `known_issues` entry whose `occurrence_count` increments).

### 4.3 Signed contribution

```proto
message SignedContribution {
  string contribution_id = 1;             // "contrib_<ulid26>"
  string contributor_subject_canonical_id = 2;
    // S11.1 §3.1 subject canonical id. May be an operator's stable id
    // or an anonymous ephemeral key (cite S12.1 §6.2 anonymous-contribution
    // path); the registry treats them identically except in farm-detection
    // (§9.2) where ephemeral keys without continuous reputation history
    // accumulate slower aggregation weight.

  string profile_id = 3;                  // back-reference

  repeated ContributedDimensionRating dimension_ratings = 10;
  repeated ContributedKnownIssue known_issues = 11;
  ContributedManifestDelta manifest_delta = 12;
    // Operator's proposed delta to recommended_manifest_delta; advisory.

  string runtime_evidence_summary_hash = 20;
    // BLAKE3 of the redacted runtime evidence summary the operator
    // attached (Phase C pass receipt, representative-session summary).
    // The full evidence stays on the operator's host; only the hash
    // travels with the contribution.

  google.protobuf.Timestamp contributed_at = 30;
  bytes contributor_signature_ed25519 = 31;
    // Detached signature over the JCS canonicalisation of fields 1..30.
    // Verified at ingest against the contributor's published public key
    // (publisher catalog or anonymous ephemeral key bundle).

  ProfileVisibility declared_visibility = 40;
    // The contributor's declared visibility for this contribution.

  uint32 contributor_weight_at_contribution_time = 50;
    // Snapshot of the per-operator weight (§5.1) at the moment the
    // contribution was ingested. This is what the aggregator uses for
    // this contribution forever; later weight changes do not retro-
    // actively re-weight historical contributions (auditability).
}
```

The signed contribution is the unit of input to the aggregator. The signature is verified against the contributor's known public key at ingest; signature failure rejects the contribution with `ContributionSignatureInvalid` and does **not** emit a farm-detection record on a single failure (innocent key-rotation typo).

## 5. Reputation algorithm

The aggregator runs in two modes:

- on each accepted contribution (incremental), updating per-dimension running aggregates;
- on a periodic rebuild (default: daily), recomputing aggregates from scratch from the full retained signed-contribution log to detect drift between the incremental and full-rebuild outputs (drift > 0.05 ordinal-distance on any dimension emits `PROFILE_RATING_AGGREGATED` with `outcome = DRIFT_DETECTED` and triggers a manual review).

### 5.1 Per-operator weight

Each operator subject carries a profile-database weight scalar in `[0.0, 1.0]`. The default at first contribution is `0.5`. The weight evolves per contribution by the rules below; the rules are mechanical, deterministic, and re-derivable from the evidence log alone — auditors can recompute any weight from the evidence chain.

Inputs that **increase** weight:

- a contribution whose `runtime_evidence_summary_hash` matches a Phase C audit pass receipt verifiable in the contributor's local segment (proof of real install): `+0.05` per accepted contribution, capped at `0.9` from this source alone;
- a contribution whose dimension rating is later corroborated by `≥ min_corroborators` independent operators within `bucket_tolerance` of the contributor's rating: `+0.02` per corroboration event, capped at `0.95` total weight.

Inputs that **decrease** weight:

- a contribution flagged as outlier (§5.4) and not subsequently corroborated: `-0.10` per flagged contribution;
- a contribution accompanied by an unverifiable `EvidenceLevel = VERIFIED_PUBLISHER` claim: `-0.20` per claim;
- detection of coordinated-farm signals (§9.2) in which this operator's id appears: weight drops to `0.0` and the operator's contributions are unwound from active aggregates (§9.2).

Weight values are persisted in the registry's per-operator weight ledger; a contribution's `contributor_weight_at_contribution_time` snapshot is the value at ingest, not at aggregation, to make historical aggregations stable. Weight changes are logged via `PROFILE_RATING_AGGREGATED` payload extension; weight ledgers themselves are not separately published (privacy: an operator's reputation is between the operator and the registry; only the aggregate effect of the weights is operator-visible).

### 5.2 Per-dimension aggregation

For a given `(profile_id, RatingDimension)` pair, let `S` be the set of accepted, non-outlier `ContributedDimensionRating` entries with `is_applicable = true`. For each rating bucket `b ∈ {PLATINUM, GOLD, SILVER, BRONZE, BORKED}`, define:

```text
weighted_bucket_count[b] = Σ_{r ∈ S, r.rating == b} r.contributor_weight_at_contribution_time
```

The aggregated rating is the bucket with the maximum `weighted_bucket_count`, with ties broken in favour of the lower (worse) bucket. This biases the aggregator toward honesty: in equal-weighted ties, the worse rating wins, which prevents single-operator inflation under tied conditions.

If `Σ_b weighted_bucket_count[b] < min_evidence_for_aggregation` (default 0.5; equivalent to one default-weight contribution), the dimension's rating is reported as `evidence_level = SELF_REPORTED` regardless of its constituent contributions' levels — there is not enough corroborated weight to claim more.

The aggregator emits a `PROFILE_RATING_AGGREGATED` receipt (§11) on every aggregation, carrying the input contribution count, the outlier count, the resulting rating and evidence level, and the running drift between this aggregation and the previous one for the same `(profile_id, dimension)` pair.

### 5.3 Headline rating

```text
applicable_dims = { d ∈ profile.dimension_ratings | d.is_applicable }
headline_rating = min_ordinal({ d.rating | d ∈ applicable_dims })
headline_evidence_level = min_evidence_level({ d.evidence_level | d ∈ applicable_dims
                                                    | d.rating == headline_rating })
```

The headline is the worst applicable dimension, never an arithmetic mean across dimensions. The L7 marketplace surface presents the headline alongside the worst dimension's name and evidence level, and the operator sees the truth that "this app is `GOLD` because save-state correctness is `GOLD`; everything else is `PLATINUM`" rather than a manufactured average.

### 5.4 Outlier detection

For a given `(profile_id, dimension)` aggregation step, with the population `S` defined as in §5.2, an individual contribution `r ∈ S` is flagged as an outlier when **all** of the following hold:

```text
1. |ordinal(r.rating) - ordinal(weighted_median(S))| ≥ outlier_distance_threshold   (default 3 buckets)
2. r.evidence_level ∈ { SELF_REPORTED, SINGLE_OPERATOR_OBSERVED }
3. count of contributions corroborating r within bucket_tolerance < min_corroborators_for_outlier_save (default 1)
4. r is not VERIFIED_PUBLISHER (which is exempt from outlier flagging until §9.3 farm checks fire)
```

A flagged contribution is excluded from the current aggregation, recorded with a `PROFILE_OUTLIER_DETECTED` receipt (§11), and its contributor's weight decrements per §5.1. The contribution is **not** removed from the contributor's local copy or from the public log — the audit trail remains; only the aggregator stops counting it.

The outlier detector is one-sided in **both** directions deliberately: a single `PLATINUM` among many `BORKED` is flagged with the same vigilance as a single `BORKED` among many `PLATINUM`. Both shapes of distortion are equally interesting; the contract does not assume that "high" ratings are benign and "low" ratings are suspect.

The threshold parameters (`outlier_distance_threshold`, `bucket_tolerance`, `min_corroborators`, `min_evidence_for_aggregation`) live in a versioned, AIOS-root-signed parameters bundle published alongside the registry. Tuning is not free-form: a parameter change is a versioned spec event that emits `PROFILE_RATING_AGGREGATED` with a `parameters_changed` marker for every aggregation done under the new parameters.

## 6. Privacy-preserving share opt-in

A contribution at `PUBLIC` visibility carries the operator's chosen identity to the global registry. A contribution at `GROUP_INTERNAL` carries it to the group only. A contribution at `PERSONAL_ONLY` is never shared. The contract enforces this with three mechanisms:

### 6.1 Default `PERSONAL_ONLY`

Every `compat.contribute_profile_observation` typed action constructed by Phase B / Phase D (S12.1) or by an L7 affordance defaults `declared_visibility = PERSONAL_ONLY`. An operator who wants to share must explicitly upgrade the visibility, and the upgrade is presented in the S5.3 approval prompt as a separate operator-comprehensible disclosure ("you are about to share this rating with the global community; your operator id will appear in the contribution as `<id>`").

### 6.2 Anonymous ephemeral keys

Per S12.1 §6.2, an operator who wants to contribute publicly without binding the contribution to a long-lived identity may emit the contribution under an ephemeral signing key derived from the operator's vault (S5.2 `KEY_DERIVE`). The ephemeral key is published in the contribution; the registry verifies the signature against it; the operator's stable id never appears in the public log. The vault retains the derivation parent; the operator can re-derive the same ephemeral key to demonstrate ownership in a later dispute, but the public log stays anonymous unless the operator chooses to disclose.

The ephemeral key is single-use per contribution. Re-using the same ephemeral key across multiple contributions makes the contributions linkable without further analysis; this is an operator choice (some operators want a stable anonymous reputation, some want unlinkability) and is exposed at the L7 surface as "stable anonymous handle" vs "fresh handle per contribution".

### 6.3 Group-internal aggregation isolation

A `GROUP_INTERNAL` contribution never leaves the group's namespace. The group's aggregator runs in the group's `/aios/groups/<group_id>/services/compat-aggregator/` namespace (cite S4.1); the group's profile database is a separate object whose `profile_id` collides with the public `profile_id` deliberately (same key shape) but whose contents are scoped to the group. The Phase B proposer in a group context queries both: it consults the group profile first and falls back to the public profile only if the group profile has insufficient evidence. INV-011 (cross-group access forbidden) is enforced by the namespace and policy layers; the group profile is never readable by another group.

## 7. Import bridges (one-shot translation, never federation)

The contract admits one-shot translation imports from four upstream registries. Each import is mechanical, attribution-preserving, and **does not** establish a federation: AIOS does not subscribe to upstream changes, does not auto-pull updated upstream ratings, does not use upstream APIs at install time. An import is a typed action `compat.import_profile_from_upstream` (§8) that produces a snapshot profile with `upstream_attribution` populated; the snapshot is then aggregated with local contributions like any other profile.

### 7.1 ProtonDB → AIOS

| ProtonDB field                    | AIOS profile field                                                                                                                                      |
| --------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| App AppId / SteamId               | `app_id` (translated via the AIOS-root-signed Steam-app-id↔AIOS-app-id table, versioned)                                                                |
| Headline rating tier              | per-dimension `LAUNCH_RELIABILITY` and `GAMEPLAY_STABILITY` ratings; other dimensions default to `is_applicable = true, evidence_level = SELF_REPORTED` |
| Recipe (launch options, env vars) | `recommended_manifest_delta` populated as advisory; **never auto-applied**                                                                              |
| Per-report timestamps             | `created_at` of the synthesised contribution; bridge contributor id = `_system:bridge:protondb`                                                         |
| Source URL                        | `upstream_attribution[i] = { source = "protondb", ref = "https://www.protondb.com/app/<id>", imported_at = <ts> }`                                      |

The synthesised contribution is signed by the bridge subject (`_system:bridge:protondb`) whose key chain is recorded in the AIOS-root-signed publisher catalog. The contribution carries `evidence_level = SELF_REPORTED` because the upstream's per-report runtime evidence is not BLAKE3-hash-verifiable in the AIOS evidence chain; raising the level requires local re-corroboration.

### 7.2 WineHQ AppDB → AIOS

WineHQ AppDB classifies on a Platinum/Gold/Silver/Bronze/Garbage scale plus per-test-result granularity. The bridge maps:

- `Platinum` → `PLATINUM`; `Gold` → `GOLD`; `Silver` → `SILVER`; `Bronze` → `BRONZE`; `Garbage` → `BORKED`;
- per-test-result narrative is parsed for known issue patterns and synthesised into `KnownIssue` entries with `class = ISSUE_*` per the closed enum; unparseable narrative is preserved verbatim in `operator_facing_summary` truncated to 240 chars and `class = KNOWN_ISSUE_CLASS_UNSPECIFIED` is **rejected** at import (the bridge must produce a closed-enum class or skip the issue).

Bridge contributor id: `_system:bridge:winehq-appdb`. Same `evidence_level = SELF_REPORTED` discipline as ProtonDB.

### 7.3 Flathub → AIOS

Flathub metadata is more structured (manifest, finishes, rating-style metadata via OARS) but does not provide compatibility ratings per-runtime. The bridge:

- imports the Flathub `app_id` and the published manifest as evidence on `recommended_manifest_delta`;
- imports OARS content ratings as a **separate** axis recorded in the profile's `notes` (advisory, non-aggregated) — OARS is content-appropriateness, not runtime-compatibility, and the contract does not conflate them;
- assigns `EcosystemRuntime = RUNTIME_FLATPAK` and constructs a profile with all dimensions defaulted to `evidence_level = SELF_REPORTED, is_applicable = true, rating = GOLD` only when Flathub publisher has `verified` upstream status — otherwise the bridge constructs an empty profile (no synthesised ratings; only manifest evidence).

Bridge contributor id: `_system:bridge:flathub`.

### 7.4 Snapcraft → AIOS

Snapcraft store metadata provides plug/slot declarations and tracks. The bridge:

- imports plug/slot declarations as `recommended_manifest_delta` advisory;
- assigns `EcosystemRuntime = RUNTIME_SNAP`;
- when the Snapcraft track is `stable` and the publisher is in Canonical's verified-publisher list, constructs a profile with `evidence_level = SELF_REPORTED, rating = GOLD` defaults; otherwise empty profile (manifest-only).

Bridge contributor id: `_system:bridge:snapcraft`.

### 7.5 Import discipline (constitutional)

Across all four bridges:

- the import is one-shot. No bridge subscribes to upstream change feeds. A new import is a fresh typed action with operator approval.
- every dimension rating produced by a bridge has `evidence_level = SELF_REPORTED` regardless of the upstream's reported confidence; promotion to `SINGLE_OPERATOR_OBSERVED` or higher requires local AIOS contributions.
- every recommendation (`recommended_manifest_delta`, suggested launch options, plug/slot acceptance) is **advisory metadata**. The first install of an imported app on the operator's host runs full Phase A → Phase B → S5.3 → Phase C exactly as if no profile had ever been imported. The profile speeds up Phase B's proposer (better starting point) and informs the operator's approval (richer disclosure); it does not bypass any audit. `PROFILE_IMPORTED_FROM_UPSTREAM` evidence is emitted on import; `APP_OBSERVE_STARTED` (S12.1 §11) still fires on first install.
- the bridge's contributor weight is fixed at `0.30` (below the default `0.50`) so that local operator contributions outweigh bridge-imported claims as soon as a few real installs accumulate. This is a deliberate asymmetry: AIOS prefers its own evidence over upstream's.

## 8. Typed actions queued for S10.1 follow-up

This contract introduces three typed actions queued for S10.1 Wave 8 consolidation. The contract does not modify S10.1 — the orchestrator integrates the actions in Wave 8.

| Typed action id                         | Dispatch           | Subject discipline                                                                                                        |
| --------------------------------------- | ------------------ | ------------------------------------------------------------------------------------------------------------------------- |
| `compat.contribute_profile_observation` | `ISOLATED_SANDBOX` | HUMAN_USER (operator-initiated; AI subjects can construct the action envelope but the FSM blocks transition to executing) |
| `compat.import_profile_from_upstream`   | `ISOLATED_SANDBOX` | `_system:bridge:<source>` system bridge subject; operator-initiated via S5.3 approval                                     |
| `compat.review_outlier_contribution`    | `ISOLATED_SANDBOX` | HUMAN_USER (an operator with `compat.review` capability) or AIOS-root governance                                          |

Both `compat.contribute_profile_observation` and `compat.import_profile_from_upstream` go through S5.3 approval with `EXACT_ACTION` binding; the import action additionally carries the upstream source name in the approval prompt so the operator sees, e.g., "import 1 247 ProtonDB ratings as advisory metadata; no installs are performed".

INV-002 binds: an AI subject's contribution attempt is rejected by the S0.1 envelope FSM on `policy_pending → executing`. An AI subject can construct an envelope (e.g., a Phase D delta proposer notes "I would like to record this delta as a rating contribution") but the operator must approve the contribution as a separate, distinguishable approval — never collapsed into the install approval.

## 9. Adversarial robustness

This section enumerates the named adversaries this contract addresses and how it addresses each one mechanically.

### 9.1 Profile poisoning by a malicious operator

**Adversary:** a single operator submits an extreme rating (`PLATINUM` or `BORKED`) inconsistent with the consensus, hoping to skew the headline rating and drive operators toward (or away from) an app.

**Mitigation:** the outlier detector (§5.4) flags the contribution at the next aggregation. The flagged contribution is excluded from the active aggregate; the contributor's weight decrements per §5.1. The flag emits `PROFILE_OUTLIER_DETECTED` evidence (EXTENDED_60M); the contribution remains in the public log but stops counting.

A repeated pattern of outlier contributions from the same operator drives the operator's weight to the floor (`0.0`); at that point all of the operator's historical and future contributions are excluded from active aggregation. The operator id remains in the public log; auditors can reconstruct the operator's full contribution history.

### 9.2 Fake reputation farms

**Adversary:** a coordinated group creates `N` fresh operator identities, each contributing the same rating to bias the aggregate. With enough freshly created identities, the weighted bucket for the target rating could overcome consensus.

**Mitigation:** the registry tracks **signature chain age** and **weight provenance** per contributor:

- a contribution from an operator whose identity (or whose ephemeral key bundle) was first seen `< new_identity_skepticism_window` (default 30 days) ago contributes at half weight;
- a contribution whose runtime evidence summary hash cannot be cross-verified against a publicly-attestable Phase C audit pass receipt (S3.1) drops weight to `0.10`;
- correlated bursts — `≥ farm_burst_threshold` contributions (default 5) on the same `(profile_id, dimension)` pair from contributors whose first-seen timestamps cluster within `farm_burst_window` (default 24 hours) — are flagged and emit `PROFILE_REPUTATION_FARM_SUSPECTED` (FOREVER). The detector does not reject the contributions immediately; the FOREVER record triggers AIOS-root governance review, and the contributions' weights are held at `0.0` pending review.

The signature chain analysis reads the publisher catalog (S11.1 §3.1) and the per-operator first-seen ledger; both are AIOS-root-signed, append-only, and not forgeable by a contributing party.

### 9.3 Coordinated suppression of breakage reports

**Adversary:** the publisher of a popular but broken app (or a reputation farm acting on the publisher's behalf) floods the registry with `PLATINUM` contributions to drown out genuine `BRONZE` / `BORKED` reports from real operators.

**Mitigation:** the `MULTI_OPERATOR_CORROBORATED` evidence level (§3.3) requires `min_corroborators` independent operators, each carrying `SINGLE_OPERATOR_OBSERVED`-grade evidence (a verifiable Phase C audit pass + representative-session evidence summary). A flood of `SELF_REPORTED` `PLATINUM` contributions cannot lift the dimension's evidence level above `SELF_REPORTED`; the headline evidence level surfaces this, and the operator sees `headline_rating = PLATINUM, headline_evidence_level = SELF_REPORTED` — a structurally weaker disclosure than the same headline at `MULTI_OPERATOR_CORROBORATED`.

In addition: the L7 marketplace surface displays the **distribution** of recent contributions per dimension (a histogram of bucket counts in the last 30 days), making a sudden spike of `PLATINUM` contributions visible alongside the actual ratings. Suppression of breakage reports cannot suppress the visible histogram.

A confirmed coordinated suppression event (operator complaints + AIOS-root review) emits `PROFILE_REPUTATION_FARM_SUSPECTED` (FOREVER) and unwinds the suspect contributions from the active aggregate via §5.1 weight-to-zero.

### 9.4 Single-operator outlier whitewashing

**Adversary:** a contributor whose past contributions were flagged as outliers continues to contribute, hoping that with enough volume the outlier flags will become statistical noise.

**Mitigation:** outlier flags compound on the contributor's weight (§5.1). After enough flagged contributions, the contributor's weight reaches `0.0` and further contributions are recorded but not counted. The contributor cannot reset weight by creating a fresh identity without falling under the new-identity-skepticism rule (§9.2). The combination of weight decay and new-identity skepticism makes whitewashing structurally expensive.

### 9.5 Honesty class drift through rating laundering

**Adversary:** a publisher whose recipe was previously rated honestly attempts to upload contributions that effectively re-rate the app's `EcosystemHonestyClass` (e.g., by aggregating `PLATINUM` ratings on a recipe that genuinely fails `DRM_BEHAVIOR`).

**Mitigation:** `EcosystemHonestyClass` is **not** a function of profile aggregation. It is a property of the recipe (S12.1 §3.2) and is set by the recipe publisher, audited at registry ingest, and verified by Phase C runtime observations (S12.1 §9.5). A profile cannot uplift a recipe's honesty class. A recipe whose `EcosystemHonestyClass` violates observed runtime behaviour emits `APP_HONESTY_CLASS_VIOLATION` (S12.1 §11, FOREVER) regardless of the profile's headline rating; the profile is then retired with `RETIRED_DUE_TO_HONESTY_VIOLATION` and a fresh profile is created when the recipe is re-published with corrected honesty class.

### 9.6 Visibility downgrade abuse by registry

**Adversary:** an operator's `PUBLIC` contribution is silently downgraded to `GROUP_INTERNAL` or `PERSONAL_ONLY` by an attacker with registry write access, suppressing the operator's voice without their knowledge.

**Mitigation:** every visibility change emits `PROFILE_VISIBILITY_DOWNGRADED` evidence (EXTENDED_60M) signed by the registry under AIOS-root authority. The contributing operator's local copy of the contribution always retains the operator's chosen visibility; the operator can re-publish the same contribution under any anonymous ephemeral key if the registry persistently downgrades; downgrade evidence remains audit-visible. A registry that emits downgrade evidence without operator-comprehensible reason invites AIOS-root review.

### 9.7 Weight ledger forgery

**Adversary:** the registry forges per-operator weight values to over-amplify favoured contributors or under-amplify disfavoured ones.

**Mitigation:** per-operator weight is **derivable from the signed evidence log** alone. Every weight-changing event (acceptance, corroboration, outlier flag, farm suspicion) is recorded as a `PROFILE_RATING_AGGREGATED` payload extension whose receipt is part of the segment hash chain (S3.1). An auditor reads the chain and recomputes the weight; a registry-claimed weight that disagrees with the derivable weight is a forgery, detectable at audit, and emits a generic forgery record (cited from S3.1: `RECEIPT_FORGERY_DETECTED` analogue is the governance vehicle).

### 9.8 Bridge poisoning

**Adversary:** an attacker injects a malicious entry into a ProtonDB / WineHQ / Flathub / Snapcraft import path (e.g., a man-in-the-middle on the upstream API) to bias the imported profile.

**Mitigation:** the import action's payload is the upstream's signed response (where signed) plus the bridge's wrapping signature. A man-in-the-middle without the upstream's signing key cannot forge a `protondb` source; an upstream that signs nothing (most current upstreams) is treated as `SELF_REPORTED` by definition (§3.3) — the bridge's authority caps at the level upstream's actual evidence permits. Furthermore, the bridge's contributor weight is fixed at `0.30` (§7.5); even a fully successful poisoning event cannot dominate local contributions. An import's content hash is recorded in `PROFILE_IMPORTED_FROM_UPSTREAM` evidence (STANDARD_24M) for forensic re-derivation.

### 9.9 AI subject contribution forging

**Adversary:** an AI subject attempts to emit a `compat.contribute_profile_observation` action as if it were the operator, biasing aggregation toward AI-preferred outcomes.

**Mitigation:** S0.1 envelope FSM rejects the transition `policy_pending → executing` for any AI subject on this action (cite INV-002). The AI subject can populate the envelope (a Phase D delta proposer wishing to suggest a contribution); the human operator must approve via S5.3 EXACT_ACTION. An AI-initiated transition attempt emits the analogue of `APP_AI_DIRECT_INSTALL_ATTEMPTED_BLOCKED` from S12.1 — the contract reuses that evidence type rather than introducing a duplicate.

## 10. Telemetry contract

All metrics use bounded label cardinality. App ids, profile ids, contributor ids, contribution ids, observation ids, and recipe ids are NEVER labels — they appear in evidence records, never as Prometheus labels.

| Metric                                         | Type      | Labels (closed sets)                                                                                    |
| ---------------------------------------------- | --------- | ------------------------------------------------------------------------------------------------------- |
| `compat_profile_active_count`                  | gauge     | `ecosystem_runtime` (12-value enum, S12.1 §3.1), `headline_rating` (5-value enum, §3.1)                 |
| `compat_profile_aggregation_total`             | counter   | `outcome` (success / drift_detected / aggregation_rejected), `dimension` (8-value enum, §3.2)           |
| `compat_profile_outlier_detected_total`        | counter   | `dimension`, `direction` (closed: HIGH_OUTLIER / LOW_OUTLIER)                                           |
| `compat_profile_contribution_total`            | counter   | `visibility` (3-value enum, §3.4), `evidence_level` (4-value enum, §3.3)                                |
| `compat_profile_recommendation_shown_total`    | counter   | `surface` (closed: MARKETPLACE_INSTALL_PROMPT / PHASE_B_PROPOSER / PHASE_D_PROPOSER), `headline_rating` |
| `compat_profile_imported_total`                | counter   | `source` (closed: PROTONDB / WINEHQ_APPDB / FLATHUB / SNAPCRAFT)                                        |
| `compat_profile_reputation_farm_suspect_total` | counter   | `signal` (closed: BURST_SAME_DIM / EPHEMERAL_KEY_CLUSTER / WEIGHT_FORGERY_SUSPECT)                      |
| `compat_profile_visibility_downgrade_total`    | counter   | `from_visibility` (3-value enum), `to_visibility` (3-value enum)                                        |
| `compat_profile_retired_total`                 | counter   | `reason` (6-value `ProfileRetiredReason` enum)                                                          |
| `compat_profile_aggregation_drift_seconds`     | histogram | `dimension`                                                                                             |

Cardinality budget: ≤ 200 active label tuples per metric. The `ecosystem_runtime` enum has 12 values; `headline_rating` has 5; `dimension` has 8; `visibility` has 3; `evidence_level` has 4; `source` has 4; `reason` has 6. The product of any two used together is well under the budget.

## 11. Evidence record types (queued for S3.1 Wave 8)

The following eight record types are queued for the S3.1 RecordType closed vocabulary. This contract does NOT modify S3.1 — the orchestrator integrates these in Wave 8.

| Record type                         | Trigger                                                                                                                                                 | Retention class |
| ----------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------- |
| `PROFILE_CONTRIBUTED`               | An operator's `compat.contribute_profile_observation` action transitioned to `succeeded`; the contribution was ingested.                                | `STANDARD_24M`  |
| `PROFILE_RATING_AGGREGATED`         | An aggregation step (incremental or rebuild) produced a per-dimension rating; carries before/after rating, drift, contribution counts, weight changes.  | `STANDARD_24M`  |
| `PROFILE_OUTLIER_DETECTED`          | A contribution was flagged as outlier per §5.4 and excluded from the active aggregate; carries the contribution id and the deviation magnitude.         | `EXTENDED_60M`  |
| `PROFILE_RECOMMENDATION_SHOWN`      | The L7 marketplace surface (or a Phase B/D proposer) presented a profile-derived recommendation to the operator; carries the surface and the headline.  | `STANDARD_24M`  |
| `PROFILE_IMPORTED_FROM_UPSTREAM`    | A `compat.import_profile_from_upstream` action transitioned to `succeeded`; carries the source, content hash, contribution count, attribution metadata. | `STANDARD_24M`  |
| `PROFILE_REPUTATION_FARM_SUSPECTED` | The farm detector (§9.2) flagged a coordinated cluster; AIOS-root review is requested; affected contributors' weights are held at `0.0`.                | `FOREVER`       |
| `PROFILE_VISIBILITY_DOWNGRADED`     | A profile contribution's effective visibility was reduced by registry action (e.g., suppression of a confirmed-malicious public contribution).          | `EXTENDED_60M`  |
| `PROFILE_RETIRED`                   | A profile transitioned to retired; carries the `ProfileRetiredReason` and a back-reference to the surviving recipe(s) if any.                           | `EXTENDED_60M`  |

Each record carries:

- `profile_id` and `app_id`;
- `ecosystem_runtime` and `headline_rating` (5-value enum);
- `contribution_id` (where applicable) and `contributor_subject_canonical_id` (where applicable, redacted to anonymous ephemeral key fingerprint when the contributor is anonymous);
- `aggregation_input_count`, `aggregation_outlier_count`, and `weight_delta_summary` for `PROFILE_RATING_AGGREGATED`;
- `upstream_attribution[i]` for `PROFILE_IMPORTED_FROM_UPSTREAM`;
- `farm_signal` (closed enum) and `cluster_size` for `PROFILE_REPUTATION_FARM_SUSPECTED`;
- redaction discipline per S0.1 / S3.1: no raw operator narrative beyond the 240-char `operator_facing_summary` (already redaction-passed at contribution time), no raw upstream payload, no PII.

Each record's payload is wrapped in the canonical `EvidenceReceipt` envelope of S0.1 §3 with `RetentionClass` matching the table above; mismatch is rejected at append with `RetentionClassMismatch`.

## 12. Worked examples

### 12.1 PLATINUM Steam game with high corroboration

```text
Step 1: 312 operators, over 8 months, contribute observations for
        profile:hogwarts-legacy:windows_proton.

        Per-operator distribution (LAUNCH_RELIABILITY):
          PLATINUM contributions  : 287    (avg weight 0.78)
          GOLD     contributions  : 18     (avg weight 0.62)
          SILVER   contributions  : 4      (avg weight 0.55)
          BORKED   contributions  : 3      (avg weight 0.40, all flagged
                                            outliers — see step 4)

        Per-operator distribution (SAVE_STATE_CORRECTNESS):
          PLATINUM contributions  : 295
          GOLD     contributions  : 15
          SILVER   contributions  : 2

Step 2: Aggregator runs. For LAUNCH_RELIABILITY:
          weighted_bucket_count[PLATINUM] = 287 × 0.78 ≈ 223.86
          weighted_bucket_count[GOLD]     = 18 × 0.62  ≈ 11.16
          weighted_bucket_count[SILVER]   = 4 × 0.55   = 2.20
          weighted_bucket_count[BORKED]   = 3 × 0.40   = 1.20  (excluded; outliers)
          → aggregate = PLATINUM, evidence_level = MULTI_OPERATOR_CORROBORATED

Step 3: Same arithmetic for SAVE_STATE_CORRECTNESS yields PLATINUM /
        MULTI_OPERATOR_CORROBORATED. All other dimensions similar.

        headline_rating = PLATINUM (worst applicable dimension is PLATINUM)
        headline_evidence_level = MULTI_OPERATOR_CORROBORATED

Step 4: Three BORKED outliers in LAUNCH_RELIABILITY were flagged at their
        respective ingest steps:
          - distance from weighted median = 4 buckets (> threshold 3);
          - evidence_level = SELF_REPORTED;
          - no corroboration within bucket_tolerance = 1.
        Each emitted PROFILE_OUTLIER_DETECTED (EXTENDED_60M).
        The three contributors' weights decreased per §5.1.
        The contributions remain in the public log; the aggregator
        excluded them.

Step 5: The L7 marketplace surface presents:
          headline: PLATINUM
          evidence: MULTI_OPERATOR_CORROBORATED (312 operators, 8 months)
          worst applicable dimension: LAUNCH_RELIABILITY (PLATINUM)
          known issues: 0 unresolved (4 historical, all resolved by upstream)
          recommended_manifest_delta: (advisory only) GAMESCOPE_SDL_BACKEND=1
          PROFILE_RECOMMENDATION_SHOWN emitted (STANDARD_24M).

Step 6: Operator approves install. Phase A → Phase B → S5.3 → Phase C
        runs as normal per S12.1; the high-corroboration profile speeds
        Phase B's proposer (better starting manifest) but does not bypass
        any audit gate.
```

### 12.2 BORKED app with a single-operator outlier under review

```text
Step 1: profile:obscure-driver-utility:linux_native has 14 operator
        contributions on LAUNCH_RELIABILITY:
          BORKED   : 13 contributions (avg weight 0.65,
                     each with SINGLE_OPERATOR_OBSERVED evidence:
                     Phase C audit failed at first launch in all 13 cases)
          PLATINUM : 1  contribution  (weight 0.55, evidence_level
                     = SELF_REPORTED, no Phase C evidence attached)

Step 2: The single PLATINUM contribution is processed by the outlier
        detector at ingest:
          ordinal(PLATINUM) = 0
          ordinal(weighted_median) = 4 (BORKED)
          distance = 4 ≥ outlier_distance_threshold (3): YES
          evidence_level = SELF_REPORTED: YES
          corroboration count: 0 < min_corroborators_for_outlier_save: YES
          not VERIFIED_PUBLISHER: YES
          → flagged as outlier
        PROFILE_OUTLIER_DETECTED emitted (EXTENDED_60M) with
          deviation = 4 buckets, direction = HIGH_OUTLIER.

Step 3: Aggregator excludes the outlier. For LAUNCH_RELIABILITY:
          weighted_bucket_count[BORKED] = 13 × 0.65 ≈ 8.45
          (all other buckets: 0)
          → aggregate = BORKED, evidence_level = MULTI_OPERATOR_CORROBORATED
        PROFILE_RATING_AGGREGATED emitted (STANDARD_24M).

Step 4: An L7 affordance shows the outlier review action to operators
        with `compat.review` capability:
          "1 PLATINUM rating among 13 BORKED — review?"
        An AIOS-root governance reviewer accepts the action; the
        review confirms the PLATINUM contribution was made by a single
        operator who never actually ran the app (no Phase C evidence).
        The reviewer marks the contribution `held = true`; the
        contributor's weight floor is set to 0.10.

Step 5: The L7 marketplace surface presents:
          headline: BORKED
          evidence: MULTI_OPERATOR_CORROBORATED (13 operators)
          worst applicable dimension: LAUNCH_RELIABILITY (BORKED)
          recommendation: "Do not install without explicit override."
          known issues: 1 unresolved (ISSUE_CRASH_ON_LAUNCH)
          PROFILE_RECOMMENDATION_SHOWN emitted (STANDARD_24M).

Step 6: An operator who chooses to proceed despite the BORKED rating
        sees an explicit override prompt at S5.3; the BORKED disclosure
        is part of the EXACT_ACTION binding text, and the approval
        evidence carries the disclosure hash (S12.1 §11 / S5.3
        approval evidence chain).
```

### 12.3 Profile import from ProtonDB with attribution

```text
Step 1: Operator clicks "import ProtonDB ratings for these 1 247 apps"
        in the L7 administration surface (one-shot bulk import).

Step 2: A typed action `compat.import_profile_from_upstream` is
        constructed:
          subject = HUMAN_USER (operator)
          dispatch = ISOLATED_SANDBOX
          inputs = {
            source = "protondb",
            artifact_url = "https://www.protondb.com/api/v1/.../export.json",
            artifact_hash = BLAKE3(<downloaded bytes>),
            count = 1247,
          }
        The bridge subject `_system:bridge:protondb` performs the
        download in the sandbox; the artifact_hash is recorded.

Step 3: Approval prompt shows:
          "Import 1 247 ProtonDB ratings as advisory metadata.
           No installs are performed.
           Each rating will be SELF_REPORTED evidence_level.
           Bridge contributor weight is fixed at 0.30 — local
           operator contributions will outweigh this import as
           soon as a few real installs accumulate.
           Source attribution will be preserved."
        Operator approves; EXACT_ACTION binding consumed.

Step 4: For each of the 1 247 entries, a synthesised SignedContribution
        is constructed by the bridge:
          contributor_subject_canonical_id = "_system:bridge:protondb"
          dimension_ratings:
            LAUNCH_RELIABILITY = <upstream tier mapping>
            GAMEPLAY_STABILITY = <upstream tier mapping>
            VISUAL_QUALITY     = <upstream tier mapping>
            AUDIO_FUNCTIONALITY = SELF_REPORTED, is_applicable = false
                                  unless upstream has dedicated audio data
            (all dimensions evidence_level = SELF_REPORTED)
          known_issues = parsed from upstream report narrative,
                         all class fields ∈ closed KnownIssueClass enum;
                         narrative that cannot be classified is
                         dropped (not silently mapped to UNSPECIFIED).
          contributor_signature_ed25519 = <bridge key>
          contributor_weight_at_contribution_time = 0.30
        upstream_attribution[0] = {
          source = "protondb",
          ref = "https://www.protondb.com/app/<id>",
          imported_at = <ts>,
          import_artifact_hash = <BLAKE3>,
        }

Step 5: PROFILE_IMPORTED_FROM_UPSTREAM emitted once for the action,
        carrying source, artifact_hash, and count = 1 247.
        Per-profile PROFILE_CONTRIBUTED is also emitted for each
        synthesised contribution (1 247 records, STANDARD_24M).
        Aggregation runs; per-dimension PROFILE_RATING_AGGREGATED
        records emitted.

Step 6: Two weeks later, an operator installs one of the imported
        apps locally. The first local install runs full Phase A →
        Phase B → S5.3 → Phase C exactly as if the profile had not
        been imported. Phase B's proposer reads the imported profile
        as advisory starting metadata (better default for the proposed
        manifest) but constructs the proposal from Phase A's
        ObservedBehavior summary as primary evidence.

        After install, the operator contributes a SINGLE_OPERATOR_OBSERVED
        rating (with attached Phase C evidence summary hash) at
        weight 0.50. After three such corroborated local contributions,
        the headline_evidence_level uplifts from SELF_REPORTED to
        MULTI_OPERATOR_CORROBORATED — local AIOS evidence is now the
        primary basis; ProtonDB attribution remains visible but no
        longer dominates the aggregate.
```

## 13. Acceptance criteria

- [ ] `CompatibilityRating` is a closed enum with five values: `PLATINUM`, `GOLD`, `SILVER`, `BRONZE`, `BORKED`.
- [ ] `RatingDimension` is a closed enum with eight values: `LAUNCH_RELIABILITY`, `GAMEPLAY_STABILITY`, `VISUAL_QUALITY`, `AUDIO_FUNCTIONALITY`, `INPUT_HANDLING`, `NETWORK_BEHAVIOR`, `SAVE_STATE_CORRECTNESS`, `DRM_BEHAVIOR`.
- [ ] `EvidenceLevel` is a closed enum with four values: `SELF_REPORTED`, `SINGLE_OPERATOR_OBSERVED`, `MULTI_OPERATOR_CORROBORATED`, `VERIFIED_PUBLISHER`.
- [ ] `ProfileVisibility` is a closed enum with three values: `PUBLIC`, `GROUP_INTERNAL`, `PERSONAL_ONLY`. Default is `PERSONAL_ONLY`.
- [ ] `ProfileRetiredReason` is a closed enum with six values per §3.5; `KnownIssueClass` is a closed enum with eleven values plus the `UNSPECIFIED` sentinel rejected at ingest.
- [ ] `CompatibilityProfile` is keyed by `profile:<app_id>:<ecosystem_runtime_lower>`; `EcosystemRuntime` consumed from S12.1 §3.1 (12-value enum).
- [ ] `headline_rating` is the worst applicable dimension's rating (never an arithmetic mean); `headline_evidence_level` is the lowest evidence level among the dimensions that produced the headline.
- [ ] Per-operator weight is in `[0.0, 1.0]`, defaults to `0.5` at first contribution, evolves per §5.1, and is **derivable from the signed evidence chain** (auditor can recompute from the log alone).
- [ ] Aggregation uses weighted-bucket counts per §5.2; ties break in favour of the lower (worse) bucket; insufficient weight (`< min_evidence_for_aggregation`, default `0.5`) reports the dimension at `evidence_level = SELF_REPORTED`.
- [ ] Outlier detection per §5.4 flags contributions whose distance from weighted median ≥ `outlier_distance_threshold` (default 3 buckets) AND whose evidence level ≤ `SINGLE_OPERATOR_OBSERVED` AND uncorroborated; flagging is symmetric in both directions (high and low outliers equally).
- [ ] Default visibility is `PERSONAL_ONLY`; visibility upgrade requires S5.3 EXACT_ACTION approval and is operator-comprehensible at the prompt.
- [ ] Anonymous ephemeral keys per S12.1 §6.2 are admitted; the registry verifies ephemeral signatures without learning the operator's stable id.
- [ ] Group-internal contributions never reach the public registry; INV-011 (cross-group access forbidden) is enforced at the namespace and policy layers.
- [ ] Imports from ProtonDB / WineHQ AppDB / Flathub / Snapcraft are one-shot translation actions, not federation; bridge contributor weight is fixed at `0.30`; imported dimensions default to `evidence_level = SELF_REPORTED`; no upstream change feed is subscribed.
- [ ] **Imported profiles inherit upstream reputation but ALL recommendations route through local Phase A pre-flight (S12.1) — import is metadata-only.** First install on the local host runs full Phase A → Phase B → S5.3 → Phase C regardless of imported headline rating.
- [ ] `upstream_attribution` is preserved on every imported contribution; the source, ref, imported_at, and artifact hash are recorded.
- [ ] AI subjects cannot transition `compat.contribute_profile_observation`, `compat.import_profile_from_upstream`, or `compat.review_outlier_contribution` to `executing` (cite INV-002, S0.1 envelope FSM).
- [ ] Profile poisoning by a single malicious operator is mitigated by the outlier detector (§9.1); the contributor's weight degrades per §5.1; flagged contributions remain in the public log but stop counting.
- [ ] Fake reputation farms are mitigated by signature-chain age and weight provenance (§9.2); coordinated bursts emit `PROFILE_REPUTATION_FARM_SUSPECTED` (FOREVER) and hold weights at `0.0` pending AIOS-root review.
- [ ] Coordinated suppression of breakage reports cannot lift the `headline_evidence_level` above `SELF_REPORTED` without genuine `MULTI_OPERATOR_CORROBORATED` Phase C-attached evidence (§9.3).
- [ ] `EcosystemHonestyClass` is a property of the recipe (S12.1 §3.2); a profile cannot uplift a recipe's honesty class. Honesty violations retire the profile with `RETIRED_DUE_TO_HONESTY_VIOLATION` (§9.5).
- [ ] Visibility downgrades emit `PROFILE_VISIBILITY_DOWNGRADED` (EXTENDED_60M); the contributor's local copy retains chosen visibility (§9.6).
- [ ] Per-operator weight is derivable from the signed evidence chain; weight ledger forgery is detectable at audit (§9.7).
- [ ] Bridge poisoning is mitigated by fixed bridge weight `0.30`, source-name attribution, and import artifact hash recording (§9.8).
- [ ] AI-initiated contributions are blocked at the S0.1 envelope FSM; AI subjects can construct envelopes but only operators approve (§9.9).
- [ ] Telemetry conforms to §10 cardinality bounds; profile/app/contributor/contribution/observation/recipe ids never appear as labels.
- [ ] The eight evidence record types in §11 are queued for S3.1 Wave 8 consolidation: `PROFILE_CONTRIBUTED` (`STANDARD_24M`), `PROFILE_RATING_AGGREGATED` (`STANDARD_24M`), `PROFILE_OUTLIER_DETECTED` (`EXTENDED_60M`), `PROFILE_RECOMMENDATION_SHOWN` (`STANDARD_24M`), `PROFILE_IMPORTED_FROM_UPSTREAM` (`STANDARD_24M`), `PROFILE_REPUTATION_FARM_SUSPECTED` (`FOREVER`), `PROFILE_VISIBILITY_DOWNGRADED` (`EXTENDED_60M`), `PROFILE_RETIRED` (`EXTENDED_60M`).
- [ ] The three typed actions (§8) are queued for S10.1 Wave 8 consolidation; all three dispatch as `ISOLATED_SANDBOX`.
- [ ] Cite INV-002 (AI proposes never executes) on every action that an AI subject might attempt; cite INV-017 (sandbox floor constitutional) on the dispatch discipline; both invariants are verifiable from the L0 catalog.

## 13.1 Constitutional notes

This contract sits at the intersection of three constitutional commitments that AIOS makes to the operator on the **knowledge** axis. Each commitment is enforced by a different layer; this contract is the place where the three meet for compatibility knowledge specifically.

**Commitment 1 — bounded AI agency (INV-002).** The AI proposes; the operator approves; the runtime executes. There is no path in this contract that lets an AI subject contribute, import, or review a profile without explicit operator approval. Phase B / Phase D proposers (S12.1) may **read** the profile database to inform proposals; they may not write to it. The S0.1 envelope FSM rejects AI-initiated contributions at the FSM level; the rejection is mechanical, not advisory.

**Commitment 2 — default-deny everywhere (INV-008, INV-017).** Every default in this contract is restrictive. The default visibility of a contribution is `PERSONAL_ONLY`, not `PUBLIC`. The default evidence level on an imported dimension is `SELF_REPORTED`, not the upstream's claimed level. The default per-operator weight is `0.5`, not `1.0`; gains require corroborated evidence; losses are immediate on outlier detection. The default contribution count required to reach `MULTI_OPERATOR_CORROBORATED` is `≥ 3` independent operators, not the contributor's own self-corroboration. The runtime safety floor of S3.2 still wins over every layer of this contract — a profile cannot loosen a sandbox; it can only inform a Phase B proposal that the operator then approves.

**Commitment 3 — no proof, no completion (INV-014).** Every aggregation step emits a receipt; every outlier flag emits a receipt; every farm suspicion emits a FOREVER receipt; every retirement emits a receipt. A claimed compatibility rating without a verifiable evidence chain back to signed contributions is a forgery and detectable. A claimed `MULTI_OPERATOR_CORROBORATED` evidence level without `≥ min_corroborators` distinct contributors with `SINGLE_OPERATOR_OBSERVED`-grade evidence is a forgery and rejected at aggregation. The eight new record types in §11 give operators, auditors, and AIOS-root governance the visibility to reconstruct any profile's full lineage from the log alone.

## See also

- [S12.1 — App Runtime Model + Cross-Ecosystem Compatibility](./01_app_runtime_model.md) — `EcosystemRuntime`, `EcosystemHonestyClass`, `RecipeTrustClass`, Phase A/B/C/D pipeline, `AppRecipe` shape, anonymous ephemeral keys (§6.2).
- [S12.3 — Compatibility Runtime](./03_compatibility_runtime.md) — orchestration of runtime selection; concurrent contract.
- [S11.1 — Repository Model](../L10_Distribution_Ecosystem_Marketplace/01_repository_model.md) — `AIOS_COMMUNITY_REPO`, publisher catalog, `publisher_root_id`, `PublisherTrustLevel`.
- [S3.1 — Evidence Log Architecture](../L0_Governance_Evidence_Safety/03_evidence_receipt_schema.md) — receipt envelope, retention classes, `RecordType` vocabulary; this contract queues eight new record types for Wave 8.
- [L0 Constitutional Invariants](../L0_Governance_Evidence_Safety/04_invariants.md) — INV-002, INV-008, INV-014, INV-015, INV-017.
- [Rev.1 §17 — Application, Package, and Compatibility Model](../../001.AI-OS.NET--SPECREV.1/02_SPECIFICATION.md)
- [Rev.2 Master Index](../00_MASTER_INDEX.md)
