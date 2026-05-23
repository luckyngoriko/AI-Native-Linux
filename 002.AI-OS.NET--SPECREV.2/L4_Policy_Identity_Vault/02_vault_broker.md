# Vault Broker (Rev.2)

| Field          | Value                                                                                                                                                               |
| -------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Status         | `CONTRACT` (initial; written 2026-05-11)                                                                                                                            |
| Phase tag      | S5.2                                                                                                                                                                |
| Layer          | L4 Policy, Identity, Vault                                                                                                                                          |
| Schema package | `aios.vault.v1alpha1`                                                                                                                                               |
| Consumes       | S0.1 (action envelope `target` may reference `vault_capability_id`), S2.3 (policy gates capability use), S5.1 (`CapabilityBinding` scope; subject identity)         |
| Produces       | typed `VaultCapability`, use-without-reveal operation surface, capability lifecycle FSM, raw-secret-leak prevention contract, queued evidence record types for S3.1 |
| Binds          | INV-003 (secrets are capabilities), INV-015 (evidence never contains secrets), INV-018 (vault never leaks raw secrets)                                              |

## 1. Purpose

The Vault Broker is the **only component in AIOS allowed to hold raw secret material**. Every other component — Cognitive Core, agents, apps, services, even the policy kernel — operates on **capabilities**, never on secret bytes. A capability names a typed operation ("sign this blob with key X") that the broker performs internally and whose **result alone** is returned to the requester.

This sub-spec is the operational reading of two constitutional invariants:

- **INV-003 — Secrets are capabilities, not values.** Subjects request operations; the broker performs them. AI subjects in particular cannot retrieve raw bytes through any path.
- **INV-018 — Vault never leaks raw secrets.** Even HUMAN_USER subjects do not receive raw material by default; the only path is a tightly scoped reveal-to-human operation under recovery mode with a human co-signer.
- **INV-015 — Evidence never contains secrets.** Operations emit evidence projections that are mechanically incapable of carrying material.

What this spec defines:

1. The closed taxonomy of capability classes (`VaultCapabilityClass`) — the operations the broker can perform.
2. The closed taxonomy of material kinds (`VaultMaterialKind`) — what the broker stores at rest.
3. The capability lifecycle FSM (DRAFT → ACTIVE → EXPIRED | REVOKED | ROTATED).
4. The use-without-reveal operation surface (`SignBlob`, `DecryptBlob`, `GenerateMac`, `RevealSecret`, etc.).
5. Adversarial robustness: replay protection, AI-tries-SECRET_GET, forged capability ids, side-channel timing, master-key extraction.
6. Capability rotation, revocation, bundle-rollover invalidation, recovery-mode re-key.
7. The set of evidence record types this spec queues for S3.1 follow-up.
8. The full `VaultBroker` gRPC surface (`aios.vault.v1alpha1`).

What this spec does **not** define:

- Concrete cryptographic algorithms beyond what the closed `VaultMaterialKind` enum names; algorithm choices are deployment configuration, not spec text.
- Hardware Security Module integration (deferred — §16).
- Distributed vault for multi-host clusters (deferred — §16).
- Threshold or multi-party computation schemes (deferred — §16).

## 2. Core invariants

- **I1 — AI subjects cannot retrieve raw bytes.** For any subject with `is_ai = true`, the operation `SECRET_GET` (and any future class equivalent) is rejected at the request entry point, **regardless of capability binding**. The check is hard-coded in the broker; no capability grant, no policy bundle, no override path can lift it. Failure emits `SUBJECT_KIND_REJECTED_FOR_VAULT` evidence with FOREVER retention.
- **I2 — Capabilities are scoped per S5.1 §9.** A binding is valid only when the active session matches its `(subject_canonical_id, group_id, identity_bundle_version)` triple. Bundle rollover (S5.1 §8.1, §14.4) invalidates all bindings tied to the prior version; the broker re-validates per the active version on every operation.
- **I3 — Every operation emits redacted evidence.** Each successful or failing call produces a `VAULT_OPERATION` record (or one of the lifecycle records in §14) whose schema is **mechanically incapable** of carrying secret bytes, plaintext input, or signatures over high-privacy data. This binds INV-015.
- **I4 — Reveal-to-human is the only path to raw bytes.** The `SECRET_GET` operation is permitted **only** under all of these simultaneously: `kind = HUMAN_USER`, `session_class = STRONG`, `recovery_mode = true`, an explicit human co-signer approval, and an emitted `VAULT_RAW_REVEAL` evidence record with FOREVER retention. Any condition unmet → reject.
- **I5 — Bounded usage budget per capability.** Every capability carries a `usage_budget` (max operations and per-window rate cap). The broker tracks usage across sessions; budget exhaustion moves the capability to `EXPIRED` (not `REVOKED`, since exhaustion is normal). Rate caps prevent secret-by-frequency-analysis attacks (oracle attacks against decrypt/sign).
- **I6 — Material at rest is encrypted.** All material kinds in the broker's storage are encrypted with a master key derived from a boot-time secret (passphrase + hardware seed when available). The master key never leaves the vault process address space. Re-keying is a recovery-mode operation only.
- **I7 — Cross-group capability use is rejected.** A capability bound to `(subject = X, group_id = A)` cannot be exercised in a session where `primary_group_id = B`, even if X is a member of B. This is enforced at use time (per S5.1 §6.4 and I2 above).
- **I8 — Anti-replay nonce on every operation.** Every operation that mutates state or returns sensitive output (`SignBlob`, `DecryptBlob`, `GenerateMac`, `RevealSecret`) requires a unique 16-byte nonce. The broker maintains a per-capability nonce-seen window; duplicate nonces within the window are rejected with `NonceReplay`.

## 3. Capability class taxonomy

```proto
enum VaultCapabilityClass {
  VAULT_CAPABILITY_CLASS_UNSPECIFIED = 0;
  KEY_SIGN = 1;             // sign blob with private key; returns signature
  KEY_VERIFY = 2;           // verify signature with public key; returns bool
  KEY_ENCRYPT = 3;          // encrypt blob with public key; returns ciphertext
  KEY_DECRYPT = 4;          // decrypt ciphertext with private key; returns plaintext
  MAC_GENERATE = 5;         // produce HMAC/AEAD MAC; returns mac
  MAC_VERIFY = 6;           // verify MAC; returns bool
  RANDOM_GENERATE = 7;      // CSPRNG bytes; capability needed beyond 64 bytes per call
  SECRET_GET = 8;           // raw bytes — RESTRICTED (see I1, I4)
  BOOTSTRAP_KEY_SIGN = 9;   // first-boot one-shot Ed25519 sign over the firstboot marker (Wave 9)
}
```

Closed enum with **nine** values. Adding a class is a versioned spec change.

| Class                | AI-permitted | Side effect                 | Default budget          | Default rate cap          |
| -------------------- | ------------ | --------------------------- | ----------------------- | ------------------------- |
| `KEY_SIGN`           | yes          | signature returned          | 1 000 ops               | 60 ops/min                |
| `KEY_VERIFY`         | yes          | bool returned               | unlimited (no material) | none                      |
| `KEY_ENCRYPT`        | yes          | ciphertext returned         | 10 000 ops              | 600 ops/min               |
| `KEY_DECRYPT`        | yes          | plaintext returned          | 1 000 ops               | 60 ops/min                |
| `MAC_GENERATE`       | yes          | mac returned                | 10 000 ops              | 600 ops/min               |
| `MAC_VERIFY`         | yes          | bool returned               | unlimited (no material) | none                      |
| `RANDOM_GENERATE`    | yes          | random bytes returned       | by byte count           | 1 MiB / minute            |
| `SECRET_GET`         | **no**       | raw bytes returned          | 1 op (one-shot only)    | 1 op / hour               |
| `BOOTSTRAP_KEY_SIGN` | **no**       | one-shot signature returned | 1 op per host, FOREVER  | 1 op / first-boot session |

The defaults above are **floor** values. A capability binding may set tighter budgets and rate caps; it cannot loosen them. The broker enforces `min(default, requested)` in both fields.

### 3.1 `BOOTSTRAP_KEY_SIGN` exception class (Wave 9)

`BOOTSTRAP_KEY_SIGN` is a **closed-enum constitutional exception** that permits the vault broker to perform a single Ed25519 sign operation against the freshly bootstrapped vault root key **without** an upstream `CapabilityBinding` (S5.1 §9.1). It exists for one purpose only: signing the firstboot marker file at the end of S9.2 first-boot, so the system can later prove that this host completed first-boot.

**Why an exception is required.** The normal `KEY_SIGN` path requires:

1. an `ACTIVE` `CapabilityBinding` per S5.1 §9.1, which requires
2. an `approval_id` per S5.3, which requires
3. a granting subject of kind `HUMAN_USER`.

At S9.2 first-boot, none of these exist: the vault has just been bootstrapped, no approval flow is yet armed, and the first `HUMAN_USER` is registered **after** `STAGE_FIRST_GROUP_REGISTRATION` and after the firstboot marker is signed. Without `BOOTSTRAP_KEY_SIGN`, there is a chicken-and-egg deadlock at first-boot.

**Discipline (mandatory preconditions, all checked atomically).** The broker permits a `BOOTSTRAP_KEY_SIGN` operation **only** when **all** of these hold simultaneously:

1. The invoking subject's canonical id equals `_system:service:firstboot-coordinator` (the constitutional first-boot orchestrator service per S9.2 §4.2.1 — applied in W9-B).
2. The session carries `is_first_boot = true` (per S9.1 W9 `RecoveryMode.FIRST_BOOT` — applied in W9-A).
3. The firstboot marker file at the well-known path (`/aios/system/firstboot/marker.signed`, fixed by S9.2) does **not** exist yet on disk.
4. The per-host `BOOTSTRAP_KEY_SIGN` counter (held in vault broker memory and persisted into the master-key envelope on first-boot completion) has not yet been incremented; **exactly one** call per first-boot session is permitted.
5. The target material is the vault root key generated during first-boot vault bootstrap (`material_kind = ED25519_PRIVATE_KEY`, fingerprint matching the just-bootstrapped vault root).

Any precondition unmet → reject with `BootstrapKeySignNotPermitted`; do not partially sign; emit `VAULT_OPERATION` with `result = failure` and `error_code = bootstrap_key_sign_not_permitted`.

**Evidence on success.** A successful `BOOTSTRAP_KEY_SIGN` invocation emits:

```text
VAULT_BOOTSTRAP_KEY_USED  (FOREVER, queued for S3.1 Wave 10 consolidation)
  fields:
    firstboot_session_id      -- ULID of the first-boot session that issued the call
    signed_payload_digest     -- truncated BLAKE3 (BLAKE3(payload)[:32]) of the firstboot marker payload that was signed
    marker_path               -- "/aios/system/firstboot/marker.signed"
    timestamp
    operator_subject_id       -- "_system:local:operator-1" (the human at the console)
    coordinator_subject_id    -- "_system:service:firstboot-coordinator"
```

The schema is closed (no free-form payload, per §8.7); no signature bytes, no key bytes, no marker bytes.

**Permanent exhaustion after first-boot.** After the firstboot marker is written, the per-host `BOOTSTRAP_KEY_SIGN` counter is permanently set to "exhausted" inside the vault broker's persisted state (sealed under the master-key envelope). Any subsequent `BOOTSTRAP_KEY_SIGN` request on this host — including under a future first-boot session id — is rejected with `BootstrapKeyAlreadyExhausted`. This emits:

```text
BOOTSTRAP_KEY_USE_AFTER_EXHAUST_BLOCKED  (FOREVER, queued for S3.1 Wave 10 consolidation)
  fields:
    attempted_subject_id
    session_id
    timestamp
```

`BOOTSTRAP_KEY_SIGN` is therefore a **per-host, one-shot** capability class. Re-arming it requires a full reset-to-factory (S5.4 + S5.3 reset path) that wipes the vault and re-runs S9.2 first-boot from scratch.

**No AI access.** Like `SECRET_GET`, `BOOTSTRAP_KEY_SIGN` is hard-denied for any subject with `is_ai = true`. The only permitted subject is the constitutional service `_system:service:firstboot-coordinator`, which carries `is_ai = false` per S5.1 §3.

**Cross-reference.** S9.2 §5.4 step 2 (applied in W9-B) is the sole emission point of `BOOTSTRAP_KEY_SIGN` requests. No other spec, service, or path is permitted to invoke this class.

## 4. Material kind taxonomy

```proto
enum VaultMaterialKind {
  VAULT_MATERIAL_KIND_UNSPECIFIED = 0;
  ED25519_PRIVATE_KEY = 1;
  ED25519_PUBLIC_KEY = 2;
  RSA_PRIVATE_KEY = 3;
  X25519_PRIVATE_KEY = 4;
  SYMMETRIC_KEY_AES_256_GCM = 5;
  SYMMETRIC_KEY_CHACHA20_POLY1305 = 6;
  HMAC_KEY_SHA256 = 7;
  MAC_KEY_BLAKE3 = 8;
  PASSWORD_BLOB = 9;
  TOKEN_BLOB = 10;
  CERTIFICATE_PRIVATE_KEY = 11;
}
```

Closed enum. Adding a kind is a versioned spec change.

The mapping between class and acceptable kinds is constitutional:

| Class                | Acceptable `VaultMaterialKind`                                                        |
| -------------------- | ------------------------------------------------------------------------------------- |
| `KEY_SIGN`           | `ED25519_PRIVATE_KEY`, `RSA_PRIVATE_KEY`, `CERTIFICATE_PRIVATE_KEY`                   |
| `KEY_VERIFY`         | `ED25519_PUBLIC_KEY` (public counterpart of stored private key, or imported public)   |
| `KEY_ENCRYPT`        | `X25519_PRIVATE_KEY` (DH agreement), public counterparts; symmetric kinds (AEAD)      |
| `KEY_DECRYPT`        | `X25519_PRIVATE_KEY`, `RSA_PRIVATE_KEY`, `SYMMETRIC_KEY_AES_256_GCM`, `..._CHACHA20`  |
| `MAC_GENERATE`       | `HMAC_KEY_SHA256`, `MAC_KEY_BLAKE3`                                                   |
| `MAC_VERIFY`         | `HMAC_KEY_SHA256`, `MAC_KEY_BLAKE3`                                                   |
| `RANDOM_GENERATE`    | none (no key material; CSPRNG seed handled internally)                                |
| `SECRET_GET`         | `PASSWORD_BLOB`, `TOKEN_BLOB`, `CERTIFICATE_PRIVATE_KEY` (export under recovery only) |
| `BOOTSTRAP_KEY_SIGN` | `ED25519_PRIVATE_KEY` only (the freshly bootstrapped vault root key)                  |

Mismatched class/kind pairs are rejected at capability issuance with `CapabilityClassKindMismatch`. `BOOTSTRAP_KEY_SIGN` is not issued through `IssueCapability` at all (it has no `CapabilityBinding`); the class/kind constraint is enforced inside the broker's first-boot path per §3.1.

## 5. Capability lifecycle FSM

```text
                  (issuance request +
                   approval verified)
DRAFT  ───────────────────────────────►  ACTIVE
  │                                        │
  │ (issuance rejected)                    ├──► (expires_at reached)              ──► EXPIRED
  ▼                                        │
DISCARDED                                  ├──► (usage_budget exhausted)          ──► EXPIRED
                                           │
                                           ├──► (RevokeCapability called)         ──► REVOKED
                                           │
                                           ├──► (RotateCapability called)         ──► ROTATED
                                           │
                                           └──► (identity_bundle_version rolled)  ──► REVOKED
                                                                                       (with reason = bundle_rollover)
```

State transitions:

- **DRAFT → ACTIVE.** Issuance succeeded; binding signed; emits `VAULT_CAPABILITY_ISSUED` (STANDARD_24M).
- **DRAFT → DISCARDED.** Issuance rejected (mismatched class/kind, missing approval, revoked-subject). No evidence record beyond the policy-decision evidence on the issuance action itself.
- **ACTIVE → EXPIRED.** Either time-based (`expires_at`) or budget-based (`usage_budget` exhausted). No emitted lifecycle record (high volume, low risk; queryable by absence on next use).
- **ACTIVE → REVOKED.** `RevokeCapability` RPC, or implicit via bundle rollover (S5.1 §8). Emits `VAULT_CAPABILITY_REVOKED` (EXTENDED_60M).
- **ACTIVE → ROTATED.** `RotateCapability` generates new material; the `capability_id` stays the same; in-flight operations against the old material complete; new operations use new material. Emits `VAULT_CAPABILITY_ROTATED` (STANDARD_24M).

A capability in `EXPIRED`, `REVOKED`, or `ROTATED` state may not be exercised. `ROTATED` is distinct from `REVOKED` because the binding remains usable — only the underlying material has changed.

## 6. Use-without-reveal operation surface

Per `VaultCapabilityClass`, the operations the broker exposes:

### 6.1 `SignBlob`

```
SignBlob(capability_id, blob, nonce) → { signature, sign_metadata }
```

- Verifies `capability.state = ACTIVE`, `capability.class = KEY_SIGN`.
- Verifies session matches binding scope `(subject, group_id, bundle_version)`.
- Verifies `nonce` is unseen within the per-capability replay window.
- Updates usage budget; checks rate cap.
- Performs signature internally; returns signature bytes.
- Emits `VAULT_OPERATION` with `class = KEY_SIGN`, `result = success | failure`, `error_code`. Never emits the blob, never emits the signature.

### 6.2 `VerifyBlob`

```
VerifyBlob(capability_id, blob, signature) → { valid: bool, verify_metadata }
```

- `KEY_VERIFY` is the lone class permitted on `..._PUBLIC_KEY` material; an `ED25519_PRIVATE_KEY` capability cannot be used here (a separate public-key capability is required even when both private and public live under the same key pair). This is a defense-in-depth boundary.
- No nonce required (idempotent, no material returned).
- Emits `VAULT_OPERATION` with `class = KEY_VERIFY`.

### 6.3 `EncryptBlob` / `DecryptBlob`

```
EncryptBlob(capability_id, plaintext, nonce) → { ciphertext, encrypt_metadata }
DecryptBlob(capability_id, ciphertext, nonce) → { plaintext, decrypt_metadata }
```

- Decrypt is the highest-risk class for AI subjects (oracle attacks); rate cap is enforced strictly.
- For symmetric AEAD modes, the broker generates the nonce internally and includes it in `..._metadata` (the `nonce` argument is anti-replay against the broker, not the cipher nonce).
- Emits `VAULT_OPERATION`. Never emits plaintext, ciphertext, or nonce.

### 6.4 `GenerateMac` / `VerifyMac`

```
GenerateMac(capability_id, blob) → { mac }
VerifyMac(capability_id, blob, mac) → { valid: bool }
```

- Same shape as sign/verify but for MAC keys.
- `VerifyMac` is constant-time at the broker boundary regardless of internal implementation; the broker mandates constant-time return path.

### 6.5 `GenerateRandom`

```
GenerateRandom(byte_count) → { random_bytes }
```

- For `byte_count ≤ 64`, no capability is required (this is a system service).
- For `byte_count > 64`, a `RANDOM_GENERATE` capability is required, scoped to the session.
- Source: kernel CSPRNG (`getrandom(2)` on Linux) wrapped by broker; never an LLM-derived sequence, never a process-internal RNG seeded once.
- Emits `VAULT_OPERATION` with `class = RANDOM_GENERATE`. Never emits the random bytes.

### 6.6 `RevealSecret` (RESTRICTED)

```
RevealSecret(capability_id, co_signer_approval_id) → { raw_bytes }
```

Mandatory preconditions, all checked atomically:

- `subject.kind = HUMAN_USER`
- `session.session_class = STRONG`
- `session.recovery_mode = true`
- `co_signer_approval_id` references a valid, unused `Approval` record with a different `subject_canonical_id` from the requester (human co-signer) — full mechanics in [S5.3 Approval Mechanics](04_approval_mechanics.md)
- `capability.class = SECRET_GET` and `capability.is_one_shot = true`
- `capability` has not been used previously

On success:

- Returns raw bytes.
- Emits `VAULT_RAW_REVEAL` (FOREVER).
- Capability moves to `REVOKED` immediately (one-shot).

Any precondition unmet:

- Reject with the most specific error code; do not partially reveal.
- Emit `VAULT_RAW_REVEAL_REJECTED` as a `VAULT_OPERATION` with `result = failure` and `error_code` set; do not emit `VAULT_RAW_REVEAL` (which by name implies a successful reveal).

### 6.7 Common operation contract

For every operation in §6.1–§6.6:

1. Authenticate the call (session signature per S5.1 §11).
2. Resolve `capability_id`; reject if not `ACTIVE`.
3. Verify scope `(subject, group_id, bundle_version)`.
4. Apply the AI hard-deny check from I1 if `class = SECRET_GET`.
5. Verify nonce uniqueness (where applicable).
6. Check rate cap and usage budget.
7. Perform the operation in constant time where applicable.
8. Update budget counters.
9. Emit redacted evidence (§14).
10. Return result.

Any failure at steps 1–6 short-circuits **before** the cryptographic operation; the broker never partially executes.

## 7. Performance contract

| Operation                             | p50      | p95      | p99      | Hard timeout |
| ------------------------------------- | -------- | -------- | -------- | ------------ |
| `IssueCapability`                     | < 20 ms  | < 100 ms | < 300 ms | 2 s          |
| `RotateCapability`                    | < 50 ms  | < 200 ms | < 1 s    | 5 s          |
| `RevokeCapability`                    | < 5 ms   | < 50 ms  | < 200 ms | 1 s          |
| `SignBlob` (Ed25519, ≤ 4 KiB)         | < 1 ms   | < 5 ms   | < 20 ms  | 200 ms       |
| `VerifyBlob` (Ed25519)                | < 1 ms   | < 5 ms   | < 20 ms  | 200 ms       |
| `EncryptBlob` (AES-256-GCM, ≤ 1 MiB)  | < 10 ms  | < 50 ms  | < 150 ms | 1 s          |
| `DecryptBlob` (AES-256-GCM, ≤ 1 MiB)  | < 10 ms  | < 50 ms  | < 150 ms | 1 s          |
| `GenerateMac` (HMAC-SHA-256, ≤ 4 KiB) | < 500 µs | < 2 ms   | < 10 ms  | 200 ms       |
| `VerifyMac`                           | < 500 µs | < 2 ms   | < 10 ms  | 200 ms       |
| `GenerateRandom` (≤ 4 KiB)            | < 500 µs | < 2 ms   | < 10 ms  | 200 ms       |
| `RevealSecret`                        | < 100 ms | < 500 ms | < 2 s    | 10 s         |

`RevealSecret` is intentionally slow: it is rate-limited at the broker layer (1 op/hour default per capability) **and** carries a soft minimum latency of 100 ms to prevent timing-based brute force on co-signer approval identifiers.

Failure modes — all fail closed:

- `VaultBrokerInternal` → caller receives error; engine emits alert.
- `MasterKeyUnavailable` (boot before unlock, or unlock failure) → all operations fail with `VaultLocked`; new capabilities cannot be issued; existing capabilities cannot be exercised.
- `MaterialNotFound` (capability references material that has been wiped) → `MaterialUnavailable`; capability auto-revoked; emits `VAULT_CAPABILITY_REVOKED` with reason `material_unavailable`.

## 8. Adversarial robustness

### 8.1 AI subject attempts `SECRET_GET`

A subject with `is_ai = true` requests `RevealSecret` with a forged or legitimate capability id.

- Check at step 4 of §6.7: `is_ai = true` && `class = SECRET_GET` → reject with `SubjectKindRejectedForVault`.
- Emit `SUBJECT_KIND_REJECTED_FOR_VAULT` (FOREVER).
- The capability is **not** revoked (it may still be exercisable by a HUMAN_USER, though one-shot enforcement applies).

### 8.2 Forged capability id

Capability records carry an Ed25519 signature by the broker over `(capability_id, subject, group_id, class, material_kind, granted_at, expires_at)`. A forged or tampered id fails signature verification at use.

- Reject with `CapabilitySignatureInvalid`.
- Emit `VAULT_CAPABILITY_FORGERY` (FOREVER).
- Forwarded to S2.4 audit pipeline; repeated forgeries against the same subject trigger session lockout.

### 8.3 Replay attack

An attacker captures a legitimate `SignBlob(cap_id, blob, nonce)` request and replays it.

- Step 5 of §6.7: nonce uniqueness check fails.
- Reject with `NonceReplay`.
- Emit `VAULT_OPERATION` with `result = failure`, `error_code = nonce_replay`. Repeated replays from the same session escalate to S2.4 anomaly detector.

### 8.4 Rate-limit bypass via session juggling

An attacker with a valid capability tries to exceed rate caps by opening many sessions for the same `(subject, group_id)` pair.

- Rate caps are tracked **per capability_id**, not per session. Session count does not loosen the cap.
- Budget exhaustion moves the capability to `EXPIRED` regardless of session count.

### 8.5 Side-channel timing on Decrypt / Sign

Timing attacks on cryptographic primitives.

- The broker mandates that all primitives used (Ed25519 sign/verify, X25519 DH, AES-256-GCM, ChaCha20-Poly1305, HMAC, BLAKE3) have constant-time implementations under their default deployment libraries.
- Where a primitive cannot be made constant-time (e.g. older RSA implementations), the broker adds a fixed minimum latency floor (5 ms) on the operation to mask variance. This is configured per `VaultMaterialKind` × `VaultCapabilityClass` pair; the configuration is part of the deployment guide, not this spec.

### 8.6 Master-key extraction

An attacker compromises a process and tries to read the master key from broker memory.

- The vault broker runs as a privileged, isolated process with `mlock(2)` on master-key memory (no swap), `prctl(PR_SET_DUMPABLE, 0)` (no core dumps), and `seccomp` filters limiting syscalls (S3.2 sandbox profile).
- The master key never leaves the broker process address space; not in environment variables, not in files at runtime, not in evidence.
- IPC with the broker uses gRPC over unix-domain socket with peer credential check; only callers whose peer pid maps to a constitutional service or to a session-authenticated agent are accepted.

### 8.7 Evidence injection

A compromised agent emits an evidence record claiming to be a `VAULT_OPERATION` and embeds raw secret bytes in a payload field.

- Evidence schema for `VAULT_OPERATION` (queued for S3.1 §14) restricts the payload to closed fields: `class`, `material_kind`, `result`, `error_code`, `subject_canonical_id`, `capability_id_hash` (truncated BLAKE3, `BLAKE3(JCS(capability))[:32]`), `nonce_hash` (truncated BLAKE3, `BLAKE3(nonce)[:32]`), `byte_count_in`, `byte_count_out`, `latency_us`, `timestamp`. **No free-form payload.**
- An evidence record with extra fields fails schema validation at the evidence broker (S3.1) and is rejected.
- An evidence record claiming `kind = VAULT_RAW_REVEAL` from any producer **other** than the vault broker itself is rejected with `EvidenceProducerNotAuthorized` (S3.1 §X — to be added).

### 8.8 Capability lifetime extension

An attacker with capability binding tries to extend `expires_at`.

- `expires_at` is bound at issuance in the signed `VaultCapability` record. Mutation requires a new issuance (new `capability_id`).
- An issuance request that names an existing `capability_id` fails with `CapabilityIdAlreadyExists`.

## 9. Capability rotation and revocation

### 9.1 Rotation

```
RotateCapability(capability_id) → { rotated_at, new_material_fingerprint }
```

- Requires `capability.class ∈ { KEY_SIGN, KEY_DECRYPT, KEY_ENCRYPT, MAC_GENERATE, ... }` (any class with material under it; not `KEY_VERIFY` / `MAC_VERIFY` which point at public/foreign material).
- Atomic: new material is generated, the binding's material pointer flips, in-flight operations (operations whose request was accepted but not yet completed) finish against the **old** material; new operations use the **new** material.
- Old material is wiped after a short grace window (default: 60 seconds; configurable per material kind).
- `capability_id` is unchanged; usage budget and rate cap are reset.
- Emits `VAULT_CAPABILITY_ROTATED` (STANDARD_24M) with `rotation_reason` ∈ closed enum `{ scheduled, manual, suspected_compromise, key_age }`.

### 9.2 Revocation

```
RevokeCapability(capability_id, reason) → { revoked_at }
```

- `reason` ∈ closed enum `{ user_request, admin_request, suspected_compromise, bundle_rollover, material_unavailable, expired_by_budget, audit_flag }`.
- Capability moves to `REVOKED` immediately. In-flight operations fail with `CapabilityRevoked` mid-flight.
- Underlying material is wiped if no other capability references it.
- Emits `VAULT_CAPABILITY_REVOKED` (EXTENDED_60M).

### 9.3 Bundle rollover invalidation

Per S5.1 §8 / §14.4, when `identity_bundle_version` changes, all capabilities bound to the prior version are invalidated. Mechanically:

- The broker re-validates each in-use capability's `identity_bundle_version` against the active version at every operation (cheap pointer comparison).
- A version mismatch transitions the capability to `REVOKED` with `reason = bundle_rollover` and emits `VAULT_CAPABILITY_REVOKED`.
- Operators can pre-emptively re-issue capabilities under the new bundle before mass invalidation by overlapping the rollover window (deployment guidance, not spec).

## 10. Recovery-mode special case

### 10.1 Recovery snapshot

The vault broker maintains a recovery snapshot at `/aios/system/recovery/vault-snapshot`, encrypted under a separate **recovery master key** (escrowed in a sealed envelope held by the operator or a hardware token). The snapshot is updated only at recovery rehearsal and after explicit operator action; it is **not** continuously synced from the live vault.

Normal-mode operation never reads the recovery snapshot. The recovery snapshot lives outside the vault broker's normal-mode address space.

### 10.2 Recovery operations

Under `recovery_mode = true` session (per S5.1 §7):

- A `REMOTE_OPERATOR` or recovery `HUMAN_USER` can `RevealSecret` against capabilities that exist in the recovery snapshot (subject to I4 preconditions).
- The broker can re-key its master key (`Rekey` RPC). The previous master key is retired; existing capabilities are re-encrypted under the new master key. This emits `VAULT_RECOVERY_SNAPSHOT_LOADED` (FOREVER) and `VAULT_REKEYED` (FOREVER, queued for S3.1 follow-up).
- New capabilities can be issued under the recovery operator's identity; these capabilities carry a `provenance = recovery` flag so audit can distinguish recovery-originated capabilities from normal-mode ones.

### 10.3 Recovery → normal transition

Exiting recovery mode is exit-by-reboot (per S5.1 §7.2). The broker's normal-mode startup loads the active master key from boot-time unlock; the recovery snapshot is not retained in memory.

## 11. Cross-spec dependencies

| Spec                                        | Direction  | What this spec contributes / consumes                                                                                                                                                                                                                                                                                                                                  |
| ------------------------------------------- | ---------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| S0.1                                        | consumer   | Action envelope `target.vault_capability_id` references this spec's `capability_id`; broker is the executor for actions naming a capability                                                                                                                                                                                                                            |
| S2.3                                        | consumer   | Policy kernel constraint `vault_capability_required` (S2.3 §10 / §11) names a `vault_capability_id`; policy decision must precede use                                                                                                                                                                                                                                  |
| S2.4                                        | producer   | `VAULT_NO_RAW_SECRET_LEAK` PropertyType promoted in S2.4 Wave 10 §21.1.3 (ID 25) via composition of existing `policy.decision` + `evidence.exists` primitives — no dedicated vault primitive needed (Wave 16 truthful posture; the original "new audit primitive `vault_no_raw_secret_in_recent_evidence`" claim is superseded by the existing-primitive composition). |
| S3.1                                        | producer   | 10 record types queued for next S3.1 refinement (see §14): 8 from initial contract + 2 added by Wave 9 (`VAULT_BOOTSTRAP_KEY_USED`, `BOOTSTRAP_KEY_USE_AFTER_EXHAUST_BLOCKED`) for S3.1 Wave 10 consolidation                                                                                                                                                          |
| S9.1 (Wave 9, applied)                      | consumer   | `RecoveryMode.FIRST_BOOT` is the session flag (`is_first_boot = true`) that gates `BOOTSTRAP_KEY_SIGN` per §3.1                                                                                                                                                                                                                                                        |
| S9.2 (Wave 9, applied)                      | consumer   | §5.4 step 2 is the sole emission point of `BOOTSTRAP_KEY_SIGN`; §3.2.8 names the constitutional first-boot service subjects                                                                                                                                                                                                                                            |
| S3.2                                        | consumer   | Vault broker process runs under a constitutional sandbox profile (privileged, mlock, no core dumps, restricted syscalls)                                                                                                                                                                                                                                               |
| S5.1                                        | consumer   | `CapabilityBinding` skeleton (S5.1 §9) provides the binding scope `(subject, group_id, bundle_version)`; broker enforces at use                                                                                                                                                                                                                                        |
| S5.4 (`04_approval_mechanics.md`, deferred) | constraint | `RevealSecret` co-signer is an `Approval` record per S5.4 mechanics                                                                                                                                                                                                                                                                                                    |
| L0 INV-003                                  | binds      | "Secrets are capabilities" — implementation is this spec's I1, I4, §6                                                                                                                                                                                                                                                                                                  |
| L0 INV-015                                  | binds      | "Evidence never contains secrets" — enforced by §3 redacted projection and §8.7 schema validation                                                                                                                                                                                                                                                                      |
| L0 INV-018                                  | binds      | "Vault never leaks raw secrets" — implementation is this spec's I1, I4, §6.6                                                                                                                                                                                                                                                                                           |

## 12. Golden fixtures

### Fixture 1 — AI agent signs document via KEY_SIGN

```text
Setup:
  subject  = "family:family-assistant" (kind=AI_AGENT, is_ai=true)
  capability cap_signdoc bound to subject, group=family, class=KEY_SIGN,
    material=ED25519_PRIVATE_KEY, state=ACTIVE
  blob = <document bytes>
  nonce = <unseen 16 bytes>

SignBlob(cap_signdoc, blob, nonce):
  Steps 1-6 pass; cryptographic sign performed.
  Returns: { signature = <64-byte Ed25519 signature>, sign_metadata = {...} }
  Emits: VAULT_OPERATION{
    class = KEY_SIGN,
    material_kind = ED25519_PRIVATE_KEY,
    result = success,
    capability_id_hash = <truncated BLAKE3, BLAKE3(JCS(capability))[:32]>,
    byte_count_in = len(blob),
    byte_count_out = 64,
    latency_us = ...
  }
  No raw key material in evidence. No blob in evidence. No signature in evidence.
```

### Fixture 2 — AI agent attempts SECRET_GET

```text
Setup:
  subject = "family:family-assistant" (kind=AI_AGENT, is_ai=true)
  capability cap_pwd: (forged or legitimate; doesn't matter) class=SECRET_GET

RevealSecret(cap_pwd, co_signer_approval_id=...):
  Step 4 of §6.7: is_ai && class=SECRET_GET → REJECT.
  No cryptographic operation performed. No raw bytes returned.
  Error: SubjectKindRejectedForVault.
  Emits: SUBJECT_KIND_REJECTED_FOR_VAULT (FOREVER).
```

### Fixture 3 — Human user reveals password under recovery

```text
Setup:
  subject = "_system:local:operator-247" (kind=REMOTE_OPERATOR via recovery boot)
  session: session_class=STRONG, recovery_mode=true, is_ai=false
  capability cap_revealpwd: class=SECRET_GET, material=PASSWORD_BLOB,
    is_one_shot=true, state=ACTIVE, provenance=recovery
  co_signer_approval_id = appr_<ulid> (signed by a different human, fresh)

RevealSecret(cap_revealpwd, co_signer_approval_id):
  All preconditions pass.
  Returns: { raw_bytes = <password bytes> }
  Capability transitions ACTIVE → REVOKED (one-shot exhaustion).
  Emits: VAULT_RAW_REVEAL (FOREVER) with subject, capability_id_hash, co_signer_subject_id, timestamp.
  No password bytes in evidence.
```

### Fixture 4 — Capability bound to wrong group

```text
Setup:
  alice memberships = [family, homelab]; primary = family
  capability cap_homehub bound to (subject="family:alice", group_id="family", class=KEY_SIGN)
  alice switches primary group to homelab → new session as "homelab:alice"

SignBlob(cap_homehub, blob, nonce) under "homelab:alice" session:
  Step 3 of §6.7: scope check fails (binding.group_id=family ≠ session.primary=homelab).
  REJECT: CapabilityNotActiveInGroup.
  Emits: VAULT_OPERATION{ result=failure, error_code=capability_not_active_in_group }.
```

### Fixture 5 — Replay attack with same nonce

```text
Setup:
  capability cap_X (KEY_SIGN, ACTIVE)
  attacker captures legitimate SignBlob(cap_X, blob, nonce_n)

First call SignBlob(cap_X, blob, nonce_n): success.
Replay SignBlob(cap_X, blob, nonce_n):
  Step 5: nonce_n already in replay window → REJECT NonceReplay.
  Emits: VAULT_OPERATION{ result=failure, error_code=nonce_replay }.
  No cryptographic operation performed.
```

### Fixture 6 — Rotation with in-flight signature

```text
Setup:
  capability cap_R (KEY_SIGN, material=ED25519, ACTIVE)

T0: SignBlob(cap_R, blob_1, nonce_1) accepted; operation queued.
T0+1ms: RotateCapability(cap_R) called.
T0+2ms: cap_R material atomically swapped to new key; old key in 60-second grace window.
T0+3ms: Operation from T0 completes against OLD key; signature returned.
T0+5ms: SignBlob(cap_R, blob_2, nonce_2) accepted; signed under NEW key.

Emits:
  VAULT_OPERATION (T0): result=success, key fingerprint = old.
  VAULT_CAPABILITY_ROTATED (T0+1ms): rotation_reason=manual, old/new fingerprints.
  VAULT_OPERATION (T0+5ms): result=success, key fingerprint = new.
T0+60s: old key wiped from memory.
```

### Fixture 7 — Bundle rollover invalidates all old capabilities

```text
Setup:
  identity_bundle_version = idbundle_A
  capability cap_a, cap_b, cap_c bound under idbundle_A (ACTIVE)

Bundle rollover: idbundle_A → idbundle_B.

First SignBlob(cap_a, ...) after rollover:
  Re-validate step: cap_a.identity_bundle_version=A ≠ active=B.
  Capability transitions ACTIVE → REVOKED with reason=bundle_rollover.
  Operation REJECT: CapabilityRevoked.
  Emits: VAULT_CAPABILITY_REVOKED (EXTENDED_60M) with reason=bundle_rollover.

Same outcome on first use of cap_b, cap_c.
```

### Fixture 8 — Forged capability signature

```text
Attacker forges cap_id "cap_FORGED" with plausible fields but signs with attacker key.

SignBlob(cap_FORGED, blob, nonce):
  Step 2 of §6.7: capability lookup → found in attacker-supplied request,
    but signature verification under broker public key fails.
  REJECT: CapabilitySignatureInvalid.
  Emits: VAULT_CAPABILITY_FORGERY (FOREVER) with subject, attempted_cap_id_hash,
    session_id_hash. S2.4 anomaly detector invoked.
```

### Fixture 9 — First-boot bootstrap key sign (Wave 9)

```text
Setup (S9.2 first-boot, post vault bootstrap, pre firstboot marker):
  subject  = "_system:service:firstboot-coordinator" (kind=SERVICE, is_ai=false)
  session  = first-boot session, is_first_boot = true
  vault root key just generated, ED25519_PRIVATE_KEY
  marker_path = "/aios/system/firstboot/marker.signed" — does not exist
  per-host BOOTSTRAP_KEY_SIGN counter = 0 (not yet exhausted)
  payload  = <firstboot marker payload bytes>

Broker first-boot path invokes BOOTSTRAP_KEY_SIGN over payload:
  Preconditions §3.1 (1)-(5) all hold → admit.
  Sign performed against vault root key.
  Marker file written; per-host counter incremented to "exhausted".
  Emits: VAULT_BOOTSTRAP_KEY_USED (FOREVER, queued for S3.1 W10) with
    firstboot_session_id, signed_payload_digest=BLAKE3(payload)[:32],
    marker_path, operator_subject_id, coordinator_subject_id.
  No signature bytes in evidence; no key bytes in evidence.

Subsequent attempt (e.g. attacker reboots into recovery and tries to forge a marker):
  BOOTSTRAP_KEY_SIGN(...) invoked again on this host.
  Precondition (3) fails: marker already exists.
  Precondition (4) fails: per-host counter already exhausted.
  REJECT: BootstrapKeyAlreadyExhausted.
  Emits: BOOTSTRAP_KEY_USE_AFTER_EXHAUST_BLOCKED (FOREVER, queued for S3.1 W10).
  No cryptographic operation performed.
```

### Fixture 10 — AI agent attempts BOOTSTRAP_KEY_SIGN (Wave 9)

```text
Setup:
  attacker has compromised an AI_AGENT subject "family:family-assistant"
    (is_ai=true) and crafts a request masquerading as the firstboot path.

Broker receives BOOTSTRAP_KEY_SIGN-class request from this subject:
  is_ai = true → hard-deny at request entry point (mirrors I1).
  Precondition (1) also fails (subject ≠ _system:service:firstboot-coordinator).
  REJECT: SubjectKindRejectedForVault.
  Emits: SUBJECT_KIND_REJECTED_FOR_VAULT (FOREVER) with class=BOOTSTRAP_KEY_SIGN.
  No cryptographic operation performed.
```

## 13. Telemetry contract

All metrics MUST use bounded label cardinality. **`capability_id`, `subject_canonical_id`, `group_id`, `session_id`, `material_fingerprint` are NEVER labels.**

| Metric                                                | Type      | Labels (closed)                                                             |
| ----------------------------------------------------- | --------- | --------------------------------------------------------------------------- |
| `vault_operation_total`                               | counter   | `class` (closed enum), `result` (success/error), `error_code` (closed enum) |
| `vault_operation_duration_seconds`                    | histogram | `class`, `result`                                                           |
| `vault_capability_issued_total`                       | counter   | `class`, `material_kind` (closed enum)                                      |
| `vault_capability_revoked_total`                      | counter   | `reason` (closed enum)                                                      |
| `vault_capability_rotated_total`                      | counter   | `rotation_reason` (closed enum)                                             |
| `vault_active_capabilities`                           | gauge     | `class`                                                                     |
| `vault_raw_reveal_total`                              | counter   | `result` (success/rejected), `error_code`                                   |
| `vault_subject_kind_rejected_total`                   | counter   | `kind` (closed enum), `class`                                               |
| `vault_capability_forgery_total`                      | counter   | none                                                                        |
| `vault_nonce_replay_total`                            | counter   | `class`                                                                     |
| `vault_rate_cap_exceeded_total`                       | counter   | `class`                                                                     |
| `vault_master_key_state`                              | gauge     | `state` ∈ {locked, unlocked, rekey_in_progress}                             |
| `vault_recovery_snapshot_loaded_total`                | counter   | none                                                                        |
| `vault_bundle_rollover_invalidations_total`           | counter   | none                                                                        |
| `vault_bootstrap_key_used_total`                      | counter   | none (one-shot per host; expected count is 0 or 1)                          |
| `vault_bootstrap_key_use_after_exhaust_blocked_total` | counter   | none (any non-zero value indicates an attempt after first-boot exhaustion)  |

Cardinality budget: ≤ 100 active label tuples per metric. The closed enums together produce fewer than 80 distinct tuples across all metrics.

## 14. Evidence record types queued for S3.1 follow-up

This spec queues 10 new record types for the next S3.1 refinement to add to the closed `RecordType` vocabulary (8 from the original S5.2 contract + 2 added by Wave 9 for the `BOOTSTRAP_KEY_SIGN` exception class). The two Wave 9 additions are queued for **S3.1 Wave 10 consolidation** specifically (alongside other Wave 9 first-boot record types from W9-A and W9-B). Each row names the type, its retention class (per S3.1 retention enum), and its emission point.

| RecordType                                | Retention class | Emitted by               | When                                                                                                                          |
| ----------------------------------------- | --------------- | ------------------------ | ----------------------------------------------------------------------------------------------------------------------------- |
| `VAULT_CAPABILITY_ISSUED`                 | `STANDARD_24M`  | `IssueCapability`        | Successful capability issuance (DRAFT → ACTIVE).                                                                              |
| `VAULT_CAPABILITY_ROTATED`                | `STANDARD_24M`  | `RotateCapability`       | Successful rotation (material swap).                                                                                          |
| `VAULT_CAPABILITY_REVOKED`                | `EXTENDED_60M`  | `RevokeCapability`       | Revocation, including auto-revoke on bundle rollover or material loss.                                                        |
| `VAULT_OPERATION`                         | `STANDARD_24M`  | All operations §6.1–§6.5 | Every operation attempt, success or failure (high volume; redacted projection).                                               |
| `VAULT_RAW_REVEAL`                        | `FOREVER`       | `RevealSecret`           | **Successful** raw-bytes reveal under recovery + co-signer.                                                                   |
| `VAULT_CAPABILITY_FORGERY`                | `FOREVER`       | All operations           | Capability signature verification failure (§8.2).                                                                             |
| `SUBJECT_KIND_REJECTED_FOR_VAULT`         | `FOREVER`       | All operations           | AI subject attempts a class disallowed by I1 (§8.1).                                                                          |
| `VAULT_RECOVERY_SNAPSHOT_LOADED`          | `FOREVER`       | Recovery boot path       | Recovery snapshot opened (§10.1).                                                                                             |
| `VAULT_BOOTSTRAP_KEY_USED`                | `FOREVER`       | First-boot path (§3.1)   | **Successful** one-shot `BOOTSTRAP_KEY_SIGN` over the firstboot marker. (Wave 9; queued for S3.1 Wave 10)                     |
| `BOOTSTRAP_KEY_USE_AFTER_EXHAUST_BLOCKED` | `FOREVER`       | First-boot path (§3.1)   | Any attempt to invoke `BOOTSTRAP_KEY_SIGN` on a host whose firstboot marker already exists. (Wave 9; queued for S3.1 Wave 10) |

A future-S3.1 follow-up may add `VAULT_REKEYED` (FOREVER, recovery only) when the recovery rekey mechanic is refined; this spec lists it as anticipated but does not yet queue it formally.

The schema for each RecordType restricts the payload to closed fields (per §8.7). No free-form payload is permitted; the evidence broker (S3.1) rejects records whose payloads contain unexpected fields.

## 15. Acceptance criteria

- [ ] `VaultCapabilityClass` is a closed enum with **nine** values (Wave 9 added `BOOTSTRAP_KEY_SIGN`); adding a value requires a versioned spec change.
- [ ] `VaultMaterialKind` is a closed enum with eleven values; adding a value requires a versioned spec change.
- [ ] AI subjects (`is_ai = true`) are rejected from `SECRET_GET` and `BOOTSTRAP_KEY_SIGN` at the request entry point regardless of capability state; emits `SUBJECT_KIND_REJECTED_FOR_VAULT` (FOREVER).
- [ ] `BOOTSTRAP_KEY_SIGN` (§3.1) is admitted only when subject = `_system:service:firstboot-coordinator`, session.is_first_boot = true, the firstboot marker does not yet exist, and the per-host counter is not yet exhausted; emits `VAULT_BOOTSTRAP_KEY_USED` (FOREVER) on success.
- [ ] After the firstboot marker is written, any further `BOOTSTRAP_KEY_SIGN` request on this host is rejected with `BootstrapKeyAlreadyExhausted` and emits `BOOTSTRAP_KEY_USE_AFTER_EXHAUST_BLOCKED` (FOREVER).
- [ ] Capability bindings are scoped to `(subject_canonical_id, group_id, identity_bundle_version)`; broker re-validates on every operation.
- [ ] Bundle rollover invalidates all bindings tied to the prior version; emits `VAULT_CAPABILITY_REVOKED` with `reason = bundle_rollover`.
- [ ] `RevealSecret` succeeds only under `kind = HUMAN_USER` + `session_class = STRONG` + `recovery_mode = true` + valid distinct human co-signer + one-shot capability; emits `VAULT_RAW_REVEAL` (FOREVER).
- [ ] Every operation that returns sensitive output requires a unique 16-byte nonce; replays are rejected with `NonceReplay`.
- [ ] Per-capability usage budget and rate cap are enforced across sessions; bypass attempts are detected and counted.
- [ ] Capability records are signed by the broker; forged or tampered ids fail signature verification at use; emits `VAULT_CAPABILITY_FORGERY` (FOREVER).
- [ ] Capability rotation is atomic with a bounded grace window for in-flight operations; old material is wiped after the grace window.
- [ ] Master key never leaves the broker process address space; broker process runs `mlock`-pinned, non-dumpable, syscall-restricted.
- [ ] All evidence records emitted by the broker conform to closed-field schemas; no free-form payload; no raw bytes, plaintext, or signatures.
- [ ] All ten golden fixtures (§12) produce the specified outcomes (Wave 9 added Fixtures 9–10 for `BOOTSTRAP_KEY_SIGN`).
- [ ] Telemetry conforms to §13 cardinality bounds; capability/subject/group/session ids never appear as labels.
- [ ] Performance p95s in §7 are met under the deployment guide's reference hardware.

## 16. Open deferrals

These are intentionally out of scope for S5.2 and tracked elsewhere:

- **Distributed vault for multi-host clusters.** Multi-host AIOS deployments need vault replication/quorum; deferred.
- **Hardware Security Module (HSM) integration.** PKCS#11, TPM-bound key storage, secure-element key generation; deferred. The broker contract permits HSM as a backing store with no spec change to the gRPC surface.
- **Post-quantum algorithms.** Kyber (KEM), Dilithium / Falcon (signatures), SPHINCS+ — deferred to a versioned spec extension. The closed `VaultMaterialKind` enum will gain entries when these are adopted.
- **Threshold signatures.** N-of-M signing schemes for high-value keys; deferred.
- **Multi-party computation (MPC).** MPC-based key operations; deferred.
- **Per-tenant vault isolation.** Per-tenant encryption boundary independent of group identity; deferred per S4.1 Q1 default (no `tenants/` namespace in Rev.2).
- **Vault audit compaction.** `VAULT_OPERATION` is high-volume; an audit-side compaction strategy (per-day aggregates after 24h) is deferred to S3.1's retention layer.
- **Reveal-to-human delegation.** A human co-signer must be a distinct human; multi-human approval chains (e.g. 2-of-3) are deferred to S5.4 (approval mechanics).
- **Cross-vault capability migration.** Moving capabilities between vault instances during host migration; deferred.
- **Hardware-attested capability bindings.** Binding a capability to a TPM-attested device id; deferred to L8 hardware integration.

## Appendix A — Full Proto IDL

```proto
syntax = "proto3";
package aios.vault.v1alpha1;

import "google/protobuf/timestamp.proto";

// ============================================================================
// Service
// ============================================================================

service VaultBroker {
  // Capability lifecycle
  rpc IssueCapability(IssueCapabilityRequest) returns (IssueCapabilityResponse);
  rpc RotateCapability(RotateCapabilityRequest) returns (RotateCapabilityResponse);
  rpc RevokeCapability(RevokeCapabilityRequest) returns (RevokeCapabilityResponse);
  rpc GetCapability(GetCapabilityRequest) returns (GetCapabilityResponse);

  // Use-without-reveal operations
  rpc SignBlob(SignBlobRequest) returns (SignBlobResponse);
  rpc VerifyBlob(VerifyBlobRequest) returns (VerifyBlobResponse);
  rpc EncryptBlob(EncryptBlobRequest) returns (EncryptBlobResponse);
  rpc DecryptBlob(DecryptBlobRequest) returns (DecryptBlobResponse);
  rpc GenerateMac(GenerateMacRequest) returns (GenerateMacResponse);
  rpc VerifyMac(VerifyMacRequest) returns (VerifyMacResponse);
  rpc GenerateRandom(GenerateRandomRequest) returns (GenerateRandomResponse);

  // Restricted reveal path
  rpc RevealSecret(RevealSecretRequest) returns (RevealSecretResponse);

  // Engine info
  rpc GetVaultInfo(GetVaultInfoRequest) returns (GetVaultInfoResponse);
}

// ============================================================================
// Core enums
// ============================================================================

enum VaultCapabilityClass {
  VAULT_CAPABILITY_CLASS_UNSPECIFIED = 0;
  KEY_SIGN = 1;
  KEY_VERIFY = 2;
  KEY_ENCRYPT = 3;
  KEY_DECRYPT = 4;
  MAC_GENERATE = 5;
  MAC_VERIFY = 6;
  RANDOM_GENERATE = 7;
  SECRET_GET = 8;
  BOOTSTRAP_KEY_SIGN = 9;     // Wave 9 — first-boot one-shot, no CapabilityBinding (see §3.1)
}

enum VaultMaterialKind {
  VAULT_MATERIAL_KIND_UNSPECIFIED = 0;
  ED25519_PRIVATE_KEY = 1;
  ED25519_PUBLIC_KEY = 2;
  RSA_PRIVATE_KEY = 3;
  X25519_PRIVATE_KEY = 4;
  SYMMETRIC_KEY_AES_256_GCM = 5;
  SYMMETRIC_KEY_CHACHA20_POLY1305 = 6;
  HMAC_KEY_SHA256 = 7;
  MAC_KEY_BLAKE3 = 8;
  PASSWORD_BLOB = 9;
  TOKEN_BLOB = 10;
  CERTIFICATE_PRIVATE_KEY = 11;
}

enum CapabilityState {
  CAPABILITY_STATE_UNSPECIFIED = 0;
  DRAFT = 1;
  ACTIVE = 2;
  EXPIRED = 3;
  REVOKED = 4;
  ROTATED = 5;        // residual state — material rotated but capability_id still active
  DISCARDED = 6;
}

enum RotationReason {
  ROTATION_REASON_UNSPECIFIED = 0;
  ROTATION_SCHEDULED = 1;
  ROTATION_MANUAL = 2;
  ROTATION_SUSPECTED_COMPROMISE = 3;
  ROTATION_KEY_AGE = 4;
}

enum RevocationReason {
  REVOCATION_REASON_UNSPECIFIED = 0;
  USER_REQUEST = 1;
  ADMIN_REQUEST = 2;
  SUSPECTED_COMPROMISE = 3;
  BUNDLE_ROLLOVER = 4;
  MATERIAL_UNAVAILABLE = 5;
  EXPIRED_BY_BUDGET = 6;
  AUDIT_FLAG = 7;
}

enum VaultErrorCode {
  VAULT_ERROR_CODE_UNSPECIFIED = 0;
  CAPABILITY_NOT_FOUND = 1;
  CAPABILITY_NOT_ACTIVE = 2;
  CAPABILITY_NOT_ACTIVE_IN_GROUP = 3;
  CAPABILITY_SIGNATURE_INVALID = 4;
  CAPABILITY_REVOKED = 5;
  CAPABILITY_EXPIRED = 6;
  CAPABILITY_CLASS_KIND_MISMATCH = 7;
  CAPABILITY_ID_ALREADY_EXISTS = 8;
  SUBJECT_KIND_REJECTED_FOR_VAULT = 9;
  NONCE_REPLAY = 10;
  RATE_CAP_EXCEEDED = 11;
  USAGE_BUDGET_EXHAUSTED = 12;
  CO_SIGNER_REQUIRED = 13;
  CO_SIGNER_INVALID = 14;
  RECOVERY_REQUIRED = 15;
  STRONG_SESSION_REQUIRED = 16;
  HUMAN_USER_REQUIRED = 17;
  ONE_SHOT_ALREADY_USED = 18;
  MASTER_KEY_UNAVAILABLE = 19;
  MATERIAL_NOT_FOUND = 20;
  VAULT_BROKER_INTERNAL = 21;
  BOOTSTRAP_KEY_SIGN_NOT_PERMITTED = 22;     // Wave 9 — preconditions §3.1 unmet
  BOOTSTRAP_KEY_ALREADY_EXHAUSTED = 23;       // Wave 9 — firstboot marker already exists
}

// ============================================================================
// Core types
// ============================================================================

message UsageBudget {
  uint64 max_operations = 1;       // 0 means unlimited (only valid for VERIFY classes)
  uint64 used_operations = 2;
  uint32 rate_cap_per_minute = 3;  // 0 means unlimited
}

message VaultCapability {
  string capability_id = 1;                          // cap_<ulid>
  string subject_canonical_id = 2;
  string group_id = 3;
  string identity_bundle_version = 4;                // idbundle_<hex>
  VaultCapabilityClass class = 5;
  VaultMaterialKind material_kind = 6;
  string material_fingerprint = 7;                   // truncated BLAKE3 (BLAKE3(JCS(public_projection))[:32]) over material public projection
  CapabilityState state = 8;
  google.protobuf.Timestamp granted_at = 9;
  google.protobuf.Timestamp expires_at = 10;
  string granted_by = 11;                            // canonical_subject_id of granter
  string approval_id = 12;
  bool is_one_shot = 13;
  UsageBudget usage_budget = 14;
  string provenance = 15;                            // "normal" | "recovery"
  bytes ed25519_signature = 16;                      // broker signs (capability_id || subject || group_id || class || material_kind || material_fingerprint || granted_at || expires_at)
}

message OperationMetadata {
  google.protobuf.Timestamp timestamp = 1;
  uint64 latency_us = 2;
  uint64 byte_count_in = 3;
  uint64 byte_count_out = 4;
}

// ============================================================================
// Capability lifecycle RPCs
// ============================================================================

message IssueCapabilityRequest {
  string session_id = 1;
  string subject_canonical_id = 2;
  string group_id = 3;
  VaultCapabilityClass class = 4;
  VaultMaterialKind material_kind = 5;
  google.protobuf.Timestamp expires_at = 6;
  string approval_id = 7;
  bool is_one_shot = 8;
  UsageBudget requested_budget = 9;          // broker enforces min(default, requested)
  oneof material_source {
    bool generate_new = 10;                  // broker generates new material under material_kind
    string existing_material_id = 11;        // bind to pre-existing material (e.g. rotated counterpart)
  }
}

message IssueCapabilityResponse {
  oneof result {
    VaultCapability capability = 1;
    VaultError error = 2;
  }
}

message RotateCapabilityRequest {
  string session_id = 1;
  string capability_id = 2;
  RotationReason reason = 3;
}

message RotateCapabilityResponse {
  oneof result {
    VaultCapability rotated = 1;             // same capability_id; new material_fingerprint
    VaultError error = 2;
  }
}

message RevokeCapabilityRequest {
  string session_id = 1;
  string capability_id = 2;
  RevocationReason reason = 3;
}

message RevokeCapabilityResponse {
  oneof result {
    google.protobuf.Timestamp revoked_at = 1;
    VaultError error = 2;
  }
}

message GetCapabilityRequest {
  string session_id = 1;
  string capability_id = 2;
}

message GetCapabilityResponse {
  oneof result {
    VaultCapability capability = 1;
    VaultError error = 2;
  }
}

// ============================================================================
// Use-without-reveal RPCs
// ============================================================================

message SignBlobRequest {
  string session_id = 1;
  string capability_id = 2;
  bytes blob = 3;
  bytes nonce = 4;                            // 16 bytes; anti-replay
}

message SignBlobResponse {
  oneof result {
    bytes signature = 1;
    VaultError error = 2;
  }
  OperationMetadata metadata = 3;
}

message VerifyBlobRequest {
  string session_id = 1;
  string capability_id = 2;
  bytes blob = 3;
  bytes signature = 4;
}

message VerifyBlobResponse {
  oneof result {
    bool valid = 1;
    VaultError error = 2;
  }
  OperationMetadata metadata = 3;
}

message EncryptBlobRequest {
  string session_id = 1;
  string capability_id = 2;
  bytes plaintext = 3;
  bytes nonce = 4;
  bytes associated_data = 5;                  // for AEAD modes
}

message EncryptBlobResponse {
  oneof result {
    bytes ciphertext = 1;
    VaultError error = 2;
  }
  OperationMetadata metadata = 3;
}

message DecryptBlobRequest {
  string session_id = 1;
  string capability_id = 2;
  bytes ciphertext = 3;
  bytes nonce = 4;
  bytes associated_data = 5;
}

message DecryptBlobResponse {
  oneof result {
    bytes plaintext = 1;
    VaultError error = 2;
  }
  OperationMetadata metadata = 3;
}

message GenerateMacRequest {
  string session_id = 1;
  string capability_id = 2;
  bytes blob = 3;
}

message GenerateMacResponse {
  oneof result {
    bytes mac = 1;
    VaultError error = 2;
  }
  OperationMetadata metadata = 3;
}

message VerifyMacRequest {
  string session_id = 1;
  string capability_id = 2;
  bytes blob = 3;
  bytes mac = 4;
}

message VerifyMacResponse {
  oneof result {
    bool valid = 1;
    VaultError error = 2;
  }
  OperationMetadata metadata = 3;
}

message GenerateRandomRequest {
  string session_id = 1;
  uint32 byte_count = 2;
  string capability_id = 3;                   // optional; required when byte_count > 64
}

message GenerateRandomResponse {
  oneof result {
    bytes random_bytes = 1;
    VaultError error = 2;
  }
  OperationMetadata metadata = 3;
}

// ============================================================================
// Restricted reveal path
// ============================================================================

message RevealSecretRequest {
  string session_id = 1;
  string capability_id = 2;
  string co_signer_approval_id = 3;
}

message RevealSecretResponse {
  oneof result {
    bytes raw_bytes = 1;                      // returned only under all preconditions per §6.6
    VaultError error = 2;
  }
  OperationMetadata metadata = 3;
}

// ============================================================================
// Engine info
// ============================================================================

message GetVaultInfoRequest {}

message GetVaultInfoResponse {
  string schema_version = 1;                   // "aios.vault.v1alpha1"
  string identity_bundle_version = 2;
  uint64 active_capability_count = 3;
  bool master_key_unlocked = 4;
  bool recovery_mode = 5;
  google.protobuf.Timestamp started_at = 6;
}

// ============================================================================
// Error envelope
// ============================================================================

message VaultError {
  VaultErrorCode code = 1;
  string message = 2;
}
```

## See also

- [S0.1 — Action Envelope and Lifecycle](../XX_Cross_Cutting/01_action_envelope_lifecycle.md)
- [S2.3 — Policy Kernel](01_policy_kernel.md)
- [S5.1 — Identity Model](03_identity_model.md)
- [S5.3 — Approval Mechanics](04_approval_mechanics.md)
- [S5.4 — Emergency Override](05_emergency_override.md)
- [L0 — Invariants (INV-003, INV-015, INV-018)](../L0_Governance_Evidence_Safety/04_invariants.md)
- [L4 Overview](00_overview.md)
- [Rev.1 §13 — Capability Runtime Contract](../../001.AI-OS.NET--SPECREV.1/02_SPECIFICATION.md)
- [Rev.2 Master Index](../00_MASTER_INDEX.md)
