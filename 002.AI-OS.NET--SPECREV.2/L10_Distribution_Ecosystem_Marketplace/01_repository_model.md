# Repository Model + Trust Roots (Rev.2)

| Field          | Value                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| -------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Status         | `REAL` (initial; written 2026-05-09; E1 evidence — file exists, structural contract complete)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| Phase tag      | S11.1                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| Layer          | L10 Distribution, Ecosystem, Marketplace                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| Schema package | `aios.distribution.v1alpha1`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| Consumes       | L0 INV-002 (AI proposes never executes), INV-008 (default-deny policy), INV-013 (AI cannot perform system admin), INV-014 (no proof no completion), INV-017 (sandbox floor constitutional); S5.2 Vault Broker (`KEY_SIGN` / `KEY_VERIFY`); S5.3 Approval Mechanics (`request_approval` outcome, `EXACT_ACTION` binding); S3.2 Sandbox Composition (`SandboxProfile` shape); S9.1 Recovery Boundary (`RecoveryMutableScope`, `RecoveryMode = RECOVERY`); S8.1 Network Policy (`NetworkOutboundManifest`); S3.1 Evidence Log (`RecordType` vocabulary, `FOREVER` retention class); S1.1 Capability Catalog (declared-capability resolution); S4.1 Namespace Layout (`installable_scope`) |
| Produces       | typed `PublisherTrustLevel` / `RepositoryKind` / `UpdateChannel` / `PackageKind` / `InstallScope` / `PackageInstallState` / `PackageVerificationResult` / `MirrorSemantic` / `TakedownReason` enums; the `PackageManifest` contract; the closed install pipeline FSM; the publisher key rotation discipline; the deplatform discipline; the capability-lie audit; nineteen evidence record types queued for S3.1; one L0 invariant candidate (`PACKAGE_TRUST_CHAIN_BOUND`) queued narrative-only                                                                                                                                                                                       |

## §1 Purpose

L10 distribution is the place where AIOS apps, agents, themes, kernel candidates, and L0 invariant bundles **physically arrive on a host**: signed, content-addressed, capability-declared, reputation-tracked. Until this contract, every other layer assumed packages "appear" with the right manifest, the right signature, the right capability declaration, and the right sandbox profile — but no contract said where that assumption is enforced, by whom, against which adversaries, with which evidence.

This sub-spec closes that loop. It is the **trust root** of AIOS: every binary running on the host can be traced back through at most three Ed25519 signatures to the AIOS root key, and the root key itself is firmware-pinned at first boot. There is no other admission path. A package that cannot prove its chain to the AIOS root is rejected; a publisher that loses its key is rotated under recovery mode; a publisher that turns malicious is deplatformed with FOREVER evidence.

Five constitutional risks define the threat model, each addressed by a named mechanism in this contract:

1. **Supply-chain compromise** — malicious package upstream. Addressed by signed manifests, content hashing, and per-publisher trust grading.
2. **Publisher impersonation** — forged signatures. Addressed by the three-tier signing chain anchored at the AIOS root.
3. **Mirror tampering** — legitimate publisher, tampered transport. Addressed by content-hash verification at the host before unpacking; mirrors **never re-sign**.
4. **Capability lie** — manifest claims fewer capabilities than the package actually exercises at runtime. Addressed by a 60-second first-run capability audit that compares declared vs observed and quarantines on drift.
5. **Takedown evasion** — a deplatformed publisher reappearing under a new identity. Addressed by the publisher-root-id pin in the AIOS-root-signed publisher catalog and FOREVER evidence on every interaction with a `DEPLATFORMED` publisher.

This spec is the contract surface that every later L10 sub-spec (`02_marketplace.md`, `03_external_integrations.md`) builds on. Marketplace UX (publisher onboarding, listing review, ratings) and external-integration deep-spec (Flathub mirror semantics, OCI re-packaging pipeline, distro repo bridges) remain `SHELL` and are referenced abstractly only.

## §2 Scope

This spec **defines**:

1. The three-tier trust root chain (AIOS root → publisher root → package signing key) with chain-depth bound `≤ 3`.
2. The closed `PublisherTrustLevel` enum with five tiers (`AIOS_ROOT`, `VERIFIED`, `COMMUNITY`, `DEPRECATED`, `DEPLATFORMED`).
3. The closed `RepositoryKind` enum with five repository classes.
4. The closed `UpdateChannel` enum with four channels.
5. The closed `PackageKind` enum with nine kinds (covering apps, agents, themes, invariant bundles, policy bundles, identity bundles, kernel candidates, adapters, capability catalog deltas).
6. The closed `InstallScope` enum and its mapping to S4.1 `installable_scope`.
7. The closed `PackageInstallState` FSM with ten states.
8. The closed `PackageVerificationResult` enum with ten outcomes.
9. The closed `MirrorSemantic` enum with three semantics; the contract that **mirrors never re-sign**.
10. The closed `TakedownReason` enum with seven reasons.
11. The `PackageManifest` proto contract: every field, validation rule, failure mode.
12. The seventeen-step install pipeline FSM (fail-closed, strictly ordered).
13. The recovery-only package classes: `INVARIANT_BUNDLE`, `POLICY_BUNDLE`, `IDENTITY_BUNDLE`, `KERNEL_CANDIDATE`, `CAPABILITY_CATALOG_DELTA`.
14. The "AI subjects cannot install" rule (the package-distribution analog of INV-002).
15. The first-run capability lie audit: 60-second runtime observation, declared vs observed, quarantine on drift, FOREVER `CAPABILITY_LIE_DETECTED` evidence.
16. The mirror tampering detection contract: content-hash check at the host, mirror auto-blacklist on repeated mismatches.
17. The publisher root key rotation flow: old root signs new root, AIOS root co-signs, recovery-mode operation, FOREVER evidence.
18. The deplatform / takedown discipline: AIOS-root-cosigned takedown, 30-day grace period, auto-quarantine on next health check, FOREVER evidence.
19. The binding to S8.1 `NetworkOutboundManifest`: every package's network manifest is part of the signed package manifest; modifications require re-issue + re-approval.
20. The external-bridge (Flathub / OCI / distro) discipline: bridges are never admitted to `AIOS_ROOT` or `VERIFIED` trust; the bridge re-packages under an AIOS bridge-signing key with audit metadata.
21. Adversarial robustness: fake AIOS root key, replay of older signed packages, downgrade attacks, spoofed `publisher_root_id`, concurrent installs, timing-channel mirror, publisher-key compromise.
22. Bounded-cardinality telemetry contract.
23. Nineteen evidence record types queued for S3.1.
24. Three worked examples (happy-path VERIFIED install, AI-requested install routed to operator, publisher key compromise → deplatform → quarantine).

This spec **does not** define:

- Marketplace UX, publisher onboarding workflow, listing review process, user ratings, search and discovery (`02_marketplace.md` — `SHELL`).
- Wire format of the Flathub mirror, OCI registry re-packaging pipeline, or distro repo bridges (`03_external_integrations.md` — `SHELL`).
- Hardware Security Module (HSM) integration for AIOS root or publisher roots — deferred.
- Threshold or multi-party signing schemes for the AIOS root — deferred (single-host-single-root for now).
- Distributed package mirrors with consensus-bound integrity — deferred.
- Cross-host package state federation (one host's `QUARANTINED` does not automatically propagate to peer hosts) — deferred.
- The dedicated kernel A/B promotion pipeline (S9.3) beyond noting that `KERNEL_CANDIDATE` installs feed into it.
- The L5 capability catalog mutation flow beyond noting that `CAPABILITY_CATALOG_DELTA` packages are routed to it.

This spec is the **contract surface** that the marketplace (`02`) and external bridges (`03`) consume; their sub-specs add UX and bridge-specific mechanics on top of, never around, the trust roots and install pipeline defined here.

## §3 Vocabulary (closed enums)

All vocabularies in this section are closed. Adding a value is a versioned spec change. Bundle compilers, manifest validators, and the install pipeline MUST reject values outside the enum at parse time. None of these enums admits an `OPEN` or `OTHER` value; the intent is to make distribution semantics fully mechanical.

### §3.1 `PublisherTrustLevel`

Closed enum, five tiers. Higher tiers grant broader default capability budgets and broader package kinds; lower tiers are confined by the sandbox floor and tighter quotas.

| Value          | Semantics                                                                                                                                                     | Default capability budget                              | Allowed package kinds                                                                                                                              |
| -------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------- |
| `AIOS_ROOT`    | The AIOS organisation itself. The constitutional issuer of root-bound packages — invariant bundles, identity bundles, kernel candidates, capability catalogs. | Full (subject only to constitutional invariants)       | All nine kinds, including `INVARIANT_BUNDLE`, `IDENTITY_BUNDLE`, `KERNEL_CANDIDATE`, `CAPABILITY_CATALOG_DELTA`.                                   |
| `VERIFIED`     | Publisher passed onboarding review by AIOS root. Default trust for ecosystem-grade publishers (e.g. major OS-image vendors, well-known agent providers).      | Broad (apps, agents, adapters, themes; bounded VRAM).  | `APP`, `AGENT`, `THEME`, `ADAPTER`, `POLICY_BUNDLE` (with policy-authorship grant), `CAPABILITY_CATALOG_DELTA` (with translator-authorship grant). |
| `COMMUNITY`    | Lightweight self-attestation; no AIOS-root onboarding review. Sandbox floor is **strictly** enforced; small VRAM, network, secret budgets.                    | Tight (CPU bound, no GPU compute, network allowlisted) | `APP`, `AGENT`, `THEME`, `ADAPTER` only.                                                                                                           |
| `DEPRECATED`   | Publisher being phased out. No new packages admitted from this publisher. Existing installs continue running but are not auto-updatable on `STABLE` channel.  | Frozen at last-grant levels                            | None (no new installs); existing installs preserve last-active kind.                                                                               |
| `DEPLATFORMED` | Publisher explicitly removed by AIOS root (cosigned takedown event). Existing installs auto-quarantine on next health check; FOREVER evidence on all touches. | None                                                   | None.                                                                                                                                              |

Trust level is recorded in the AIOS-root-signed publisher catalog (`pubcat_<hex>`). A package's manifest may not lie about its publisher's trust level; the install pipeline cross-checks against the catalog. Mismatch → `MANIFEST_FORGED` and FOREVER `MANIFEST_FORGED` evidence.

### §3.2 `RepositoryKind`

Closed enum, five repository classes. Every package fetched by the host has exactly one originating repository kind; cross-kind admission is forbidden.

| Value                 | Semantics                                                                                                           | Trust level admitted | Recovery-only                                                                                                            |
| --------------------- | ------------------------------------------------------------------------------------------------------------------- | -------------------- | ------------------------------------------------------------------------------------------------------------------------ |
| `AIOS_ROOT_REPO`      | Canonical AIOS-root-published packages. The constitutional source for invariant, identity, and kernel artefacts.    | `AIOS_ROOT` only     | Conditional (recovery-only for `INVARIANT_BUNDLE` / `IDENTITY_BUNDLE` / `KERNEL_CANDIDATE` / `CAPABILITY_CATALOG_DELTA`) |
| `AIOS_VERIFIED_REPO`  | Publishers at `VERIFIED` trust. Default ecosystem repository.                                                       | `VERIFIED` only      | No                                                                                                                       |
| `AIOS_COMMUNITY_REPO` | Publishers at `COMMUNITY` trust. Tight defaults; sandbox floor enforced.                                            | `COMMUNITY` only     | No                                                                                                                       |
| `AIOS_RECOVERY_REPO`  | Recovery-critical packages (invariant bundles, policy bundles, identity bundles, kernel candidates).                | `AIOS_ROOT` only     | Yes (every install requires `RecoveryMode = RECOVERY` per S9.1 §3.2)                                                     |
| `EXTERNAL_BRIDGE`     | Bridges to Flathub, OCI registries, distro repos. Sandboxed and rate-limited. **Never admitted above `COMMUNITY`.** | `COMMUNITY` only     | No                                                                                                                       |

A package's `RepositoryKind` is derived deterministically from its source URL prefix recorded by the fetch step (§5). Cross-kind admission attempts (e.g. a `KERNEL_CANDIDATE` arriving from `AIOS_VERIFIED_REPO`) are rejected with `RepositoryKindMismatch` and emit FOREVER `PACKAGE_VERIFICATION_FAILED` evidence.

### §3.3 `UpdateChannel`

Closed enum, four channels. Every package version declares one channel; the host's per-package channel preference is set by the operator at install time and may be widened only with explicit approval.

| Value                  | Semantics                                                                                               | Default for     |
| ---------------------- | ------------------------------------------------------------------------------------------------------- | --------------- |
| `STABLE`               | Default. Full review. Auto-update permitted within publisher's update window.                           | All packages.   |
| `BETA`                 | Publisher-marked beta. Requires explicit operator opt-in **per package**. Never auto-set.               | None.           |
| `RECOVERY_CRITICAL`    | Only valid for `AIOS_RECOVERY_REPO`. Updates require recovery-mode approval. Auto-update is forbidden.  | Recovery repo.  |
| `DEPRECATED_RETENTION` | No new versions. Existing installs continue. Auto-quarantine triggers on the package's `eol_at` if set. | Phase-out flag. |

A package marked `BETA` may not transition to `STABLE` without re-issuance (new `manifest_canonical_hash`, fresh signature, fresh approval). A package marked `RECOVERY_CRITICAL` may not be installed outside recovery (S9.1 §3.6 `RecoveryMutableScope`).

### §3.4 `PackageKind`

Closed enum, nine kinds. Each kind declares the schema fields the manifest must populate, the install scope it may target, and whether it is recovery-only.

| Value                      | Semantics                                                                                       | Recovery-only | Sandbox profile required | Capability declaration required |
| -------------------------- | ----------------------------------------------------------------------------------------------- | ------------- | ------------------------ | ------------------------------- |
| `APP`                      | User-facing application.                                                                        | No            | Yes                      | Yes                             |
| `AGENT`                    | AI agent persona; auto-binds to AI subject scope at install time.                               | No            | Yes (AI floor; S3.2)     | Yes                             |
| `THEME`                    | Visual theme bundle per L7.X. **Cannot include code or extension binaries.**                    | No            | N/A (declarative only)   | No                              |
| `INVARIANT_BUNDLE`         | L0 invariant signed bundle. Only `AIOS_ROOT_REPO`.                                              | Yes           | N/A                      | N/A                             |
| `POLICY_BUNDLE`            | S2.3 policy bundle. `AIOS_ROOT` or `VERIFIED` with explicit policy-authorship grant.            | Conditional   | N/A                      | N/A                             |
| `IDENTITY_BUNDLE`          | L4.3 identity bundle. Only `AIOS_ROOT_REPO`.                                                    | Yes           | N/A                      | N/A                             |
| `KERNEL_CANDIDATE`         | Dedicated kernel image per L1.3 (S9.3 deferred). Recovery-only install with A/B promotion.      | Yes           | N/A                      | N/A                             |
| `ADAPTER`                  | L3 adapter binary. Signed manifest mandatory; capability declaration mandatory.                 | No            | Yes                      | Yes                             |
| `CAPABILITY_CATALOG_DELTA` | L5/S1.1 capability catalog updates. `AIOS_ROOT` or `VERIFIED` with translator-authorship grant. | Yes           | N/A                      | N/A                             |

A `THEME` package whose archive contains executable bits, `.so`/`.dll`/`.dylib`, or any file whose magic bytes match a known executable format is rejected at content validation with `BUNDLE_TAMPERED` and emits `PACKAGE_VERIFICATION_FAILED` (extended-60M).

### §3.5 `InstallScope`

Closed enum, four scopes. Maps to S4.1 namespace layout.

| Value          | Semantics                                                                                         | Approver               |
| -------------- | ------------------------------------------------------------------------------------------------- | ---------------------- |
| `SYSTEM_ONLY`  | Writes to `/aios/system/...`. Recovery-only install (binds INV-012, S9.1 `RecoveryMutableScope`). | Recovery operator      |
| `GROUP_SCOPED` | Writes to `/aios/groups/<group_id>/system/...`. Group-operator approval.                          | Group operator         |
| `USER_SCOPED`  | Writes to `/aios/groups/<group_id>/users/<user_id>/...`. User approval.                           | User                   |
| `EITHER`       | Auto-determined by `manifest.installable_scope`; resolved at install time against S4.1 namespace. | Resolved scope's owner |

A `USER_SCOPED` install of a manifest whose `installable_scope` is `SYSTEM_ONLY` is rejected with `InstallScopeViolation` at the manifest-validation step (§5 step 6).

### §3.6 `PackageInstallState`

Closed FSM, ten states. The install pipeline (§5) walks this FSM strictly forward; back-transitions are forbidden except `INSTALLING → INSTALL_FAILED` (atomic-install rollback).

| Value               | Semantics                                                                                                                |
| ------------------- | ------------------------------------------------------------------------------------------------------------------------ |
| `DRAFT`             | Operator initiated install; not yet validated. Created when the L7 marketplace surface emits an install action envelope. |
| `VALIDATING`        | Signature, manifest, capability check in progress.                                                                       |
| `AWAITING_APPROVAL` | Policy returned `request_approval` (S5.3); approval prompt delivered via `EXACT_ACTION` binding.                         |
| `APPROVED`          | Approval granted; binding consumed; ready to install.                                                                    |
| `INSTALLING`        | Atomic install in progress (writing files, running install hooks).                                                       |
| `ACTIVE`            | Live; subject to runtime monitoring (capability-lie audit window, ongoing health checks).                                |
| `QUARANTINED`       | Manifest violation, signature failure, deplatform event, capability-lie detected, or runtime breach.                     |
| `UNINSTALLING`      | Active uninstall in progress; capability bindings revoked; files removed.                                                |
| `REMOVED`           | Terminal: package fully uninstalled.                                                                                     |
| `INSTALL_FAILED`    | Terminal: install aborted before `ACTIVE`. Reason is recorded from `PackageVerificationResult` or pipeline-step failure. |

Allowed forward transitions:

```text
DRAFT ─▶ VALIDATING ─┬─▶ INSTALL_FAILED                (signature / chain / manifest fail)
                     ├─▶ AWAITING_APPROVAL ─┬─▶ APPROVED ─▶ INSTALLING ─┬─▶ ACTIVE
                     │                      │                            └─▶ INSTALL_FAILED
                     │                      └─▶ INSTALL_FAILED            (atomic rollback)
                     │                         (approval denied/expired)
                     └─▶ INSTALL_FAILED               (manifest / capability / network fail)

ACTIVE ─┬─▶ QUARANTINED                                (lie / breach / takedown)
        ├─▶ UNINSTALLING ─▶ REMOVED
        └─▶ stays ACTIVE
```

Terminal states: `ACTIVE` (until uninstalled), `QUARANTINED` (until operator review), `REMOVED`, `INSTALL_FAILED`.

### §3.7 `PackageVerificationResult`

Closed enum, ten outcomes. Every step in the install pipeline (§5) that can fail returns one of these. All failures emit FOREVER or extended-60M evidence per the table in §13.

| Value                    | Trigger                                                                                           |
| ------------------------ | ------------------------------------------------------------------------------------------------- |
| `VERIFIED_AIOS_ROOT`     | Chain ends at AIOS root; trust level inherited from publisher catalog.                            |
| `VERIFIED_PUBLISHER`     | Chain valid; publisher signature good; publisher in `VERIFIED` or `COMMUNITY` trust.              |
| `SIGNATURE_FAILED`       | Ed25519 verify failed at any chain hop.                                                           |
| `TRUST_CHAIN_BROKEN`     | Publisher root not signed by AIOS root, or revoked, or absent from publisher catalog.             |
| `TRUST_CHAIN_TOO_DEEP`   | More than three signature hops from AIOS root to package signing key.                             |
| `PUBLISHER_DEPLATFORMED` | Publisher in `DEPLATFORMED` state at fetch time.                                                  |
| `HASH_MISMATCH`          | `BLAKE3(content)` differs from `manifest.content_hash`.                                           |
| `MANIFEST_FORGED`        | Manifest fields tampered post-sign (e.g. trust level claim inconsistent with publisher catalog).  |
| `CAPABILITY_LIE`         | Declared capabilities differ from runtime-observed capabilities at first-run audit (§7).          |
| `BUNDLE_TAMPERED`        | Any other content tamper (executable bits in a `THEME`, archive corruption, hook escape attempt). |

### §3.8 `MirrorSemantic`

Closed enum, three semantics. **Mirrors NEVER re-sign packages.** They serve the same signed bytes verbatim or fail. Mirror tampering is detected by the host-side content-hash check before unpacking (§5 step 5).

| Value    | Semantics                                                                                                                        |
| -------- | -------------------------------------------------------------------------------------------------------------------------------- |
| `ORIGIN` | The publisher's authoritative server. The canonical fetch target.                                                                |
| `CACHED` | Third-party mirror; serves the same signed bytes; **cannot** re-sign or modify. Tampering detected by host-side hash check.      |
| `LOCAL`  | Operator's own offline mirror (e.g. for airgap installs). Same content-hash discipline; the operator self-attests bytes-on-disk. |

A mirror-served package whose content hash differs from the manifest is rejected with `HASH_MISMATCH`. Repeated mismatches from the same mirror within a 24-hour window auto-blacklist the mirror with FOREVER `MIRROR_HASH_MISMATCH_BLACKLISTED` evidence; subsequent fetches from that mirror are pre-rejected at the fetch step.

### §3.9 `TakedownReason`

Closed enum, seven reasons. Every `PUBLISHER_DEPLATFORMED` event records exactly one reason in its FOREVER evidence record.

| Value                          | Semantics                                                                                                           |
| ------------------------------ | ------------------------------------------------------------------------------------------------------------------- |
| `MALICIOUS_BEHAVIOR_DETECTED`  | Runtime breach, capability abuse, or operator-flagged malicious behaviour confirmed by AIOS-root review.            |
| `SUPPLY_CHAIN_COMPROMISE`      | The publisher's build pipeline or signing infrastructure was compromised; packages from a window are untrustworthy. |
| `CAPABILITY_LIE_DETECTED`      | First-run capability audit detected sustained drift between declared and observed capabilities across packages.     |
| `LEGAL_REQUIREMENT`            | Mandatory takedown under operator-jurisdiction legal order (e.g. court order, sanctions list).                      |
| `PUBLISHER_REQUEST`            | Publisher voluntarily withdrew (e.g. organisational dissolution, key retirement).                                   |
| `KEY_COMPROMISE`               | Publisher root key (or one of its package signing keys) confirmed compromised; immediate rotation forced.           |
| `ABANDONED_AFTER_INACTIVE_TTL` | Publisher inactive beyond a configurable inactivity TTL (default 24 months) with no rotation or update.             |

## §4 The trust root chain

The trust root is a three-tier signing chain anchored at the AIOS root key. Every package's signature must chain back to the AIOS root through **at most three hops**. Chains longer than three are rejected with `TRUST_CHAIN_TOO_DEEP` and emit FOREVER `TRUST_CHAIN_TOO_DEEP` evidence.

### §4.1 Tier 1 — AIOS root key

- **Generation.** Ed25519 keypair generated at first boot under `_system` scope. Generated once per host installation; the same root spans the lifetime of the AIOS install.
- **Storage.** Private material in vault under `vault://aios/system/root_signing` with `VaultMaterialKind = ED25519_PRIVATE_KEY` and capability class `KEY_SIGN` (S5.2 §3, §4). Vault enforces `ON_REVEAL_ONLY`: the key is never returned to a requester; only the broker performs `SignBlob` operations against it.
- **Public key distribution.** The public key is embedded in a firmware-or-installer-signed bundle delivered with the AIOS installation media. Verification at boot is firmware-pinned: a bootloader stage (S9.2 — installer flow, deferred) loads the public key from a known offset in the signed firmware/installer artefact and stores its hash in the kernel command line. Mismatch at boot drops the system into recovery with `INVARIANT_BUNDLE_SIGNATURE_FAILURE` per S9.1 §3.3.
- **Rotation.** Cannot be rotated except via a recovery-mode operation with co-signed evidence:
  1. Recovery-mode boot (S9.1 §3.2 `RecoveryMode = RECOVERY`).
  2. `HUMAN_USER` subject in `_system` scope.
  3. Two-human co-signer approval (`ApprovalStrength = DUAL` per S5.3 §3.3).
  4. New AIOS root keypair generated in vault.
  5. New public key signs a self-attestation linking it to the old key.
  6. Operator must **re-flash the firmware-pinned offset** (or re-install) for new boots to trust the new key.
  7. FOREVER `AIOS_ROOT_KEY_ROTATED` evidence record.

The firmware-pinned step is constitutional: a software-only AIOS root rotation cannot change the boot-time trust anchor. This bounds the worst-case compromise to "the next install".

### §4.2 Tier 2 — Publisher root keys

- **Generation.** Each onboarding publisher generates an Ed25519 keypair on their own infrastructure (or an AIOS-managed HSM if HSM integration ships in a later revision).
- **Identity.** `publisher_root_id` = `pub:<vendor>` where `<vendor>` is a URL-safe lowercase identifier registered in the AIOS-root-signed publisher catalog.
- **Signing.** The AIOS root signs the publisher root's public key + identity + initial trust level + onboarding evidence pointer. The signature is recorded in the publisher catalog (`pubcat_<hex_lower(BLAKE3(jcs(catalog)))[:32]>`).
- **Rotation.** Publisher root key rotation requires:
  1. Old publisher root signs the new publisher root (chain continuity).
  2. AIOS root co-signs the rotation event (the **only** auth that can break a publisher chain).
  3. Recovery-mode operation on every host that wants to honour the new root (or hot-load via `CAPABILITY_CATALOG_DELTA`-style publisher catalog delta in normal mode if AIOS root signs; recovery is mandatory if the rotation is reactive to a `KEY_COMPROMISE`).
  4. All package signing keys signed by the old root continue to verify until their explicit revocation; new keys sign forward.
  5. FOREVER `PUBLISHER_KEY_ROTATED` evidence record.

### §4.3 Tier 3 — Package signing keys

- **Generation.** Each publisher generates one or more Ed25519 keypairs. Multiple keys per publisher are common (one per build pipeline, one per channel, etc.).
- **Identity.** `package_signing_key_id` = `pks:<vendor>:<role>` where `<role>` is publisher-managed.
- **Signing.** The publisher root signs the package signing key's public key + identity + key validity window.
- **Rotation.** Publisher-managed; recorded in the publisher's package-signing-key catalog. The publisher publishes a new package-signing-key catalog version; AIOS hosts pull the new catalog at the next package fetch.

### §4.4 Chain depth and verification

The chain has **exactly three signatures** in the canonical case:

```text
AIOS root        ─signs─▶  publisher root      (in publisher catalog)
publisher root   ─signs─▶  package signing key (in publisher's signing-key catalog)
package signing  ─signs─▶  PackageManifest     (manifest.ed25519_signature)
key
```

The verification step (§5 step 3) walks the chain from manifest to AIOS root:

1. Verify `PackageManifest.ed25519_signature` against the `package_signing_key`'s public key.
2. Verify `package_signing_key` against the `publisher_root`'s public key.
3. Verify `publisher_root` against the **firmware-pinned** AIOS root public key.
4. Reject if any step fails (`SIGNATURE_FAILED` if Ed25519 fails; `TRUST_CHAIN_BROKEN` if a key is revoked or absent from the catalogs).
5. Reject if more than three signatures are required — `TRUST_CHAIN_TOO_DEEP`.

Bypass attempts (e.g. a package whose signature was made directly by the AIOS root, skipping the publisher) are rejected with `TRUST_CHAIN_BROKEN`: the canonical shape is exactly three signatures, no more, no fewer. The AIOS root **does not sign packages directly** — it signs only publisher roots and publisher catalog versions.

### §4.5 Catalogs

Two AIOS-root-signed catalogs hold the chain state on every host:

- **Publisher catalog** `pubcat_<hex>` — list of `(publisher_root_id, public_key, trust_level, onboarding_evidence_pointer, activated_at, retired_at)` entries. AIOS-root-signed; refreshed via signed delta on each fetch.
- **Per-publisher signing-key catalog** `pksigcat_<vendor>_<hex>` — list of `(package_signing_key_id, public_key, validity_window, revoked_at)` entries. Publisher-root-signed.

A package whose `publisher_root_id` is absent from the active publisher catalog → `TRUST_CHAIN_BROKEN`. A package whose `package_signing_key_id` is absent from the active publisher signing-key catalog → `TRUST_CHAIN_BROKEN`. A package whose signing key is in the catalog but `revoked_at` predates the manifest's `issued_at` → `TRUST_CHAIN_BROKEN`.

## §5 The `PackageManifest` contract

Each package ships with a signed `PackageManifest`. The manifest is the only contract surface the host trusts; package contents are trusted only because the manifest binds their content hash.

```proto
syntax = "proto3";
package aios.distribution.v1alpha1;

import "google/protobuf/timestamp.proto";
import "aios/sandbox/v1alpha1/sandbox_profile.proto";   // S3.2 SandboxProfile
import "aios/network/v1alpha1/network_outbound.proto";  // S8.1 NetworkOutboundManifest

message PackageManifest {
  // Identity --------------------------------------------------------------
  string package_id = 1;                  // "pkg:<vendor>:<name>"
  string version = 2;                     // semver "X.Y.Z[-prerelease][+build]"
  PackageKind kind = 3;
  PublisherTrustLevel publisher_trust = 4;
  string publisher_root_id = 5;           // "pub:<vendor>"
  string package_signing_key_id = 6;      // "pks:<vendor>:<role>"

  // Content binding -------------------------------------------------------
  string content_hash = 7;                // hex_lower(BLAKE3(content))[:32]
  string manifest_canonical_hash = 8;     // hex_lower(BLAKE3(JCS(this without signature)))[:32]
  bytes ed25519_signature = 9;            // signed by package_signing_key over manifest_canonical_hash

  // Install scope ---------------------------------------------------------
  string installable_scope = 10;          // SYSTEM_ONLY | GROUP_ONLY | EITHER (cite S4.1)

  // Sandbox + capability declaration --------------------------------------
  aios.sandbox.v1alpha1.SandboxProfile required_sandbox = 11;     // typed S3.2 profile
  repeated string declared_capabilities = 12;                       // capability ids the package will request
  aios.network.v1alpha1.NetworkOutboundManifest network_manifest = 13;  // S8.1 §G

  // Lifecycle -------------------------------------------------------------
  google.protobuf.Timestamp issued_at = 14;
  google.protobuf.Timestamp eol_at = 15;  // optional EOL date; auto-quarantine on or after this
  UpdateChannel channel = 16;

  // Repository linkage (set at fetch time, not by publisher) --------------
  RepositoryKind originating_repository = 17;
  string mirror_url = 18;                 // recorded by fetcher for evidence
  MirrorSemantic mirror_semantic = 19;    // ORIGIN / CACHED / LOCAL
}
```

### §5.1 Field-by-field validation rules

| Field                     | Validation                                                                                                                                             | Failure mode                           |
| ------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------ | -------------------------------------- |
| `package_id`              | Regex `^pkg:[a-z0-9-]{1,64}:[a-z0-9-]{1,128}$`. `vendor` segment must equal the `vendor` segment of `publisher_root_id`.                               | `MANIFEST_FORGED`                      |
| `version`                 | Strict semver per <https://semver.org>; build metadata permitted; pre-release permitted.                                                               | `MANIFEST_FORGED`                      |
| `kind`                    | Closed enum value; must be one of the nine `PackageKind` values.                                                                                       | `MANIFEST_FORGED`                      |
| `publisher_trust`         | Closed enum value; must equal the trust level recorded for `publisher_root_id` in the active publisher catalog.                                        | `MANIFEST_FORGED`                      |
| `publisher_root_id`       | Regex `^pub:[a-z0-9-]{1,64}$`; must be present in the active publisher catalog with `retired_at` unset (or in the future).                             | `TRUST_CHAIN_BROKEN`                   |
| `package_signing_key_id`  | Regex `^pks:[a-z0-9-]{1,64}:[a-z0-9-]{1,64}$`; must be present in the publisher's signing-key catalog with `revoked_at` unset.                         | `TRUST_CHAIN_BROKEN`                   |
| `content_hash`            | 32-char lowercase hex (128 bits of BLAKE3); must match `BLAKE3(content)` truncated identically. Computed by host at fetch time.                        | `HASH_MISMATCH`                        |
| `manifest_canonical_hash` | 32-char lowercase hex; must equal `BLAKE3(JCS(manifest with `ed25519_signature` cleared))[:32]`. JCS = RFC 8785 JSON Canonicalisation Scheme.          | `MANIFEST_FORGED`                      |
| `ed25519_signature`       | 64-byte Ed25519 signature over the bytes of `manifest_canonical_hash` (lowercase hex string ASCII bytes).                                              | `SIGNATURE_FAILED`                     |
| `installable_scope`       | One of `SYSTEM_ONLY`, `GROUP_ONLY`, `EITHER`; cited from S4.1.                                                                                         | `MANIFEST_FORGED`                      |
| `required_sandbox`        | Valid `SandboxProfile` per S3.2 — host capabilities must be sufficient or composition fails.                                                           | `BUNDLE_TAMPERED` (validation)         |
| `declared_capabilities`   | Each entry must resolve in the L5/S1.1 capability catalog at the active catalog version. Empty is permitted only for `THEME`.                          | `BUNDLE_TAMPERED`                      |
| `network_manifest`        | Valid `NetworkOutboundManifest` per S8.1 §G — signed by L8 service signing key OR included in package manifest's signed envelope (per-package option). | `BUNDLE_TAMPERED`                      |
| `issued_at`               | Must precede current host time by ≤ `MAX_FUTURE_DRIFT` (default 5 min); must precede `eol_at` if set.                                                  | `MANIFEST_FORGED`                      |
| `eol_at`                  | Optional. If set and `<= now()`, package is auto-quarantined (`PACKAGE_QUARANTINED` FOREVER; reason = `MANIFEST_EOL`).                                 | (runtime quarantine, not install fail) |
| `channel`                 | Closed enum value; must match repository constraints (`RECOVERY_CRITICAL` only on `AIOS_RECOVERY_REPO`).                                               | `MANIFEST_FORGED`                      |
| `originating_repository`  | Closed enum value; cross-checked against fetch URL prefix.                                                                                             | `RepositoryKindMismatch`               |
| `mirror_url`              | Recorded by host; not signed by publisher; used for evidence and mirror-blacklist tracking.                                                            | (no validation)                        |
| `mirror_semantic`         | Closed enum value; `ORIGIN` only when fetch URL matches publisher's authoritative origin (recorded in publisher catalog).                              | `MANIFEST_FORGED`                      |

### §5.2 Manifest canonicalisation

The signing surface is the **canonical hash** `manifest_canonical_hash`, not the raw proto bytes. Canonicalisation:

1. Project the manifest into JSON via the deterministic proto3 → JSON projection (field names in proto field number order; no whitespace; numeric values rendered without exponent).
2. Apply RFC 8785 JCS to the JSON.
3. Hash with BLAKE3.
4. Truncate to 128 bits and lowercase-hex-encode.

The `ed25519_signature` field is signed over the **ASCII bytes** of the lowercase-hex `manifest_canonical_hash` string. This indirection is intentional: it keeps the signing surface a 64-byte payload and makes the signature easy to verify against either the proto or the JSON projection without re-canonicalising.

A manifest whose computed `manifest_canonical_hash` does not match the recorded value → `MANIFEST_FORGED` (the publisher tampered with their own manifest after signing, or the manifest was edited in transit). A manifest whose `ed25519_signature` does not verify against the recorded `manifest_canonical_hash` → `SIGNATURE_FAILED`.

## §6 The install pipeline (closed FSM)

Strictly ordered, fail-closed. Every step has a closed failure outcome from `PackageVerificationResult`. Any step failing returns the FSM to `INSTALL_FAILED` with the failure recorded; no step is "best-effort". The pipeline below is normative.

```text
┌─────────────────────────────────────────────────────────────────────────┐
│                           INSTALL PIPELINE                              │
├─────────────────────────────────────────────────────────────────────────┤
│  1. Fetch                          (LOCAL → CACHED → ORIGIN)            │
│  2. Signature verify               (Ed25519 over canonical manifest)    │
│  3. Trust chain verify             (chain depth ≤ 3; catalogs lookup)   │
│  4. Publisher state check          (DEPLATFORMED → reject; DEPRECATED   │
│                                     → no new install)                   │
│  5. Content hash verify            (BLAKE3(content) = content_hash)     │
│  6. Manifest field validation      (enum closures, regex, semver)       │
│  7. Sandbox profile validation     (S3.2 ComposeProfile feasibility)    │
│  8. Capability declaration         (each declared cap in L5 catalog)    │
│     vs catalog                                                          │
│  9. Network manifest validation    (S8.1 NetworkOutboundManifest)       │
│ 10. Policy decision                (S2.3 EvaluatePolicy)                │
│ 11. Approval                        (S5.3 EXACT_ACTION binding)         │
│ 12. Recovery-mode gate             (SYSTEM_ONLY → recovery required)    │
│ 13. Mark APPROVED → INSTALLING                                          │
│ 14. Atomic install                 (staging + pointer flip; rollback)   │
│ 15. Capability binding             (L4 capability bindings issued)      │
│ 16. Mark ACTIVE; emit              (PACKAGE_INSTALLED STANDARD_24M)     │
│     PACKAGE_INSTALLED                                                   │
│ 17. First-run capability lie       (60-second observation window;       │
│     audit                          QUARANTINE + FOREVER on drift)       │
└─────────────────────────────────────────────────────────────────────────┘
```

### §6.1 Step 1 — Fetch

- Mirror hierarchy: `LOCAL` (operator's own offline mirror) → `CACHED` (third-party mirror) → `ORIGIN` (publisher's authoritative server). The host attempts in this order; failure of one tier moves to the next.
- Each mirror attempt records `mirror_url` and `mirror_semantic` in the staging area's metadata.
- Bytes are written to a pre-install staging area at `/aios/system/distribution/staging/<package_canonical_hash>/`.
- Network access uses the L8 outbound discipline: the distribution service has a `NetworkOutboundManifest` allowlisting the configured publisher origins and configured CACHED mirrors; LOCAL is filesystem-only.
- Fetch failure modes: connection refused, TLS failure, HTTP 4xx/5xx, byte-budget exceeded (default 4 GiB per fetch), timeout (default 5 minutes). All retried with exponential backoff up to a per-fetch retry budget, then `INSTALL_FAILED` with reason `FETCH_EXHAUSTED`.
- Successful fetch emits `PACKAGE_FETCH_STARTED` STANDARD_24M evidence.

### §6.2 Step 2 — Signature verify

- Verify `manifest.ed25519_signature` against `package_signing_key`'s public key (looked up in the publisher's signing-key catalog).
- Computed over the ASCII bytes of the lowercase-hex `manifest_canonical_hash`.
- Failure → `SIGNATURE_FAILED`; FOREVER `PACKAGE_VERIFICATION_FAILED` evidence.

### §6.3 Step 3 — Trust chain verify

- Walk the chain (manifest → package signing key → publisher root → AIOS root).
- Reject if any step fails Ed25519, if any catalog lookup misses, if any key is revoked, or if more than three hops are required.
- Failure → `TRUST_CHAIN_BROKEN` or `TRUST_CHAIN_TOO_DEEP`; FOREVER evidence (record types `TRUST_CHAIN_BROKEN` / `TRUST_CHAIN_TOO_DEEP`).

### §6.4 Step 4 — Publisher state check

- Look up `publisher_root_id` in the active publisher catalog.
- If `trust_level = DEPLATFORMED` → `PUBLISHER_DEPLATFORMED`; FOREVER evidence; install fails.
- If `trust_level = DEPRECATED` → reject **new** installs; existing installs are unaffected (they continue running until uninstalled or their `eol_at`); FOREVER `PACKAGE_VERIFICATION_FAILED` (reason = `PUBLISHER_DEPRECATED`) evidence.
- If `trust_level ∈ {AIOS_ROOT, VERIFIED, COMMUNITY}` → continue.

### §6.5 Step 5 — Content hash verify

- Compute `BLAKE3(content)` over the staging-area bytes.
- Compare against `manifest.content_hash` (same truncation, same encoding).
- Mismatch → `HASH_MISMATCH`; FOREVER `MIRROR_HASH_MISMATCH_BLACKLISTED` evidence if the source was a mirror; mirror auto-blacklist counter incremented; install fails.
- This is the **mirror tampering detection point**. Mirrors never re-sign; they cannot pass this check on tampered content.

### §6.6 Step 6 — Manifest field validation

- Walk every field per the table in §5.1. Failures → `MANIFEST_FORGED` or `BUNDLE_TAMPERED`.
- Strictness:
  - Closed enums: any value outside the spec → reject.
  - Regex: any failure → reject.
  - Cross-field consistency (e.g. `publisher_trust` vs catalog) → reject.
  - Future-drift (`issued_at` more than 5 minutes ahead) → reject.

### §6.7 Step 7 — Sandbox profile validation

- Hand the manifest's `required_sandbox` plus the host's `HostCapabilitySnapshot` to the S3.2 `SandboxComposer.ValidateProfile`.
- If composition would fail (host lacks Landlock and the profile requires Landlock; AI floor cannot be satisfied; etc.) → `BUNDLE_TAMPERED` with reason `SANDBOX_INFEASIBLE`; install fails.
- If composition would succeed but require degraded fallbacks → policy decides at step 10 whether the degradation is approved.

### §6.8 Step 8 — Capability declaration vs catalog

- For each entry in `manifest.declared_capabilities`, look up the capability in the active L5/S1.1 capability catalog.
- Unknown capability → `BUNDLE_TAMPERED`; reason = `UNKNOWN_CAPABILITY`; install fails.
- Capability marked AI-forbidden but the package's kind is `AGENT` → `BUNDLE_TAMPERED`; reason = `AI_FORBIDDEN_CAPABILITY_DECLARED_ON_AGENT`.
- Capability requires `system_admin` flag but the manifest's `installable_scope` is not `SYSTEM_ONLY` → `BUNDLE_TAMPERED`; reason = `CAPABILITY_SCOPE_MISMATCH`.

### §6.9 Step 9 — Network manifest validation

- Validate `manifest.network_manifest` against S8.1 §G:
  - Each allowlist entry must use a known `AllowlistEntryKind`.
  - FQDN entries must pass syntactic validation; effective fan-out is checked at runtime, not install time.
  - Public-internet entries declared by an `AGENT` package → reject (AI subjects never receive `ALLOW_INTERNET`; cite S8.1 I4).
- Failure → `BUNDLE_TAMPERED`; reason = `NETWORK_MANIFEST_INVALID`.

### §6.10 Step 10 — Policy decision

- Build an action envelope `package.install` (per S0.1) with subject = the operator's session subject, target = the package canonical hash, context = the validated manifest.
- Submit to S2.3 Policy Kernel via `EvaluatePolicy`.
- Possible outcomes per S2.3 §15:
  - `ALLOW` → continue to step 11 with auto-binding (rare; only `AIOS_VERIFIED_REPO` low-risk apps within operator's preferences).
  - `REQUIRE_APPROVAL` → continue to step 11 with `request_approval` flow.
  - `DENY` → install fails with `INSTALL_FAILED`; FOREVER `PACKAGE_INSTALL_FAILED` (reason = `POLICY_DENIED`).
  - Hard-deny (e.g. `AISystemAdminBlocked` if an AI subject submitted) → install fails immediately; FOREVER evidence.

### §6.11 Step 11 — Approval

- Per S5.3 §3.5 `ApprovalBindingScope = EXACT_ACTION`.
- Binding bound to **the package canonical hash** (`hex_lower(BLAKE3(JCS(manifest with signature cleared)))[:32]`). Any change to manifest fields between approval and execute → binding voids per S5.3 `ApprovalDenialReason = ACTION_REVISED`.
- Approver must be `HUMAN_USER` per S5.3 §3.7 (cite §8 below — "AI subjects cannot install").
- TTL per `ApprovalTtlClass = TTL_SHORT` by default (5 minutes); `TTL_RECOVERY` (30 minutes) for `SYSTEM_ONLY`-scope installs in recovery.
- On `GRANTED` → consume binding (S5.3 §6 FSM `GRANTED → CONSUMED`); transition state `AWAITING_APPROVAL → APPROVED`.
- On `DENIED` / `EXPIRED` / `REVOKED` / `FAILED_DELIVERY` → `INSTALL_FAILED`; FOREVER `PACKAGE_INSTALL_FAILED` evidence with the denial reason.

### §6.12 Step 12 — Recovery-mode gate

- For `installable_scope = SYSTEM_ONLY` and for `kind ∈ {INVARIANT_BUNDLE, POLICY_BUNDLE, IDENTITY_BUNDLE, KERNEL_CANDIDATE, CAPABILITY_CATALOG_DELTA}` (see §7), the operator must be in recovery mode (S9.1 §3.2 `RecoveryMode = RECOVERY`).
- Outside recovery → reject with `RecoveryRequiredForPackageKind`; FOREVER `PACKAGE_INSTALL_FAILED` (reason = `RECOVERY_REQUIRED_FOR_PACKAGE_KIND`).
- This check binds INV-012 (recovery required for system mutation) and S9.1 `RecoveryMutableScope`.

### §6.13 Step 13 — Mark `APPROVED → INSTALLING`

- Atomic state transition.
- Capability bindings are **prepared** but not yet issued (issued at step 15).
- Emit no evidence at this step (covered by step 16).

### §6.14 Step 14 — Atomic install

- Write package contents to a content-addressed staging path under `/aios/system/distribution/installed-staging/<content_hash>/`.
- Run install hooks (if declared by the manifest; hooks are themselves sandboxed under the manifest's `required_sandbox`).
- On success: atomic pointer flip — the active install pointer for `package_id` is updated to point at the new content-addressed path. Old pointer is retained for rollback within 30 days.
- On failure: rollback — staging path is removed; pointer is unchanged; transition `INSTALLING → INSTALL_FAILED`; FOREVER `PACKAGE_INSTALL_FAILED` (reason = `ATOMIC_INSTALL_FAILED`).

### §6.15 Step 15 — Capability binding

- Issue runtime capability bindings per `manifest.declared_capabilities`. Each binding goes through the L4 capability service with TTL, scope, and the package canonical hash recorded as the binding's authorising-evidence pointer.
- Failure to issue any binding (e.g. capability service unavailable, capability concurrency limit exceeded) → rollback step 14; transition `INSTALLING → INSTALL_FAILED`; FOREVER `PACKAGE_INSTALL_FAILED` (reason = `CAPABILITY_BINDING_FAILED`).

### §6.16 Step 16 — Mark `ACTIVE`

- Transition `INSTALLING → ACTIVE`.
- Emit `PACKAGE_INSTALLED` STANDARD_24M evidence.
- Start the first-run capability audit window.

### §6.17 Step 17 — First-run capability lie audit

- Within the first **60 seconds** of the package's runtime activity (any subject acting on behalf of the package's identity emits an action that reaches the L3 Capability Runtime), L4 + L8 monitor every capability invocation.
- Observed capabilities are recorded in a per-package observation set.
- At the end of the 60-second window, the observation set is compared against `manifest.declared_capabilities`:
  - Observed ⊆ declared → audit passes; `PACKAGE_AUDIT_PASSED` STANDARD_24M evidence.
  - Observed ⊄ declared (any observed capability not declared) → **audit fails**; transition `ACTIVE → QUARANTINED`; FOREVER `CAPABILITY_LIE_DETECTED` evidence with the capability id and the package canonical hash.
- A package may declare more capabilities than it observes (over-declaration is fine); under-declaration is the lie surface.
- Re-audit cannot lift the quarantine; the package must be uninstalled and re-installed at a new manifest version with corrected declarations.

## §7 Recovery-only package classes

Five `PackageKind` values are constitutionally recovery-only:

| Kind                       | Why recovery-only                                                                   |
| -------------------------- | ----------------------------------------------------------------------------------- |
| `INVARIANT_BUNDLE`         | Mutates L0 invariant catalog; binds INV-012; per S9.1 `RecoveryMutableScope`.       |
| `POLICY_BUNDLE`            | Mutates `/aios/system/policy/`; binds INV-012; per S9.1 §3.6.                       |
| `IDENTITY_BUNDLE`          | Mutates `/aios/system/identity/`; binds INV-012; per S9.1 §3.6.                     |
| `KERNEL_CANDIDATE`         | Stages a new kernel image; A/B promotion is an L1.3 / S9.3 recovery-only operation. |
| `CAPABILITY_CATALOG_DELTA` | Mutates `/aios/system/capabilities/`; binds INV-012.                                |

Outside recovery (S9.1 §3.2 `RecoveryMode != RECOVERY`), the install pipeline rejects at step 12 with `RecoveryRequiredForPackageKind`. The error is constitutional — no policy bundle, no operator override, no emergency-override path can lift it without a recovery boot.

`POLICY_BUNDLE` and `CAPABILITY_CATALOG_DELTA` from `VERIFIED` publishers (with explicit policy-authorship-grant or translator-authorship-grant respectively) follow the same recovery-only discipline; the grant changes the publisher's authorisation surface, not the recovery requirement.

## §8 AI subjects cannot install

This is the package-distribution analog of INV-002 (AI proposes, never executes). It binds INV-002 + INV-013 (AI cannot perform system admin) + S5.3 (HUMAN_USER required for approver).

The rule: **AI subjects can REQUEST a package install (typed action) but the install action requires a HUMAN_USER subject in the approval.** AI subjects can never directly install.

Mechanically:

1. An AI subject (`is_ai = true`) emits an action envelope `package.install.request` (per S0.1) with target = a package id + version + source repository.
2. The action transits to the operator's approval queue (S5.3); the approver subject filter is set to `HUMAN_USER`.
3. The operator reviews the request on a trust-bearing surface (S5.3 §3.2 `ApprovalChannel`; default `KDE_NATIVE_PROMPT` or `WEB_LOOPBACK_PROMPT`).
4. On `GRANTED`, the **operator's** subject becomes the install pipeline's caller from step 1 onward. The AI subject's role ends at step 0.5 (the request).
5. The install action is bound to the operator's `EXACT_ACTION` binding, not the AI's. The AI never holds an install binding.
6. Evidence records both subjects: `proposing_subject` = the AI, `executing_subject` = the operator.

Direct attempts by an AI subject to bypass the request flow (e.g. emitting `package.install` directly) are hard-denied at S2.3 with `AISystemAdminBlocked`; FOREVER `PACKAGE_VERIFICATION_FAILED` (reason = `AI_DIRECT_INSTALL_DENIED`) evidence; the AI subject cannot retry within a back-off window.

This applies uniformly to all `PackageKind` values. There is no AI-permitted shortcut even for `THEME` packages (which carry no executable code) — the constitutional rule is uniform.

## §9 The first-run capability lie audit

The 60-second first-run audit closes the supply-chain capability-lie surface. Without it, a package could declare a minimal capability set at install time (passing policy review) and then exercise broader capabilities at runtime (escaping policy review).

### §9.1 Observation surface

Within the audit window, every capability invocation by a subject acting on behalf of the package emits an observation event into a per-package observation set:

- L3 Capability Runtime: every typed action submitted by the package's subject is observed (action kind + target capability).
- L4 Vault Broker: every capability use (sign/verify/encrypt/decrypt) is observed.
- L8 Network Policy: every outbound connection's capability binding is observed.
- L7 Renderer: every UI capability use (chrome zone access, surface composition class) is observed.

Observations are recorded in a tight in-memory ring buffer per package; the buffer flushes to the evidence log on audit completion, not on individual events (to bound evidence-log volume).

### §9.2 Audit decision

At the end of the 60-second window:

```text
declared = set(manifest.declared_capabilities)
observed = ring_buffer.distinct_capability_ids()

if observed ⊆ declared:
    emit PACKAGE_AUDIT_PASSED STANDARD_24M
    transition: stays ACTIVE
else:
    drift = observed - declared
    emit CAPABILITY_LIE_DETECTED FOREVER  (with drift, package canonical hash, observed events)
    transition: ACTIVE → QUARANTINED
    revoke all capability bindings issued at step 15
    halt the package's runtime
```

### §9.3 Edge cases

- A package that emits no actions during the window passes the audit by default (empty observed set is trivially a subset). This is intentional — short-running, idempotent packages should not be penalised by an audit timeout.
- A package whose first action is at second 65 (just after the window closes) does not trigger a re-audit; the audit is one-shot. Drift detected after the window is handled by ongoing runtime monitoring (per L4 binding TTL and L8 outbound enforcement) but does not produce `CAPABILITY_LIE_DETECTED` retroactively; instead it produces the standard L8 / L4 capability-violation evidence.
- A package whose declarations are intentionally tightened in a re-issue (e.g. v1.2.0 declares fewer capabilities than v1.1.0) gets a fresh audit on each install — the audit is per-install, not per-package-id.

### §9.4 Quarantine release

A `QUARANTINED` package via `CAPABILITY_LIE_DETECTED` cannot be released by re-audit. Release requires:

1. Operator review of the FOREVER evidence record.
2. Uninstall the quarantined version.
3. Wait for the publisher to issue a new manifest version with corrected `declared_capabilities`.
4. Install the new version (fresh install pipeline, fresh audit).

There is no "false-positive" release path — the audit is mechanically deterministic; a non-empty drift means the package's declarations did not match its behaviour, which is the lie surface by definition.

## §10 Mirror tampering detection

Mirrors never re-sign packages. They serve the **same signed bytes** verbatim or fail. The host detects mirror tampering at step 5 of the install pipeline (content hash verify):

- Compute `BLAKE3(content)` over the bytes the mirror served.
- Compare against `manifest.content_hash` (signed by the publisher).
- Mismatch → `HASH_MISMATCH`; install fails; FOREVER `PACKAGE_VERIFICATION_FAILED` evidence; mirror's mismatch counter incremented.

A mirror's mismatch counter is tracked on a 24-hour sliding window. When the counter exceeds threshold (default `≥ 3` mismatches in 24 hours), the mirror is auto-blacklisted at the host:

- FOREVER `MIRROR_HASH_MISMATCH_BLACKLISTED` evidence (with mirror URL, the three-or-more package canonical hashes that mismatched, and the rolling counter).
- Subsequent fetches that would target this mirror are pre-rejected at step 1 (fetch); the host falls through to the next mirror in the hierarchy.
- Blacklist persists for 30 days by default; operator can lift earlier with explicit acknowledgement (FOREVER evidence on lift).

A mirror that returns a hash with timing channel (computing `BLAKE3` against tampered content takes longer than against valid content, leaking via timing) is bounded by host-side **constant-time** content-hash check: BLAKE3 is run against the full content regardless of early-mismatch detection; the comparison is constant-time over the 32-byte hash. Timing-channel exploits at the mirror level cannot leak the publisher's signing key.

## §11 Publisher root key rotation

Publisher root key rotation is a recovery-mode operation by AIOS-root cosignature. The flow:

1. Recovery boot on every host that needs to honour the new root (or, for non-reactive rotation, a hot-load via signed publisher catalog delta).
2. Publisher generates new Ed25519 keypair on their infrastructure.
3. **Old publisher root signs the new public key** (chain continuity).
4. AIOS root signs the rotation event:

   ```proto
   message PublisherRotationEvent {
     string publisher_root_id = 1;        // pub:<vendor>
     bytes old_public_key = 2;
     bytes new_public_key = 3;
     bytes old_root_signature_over_new = 4;  // chain continuity
     bytes aios_root_signature = 5;          // authorising the break
     google.protobuf.Timestamp rotated_at = 6;
     TakedownReason reason = 7;              // KEY_COMPROMISE, PUBLISHER_REQUEST, etc.
   }
   ```

5. AIOS root publishes a new publisher catalog version (`pubcat_<new_hex>`) with the rotation recorded.
6. Hosts pull the new catalog at next package fetch; from that moment, the new public key is the authoritative root for the publisher.
7. **All package signing keys signed by the old root continue to verify** until their explicit revocation by the publisher. New package signing keys must be signed by the new root.
8. FOREVER `PUBLISHER_KEY_ROTATED` evidence is emitted on every host that loads the new catalog (with the old/new public keys, reason, and rotated_at).

Reactive rotation under `KEY_COMPROMISE`:

- All package signing keys signed by the old root within the compromise window are **immediately revoked** (publisher specifies the window).
- All packages signed by those revoked keys transition to `QUARANTINED` on the next health check.
- Existing installs within the quarantine pool require re-sign with new key + new install (per §6 atomic install) to leave quarantine.

## §12 Deplatform / takedown discipline

Deplatforming is the constitutional response to a publisher that has gone malicious, abandoned, compromised, or legally-required-to-be-removed. The flow:

1. AIOS root cosigns a takedown event:

   ```proto
   message PublisherDeplatformEvent {
     string publisher_root_id = 1;
     TakedownReason reason = 2;
     google.protobuf.Timestamp deplatformed_at = 3;
     google.protobuf.Timestamp grace_period_ends_at = 4;  // default deplatformed_at + 30 days
     string evidence_pointer = 5;                          // pointer to the AIOS-root review record
     bytes aios_root_signature = 6;
   }
   ```

2. AIOS root publishes a new publisher catalog version with the publisher's `trust_level = DEPLATFORMED` and `retired_at` set.
3. Hosts pull the new catalog; FOREVER `PUBLISHER_DEPLATFORMED` evidence is emitted on every host with the reason.
4. **All packages from the publisher transition `ACTIVE → QUARANTINED` on next health check** (default within 60 minutes of catalog refresh).
5. New installs from the publisher are rejected at step 4 of the install pipeline with `PUBLISHER_DEPLATFORMED`.
6. Existing installs continue running for `grace_period_ends_at - deplatformed_at` (default 30 days).
7. At grace period end, existing installs auto-uninstall. Operator can extend per-package with explicit acknowledgement (FOREVER `PACKAGE_DEPLATFORM_GRACE_EXTENDED` evidence; max one extension of up to 30 days).
8. Every interaction with a `DEPLATFORMED`-publisher package emits FOREVER evidence (`PACKAGE_DEPLATFORMED_INTERACTION`) so audit can reconstruct the wind-down.

The takedown event is **not reversible** by ordinary publisher action. A formerly-deplatformed publisher returning under a new identity is treated as a fresh publisher with `COMMUNITY` trust by default; AIOS root can grade them up only after the standard onboarding review.

## §13 Network outbound manifest binding

Per S8.1 §G, every package's `NetworkOutboundManifest` is enforced by L8 at runtime. The manifest is part of the signed package manifest (field 13 in §5); modifications require re-issue of the package + re-approval.

Binding details:

- `NetworkOutboundManifest.signing_key_id` (per S8.1) may be the package signing key OR an L8 service signing key — in either case the signature surface is fixed at install time.
- The L8 connection correlator (S8.1 §10.3) maps a process's subject id to its package's installed manifest at runtime; outbound connections are evaluated against the package's network manifest.
- Manifest mutations mid-install (e.g. operator edits the file after fetch) → step 6 manifest field validation fails (`manifest_canonical_hash` mismatch).
- Manifest mutations post-install are impossible: the active manifest is content-addressed at the install pointer; any change requires a new install at a new manifest version.
- A package whose runtime outbound traffic exceeds its network manifest is hard-denied at L8 with `OUTBOUND_NOT_IN_ALLOWLIST`; the L8 service may quarantine the package on repeated breach (per S8.1 I8 / I11). Quarantine via L8 transitions the package to `QUARANTINED` and emits FOREVER `PACKAGE_QUARANTINED` (reason = `NETWORK_BREACH`).

## §14 External bridges (Flathub / OCI / distro)

External bridges are the L10 pathway by which non-native packages reach AIOS. They are **never admitted to `AIOS_ROOT` or `VERIFIED` trust**: bridges always re-package upstream content under `COMMUNITY` trust at best.

### §14.1 Bridge architecture

A bridge is itself an AIOS package (`PackageKind = APP` or a dedicated bridge kind in a future revision) running under a tight sandbox profile. The bridge:

1. Fetches the upstream package (Flatpak, OCI image, distro RPM/DEB) using the upstream's transport.
2. Verifies the upstream signature (Flatpak GPG, OCI cosign, distro repo signature) per the upstream's discipline.
3. **Re-packages** the upstream content under an AIOS bridge-signing key:
   - The bridge holds an Ed25519 keypair signed by an AIOS bridge publisher (a `VERIFIED` publisher whose role is bridging).
   - The bridge synthesises an AIOS `PackageManifest` whose:
     - `publisher_root_id` = the bridge publisher,
     - `package_signing_key_id` = the bridge's signing key,
     - `package_id` = `pkg:bridge-<source>:<upstream-name>` (e.g. `pkg:bridge-flathub:org.gimp.GIMP`),
     - `publisher_trust = COMMUNITY`,
     - `originating_repository = EXTERNAL_BRIDGE`,
     - `mirror_semantic = ORIGIN` (the bridge is the origin for the AIOS-shaped package),
     - `required_sandbox` = a tight Flatpak/OCI/distro-aware sandbox profile,
     - `declared_capabilities` = an explicit minimal set; the bridge **cannot** declare AI-forbidden or system-admin capabilities,
     - `network_manifest` = bounded by the upstream's declared network plus an upper-bound default.
   - Bridge audit metadata (upstream URL, upstream signature, upstream version, bridge run id) is recorded in evidence at admission.

### §14.2 Admission

A bridge-emitted package follows the standard install pipeline (§6). The pipeline treats the bridge's signing chain as canonical: the bridge publisher → the bridge signing key → the synthesised manifest, with the bridge publisher itself being a regular `VERIFIED` AIOS publisher.

If the upstream signature fails (e.g. Flatpak GPG signature is invalid for the fetched content), the bridge **must not** synthesise an AIOS manifest; instead it emits FOREVER `EXTERNAL_BRIDGE_UPSTREAM_SIGNATURE_FAILED` evidence and aborts.

If the upstream signature succeeds, the bridge admits the package and emits `EXTERNAL_BRIDGE_PACKAGE_ADMITTED` STANDARD_24M evidence with the upstream metadata.

### §14.3 Trust ceiling

The synthesised manifest is `COMMUNITY` trust regardless of the upstream's reputation. A widely-trusted Flathub package becomes `COMMUNITY` on AIOS — the upstream's signature is trusted only insofar as the bridge admits it; AIOS does not promote upstream trust into AIOS-root or AIOS-verified trust.

A future revision may introduce a `BRIDGE_VERIFIED` tier for vetted bridges, but in this contract that tier does not exist.

### §14.4 Sandboxing and rate limits

Bridges are sandboxed under a tight S3.2 profile (filesystem: read-only on `/aios/system`, write-only to a bridge-specific staging area; network: outbound to upstream servers only; no GPU; bounded CPU and memory).

Bridges are rate-limited by the host: at most `MAX_BRIDGE_FETCHES_PER_HOUR` (default 60) admissions per bridge. Excess admissions are deferred. Rate limit excess does not produce evidence on each excess; the limit is a backpressure mechanism.

## §15 Adversarial robustness

This section enumerates the adversarial vectors the contract defends against, in the order of §1's threat model plus additional vectors.

### §15.1 Fake AIOS root key

**Vector.** An attacker generates an Ed25519 keypair, claims it as the AIOS root, and signs malicious publisher catalogs.

**Defense.** The AIOS root public key is **firmware-pinned** at first boot: the bootloader (S9.2 deferred) reads it from a known offset in the signed firmware/installer artefact and stores its hash in the kernel command line. Mismatch at boot drops the system into recovery (`INVARIANT_BUNDLE_SIGNATURE_FAILURE`). A software-level fake root cannot survive a reboot; an attacker would need to re-flash the firmware-pinned offset, which itself is firmware-signed.

This defense binds INV-001 (recovery independent of L5) and S9.1 §3.3 entry reasons.

### §15.2 Replay of older signed packages

**Vector.** An attacker captures a legitimately-signed `pkg:vendor:foo@1.0.0` and serves it after `pkg:vendor:foo@1.1.0` is released, hoping a host that lost network re-installs the older version.

**Defense.** Per-package version ordering is enforced at step 6 (manifest field validation): the host tracks the highest-version `package_id` ever installed in a per-package monotonic counter; replay of a strictly-older version is rejected with `PACKAGE_DOWNGRADE_BLOCKED` and FOREVER evidence (record type `PACKAGE_DOWNGRADE_BLOCKED` extended-60M).

### §15.3 Downgrade attacks

**Vector.** An operator (or AI subject acting via the request flow) is socially-engineered into downgrading an active package to a version with a known vulnerability.

**Defense.** A downgrade is detected at step 6 and rejected by default (`PACKAGE_DOWNGRADE_BLOCKED`). Explicit downgrade requires an additional approval (`ApprovalStrength = STRONG`) plus FOREVER `PACKAGE_DOWNGRADE_APPROVED` evidence. AI subjects cannot request a downgrade — the request action `package.downgrade` is hard-denied for AI subjects (cite INV-002).

### §15.4 Spoofed `publisher_root_id`

**Vector.** A package manifest claims `publisher_root_id = pub:openai` while being signed by an unrelated publisher's signing key.

**Defense.** Step 3 (trust chain verify) walks the chain: the package signing key must be signed by the claimed publisher root. Any mismatch → `TRUST_CHAIN_BROKEN`. The publisher catalog is the single source of truth for which public key corresponds to which `publisher_root_id`; a spoofed claim cannot pass verification.

### §15.5 Concurrent install of conflicting versions

**Vector.** Two operators simultaneously approve installs of `pkg:vendor:foo@1.0.0` and `pkg:vendor:foo@1.1.0`, racing to claim the same install pointer.

**Defense.** The install pipeline FSM enforces **single-active install per package_id**: step 13 (`APPROVED → INSTALLING`) atomically claims the install pointer; the second concurrent approval finds the pointer claimed and either waits (default; up to 5 minutes) or fails (`CONCURRENT_INSTALL_DETECTED`).

The pointer flip in step 14 is atomic; one of the two installs wins; the loser rolls back.

### §15.6 Mirror with timing-channel

**Vector.** A mirror modulates response timing based on the requested package's content hash, leaking signing keys via timing.

**Defense.** Host-side BLAKE3 content-hash check is **constant-time** over the 32-byte hash comparison; the host always reads the full package content from the mirror before computing the hash; early-abort on mismatch is forbidden. Timing-channel exploits at the mirror level cannot leak the publisher's signing key, because the publisher's signing key is held in the publisher's vault (off-host) and is never reachable through the mirror's content.

### §15.7 Publisher-key compromise

**Vector.** A publisher's package signing key is compromised; the attacker signs malicious packages.

**Defense.** Detection: AIOS root, the publisher, or an automated audit detects the compromise. Response:

1. Publisher submits a `KEY_COMPROMISE` rotation per §11.
2. Publisher revokes the compromised package signing key in the per-publisher signing-key catalog (`revoked_at` set).
3. AIOS root cosigns a publisher catalog delta acknowledging the revocation.
4. All packages signed by the revoked key transition `ACTIVE → QUARANTINED` on next health check.
5. New installs of those packages are rejected at step 3 (`TRUST_CHAIN_BROKEN`; key revoked).
6. Existing installs require re-sign by the publisher with the new key + new install (per §6 atomic install) to leave quarantine.
7. FOREVER `PUBLISHER_KEY_ROTATED` evidence with reason `KEY_COMPROMISE`.

This defense binds INV-014 (no proof, no completion) — quarantined packages cannot claim `REAL` operational status until re-signed.

### §15.8 Manifest forgery post-fetch

**Vector.** A man-in-the-middle modifies the manifest fields between fetch and verification.

**Defense.** The signature is computed over `manifest_canonical_hash`, which is computed at the host from the fetched manifest. Any post-fetch modification of any field changes the canonical hash, which fails the signature verification at step 2 (`SIGNATURE_FAILED`). The signature is the integrity check; transport-level integrity (TLS) is a defense-in-depth layer.

### §15.9 Capability-lie at version boundary

**Vector.** A `v1.0.0` package declares minimal capabilities (passes audit). A `v1.1.0` re-issue declares the same capabilities but exercises broader ones at runtime.

**Defense.** The first-run audit is **per-install**, not per-package-id. Each install (including upgrades) gets a fresh 60-second audit window. Drift detected on the upgrade triggers `CAPABILITY_LIE_DETECTED` and quarantines the upgrade — the previous `v1.0.0` is preserved in the rollback pointer (per §6.14).

## §16 Telemetry contract

Bounded-cardinality metrics. Subject id, package id, publisher id, and signing key id are **never** label values (they would unbounded-explode the cardinality space).

| Metric                                         | Type      | Labels (closed)                                                                                                                                                                                | Cardinality budget |
| ---------------------------------------------- | --------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------ |
| `package_install_total`                        | counter   | `result` (`success`/`policy_denied`/`signature_failed`/`chain_broken`/`hash_mismatch`/`forged`/`tampered`/`approval_denied`/`atomic_failed`), `publisher_trust` (closed `PublisherTrustLevel`) | ≤ 45               |
| `package_active_total`                         | gauge     | `kind` (closed `PackageKind`), `channel` (closed `UpdateChannel`), `trust_level` (closed `PublisherTrustLevel`)                                                                                | ≤ 180              |
| `package_quarantined_total`                    | counter   | `reason` (`capability_lie`/`network_breach`/`publisher_deplatformed`/`manifest_eol`/`runtime_breach`/`upstream_signature_failed`)                                                              | ≤ 6                |
| `mirror_fetch_total`                           | counter   | `semantic` (closed `MirrorSemantic`), `result` (`success`/`hash_mismatch`/`network_failed`/`blacklisted`)                                                                                      | ≤ 12               |
| `mirror_hash_mismatch_blacklist_total`         | counter   | (none)                                                                                                                                                                                         | 1                  |
| `publisher_takedown_total`                     | counter   | `reason` (closed `TakedownReason`)                                                                                                                                                             | ≤ 7                |
| `capability_lie_detected_total`                | counter   | `publisher_trust` (closed `PublisherTrustLevel`)                                                                                                                                               | ≤ 5                |
| `external_bridge_admit_total`                  | counter   | `source` (`FLATHUB`/`OCI`/`DISTRO`)                                                                                                                                                            | ≤ 3                |
| `package_install_pipeline_step_failures_total` | counter   | `step` (`fetch`/`signature`/`chain`/`publisher_state`/`hash`/`manifest`/`sandbox`/`capability`/`network`/`policy`/`approval`/`recovery_gate`/`atomic`/`binding`)                               | ≤ 14               |
| `package_install_pipeline_duration_seconds`    | histogram | `step` (same as above)                                                                                                                                                                         | ≤ 14 (× buckets)   |
| `package_downgrade_blocked_total`              | counter   | (none)                                                                                                                                                                                         | 1                  |

NEVER as labels: `package_id`, `publisher_root_id`, `package_signing_key_id`, `subject_id`, `mirror_url`, `manifest_canonical_hash`, `content_hash`.

Total cardinality budget: ≤ 300 active label tuples across all L10 metrics.

## §17 Evidence record types (queue for S3.1)

Nineteen new record types are queued for the next S3.1 consolidation.

| Record type                                 | Retention class | Trigger                                                                                |
| ------------------------------------------- | --------------- | -------------------------------------------------------------------------------------- |
| `PACKAGE_FETCH_STARTED`                     | STANDARD_24M    | Fetch attempt started (step 1).                                                        |
| `PACKAGE_VERIFIED`                          | STANDARD_24M    | All verification steps (2–9) passed.                                                   |
| `PACKAGE_VERIFICATION_FAILED`               | EXTENDED_60M    | Any verification step failed; carries `PackageVerificationResult`.                     |
| `PACKAGE_APPROVAL_REQUESTED`                | STANDARD_24M    | Step 11 entered `AWAITING_APPROVAL`.                                                   |
| `PACKAGE_INSTALLED`                         | STANDARD_24M    | Step 16 transitioned to `ACTIVE`.                                                      |
| `PACKAGE_INSTALL_FAILED`                    | EXTENDED_60M    | FSM reached `INSTALL_FAILED`.                                                          |
| `PACKAGE_QUARANTINED`                       | FOREVER         | FSM transitioned `ACTIVE → QUARANTINED`; carries reason.                               |
| `PACKAGE_UNINSTALLED`                       | STANDARD_24M    | FSM transitioned `UNINSTALLING → REMOVED`.                                             |
| `PACKAGE_DOWNGRADE_BLOCKED`                 | EXTENDED_60M    | Step 6 detected version downgrade.                                                     |
| `CAPABILITY_LIE_DETECTED`                   | FOREVER         | First-run audit failed (§9).                                                           |
| `TRUST_CHAIN_BROKEN`                        | FOREVER         | Step 3 chain verify failed (revoked key, missing catalog, etc.).                       |
| `TRUST_CHAIN_TOO_DEEP`                      | FOREVER         | Step 3 chain depth > 3.                                                                |
| `MANIFEST_FORGED`                           | FOREVER         | Step 6 detected forged manifest field (canonical hash mismatch, trust-level mismatch). |
| `MIRROR_HASH_MISMATCH_BLACKLISTED`          | FOREVER         | Mirror's mismatch counter exceeded threshold; mirror auto-blacklisted (§10).           |
| `PUBLISHER_KEY_ROTATED`                     | FOREVER         | Publisher root key rotation completed (§11).                                           |
| `PUBLISHER_DEPLATFORMED`                    | FOREVER         | AIOS root cosigned takedown event (§12).                                               |
| `EXTERNAL_BRIDGE_PACKAGE_ADMITTED`          | STANDARD_24M    | A bridge admitted an upstream package (§14.2).                                         |
| `EXTERNAL_BRIDGE_UPSTREAM_SIGNATURE_FAILED` | EXTENDED_60M    | A bridge's upstream signature verification failed (§14.2).                             |
| `AIOS_ROOT_KEY_ROTATED`                     | FOREVER         | AIOS root key rotation completed (§4.1; recovery-mode operation).                      |

These record types extend S3.1 §4 `RecordType` enum at the next S3.1 consolidation. Until then, this contract treats them as queued; emitters write them via the existing `EvidenceLog.Append` RPC with the proposed enum value reserved.

## §18 Cross-spec dependencies

| Spec                                           | Direction           | What this contract relies on / produces                                                                                                                                      |
| ---------------------------------------------- | ------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| L0 INV-002, INV-008, INV-013, INV-014, INV-017 | constraint          | AI cannot install; default-deny on policy outcome; AI cannot perform system admin; no proof-no completion; sandbox floor cannot be loosened.                                 |
| S5.2 Vault Broker                              | consumer            | AIOS root signing key, publisher root signing keys, package signing keys all held under vault `KEY_SIGN` capabilities; vault performs sign/verify without exposing material. |
| S5.3 Approval Mechanics                        | consumer            | Install approval uses `EXACT_ACTION` binding bound to package canonical hash; HUMAN_USER required for approver; TTL classes mapped per §6.11.                                |
| S3.2 Sandbox Composition                       | consumer            | `manifest.required_sandbox` validated by `SandboxComposer.ValidateProfile` at step 7; runtime sandbox composed from manifest + S3.2 floors at install-time.                  |
| S9.1 Recovery Boundary                         | consumer            | Recovery-only package classes (§7) require `RecoveryMode = RECOVERY`; binds `RecoveryMutableScope` for `/aios/system/policy`, `/aios/system/identity`, etc.                  |
| S8.1 Network Policy                            | consumer + producer | `manifest.network_manifest` is part of the signed manifest; L8 enforces at runtime; bridges use L8 outbound discipline; this contract produces the install-time validation.  |
| S3.1 Evidence Log                              | producer            | Nineteen new record types queued (§17); existing `Append` RPC consumed.                                                                                                      |
| S1.1 Capability Translator + Catalog           | consumer            | `manifest.declared_capabilities` resolved against the active L5 capability catalog; catalog version pinned in evidence.                                                      |
| S4.1 Namespace Layout                          | consumer            | `installable_scope` mapped to `/aios/system`, `/aios/groups/<g>/system`, `/aios/groups/<g>/users/<u>`.                                                                       |
| S2.3 Policy Kernel                             | consumer            | `EvaluatePolicy` at step 10 returns `ALLOW` / `REQUIRE_APPROVAL` / `DENY`; hard-deny `AISystemAdminBlocked` for AI direct installs.                                          |
| L10 marketplace (`02`)                         | producer            | This contract is the trust root the marketplace UX consumes; marketplace cannot loosen any verification step defined here.                                                   |
| L10 external bridges (`03`)                    | producer            | This contract defines the bridge admission discipline; bridge sub-spec adds upstream-specific mechanics (Flathub GPG, OCI cosign, distro repo signatures).                   |

## §19 L0 invariant candidate (queued narrative-only)

This contract identifies one constitutional invariant candidate for promotion to the L0 catalog:

- **`PACKAGE_TRUST_CHAIN_BOUND`** — every running binary on an AIOS host can be traced back through at most three Ed25519 signatures to a firmware-pinned AIOS root key; no policy bundle, identity bundle, or operator override can loosen the chain depth or the firmware pin.

This invariant is **not authored here**. It is queued for L0 INV catalog promotion in the next L0 revision, alongside the S8.1 candidate `NETWORK_DEFAULT_DENY_OUTBOUND`. Until promoted, the §4 chain-depth rule and the §15.1 firmware-pin defense are the operational floor.

Per L0 §3 I1 (closed list), invariant catalog mutation is a versioned spec change and recovery-mode invariant-bundle update — narrative queueing here does not bypass that discipline.

## §20 Worked examples

### §20.1 Example 1 — Operator installs a `VERIFIED`-publisher app (happy path)

**Setup.**

- Operator: `alice` (HUMAN_USER, primary group = `home`, session class = STRONG).
- Package: `pkg:gimp-org:gimp@2.10.36`, kind = `APP`, publisher = `pub:gimp-org` at `VERIFIED` trust.
- Source: `AIOS_VERIFIED_REPO` via `mirror.aios.org` (`MirrorSemantic = CACHED`).
- Manifest declares: `installable_scope = USER_SCOPED`, `required_sandbox = (Landlock fs/network/proc; AI floor not applicable; user namespace)`, `declared_capabilities = [filesystem.read.user, filesystem.write.user, x11.display, audio.playback]`, `network_manifest = (HOST_FQDN: download.gimp.org; FQDN_FANOUT ≤ 4)`.

**Pipeline trace.**

1. **Fetch.** Mirror hierarchy: LOCAL miss → `mirror.aios.org` hit. `PACKAGE_FETCH_STARTED` STANDARD_24M.
2. **Signature verify.** Ed25519 against `pks:gimp-org:release-2025` succeeds.
3. **Trust chain verify.** `pks:gimp-org:release-2025` ← `pub:gimp-org` ← AIOS root. Three hops, valid.
4. **Publisher state check.** `pub:gimp-org` at `VERIFIED`, not retired.
5. **Content hash verify.** `BLAKE3(content)[:32]` matches manifest. Mirror serves verbatim.
6. **Manifest field validation.** All fields valid.
7. **Sandbox profile validation.** Host has Landlock; profile composes; AI floor not relevant (kind = APP, not AGENT).
8. **Capability declaration.** All four declared capabilities resolve in active L5 catalog.
9. **Network manifest validation.** `download.gimp.org` is a valid FQDN; fan-out = 1 (well within bound 16).
10. **Policy decision.** S2.3 returns `REQUIRE_APPROVAL`.
11. **Approval.** `EXACT_ACTION` binding; channel = `KDE_NATIVE_PROMPT`; strength = `WEAK`; TTL = TTL_SHORT (5 min). Alice approves. `APPROVED`.
12. **Recovery-mode gate.** Not applicable (`installable_scope = USER_SCOPED`). Skip.
13. **`APPROVED → INSTALLING`.**
14. **Atomic install.** Files written to `/aios/groups/home/users/alice/apps/staging/<content_hash>/`; install hook completes; pointer flip.
15. **Capability binding.** Four bindings issued through L4.
16. **Mark `ACTIVE`.** `PACKAGE_INSTALLED` STANDARD_24M evidence emitted.
17. **First-run capability lie audit.** Within 60 seconds, GIMP exercises `filesystem.read.user`, `x11.display`, `audio.playback`. Observed = {3 caps} ⊆ Declared = {4 caps}. Audit passes; `PACKAGE_AUDIT_PASSED` STANDARD_24M.

**Final state.** `pkg:gimp-org:gimp@2.10.36` `ACTIVE` under Alice's user scope. Three evidence records emitted (fetch, installed, audit-passed) plus the policy and approval evidence from S2.3 / S5.3.

### §20.2 Example 2 — AI agent requests an app install (request flow)

**Setup.**

- AI subject: `ai:home:tutoring-agent-7` (`is_ai = true`, primary group = `home`, session class = STRONG).
- Operator: `alice` (HUMAN_USER, available on KDE).
- Package: `pkg:khan-academy:offline-pack@1.4.0`, kind = `APP`, publisher = `pub:khan-academy` at `VERIFIED`.

**Trace.**

1. AI agent emits `package.install.request` action envelope (S0.1) with target = `pkg:khan-academy:offline-pack@1.4.0`, `source_repo = AIOS_VERIFIED_REPO`. `proposing_subject_id = ai:home:tutoring-agent-7`.
2. S2.3 evaluates the request: AI subject + install kind → `REQUIRE_APPROVAL` with `approver_subject_filter = HUMAN_USER`.
3. S5.3 delivers approval prompt to Alice via `KDE_NATIVE_PROMPT` (channel = CHROME zone; cite L7.1 INV-023). Prompt clearly labels: "AI subject `tutoring-agent-7` is requesting installation of …".
4. Alice grants. `EXACT_ACTION` binding consumed by **Alice** (not the AI).
5. From step 1 of the install pipeline onward, the executing subject is Alice. The AI subject's role ended at step 0 of this sequence (the request).
6. Pipeline runs as in §20.1 (trust chain valid, content hash valid, etc.).
7. `PACKAGE_INSTALLED` emitted with both subjects recorded:
   - `proposing_subject = ai:home:tutoring-agent-7`,
   - `executing_subject = alice@home`.

**Adversarial variant.** If `tutoring-agent-7` had emitted `package.install` directly (skipping the request action), S2.3 hard-deny `AISystemAdminBlocked` would fire; FOREVER `PACKAGE_VERIFICATION_FAILED` (reason = `AI_DIRECT_INSTALL_DENIED`); the AI would face a back-off window before re-attempt.

### §20.3 Example 3 — Publisher key compromise → deplatform → quarantine

**Setup.**

- Publisher: `pub:vendor-x` at `VERIFIED` trust.
- Compromise window: 2026-04-15 00:00 UTC to 2026-04-17 12:00 UTC.
- AIOS root cosigns deplatform event 2026-04-18 09:00 UTC with `TakedownReason = KEY_COMPROMISE`.

**Trace on every host.**

1. Host pulls new publisher catalog version at 2026-04-18 09:30 UTC (next package fetch).
2. New catalog has `pub:vendor-x` with `trust_level = DEPLATFORMED`, `retired_at = 2026-04-18 09:00 UTC`.
3. FOREVER `PUBLISHER_DEPLATFORMED` evidence emitted with `reason = KEY_COMPROMISE`, `grace_period_ends_at = 2026-05-18 09:00 UTC` (30 days).
4. Health check sweeps active packages at 2026-04-18 10:00 UTC:
   - All packages signed by any `pub:vendor-x` package signing key in the compromise window transition `ACTIVE → QUARANTINED`.
   - FOREVER `PACKAGE_QUARANTINED` (reason = `PUBLISHER_DEPLATFORMED`) per package.
   - Capability bindings revoked.
5. New install attempts of `pub:vendor-x` packages are rejected at step 4 (publisher state check) with `PUBLISHER_DEPLATFORMED`.
6. Operator review window: 30 days. Operator can extend per-package grace once, up to additional 30 days, with FOREVER `PACKAGE_DEPLATFORM_GRACE_EXTENDED` evidence.
7. At grace end, residual installs auto-uninstall; FOREVER `PACKAGE_UNINSTALLED` evidence (reason = `DEPLATFORM_GRACE_EXPIRED`).

**Recovery path for affected installs.** If the publisher rotates keys (per §11) and re-signs the legitimately-built packages with the new key, the operator can re-install the new versions through the standard pipeline; the new manifest's signing chain is verified against the rotated publisher root; quarantined versions are uninstalled before the new install (per §6.14 atomic discipline).

## §21 Open deferrals

- **HSM integration for AIOS root and publisher roots.** This spec assumes the AIOS root key lives in the AIOS vault (`vault://aios/system/root_signing`) under software encryption. Future revisions may move to an HSM-backed root for additional defense-in-depth. Deferred.
- **Threshold or multi-party signing for AIOS root.** A 2-of-3 or k-of-n scheme would prevent a single-host compromise from yielding the AIOS root key. Deferred (single-host-single-root for now).
- **Distributed package mirrors with consensus-bound integrity.** Cross-mirror Byzantine agreement on package content. Deferred.
- **Cross-host package state federation.** One host's `QUARANTINED` decision does not automatically propagate to peer hosts. Federation deferred.
- **`BRIDGE_VERIFIED` trust tier.** A future trust tier between `VERIFIED` and `COMMUNITY` for vetted bridges. Not in this revision.
- **Marketplace UX** (`02_marketplace.md`) — `SHELL`. Publisher onboarding workflow, listing review, ratings, search, discovery.
- **External-integration deep-spec** (`03_external_integrations.md`) — `SHELL`. Flathub GPG verification mechanics, OCI cosign integration, distro repo bridge details (apt/dnf/pacman).
- **Package downgrade approval mechanics.** §15.3 names `PACKAGE_DOWNGRADE_APPROVED` as the explicit-downgrade evidence; the precise approval flow (whether `STRONG` is sufficient, whether dual-control is required for security-sensitive packages) is operator-policy and may be tightened in a later revision.
- **L0 promotion of `PACKAGE_TRUST_CHAIN_BOUND`.** Queued for the next L0 invariant catalog revision (§19).
- **Per-machine attestation as additional trust factor.** Augmenting the chain with TPM-attested host state on package admission. Deferred.
- **Publisher reputation feedback loop.** A formal reputation channel (operator reports → AIOS root review → potential takedown) is a marketplace concern; only the takedown side is contract-grade here.

## §22 Acceptance criteria

- [ ] `PublisherTrustLevel`, `RepositoryKind`, `UpdateChannel`, `PackageKind`, `InstallScope`, `PackageInstallState`, `PackageVerificationResult`, `MirrorSemantic`, `TakedownReason` are closed enums with the exact value sets in §3.
- [ ] `PackageManifest` proto has nineteen fields per §5; every field has a stated validation rule and failure mode in §5.1.
- [ ] The install pipeline has exactly seventeen steps per §6, strictly ordered, fail-closed.
- [ ] The trust chain has chain depth ≤ 3 enforced at step 3; longer chains rejected with `TRUST_CHAIN_TOO_DEEP` and FOREVER evidence.
- [ ] Five `PackageKind` values are recovery-only per §7.
- [ ] AI subjects cannot install per §8; the `package.install` action by an AI subject is hard-denied at S2.3.
- [ ] First-run capability lie audit runs for 60 seconds; `observed ⊄ declared` triggers `CAPABILITY_LIE_DETECTED` FOREVER and quarantines the package per §9.
- [ ] Mirrors never re-sign per §10; mirror auto-blacklist on ≥ 3 mismatches in 24 hours.
- [ ] Publisher root key rotation is recovery-mode + AIOS-root cosignature per §11; FOREVER `PUBLISHER_KEY_ROTATED` evidence.
- [ ] Deplatform discipline emits FOREVER `PUBLISHER_DEPLATFORMED`; 30-day grace; auto-quarantine on next health check per §12.
- [ ] `manifest.network_manifest` is part of the signed manifest; modifications require re-issue per §13.
- [ ] External bridges are never admitted above `COMMUNITY` trust per §14.3.
- [ ] All nine adversarial vectors in §15 have a named defense.
- [ ] Telemetry total cardinality ≤ 300 active label tuples per §16.
- [ ] Nineteen evidence record types are queued for S3.1 per §17.
- [ ] One L0 invariant candidate (`PACKAGE_TRUST_CHAIN_BOUND`) is queued narrative-only per §19.
- [ ] Three worked examples (§20) trace deterministically through the pipeline.

## §23 See also

- [L10 Overview](00_overview.md)
- [L0 Constitutional Invariants](../L0_Governance_Evidence_Safety/04_invariants.md) — INV-002, INV-008, INV-013, INV-014, INV-017
- [S5.2 Vault Broker](../L4_Policy_Identity_Vault/02_vault_broker.md) — `KEY_SIGN`, `KEY_VERIFY`
- [S5.3 Approval Mechanics](../L4_Policy_Identity_Vault/04_approval_mechanics.md) — `EXACT_ACTION` binding
- [S3.2 Sandbox Composition](../L6_Apps_Packages_Compatibility/04_sandbox_composition.md) — `SandboxProfile`
- [S9.1 Recovery Boundary](../L1_Kernel_Bootstrap_Recovery/01_recovery_boundary.md) — `RecoveryMutableScope`
- [S8.1 Network Policy](../L8_Network_Hardware_Devices/02_network_policy.md) — `NetworkOutboundManifest`
- [S3.1 Evidence Log](../L9_Observability_Admin_Operations/01_evidence_log.md) — `RecordType`, FOREVER retention
- [S4.1 Namespace Layout](../L2_AIOS_FS/05_namespace_layout.md) — `installable_scope`
- [S2.3 Policy Kernel](../L4_Policy_Identity_Vault/01_policy_kernel.md) — `EvaluatePolicy`, hard-denies
- [Rev.1 §6 — Layer rules](../../001.AI-OS.NET--SPECREV.1/02_SPECIFICATION.md)
- [Rev.2 Master Index](../00_MASTER_INDEX.md)

---

Status: `REAL`
Evidence: `E1` (file exists; structural contract complete; closed enums declared; install pipeline FSM strictly ordered; nineteen evidence record types queued for S3.1; three worked examples trace deterministically; cross-spec dependencies enumerated; L0 invariant candidate queued narrative-only)
