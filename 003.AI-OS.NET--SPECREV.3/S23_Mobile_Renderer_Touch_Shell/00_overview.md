# S23 - Mobile Renderer and Touch Shell

| Field     | Value                                                                                                                                                                                                                      |
| --------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Status    | `CONTRACT` (created 2026-05-29; awaiting implementation evidence)                                                                                                                                                          |
| Phase tag | S23                                                                                                                                                                                                                        |
| Layer     | Cross-cutting: L7 primary; L4, L5, L9                                                                                                                                                                                      |
| Consumes  | S7.2 Shared UI Schema, S5.3 Approval Mechanics, S2.3 Policy Kernel, S3.1 Evidence Log, S20 Native AI Control Plane and AI Terminal                                                                                         |
| Produces  | `MobileSurface`, `MobileSurfaceMode`, `MobileApprovalRequest`, `OfflineApprovalToken`, `RecoveryPairingQR`, `PocketNode`, `VoiceSurface`, `VoiceIntent`, mobile/voice approval lifecycle, mobile approval evidence records |

## 1. Responsibility

S23 defines how AIOS reaches phone- and tablet-class devices and how voice
becomes an input/output surface, without creating a second, weaker administration
plane.

Two device tracks (DEC-R3-004) plus one modality:

```text
AIOS_MOBILE_RENDERER  -> phone/tablet as a signed approval and monitoring console
                         for a desktop/server AIOS host
AIOS_PHONE_EDITION    -> AIOS running on phone-class hardware on mainline Linux
                         (Plasma Mobile / phosh), Android apps via Waydroid/VM
VoiceSurface          -> TTS/STT and conversational binding over the Shared UI
                         Schema, reusing S20 typed actions (DEC-R3-007)
```

The renderer track is the priority deliverable. It strengthens the existing
desktop/server product immediately and defers the harder L1 substrate fork. The
phone edition is specified here as a renderer that may also be a host, never as a
fork of the 11-layer model.

The constitutional rule that governs this whole contract: **a phone, a tablet, or
a voice channel is a policy surface, not an administrator.** It carries human
consent into the Policy Kernel; it never holds authority of its own. AI on these
surfaces still only proposes and explains (S20).

Invariant links: INV-001, INV-002, INV-005, INV-006, INV-009, INV-013, INV-017,
INV-019, INV-023, INV-031 (introduced below).

## 2. Product principle

The operator should be able to approve, monitor, and emergency-stop the system
from a pocket device, with the same exact-action discipline as the desktop, and
to talk to the OS without the OS ever treating spoken words as authority.

```text
desktop/server action needs human consent
  -> MobileApprovalRequest carries the exact action hash + risk diff
  -> operator reads the diff on the phone
  -> biometric / PIN binds consent to that exact hash (S5.3 EXACT_ACTION)
  -> host executes only on hash match
  -> evidence records every step
```

What a phone or tablet can do is therefore bounded to what a human operator could
authorize at the desktop. It never gains a private back door.

## 3. Reference patterns

| Pattern                                                                                     | S23 use                                                                                                                                               |
| ------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------- |
| [Kirigami](https://develop.kde.org/frameworks/kirigami/)                                    | Convergent desktop/mobile UI framework; the recommended toolkit for the AIOS touch shell and phone-edition control center.                            |
| [Plasma Mobile](https://www.plasma-mobile.org/)                                             | Mainline-Linux phone shell target for `AIOS_PHONE_EDITION` (alongside phosh).                                                                         |
| [Waydroid](https://docs.waydro.id/)                                                         | Containerized Android for running Android apps on the phone edition without an AOSP base.                                                             |
| [FIDO2 / WebAuthn](https://fidoalliance.org/fido2/)                                         | Hardware-backed assertion model for binding biometric/PIN consent to an exact action hash.                                                            |
| [Android StrongBox / Keystore](https://developer.android.com/privacy-and-security/keystore) | Hardware-backed key storage pattern for the offline approval token and Pocket Node vault shard, when the renderer runs on Android via the mobile app. |
| [WebAuthn signature counter](https://www.w3.org/TR/webauthn-2/#signature-counter)           | Anti-replay / clone-detection model reused for `OfflineApprovalToken` single-use semantics.                                                           |
| [PipeWire](https://docs.pipewire.org/)                                                      | Audio capture/playback routing for the voice surface microphone/consent indicators.                                                                   |

## 4. Mobile surface mode enum

```text
MobileSurfaceMode =
  AIOS_MOBILE_RENDERER
| AIOS_PHONE_EDITION
```

```text
MobileTransport =
  LAN_DIRECT          # phone and host on the same trusted LAN
| RELAY_AUTHENTICATED # mutually authenticated relay (still localhost-default per INV-006)
| OFFLINE_TOKEN       # LAN/relay down; pre-issued single-use token path
| QR_PAIRING          # in-person QR handshake for recovery / first pairing
```

```text
MobileFormFactor =
  PHONE
| TABLET
| HANDHELD
| WATCH_GLANCE   # notification + emergency-stop only; cannot approve
```

Unknown values for `MobileSurfaceMode`, `MobileTransport`, and `MobileFormFactor`
are rejected by the surface registration validator. A surface whose declared mode
is not in the closed enum fails closed and never registers.

`WATCH_GLANCE` is a deliberately reduced surface: it may display alerts and trigger
emergency stop / quarantine, but it can never render an `APPROVAL_PROMPT` node and
can never bind an action. Approval is reserved for `PHONE`, `TABLET`, and
`HANDHELD` form factors with a hardware-backed authenticator.

## 5. MobileSurface object

A `MobileSurface` is a registered, identity-bound rendering and consent endpoint.
It is a renderer over the Shared UI Schema (S7.2); it adds no new node kinds and
no new authority.

```yaml
mobile_surface:
  surface_id: "msrf_<ULID>"
  mode: AIOS_MOBILE_RENDERER
  form_factor: PHONE
  transport: LAN_DIRECT
  bound_subject:
    actor_kind: HUMAN_OPERATOR # or HUMAN_USER
    identity_ref: "S5.1 identity id"
  device_attestation:
    platform: "android|ios|linux-mobile"
    hardware_keystore: true # StrongBox / TPM-backed key required for approval
    public_key_ref: "key:msrf:..." # device key registered with Approval Mechanics
    attestation_receipt: "evr_..."
  capabilities:
    can_view_risk_diff: true
    can_view_package_passport: true
    can_view_evidence_receipt: true
    can_approve: true # false for WATCH_GLANCE
    can_emergency_stop: true
    can_quarantine_app: true
  policy:
    allowed_profiles: [DEV_RELAXED, SECURE_DEFAULT, STIG_ALIGNED]
    forbidden_profiles: [] # AIRGAP_HIGH narrows transport, see §10
    max_offline_tokens: 8
    offline_token_ttl_seconds: 3600
  lifecycle:
    state: REGISTERED
  evidence:
    pairing_receipt: "evr_..."
```

A surface with `can_approve: true` MUST have `hardware_keystore: true` and a
registered `public_key_ref`. The registration validator rejects an approval-capable
surface that lacks a hardware-backed authenticator. AI subjects
(`AI_NATIVE_SUBJECT`, `AI_AGENT_CAPSULE`) cannot register, own, or operate a
`MobileSurface` and cannot be a `bound_subject`.

## 6. Phone-as-signed-approval-console flow

This is the core mechanism of the renderer track. It is a renderer-level binding
of S5.3 `EXACT_ACTION` approval onto a remote, hardware-backed device. The phone
does not decide; it transports human consent bound to one immutable action hash.

```text
[host]  typed action reaches Policy Kernel (S2.3)
        kernel returns REQUIRE_APPROVAL, scope = EXACT_ACTION
        Approval Mechanics (S5.3) builds ApprovalRequest with
          bound_action_canonical_hash = H
  -> emit MOBILE_APPROVAL_REQUESTED
[wire]  MobileApprovalRequest{ H, risk_diff, ttl } pushed to the bound MobileSurface
[phone] operator sees the EXACT risk diff rendered from the Shared UI Schema
          APPROVAL_PROMPT node: what data / devices / network / capsules are touched
        operator authorizes with biometric or device PIN
        device hardware key signs EXACTLY H (FIDO2-style assertion)
  -> emit MOBILE_APPROVAL_SIGNED { surface_id, H, assertion, counter }
[host]  Approval Mechanics verifies the signature against the registered device key
        and the action-revision invariant (S5.3): any change to H voids the binding
        Capability Runtime (S10.1) executes ONLY if the live action hash == H
        binding is single-use; CONSUMED is terminal (S5.3 anti-replay)
  -> evidence chain continues through ExecuteAction / VerifyAction
```

Binding rules (inherited from S5.3, restated for the mobile transport):

- The device signs the canonical action hash, not a free-form "approve" flag. A
  rendered approval gesture that is not a hardware signature over `H` is treated as
  untrusted and ignored (S5.3 non-`APPROVAL_PROMPT` rule).
- The action-revision invariant holds end to end: if the host's action changes by
  one byte, `H` changes, and the mobile signature no longer matches — the binding
  voids and execution fails closed.
- One `MobileApprovalRequest` binds one `ActionRequestId`. It is single-use. A
  replayed signature (same authenticator counter or already-`CONSUMED` binding)
  fails closed.
- The phone can never widen scope to `ACTION_FAMILY`; only a desktop human-co-signer
  path may request a family scope per S5.3. The mobile surface is `EXACT_ACTION` only.

```yaml
mobile_approval_request:
  request_id: "mapr_<ULID>"
  surface_id: "msrf_<ULID>"
  bound_action_request_id: "actreq_..." # S5.3 / L3 ActionRequestId
  bound_action_canonical_hash: "sha256:H" # the exact hash the device must sign
  risk_diff_ref: "uitree:..." # Shared UI Schema APPROVAL_PROMPT node
  security_profile: SECURE_DEFAULT
  strength: STRONG # S5.3 ApprovalStrength enum (owned by S5.3)
  scope: EXACT_ACTION
  ttl_seconds: 300
  state: PUSHED
```

## 7. Mobile approval lifecycle (FSM)

```text
PUSHED
  -> VIEWED            # operator opened the risk diff on the device
  -> SIGNED            # hardware authenticator signed exactly H
  -> VERIFIED          # host verified signature against registered device key
  -> CONSUMED          # Capability Runtime spent the binding on the exact action
PUSHED  -> EXPIRED      # ttl elapsed before SIGNED
VIEWED  -> DECLINED     # operator rejected
SIGNED  -> REJECTED     # signature/hash/counter mismatch on host (fails closed)
any non-terminal -> REVOKED   # emergency stop / profile change voids pending requests
```

Terminal states: `CONSUMED`, `EXPIRED`, `DECLINED`, `REJECTED`, `REVOKED`.
Unknown states are rejected by the approval-state validator. A request can leave a
terminal state only by being superseded by a new `MobileApprovalRequest` with a
new hash. There is no transition that turns a `REJECTED` or `REVOKED` request into
`CONSUMED`.

## 8. Offline approval token

When LAN and relay are both down, a pre-issued, single-use, short-TTL token lets a
known operator authorize a pre-declared high-risk action class without a live
channel. The token is a constrained, hardware-bound capability — not a standing
admin credential.

The token's risk ceiling is drawn from a closed band enum (S23-local; distinct from
the S20 `AIContextRisk` enum):

```text
ApprovalRiskBand =
  LOW
| MEDIUM
| HIGH
| CRITICAL
```

Unknown values are rejected by the mobile approval validator.

```yaml
offline_approval_token:
  token_id: "oatk_<ULID>"
  surface_id: "msrf_<ULID>"
  issued_for_action_class: "package.install.signed" # a narrow class, not "any"
  bound_action_canonical_hash: "sha256:H" # still pins one exact action
  max_risk_class: MEDIUM # ApprovalRiskBand; tokens cannot pre-approve HIGH/CRITICAL
  single_use: true
  not_before: "iso8601"
  expires_at: "iso8601" # short TTL; bounded by surface policy
  device_key_ref: "key:msrf:..."
  issuer_evidence: "evr_..." # OFFLINE_TOKEN_ISSUED receipt
```

Offline token rules:

- A token still pins one `bound_action_canonical_hash`. It does not authorize a
  category broadly; it pre-authorizes one exact action that may complete while the
  channel is down.
- Tokens are single-use and short-lived. Reuse fails closed.
- A token can never carry `max_risk_class` above `MEDIUM`. HIGH/CRITICAL actions
  always require a live, online `MOBILE_APPROVAL_SIGNED` or a desktop human path.
- AI subjects cannot request, hold, mint, or spend an offline token.
- Under `AIRGAP_HIGH` the offline token path is the _primary_ approval transport
  (there is no live relay), and strength is forced to `STRONG`.
- When connectivity returns, every spent token is reconciled into the live evidence
  chain; an unreconcilable token spend raises a `MOBILE_APPROVAL_REQUESTED`-class
  anomaly for operator review.

## 9. QR recovery pairing and Pocket Node

### 9.1 QR recovery pairing

First pairing and emergency re-pairing use an in-person QR handshake displayed on
the host recovery console. This binds a device key to an operator identity without
requiring a working network and without the Cognitive Core.

```text
[host recovery console]  shows RecoveryPairingQR (signed, short-lived nonce)
[phone]                  scans QR, completes mutual key exchange in person
[host]                   registers the device public key against the operator identity
  -> emit RECOVERY_PAIRING_QR
```

```yaml
recovery_pairing_qr:
  pairing_id: "rpqr_<ULID>"
  host_nonce: "base32:..." # single-use, short TTL
  host_pubkey: "key:host:..."
  intended_actor_kind: HUMAN_OPERATOR
  channel: IN_PERSON # QR is shown physically; not transmitted over network
  expires_at: "iso8601"
  evidence_receipt: "evr_..."
```

The QR pairing path is rendered by the recovery surface and works with no AI and no
network (see §11, re-phrased INV-001). A scanned QR only establishes a device key;
it never itself approves a system action.

### 9.2 AIOS Pocket Node

`AIOS_PHONE_EDITION` may additionally act as a small AIOS node — the Pocket Node.
This is the optional, more capable end of the phone track and is gated harder than
the renderer.

```yaml
pocket_node:
  node_id: "pnode_<ULID>"
  surface_id: "msrf_<ULID>"
  roles:
    vault_shard_holder: true # holds a Shamir-style shard, never the full vault
    evidence_replica: true # replicated, append-only evidence summaries only
    emergency_recovery_key: true
    low_power_ai_local: false # local AI is proposal-only; still never root
  vault_shard:
    scheme: "shamir|threshold"
    shard_index: 2
    threshold: "k-of-n" # one phone alone can never reconstruct the vault
  evidence_replica:
    mode: APPEND_ONLY # phone receives summaries; cannot mutate the chain
    merkle_anchor: "sha256:..." # ties to DEC-R3-003 Merkle-DAG evidence model
  policy:
    allowed_profiles: [DEV_RELAXED, SECURE_DEFAULT]
    forbidden_profiles: [STIG_ALIGNED, AIRGAP_HIGH] # high-assurance hosts do not export shards to phones by default
```

Pocket Node rules:

- A Pocket Node holds at most a vault _shard_, never the whole vault. One device
  alone can never reconstruct secrets (threshold scheme, S5.3/Vault Broker semantics).
- The replicated evidence on the phone is append-only summary material. The phone
  cannot mutate, prune, or rewrite the host evidence chain (INV-005).
- A Pocket Node that runs local low-power AI keeps the S20 boundary: that AI
  proposes and explains only; it is never root, never self-approves, never weakens
  the host's security profile.
- Pocket Node vault-shard export is forbidden under `STIG_ALIGNED` and `AIRGAP_HIGH`
  unless a recovery-approved exception exists (S16.1 exception discipline).

## 10. Security profile gates

The mobile and voice surfaces are policy surfaces; the profile narrows transport,
approval strength, and what the surface may do — it never opens a bypass.

| Profile          | Mobile / voice rule                                                                                                                                                                                                 |
| ---------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `DEV_RELAXED`    | All transports allowed with warning; offline tokens up to `MEDIUM`; voice intents allowed; relay pairing convenient.                                                                                                |
| `SECURE_DEFAULT` | Hardware-backed authenticator required to approve; localhost/LAN default per INV-006; `STRONG` strength for HIGH risk; voice intents always re-confirmed for HIGH risk.                                             |
| `STIG_ALIGNED`   | Approval requires hardware authenticator + `STRONG`; no broad relay; Pocket Node vault-shard export only by recovery-approved exception; voice cannot bind HIGH/CRITICAL actions, only propose them.                |
| `AIRGAP_HIGH`    | No live relay; `OFFLINE_TOKEN` and `QR_PAIRING` are the only transports; offline tokens `MEDIUM` ceiling, `STRONG` forced; no Pocket Node shard export; voice STT runs on-device only, never via external provider. |

Hard denies (Policy Kernel must deny on every profile):

- no `MobileSurface` or `VoiceSurface` may approve an action it cannot also render
  as a Shared UI Schema `APPROVAL_PROMPT` node carrying the exact action hash
- no AI subject may own, register, sign on, or be the bound subject of a mobile or
  voice approval surface
- no approval may bind to anything other than one `EXACT_ACTION` canonical hash
- no offline token may pre-approve `HIGH` or `CRITICAL` risk
- no mobile/voice path may weaken the active `SecurityProfile`
- no mobile/voice path may mutate the evidence chain
- no voice input may be treated as a trusted instruction (see §12)

## 11. Recovery without AI on keyboardless devices (INV-001 restated)

INV-001 says the machine boots and recovers without any LLM. On a keyboardless
phone or tablet that rule must be expressed in touch and QR terms.

Re-phrasing for `AIOS_PHONE_EDITION` and recovery consoles:

```text
INV-001 (mobile form): A keyboardless AIOS device must reach a usable recovery
state using only touch, on-screen controls, and in-person QR pairing — with no
text keyboard, no network, and no Cognitive Core (S5) participation.
```

Concrete requirements:

- The recovery surface renders large touch targets, an on-screen secure keypad for
  PIN, and the `RecoveryPairingQR`; it depends on no AI and no network.
- The active `SecurityProfile` is displayed on the recovery surface (mirrors S16.1
  acceptance: recovery can show the profile without the Cognitive Core).
- Emergency stop and quarantine are reachable from the lock surface and from
  `WATCH_GLANCE`, because halting is always safe and never requires approval.
- Voice is never on the recovery-critical path: a mute or dead microphone must
  never prevent recovery.

## 12. Voice renderer (VoiceSurface) — DEC-R3-007

The voice surface is TTS output and STT input over the Shared UI Schema. It adds a
_binding_, not authority. Every action a voice user can cause is a typed action that
the S20 AI terminal could also produce — and it flows through the identical Policy
Kernel and Approval Mechanics path.

```yaml
voice_surface:
  surface_id: "vsrf_<ULID>"
  bound_subject:
    actor_kind: HUMAN_USER # or HUMAN_OPERATOR
    identity_ref: "S5.1 identity id"
  io:
    tts: true
    stt: true
    stt_provider: "on-device|policy-approved-external"
  binding:
    conversational: true # L5 Cognitive Core surface emitting S20 typed actions
    new_authority: false # voice never gains authority the AI terminal lacks
  policy:
    untrusted_input: true # all spoken text is untrusted data
    high_risk_requires_visual_confirm: true
```

```text
VoiceIntent =
  RECEIVED            # raw STT text captured; classified untrusted
| CLASSIFIED          # passed through S20 PromptBoundaryClassifier as DATA, not instruction
| MAPPED_TO_TYPED_ACTION
| REJECTED_AS_UNSAFE  # prohibited pattern / prompt-injection shaped input
```

Unknown `VoiceIntent` states are rejected by the voice intent validator.

Voice security rules (binding S20):

- Spoken text is **untrusted text**. It enters as `RECEIVED`, then passes through
  the S20 `PromptBoundaryClassifier` exactly like terminal/log/package/web text. It
  can inform a proposal; it can never be elevated to a trusted instruction.
- The voice path produces the same typed actions as the S20 AI terminal and routes
  them through S2.3 Policy Kernel and S5.3 Approval Mechanics. There is no
  voice-only execution path.
- A HIGH/CRITICAL action proposed by voice always requires a visual
  `APPROVAL_PROMPT` confirmation (on a `MobileSurface` or desktop). "Just say yes"
  can never bind a high-risk action; the EXACT_ACTION hash must still be confirmed
  on a hardware-backed surface. Under `STIG_ALIGNED`/`AIRGAP_HIGH` voice cannot bind
  HIGH/CRITICAL at all — only propose.
- TTS output and the microphone state are surfaced with an explicit "AI is
  speaking / microphone is live" indicator; the voice surface cannot hide that it
  is AI (S20 transparency rule).

## 13. New invariant

This contract introduces one new constitutional rule (extends, never replaces,
INV-001..027 per DEC-R3-010):

```text
INV-031: A render/approval surface is a policy surface, never an authority. It may
only carry human consent bound to one exact action hash (S5.3 EXACT_ACTION) into
the Policy Kernel; it can never decide, self-approve, widen scope beyond the bound
hash, hold standing admin authority, or weaken the active SecurityProfile.
```

## 14. Evidence records

S23 adds these record types:

```text
MOBILE_SURFACE_REGISTERED
MOBILE_APPROVAL_REQUESTED
MOBILE_APPROVAL_VIEWED
MOBILE_APPROVAL_SIGNED
MOBILE_APPROVAL_REJECTED
MOBILE_APPROVAL_REVOKED
OFFLINE_TOKEN_ISSUED
OFFLINE_TOKEN_SPENT
OFFLINE_TOKEN_RECONCILED
RECOVERY_PAIRING_QR
POCKET_NODE_REGISTERED
POCKET_NODE_SHARD_SEALED
VOICE_INTENT_RECEIVED
VOICE_INTENT_REJECTED
MOBILE_EMERGENCY_STOP_TRIGGERED
```

Minimum fields for `MOBILE_APPROVAL_SIGNED`:

```text
request_id
surface_id
bound_subject_identity_ref
bound_action_request_id
bound_action_canonical_hash
authenticator_assertion_ref
authenticator_counter
security_profile
strength
signed_at
evidence_receipt_id
```

## 15. Non-goals

- Do not make the phone or tablet a second, unsupervised admin plane.
- Do not let a mobile or voice surface approve anything by a free-form gesture or
  spoken "yes" instead of a hardware signature over the exact action hash.
- Do not let an offline token become a standing or broad-scope credential.
- Do not fork the 11-layer model for the phone edition; mainline Linux only.
- Do not adopt an AOSP base; Android apps run via Waydroid/VM (DEC-R3-004).
- Do not put voice on the recovery-critical path or treat spoken text as trusted.
- Do not export the full vault to a Pocket Node; shard-only, threshold-gated.
- Do not let any mobile/voice path mutate evidence or weaken the security profile.

## 16. Acceptance criteria

S23 is `REAL` only when:

1. `MobileSurfaceMode`, `MobileTransport`, `MobileFormFactor`, the mobile approval
   FSM states, and `VoiceIntent` states are closed enums; the validators reject
   unknown values and fail closed.
2. An approval-capable `MobileSurface` cannot register without a hardware-backed
   authenticator and a registered device key.
3. A `MobileApprovalRequest` binds exactly one `ActionRequestId` and one canonical
   action hash; a one-byte change to the host action voids the binding (S5.3
   action-revision invariant) and execution fails closed.
4. A mobile signature verifies against the registered device key, is single-use,
   and is rejected on counter reuse or after `CONSUMED`.
5. An offline approval token is single-use, short-TTL, pins one exact action hash,
   cannot exceed `MEDIUM` risk, and is reconciled into the evidence chain when the
   channel returns.
6. QR recovery pairing establishes a device key in person with no network and no
   Cognitive Core, and a scanned QR by itself approves no action.
7. A keyboardless device reaches a usable recovery state and shows the active
   `SecurityProfile` using only touch and QR, with no AI (INV-001 mobile form).
8. Voice input enters as untrusted text, passes the S20 prompt-boundary classifier,
   and only ever produces typed actions routed through Policy Kernel and Approval
   Mechanics; no voice-only execution path exists.
9. A HIGH/CRITICAL action proposed by voice requires a visual exact-action
   confirmation, and under `STIG_ALIGNED`/`AIRGAP_HIGH` voice cannot bind it at all.
10. No AI subject can own, register, sign on, or be the bound subject of any mobile
    or voice surface; INV-031 holds across every code path.
11. A Pocket Node holds at most a threshold vault shard and an append-only evidence
    replica; it can reconstruct no secret alone and can mutate no evidence.
12. Every approval, token, pairing, and voice-rejection step emits its evidence
    record.

## 17. See also

- [S7.2 Shared UI Schema](../../002.AI-OS.NET--SPECREV.2/L7_Interaction_Renderers/02_shared_ui_schema.md)
- [S5.3 Approval Mechanics](../../002.AI-OS.NET--SPECREV.2/L4_Policy_Identity_Vault/04_approval_mechanics.md)
- [S2.3 Policy Kernel](../../002.AI-OS.NET--SPECREV.2/L4_Policy_Identity_Vault/01_policy_kernel.md)
- [S3.1 Evidence Log](../../002.AI-OS.NET--SPECREV.2/L9_Observability_Admin_Operations/01_evidence_log.md)
- [S20 Native AI Control Plane and AI Terminal](../S20_Native_AI_Control_Plane_Terminal/00_overview.md)
- [Rev.3 Holistic Specification](../00_REV3_HOLISTIC_SPEC.md)
- [Rev.3 Design Decisions](../02_design_decisions.md)
- [Rev.3 Planning Notes](../00_PLANNING_NOTES.md)
