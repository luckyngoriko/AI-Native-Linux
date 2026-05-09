# Network Policy (Rev.2)

| Field          | Value                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| -------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Status         | `REAL` (initial; written 2026-05-09)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| Phase tag      | S8.1                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| Layer          | L8 Network, Hardware, Devices                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| Schema package | `aios.network.v1alpha1`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| Consumes       | S0.1 (typed action envelopes for exposure / outbound grants), S1.3 (allowlist manifests stored as AIOS-FS objects), S2.3 Policy Kernel (decisions for `RequestExposure` / `GrantOutbound`), S2.4 Verification Grammar (network primitives queued), S3.1 Evidence Log (record append), S3.2 Sandbox Composition (`NetworkMode` floor binding), S5.1 Identity Model (subject `is_ai`, `recovery_mode`, `primary_group_id`), S4.1 Namespace Layout (`/aios/groups/<g>/` per-group network namespaces), L4.2 Vault Broker (AI external-model brokering), L7.5 Web Renderer (`WebExposureState`)                                                                                                |
| Produces       | typed `NetworkPosture`, closed `OutboundDirective`, closed `InboundExposureClass`, closed `ProtocolFamily`, closed `AllowlistEntryKind`, closed `PortPolicy`, closed `ExposureApprovalState` FSM, closed `NetworkPolicyErrorCode`, closed `AICrossOriginPosture`; `NetworkPolicyService` gRPC; per-subject `ExposureGrant` + `OutboundGrant` records; per-app outbound manifest discipline; AI external-model-call brokered pattern; backend integration contract (nftables / systemd-resolved / WireGuard / network namespaces); 18 evidence record types queued for S3.1; three S2.4 verification primitives queued; one L0 invariant candidate (`NETWORK_DEFAULT_DENY_OUTBOUND`) queued |

## 1. Purpose

The Network Policy plane is the single largest exposure surface in AIOS. Every Web renderer port that a tablet on the LAN can hit, every external-model call an AI agent attempts, every LAN service an operator publishes from a homelab box, every direct outbound socket an installed app opens passes through this contract. Without it:

- INV-006 (web UI localhost-only by default) is constitutional intent without an enforcement plane: the renderer can declare `EXPOSURE_LOOPBACK` but nothing at the kernel boundary stops a buggy adapter from binding `0.0.0.0:9443`.
- INV-011 (cross-group access forbidden by default) has no expression at the network layer: two groups on the same host could share a localhost port without the namespace catalog noticing.
- The S3.2 `NetworkMode` enum has been a closed vocabulary since 2026-05-08 but the **runtime enforcer** that turns `LOOPBACK_ONLY` into "the kernel refuses non-loopback connect()" lived nowhere.
- The S7.5 `WebExposureState` (`LOOPBACK` / `LAN` / `PUBLIC` / `RECOVERY`) has been closed since the L7.5 contract but had no canonical L8-side enforcer; the renderer-side declaration could drift from kernel-side reality.
- Per-app outbound discipline named in Rev.1 §18 had no mechanical specification: "an app's outbound is declared explicitly" was an architectural assertion without a typed surface.
- The AI external-model-call pattern (the most security-sensitive cross-origin event in AIOS) had no contract: how does an AI agent talk to `models.openai.com` without seeing the API key, and how does the network layer prove that the connection went through the vault broker rather than around it?

This spec closes that loop. It is the network policy plane of L8: local-first network posture, per-subject outbound rules, exposure approval gates, AI cross-origin discipline, adversarial defense, kernel-backend integration. It is **not** the hardware graph (S8.x — separate sub-spec on device detection, classification, lifecycle), **not** DNS or VPN management (S8.3 — resolver backend, WireGuard mechanics, mDNS gating), **not** firmware trust (S8.4). Those are referenced abstractly only.

This spec fixes:

1. The default-deny constitutional posture for both inbound and outbound traffic at every subject boundary.
2. The closed `NetworkPosture`, `OutboundDirective`, `InboundExposureClass`, `ProtocolFamily`, `AllowlistEntryKind`, `PortPolicy`, `ExposureApprovalState`, `NetworkPolicyErrorCode`, `AICrossOriginPosture` enums.
3. The `NetworkPolicyService` gRPC surface that is the only entry point into network-policy mutation.
4. The per-app outbound manifest (`network_outbound_manifest`) discipline: signed, append-only at the subject level, breach-degraded.
5. The L7.5 / L8 binding: `WebExposureState` is the renderer-side declaration; this contract is the enforcer; the forbidden `LAN → PUBLIC` direct transition is enforced again at L8.
6. The S3.2 / L8 binding: `SandboxProfile.network.mode` is the per-process floor; most-restrictive-wins between sandbox and subject directive.
7. The AI external-model-call pattern: AI subjects never reach the public internet directly; vault-brokered capability handle is the only path; FOREVER evidence at every brokered call.
8. The public exposure constitutional discipline: recovery-mode + STRONG approval + co-signer + FOREVER evidence + 4-hour TTL + 5-minute heartbeat.
9. Backend integration: nftables (primary), iptables (fallback with FOREVER evidence), systemd-resolved (DoT discipline), WireGuard (VPN), Linux network namespaces (per-group isolation).
10. Adversarial robustness: allowlist tampering, subject-id spoofing, dual-stack bypass, DNS rebinding, ARP spoofing, mid-flight revocation tear-down within 250 ms.
11. Bounded-cardinality telemetry; closed evidence record-type catalog; FOREVER-retained constitutional records.
12. The cross-spec follow-up queue (one L0 invariant candidate, 18 record types for S3.1, three primitives for S2.4, three condition fields for S2.3).

## 2. Core invariants

- **I1 — Default deny, both directions.** No subject gains outbound network unless granted. No subject gains inbound exposure beyond loopback unless granted. Public inbound additionally requires recovery-mode approval. Omitted fields resolve to the most restrictive value, never to "any". This is the network analog of the S3.2 §I1 default-deny rule and is the rationale for the queued L0 invariant candidate `NETWORK_DEFAULT_DENY_OUTBOUND`.
- **I2 — INV-006 binds.** Web renderer ports listen on `127.0.0.1` and `::1` only by default. LAN/PUBLIC require `RequestExposure` flow with policy approval and FOREVER evidence per S7.5 §5.4 / §5.5. The kernel-side bind is enforced by this contract; the renderer cannot lie about its bind to L7.5's `current_exposure_state`.
- **I3 — INV-011 binds.** Cross-group network access is forbidden by default. Per S4.1 each group occupies its own Linux network namespace; cross-namespace connections require an explicit `EvaluateConnection` allow that fires the S2.3 `CrossGroupAccessForbidden` hard-deny path on absence.
- **I4 — INV-002 binds (AI proposes, never executes).** AI subjects never receive `ALLOW_INTERNET` or arbitrary `ALLOW_LIST_ONLY`. Their network discipline is one of three values from `AICrossOriginPosture` (§5.10). External-model calls flow through the vault broker; the AI never sees the destination credential. Direct internet attempts are hard-denied with `AI_DIRECT_INTERNET_DENIED` FOREVER evidence.
- **I5 — INV-008 binds (default deny in policy).** The Policy Kernel decision for `RequestExposure` / `GrantOutbound` defaults to `DENY` if no rule matches. Absence of a matching allow rule is not implicit allow.
- **I6 — Most-restrictive-wins between S3.2 and L8.** If the sandbox's `NetworkMode` is `LOOPBACK_ONLY`, the subject's `OutboundDirective` cannot be broader than `ALLOW_LOOPBACK_ONLY` regardless of subject-level grant. The sandbox is the per-process floor; this contract is the per-subject ceiling. The intersection is enforced.
- **I7 — Signed grants.** Every `ExposureGrant` and `OutboundGrant` is Ed25519-signed by the L8 service signing key. Consumers (kernel filter rules, the connection correlator, the renderer) verify before trusting. The signing key is held in the Vault Broker (L4.2) and is reachable only via `gpu`-equivalent capability id `network.policy.sign`.
- **I8 — Append-only at subject level.** Allowlists cannot shrink mid-grant; `RevokeOutbound` is the only way to reduce. New allowlist entries require a new grant; mutations are not transitions. This binds the AI prompt-injection threat: an AI cannot quietly ask the user "please add `evil.com` to my allowlist" and have the host extend an existing grant — every change is a fresh signed grant with fresh policy approval.
- **I9 — FQDN fan-out bounded.** A `HOST_FQDN` allowlist entry is resolved at evaluation time into at most 16 IP addresses. Beyond 16, the entry is denied with `ALLOWLIST_FQDN_FANOUT_EXCEEDED` and an FOREVER-equivalent extended evidence record is emitted. This bounds DNS-based privilege expansion.
- **I10 — Public exposure is constitutional.** Public exposure transitions emit FOREVER evidence at every state change, require a HUMAN_USER subject in recovery mode, require STRONG approval per S5.3 (deferred), require a co-signer per S5.4 (deferred), are TTL-capped at four hours, and are heartbeat-tracked every five minutes. The `LAN → PUBLIC` direct transition is forbidden; the constitutional ground state for any escalation is `LOOPBACK`.
- **I11 — Failure is closed.** Every failure mode (kernel-filter unavailable, signature mismatch, FQDN fan-out exceeded, capability lie, subject unresolved) results in connection refusal. There is no "best effort" path. A degradation (e.g., nftables → iptables) is explicit and FOREVER-retained, never silent.
- **I12 — Bypass attempts are denied at the kernel boundary.** Raw sockets, AF_PACKET, direct device access (e.g., `/dev/net/*`), and packet-socket variants are denied at the sandbox capability layer (S3.2 `process.allowed_capabilities` does not include `CAP_NET_RAW` for AI or unprivileged subjects). Attempts emit `RAW_SOCKET_BYPASS_ATTEMPTED` FOREVER evidence.
- **I13 — Subject-id is not caller-asserted.** The subject id used in policy evaluation is derived from a kernel-side correlator (netlink + eBPF; §10.3), not from a userspace claim. A process cannot impersonate another subject by lying about its identity to L8.

## 3. The constitutional content this contract establishes

### 3.1 Default network posture per host

The host has a single `NetworkPosture` value at any moment, set by the operator (in normal mode) or by the recovery boot path (in recovery mode). The default at first boot is `LAN_LOCAL`; pre-recovery boot is `AIRGAP`; recovery shell startup is `LOOPBACK_ONLY`. Posture changes emit `NETWORK_POSTURE_CHANGED` FOREVER evidence.

The posture is the **outermost ceiling**. Per-subject directives cannot be broader than the host posture. A subject with `OutboundDirective = ALLOW_INTERNET` running on a host in `LAN_LOCAL` posture is degraded to `ALLOW_LAN_ONLY` at evaluation time, and the degradation is FOREVER-evidenced as `OUTBOUND_DEGRADED_TO_LOOPBACK_AUTO` (or `OUTBOUND_DEGRADED_BY_HOST_POSTURE` when the cause is host-posture rather than manifest breach).

### 3.2 INV-006 binding

INV-006 binds to L8 by:

- Web renderer (L7.5) calls `RequestExposure(LOOPBACK)` at startup, which is granted automatically (no policy approval required for the constitutional default state).
- Any non-loopback bind on the renderer port that is not backed by an active `ExposureGrant` is closed by the kernel filter (nftables `tcp dport <renderer_port>` rule scoped to interface).
- The S2.4 verification primitive `port_open(host="0.0.0.0", port=N)` for any AIOS-managed renderer port returns `FAILED` outside of an active grant.

### 3.3 INV-011 binding

INV-011 binds to L8 by:

- Each group `<g>` runs in its own Linux network namespace `aios-net-<g>`; subjects in group `<g>` see only `aios-net-<g>` interfaces.
- The bridge between `aios-net-<g>` and the host has policy-driven routing rules; cross-namespace is denied by default.
- The S2.3 `CrossGroupAccessForbidden` hard-deny fires on `EvaluateConnection` when source subject's `primary_group_id` ≠ destination subject's `primary_group_id` and the destination subject is not in `_system` scope.

### 3.4 Candidate constitutional invariant — `NETWORK_DEFAULT_DENY_OUTBOUND`

The default-deny outbound rule is, in this contract's view, a peer of INV-006 / INV-008 / INV-011 — a cross-cutting constitutional truth that cannot be loosened by any policy bundle, identity bundle, or operator override outside recovery mode. It is queued as a candidate for L0 INV catalog promotion in the next L0 revision and **not authored here**. Until promoted, the §2 I1 invariant of this contract is the operational floor.

## 4. Closed enums

All enums in this contract are **closed**. Adding or removing a value is a versioned schema change. Bundle compilers, kernel filter generators, and the connection correlator MUST reject values outside the enum at parse time.

### 4.1 `NetworkPosture` — top-level posture per host

| Value              | Meaning                                                                                               | Default for                                 |
| ------------------ | ----------------------------------------------------------------------------------------------------- | ------------------------------------------- |
| `AIRGAP`           | No networking at all; interfaces administratively down                                                | Pre-recovery boot, hostile-environment mode |
| `LOOPBACK_ONLY`    | Networking up; only `127.0.0.1` and `::1` traffic permitted                                           | Recovery shell startup                      |
| `LAN_LOCAL`        | LAN traffic permitted with per-subject outbound rules; internet hard-denied                           | First-boot default                          |
| `LAN_AND_INTERNET` | Full outbound permitted with per-subject outbound rules                                               | Operator-set normal mode                    |
| `PUBLIC_ROUTABLE`  | Host accepts inbound from public internet (rare; recovery-mode only; FOREVER evidence at every entry) | Never default                               |

### 4.2 `OutboundDirective` — per-subject closed directive

| Value                 | Meaning                                                                           | Default for                   |
| --------------------- | --------------------------------------------------------------------------------- | ----------------------------- |
| `DENY_ALL`            | No outbound traffic permitted from this subject                                   | AI subjects (default)         |
| `ALLOW_LOOPBACK_ONLY` | Outbound permitted to `127.0.0.1` / `::1` only                                    | Unprivileged service subjects |
| `ALLOW_LAN_ONLY`      | Outbound permitted to LAN CIDR(s) only                                            | Group-scoped local services   |
| `ALLOW_LIST_ONLY`     | Outbound permitted to declared `network_outbound_manifest` allowlist; rest denied | Most apps and adapters        |
| `ALLOW_INTERNET`      | Broad outbound permitted (rare; requires policy approval, FOREVER evidence)       | Never granted to AI subjects  |

### 4.3 `InboundExposureClass` — per-surface closed inbound class

| Value      | Meaning                                                                                                        | Default             |
| ---------- | -------------------------------------------------------------------------------------------------------------- | ------------------- |
| `LOOPBACK` | Bound to `127.0.0.1` / `::1` only                                                                              | Web renderer, all   |
| `LAN`      | Bound to a specific subnet (resolved at activation time); requires `RequestExposure(LAN)` with policy approval | Never default       |
| `PUBLIC`   | Bound to `0.0.0.0` / `::`; requires recovery-mode + STRONG approval + co-signer + FOREVER evidence; TTL ≤ 4h   | Never default       |
| `RECOVERY` | Loopback + dedicated recovery interface (`https://recovery.localhost`); recovery-mode subjects only            | Recovery shell only |

### 4.4 `ProtocolFamily` — closed list of protocols permitted in policies

| Value              | Meaning                                                                              |
| ------------------ | ------------------------------------------------------------------------------------ |
| `TCP`              | Stream socket (`SOCK_STREAM`)                                                        |
| `UDP`              | Datagram socket (`SOCK_DGRAM`)                                                       |
| `QUIC`             | UDP-encapsulated QUIC; treated separately for nftables marking and per-flow counters |
| `SCTP`             | Stream Control Transmission Protocol (rare; kernel-side gated)                       |
| `ICMP_ECHO`        | ICMPv4 / ICMPv6 echo only (no other ICMP types)                                      |
| `WIREGUARD_TUNNEL` | UDP/51820-class WireGuard tunnel; further outbound rides inside the tunnel           |
| `TUN_VPN`          | TUN-device VPN (OpenVPN-class); rare, gated                                          |

Raw IP, raw ethernet (AF_PACKET), TIPC, X.25, and all other protocol families are forbidden by default and not in this enum. A subject that binds an `AF_PACKET` socket triggers `RAW_SOCKET_BYPASS_ATTEMPTED` FOREVER evidence at the sandbox boundary (S3.2) before reaching the network layer.

### 4.5 `AllowlistEntryKind` — closed allowlist entry shape

| Value                   | Meaning                                                                                                |
| ----------------------- | ------------------------------------------------------------------------------------------------------ |
| `HOST_FQDN`             | Fully qualified domain name (e.g., `models.openai.com`); resolved at evaluation time; fan-out ≤ 16 IPs |
| `HOST_IP_V4`            | IPv4 literal (`10.0.0.5`)                                                                              |
| `HOST_IP_V6`            | IPv6 literal (`2001:db8::5`)                                                                           |
| `CIDR_V4`               | IPv4 CIDR block (`192.168.1.0/24`)                                                                     |
| `CIDR_V6`               | IPv6 CIDR block (`2001:db8::/32`)                                                                      |
| `LAN_SUBNET`            | Symbolic; resolved at activation time from the current interface's link-local subnet                   |
| `DNS_OVER_TLS_RESOLVER` | DoT resolver entry; restricted to AIOS root-signed resolver list                                       |
| `LOOPBACK_PORT_RANGE`   | Loopback port range entry (e.g., `127.0.0.1:9000-9999`) for intra-host service discovery               |

### 4.6 `PortPolicy` — closed per-port directive

| Value                    | Meaning                                                                              |
| ------------------------ | ------------------------------------------------------------------------------------ |
| `DENY`                   | Port not opened (default)                                                            |
| `ALLOW_INBOUND_LOOPBACK` | Port accepts loopback inbound only                                                   |
| `ALLOW_INBOUND_LAN`      | Port accepts LAN inbound bound to a `LAN_SUBNET` allowlist; requires policy approval |
| `ALLOW_INBOUND_PUBLIC`   | Port accepts public inbound; requires recovery-mode approval + FOREVER evidence      |
| `ALLOW_OUTBOUND`         | Port permitted as ephemeral outbound source (informational; rarely policy-pinned)    |

### 4.7 `ExposureApprovalState` — closed approval FSM

| Value               | Meaning                                                                    |
| ------------------- | -------------------------------------------------------------------------- |
| `DRAFT`             | Request being constructed (never persisted)                                |
| `AWAITING_OPERATOR` | Submitted; pending human approval (per S5.3 deferred)                      |
| `GRANTED`           | Approved; not yet bound to live exposure                                   |
| `DENIED`            | Approval refused                                                           |
| `EXPIRED`           | TTL elapsed before activation                                              |
| `REVOKED`           | Operator-initiated revocation                                              |
| `ACTIVE`            | Currently in effect; heartbeat-tracked                                     |
| `TERMINATED`        | Cleanly torn down (TTL expiry from ACTIVE, or explicit revoke from ACTIVE) |

Lifecycle:

```text
DRAFT → AWAITING_OPERATOR
AWAITING_OPERATOR → GRANTED | DENIED | EXPIRED
GRANTED → ACTIVE | EXPIRED | REVOKED
ACTIVE → TERMINATED (TTL or operator revoke)
DENIED, EXPIRED, REVOKED, TERMINATED are terminal.
```

Forbidden: `ACTIVE → ACTIVE` (renewal is a fresh request, fresh approval, fresh grant — the existing grant must terminate first); `GRANTED → GRANTED`; any backward transition.

### 4.8 `NetworkPolicyErrorCode` — closed error vocabulary

| Code                                    | Meaning                                                                                 |
| --------------------------------------- | --------------------------------------------------------------------------------------- |
| `NETWORK_POLICY_ERROR_CODE_UNSPECIFIED` | Reserved zero                                                                           |
| `EXPOSURE_DENIED_BY_POLICY`             | S2.3 returned DENY for a `RequestExposure` call                                         |
| `EXPOSURE_REQUIRES_RECOVERY_MODE`       | PUBLIC requested without recovery_mode = true                                           |
| `EXPOSURE_REQUIRES_CO_SIGNER`           | PUBLIC requested without co-signer present                                              |
| `EXPOSURE_FORBIDDEN_TRANSITION`         | LAN → PUBLIC direct transition attempted; must downgrade to LOOPBACK first              |
| `EXPOSURE_TTL_INVALID`                  | Requested TTL exceeds class cap (PUBLIC ≤ 4h)                                           |
| `OUTBOUND_DENIED_BY_POLICY`             | S2.3 returned DENY for a `GrantOutbound` call                                           |
| `OUTBOUND_DIRECTIVE_AI_FORBIDDEN`       | Attempt to grant `ALLOW_INTERNET` / `ALLOW_LIST_ONLY` to an AI subject                  |
| `OUTBOUND_OUTSIDE_MANIFEST`             | A connection attempted by a subject is outside its declared `network_outbound_manifest` |
| `ALLOWLIST_FQDN_FANOUT_EXCEEDED`        | A `HOST_FQDN` resolved to > 16 IPs at evaluation time                                   |
| `LAN_SUBNET_DRIFT`                      | Subnet of an active LAN grant changed mid-grant; re-approval required                   |
| `LAN_PEER_DRIFT_DETECTED`               | MAC/IP of a pinned LAN peer drifted; possible ARP spoofing                              |
| `BACKEND_UNAVAILABLE_NFTABLES`          | nftables required but unavailable on this host                                          |
| `BACKEND_DEGRADED_TO_IPTABLES`          | nftables unavailable; iptables fallback chosen                                          |
| `RAW_SOCKET_BYPASS_ATTEMPTED`           | Subject tried to open a raw / packet socket outside policy                              |
| `SUBJECT_ID_CORRELATOR_FAILURE`         | Kernel correlator could not derive `subject_id` from PID; connection denied             |

### 4.9 `AICrossOriginPosture` — AI subject network discipline

| Value                    | Meaning                                                                                          | Default for           |
| ------------------------ | ------------------------------------------------------------------------------------------------ | --------------------- |
| `AI_LOOPBACK_ONLY`       | AI subject communicates only with local services on `127.0.0.1`/`::1`                            | First-boot AI default |
| `AI_VAULT_BROKERED_ONLY` | AI subject can hit external endpoints only when mediated by L4.2 vault broker on operator behalf | Approved AI subjects  |
| `AI_NO_EXTERNAL`         | AI subject has zero external access; even brokered calls denied                                  | Sensitive group AI    |

State, explicitly: AI subjects are NEVER granted `ALLOW_INTERNET` or arbitrary `ALLOW_LIST_ONLY` for free destinations. Their network reach is bounded by `AICrossOriginPosture`. External calls happen through vault-brokered capability handles; the AI never sees the destination credential. This is the network analog of vault use-without-reveal (INV-018).

## 5. Bindings and constitutional discipline

### 5.1 The L7.5 / L8 binding

`WebExposureState` (per S7.5 §5.1, closed: `LOOPBACK` / `LAN` / `PUBLIC` / `RECOVERY`) is the **renderer-side declaration**. This contract is the **enforcer**. The renderer cannot lie about its bind because the kernel-filter rule generated by L8 is what actually opens or closes the listening socket's reachability.

When L7.5 calls `GrantExposure(LAN)`, the call funnels into L8's `RequestExposure(InboundExposureClass = LAN)`. L8 transitions `DRAFT → AWAITING_OPERATOR`, hands off to S2.3 + S5.3 (deferred), and returns. On approval, L8 transitions `AWAITING_OPERATOR → GRANTED → ACTIVE`, generates the nftables rule, and emits the heartbeat record. The L7.5 chrome banner is driven by L8's `StreamExposureState` subscription — L7.5 trusts L8, not the local renderer state, when deciding whether to show the banner.

The forbidden direct `LAN → PUBLIC` transition (constitutional in S7.5 §5.2) is enforced **again** at L8: L8's FSM rejects `RequestExposure(PUBLIC)` while an active `LAN` grant exists for the same surface, with `EXPOSURE_FORBIDDEN_TRANSITION` and an evidence record. This is defense-in-depth: even if the L7.5-side check were bypassed, L8 still blocks.

### 5.2 The S3.2 / L8 binding

`SandboxProfile.network` (per S3.2 §3, closed `NetworkMode`) is the **per-process floor**. This contract is the **per-subject ceiling**. The intersection is enforced.

Mapping table (most-restrictive-wins between sandbox `NetworkMode` and subject `OutboundDirective`):

| Sandbox `NetworkMode` | Subject `OutboundDirective`    | Effective directive at connect()                              |
| --------------------- | ------------------------------ | ------------------------------------------------------------- |
| `DENY_ALL`            | any                            | `DENY_ALL`                                                    |
| `LOOPBACK_ONLY`       | `DENY_ALL`                     | `DENY_ALL`                                                    |
| `LOOPBACK_ONLY`       | `ALLOW_LOOPBACK_ONLY` or above | `ALLOW_LOOPBACK_ONLY`                                         |
| `HOST_LIMITED`        | `ALLOW_LIST_ONLY`              | intersection of host limit and subject allowlist              |
| `EXPLICIT_ALLOWLIST`  | `ALLOW_LIST_ONLY`              | intersection                                                  |
| `EXPLICIT_ALLOWLIST`  | `ALLOW_INTERNET`               | intersection (sandbox wins)                                   |
| `FULL`                | `ALLOW_LIST_ONLY`              | `ALLOW_LIST_ONLY` (subject wins because stricter)             |
| `FULL`                | `ALLOW_INTERNET`               | `ALLOW_INTERNET` (only if not AI subject; AI gets `DENY_ALL`) |

If at any point the effective directive contradicts the subject's `AICrossOriginPosture` (e.g., effective is `ALLOW_INTERNET` but AI posture is `AI_LOOPBACK_ONLY`), the AI posture wins.

### 5.3 The S0.1 binding

Exposure and outbound grants are typed actions (`aios.network.GrantExposure`, `aios.network.GrantOutbound`, `aios.network.RevokeExposure`, `aios.network.RevokeOutbound`). They flow through the S0.1 lifecycle (`policy_pending → approved → executing → succeeded`) like any other action. The L8 service is the adapter for these actions; it executes them by mutating its own kernel-filter ruleset.

### 5.4 Allowlist composition rules

- Allowlists are append-only at the subject level. There is no in-place mutation; `RevokeOutbound` is the only way to shrink. A new allowlist is a fresh `OutboundGrant` with a new `outg:<ulid26>` id; the previous grant must terminate before the new one becomes ACTIVE. This binds prompt-injection resistance: the AI cannot quietly extend its allowlist.
- `HOST_FQDN` entries are resolved at evaluation time using the AIOS resolver (S8.3 deferred); the resolved IP set is bounded (max 16 IPs per FQDN). If more, the entry is denied with `ALLOWLIST_FQDN_FANOUT_EXCEEDED` and an evidence record. This bounds DNS-based privilege expansion.
- `LAN_SUBNET` is resolved at activation time from the current interface; if the subnet drifts mid-grant (DHCP renewal changing CIDR), the grant is automatically transitioned to `AWAITING_OPERATOR` (re-approval required) and `LAN_SUBNET_DRIFT_DETECTED` evidence is emitted.
- `DNS_OVER_TLS_RESOLVER` allowlists are constitutional: only AIOS root-signed resolver lists are permitted. Arbitrary DoT resolvers cannot be added by a policy bundle.

### 5.5 Per-app outbound discipline (Rev.1 §18 binding)

Every app, service, agent, or adapter declaring outbound network use MUST have an explicit `network_outbound_manifest` field that L8 honors. The manifest is signed (by app publisher, then countersigned by operator on install), declares the allowlist entries the app needs, and is bound to the app's identity at install time.

Manifest mutation requires re-issue + re-approval. An app that ships an updated manifest in a software update is treated as a fresh `GrantOutbound` request: the operator must re-approve before the new manifest takes effect. The previous manifest stays in effect until then.

A connection attempted at runtime that is outside the active manifest is refused with `OUTBOUND_OUTSIDE_MANIFEST`, the kernel filter drops the SYN, and the L8 service emits an evidence record at FOREVER retention. Repeated breaches (≥ 3 within a 24-hour window for a single subject) automatically degrade the subject's `AICrossOriginPosture` to `AI_LOOPBACK_ONLY` (or `OutboundDirective` to `ALLOW_LOOPBACK_ONLY` for non-AI subjects), emitting `OUTBOUND_DEGRADED_TO_LOOPBACK_AUTO` FOREVER evidence. The degradation is sticky: only an operator can lift it.

### 5.6 Public exposure constitutional discipline

`PUBLIC` `InboundExposureClass` is the most security-sensitive transition in this contract. It requires:

1. Operator subject of class `HUMAN_USER` (`is_ai = false`).
2. Recovery mode active on the requesting subject (`is_recovery_mode = true`).
3. STRONG approval per S5.3 (deferred sub-spec) — the strongest approval class AIOS supports, typically MFA + interactive consent.
4. Co-signer present per S5.4 (deferred sub-spec) — a second human subject in `HUMAN_USER` class who counter-signs the approval.
5. `EXPOSURE_GRANTED` FOREVER evidence carrying `class = PUBLIC`, `cidr_allow_list`, `port`, `interface`, both approver ids.
6. Auto-termination at TTL ≤ 4 hours (`TTL_LONG`, per the convention used in §5.7 of the deferred approval spec). Re-approval required to extend.
7. Heartbeat record `PUBLIC_EXPOSURE_HEARTBEAT` STANDARD_24M every 5 minutes while `ACTIVE`.
8. `LAN → PUBLIC` direct transition is forbidden (already constitutional in S7.5 §5.2). The exposure must downgrade through `LOOPBACK` first; the constitutional ground state for any escalation is `LOOPBACK`.

Forbidden by this contract: a policy bundle that attempts to relax any of items 1–8 fails the bundle's signature compilation at S2.3 (the bundle compiler rejects rules that loosen constitutional invariants).

### 5.7 AI external-model-call brokered pattern

The canonical pattern for AI subjects calling external models (OpenAI, Anthropic, Google, etc.):

1. **AI requests a typed action.** The AI's adapter constructs a `aios.network.external_model_call` action envelope (S0.1) with `target.provider = "openai"` (closed list), `target.model_id`, `target.request_hash`. The action is submitted to the action runtime.
2. **Policy decides.** S2.3 evaluates: this requires an `external_model_invocation` capability on the AI subject. Without that capability, the action is denied with `OutboundDirectiveAIForbidden`.
3. **Vault broker holds the credential.** L4.2 (deferred) holds the provider API key as `KEY_ENCRYPT` / `MAC_GENERATE` material. The AI never sees it. The capability id `vault.external_model_credential.<provider>` is the handle.
4. **L8 evaluates the connection.** When the action moves to `executing`, the broker on the AI's behalf opens a connection to `models.openai.com:443`. L8 sees the connection request, matches it against the `AICrossOriginPosture = AI_VAULT_BROKERED_ONLY`, verifies the originating PID is the broker (not the AI) via the kernel correlator, and allows the connection.
5. **Evidence emitted.** `EXTERNAL_MODEL_CALL_BROKERED` STANDARD_24M evidence carrying `provider`, `action_id`, `vault_capability_id`. **Never** the API key, the prompt, or the response. Payload-bearing evidence is owned by the action runtime, not by L8.

Any other path is a constitutional violation. Specifically:

- AI directly calling `models.openai.com` without broker mediation: L8 sees the AI subject originating the connection, matches `AI_*_ONLY`, denies with `AI_DIRECT_INTERNET_DENIED` FOREVER evidence.
- AI extracting the API key from the broker's response: prevented at L4.2 (the broker returns operation results, not key material; INV-003 / INV-018).
- AI proxying through a local non-broker service: prevented because the local service would itself need an `OutboundGrant` for `models.openai.com`, which is denied unless the local service is the vault broker.

### 5.8 Backend integration

L8 binds to:

- **nftables (primary)** for kernel-side packet filtering. Per-grant nftables rules are generated and loaded; rules carry `comment` strings with the grant id (`expg:<ulid>` / `outg:<ulid>`) for diagnostic correlation, but never any subject id or sensitive value.
- **systemd-resolved** (or equivalent DoT-capable resolver) for DNS-over-TLS discipline. Resolver allowlist is constitutional (§5.4).
- **iptables fallback** if nftables is unavailable. Falling back emits `BACKEND_DEGRADED_NFTABLES_TO_IPTABLES` FOREVER evidence; the degradation is sticky for the boot session and the operator is warned via the L7 chrome zone.
- **WireGuard** for VPN tunnels (NAT-bound; a peer is a subject in its own right, with its own `OutboundDirective`).
- **Linux network namespaces** for per-group isolation. One `aios-net-<group_id>` namespace per group. Cross-namespace traffic is forbidden by default; allow rules are policy-driven and emit evidence.

### 5.9 Bypass discipline

Bypass attempts using raw sockets, `AF_PACKET`, or direct device access are denied at the **kernel-policy level** (sandbox capability constraints from S3.2: `process.allowed_capabilities` does not include `CAP_NET_RAW` or `CAP_NET_ADMIN` for AI / unprivileged subjects). The nftables ingress hook adds a defense-in-depth check: even if a process somehow obtained `CAP_NET_RAW`, the egress from its network namespace is filtered.

## 6. gRPC surface

`aios.network.v1alpha1.NetworkPolicyService` is the only entry point for network policy mutation. All RPCs require authenticated subjects per S5.1; mutating RPCs additionally require the subject to hold the corresponding capability.

```proto
service NetworkPolicyService {
  // Posture
  rpc GetNetworkPosture(GetNetworkPostureRequest) returns (GetNetworkPostureResponse);
  rpc SetNetworkPosture(SetNetworkPostureRequest) returns (SetNetworkPostureResponse);

  // Inbound exposure
  rpc RequestExposure(RequestExposureRequest) returns (RequestExposureResponse);
  rpc RevokeExposure(RevokeExposureRequest) returns (RevokeExposureResponse);
  rpc ListActiveExposures(ListActiveExposuresRequest) returns (ListActiveExposuresResponse);
  rpc StreamExposureState(StreamExposureStateRequest) returns (stream ExposureStateEvent);

  // Outbound discipline
  rpc GrantOutbound(GrantOutboundRequest) returns (GrantOutboundResponse);
  rpc RevokeOutbound(RevokeOutboundRequest) returns (RevokeOutboundResponse);
  rpc ListActiveOutbound(ListActiveOutboundRequest) returns (ListActiveOutboundResponse);

  // Connection evaluation (synchronous; on connect() that is not loopback)
  rpc EvaluateConnection(EvaluateConnectionRequest) returns (EvaluateConnectionResponse);

  // Health, version, info
  rpc GetNetworkPolicyInfo(GetNetworkPolicyInfoRequest) returns (GetNetworkPolicyInfoResponse);
}
```

### 6.1 `GetNetworkPosture` / `SetNetworkPosture`

```proto
message GetNetworkPostureRequest {}
message GetNetworkPostureResponse {
  NetworkPosture posture = 1;
  google.protobuf.Timestamp set_at = 2;
  string set_by_subject_canonical_id = 3;
  string evidence_record_id = 4;       // NETWORK_POSTURE_CHANGED
}

message SetNetworkPostureRequest {
  NetworkPosture target_posture = 1;
  string action_id = 2;                // S0.1 envelope; must be approved
  string reason = 3;                   // bounded; sanitized; recorded in evidence
}
message SetNetworkPostureResponse {
  oneof result {
    SetNetworkPostureAccepted accepted = 1;
    NetworkPolicyError error = 2;
  }
}
message SetNetworkPostureAccepted {
  NetworkPosture new_posture = 1;
  string evidence_record_id = 2;
}
```

`SetNetworkPosture` to `PUBLIC_ROUTABLE` requires recovery-mode + co-signer + STRONG approval, identically to a PUBLIC exposure grant.

### 6.2 `RequestExposure` / `RevokeExposure`

```proto
message RequestExposureRequest {
  string surface_id = 1;                  // L7.5 surface or service surface id
  InboundExposureClass target_class = 2;  // LAN | PUBLIC; LOOPBACK is auto-granted, RECOVERY is recovery-only
  string interface_name = 3;              // optional; "" for default
  repeated string cidr_allow_list = 4;    // for PUBLIC, empty = deny-all by default
  google.protobuf.Duration requested_ttl = 5;  // capped per class (§5.6)
  string action_id = 6;                   // S0.1 envelope; carries approval
  string reason = 7;
}

message RequestExposureResponse {
  oneof result {
    ExposureGrant grant = 1;              // ACTIVE
    NetworkPolicyError error = 2;
  }
}

message ExposureGrant {
  string grant_id = 1;                    // "expg:" + ulid26
  string surface_id = 2;
  InboundExposureClass class = 3;
  string interface_name = 4;
  repeated string cidr_allow_list = 5;
  google.protobuf.Timestamp issued_at = 6;
  google.protobuf.Timestamp expires_at = 7;
  ExposureApprovalState state = 8;
  string approver_subject_canonical_id = 9;
  string co_signer_subject_canonical_id = 10;  // empty for LAN, required for PUBLIC
  string evidence_record_id = 11;          // EXPOSURE_GRANTED
  bytes ed25519_signature = 12;
}

message RevokeExposureRequest {
  string grant_id = 1;
  string reason = 2;
  string action_id = 3;
}
message RevokeExposureResponse {
  bool revoked = 1;
  string evidence_record_id = 2;           // EXPOSURE_REVOKED
}
```

### 6.3 `GrantOutbound` / `RevokeOutbound`

```proto
message GrantOutboundRequest {
  string subject_canonical_id = 1;
  OutboundDirective directive = 2;
  repeated AllowlistEntry allowlist = 3;   // applies when directive = ALLOW_LIST_ONLY
  string manifest_hash = 4;                // hex_lower(BLAKE3(jcs(manifest)))[:32]
  google.protobuf.Duration requested_ttl = 5;
  string action_id = 6;
  string reason = 7;
}

message GrantOutboundResponse {
  oneof result {
    OutboundGrant grant = 1;
    NetworkPolicyError error = 2;
  }
}

message OutboundGrant {
  string grant_id = 1;                     // "outg:" + ulid26
  string subject_canonical_id = 2;
  OutboundDirective directive = 3;
  repeated AllowlistEntry allowlist = 4;
  string manifest_hash = 5;
  google.protobuf.Timestamp issued_at = 6;
  google.protobuf.Timestamp expires_at = 7;
  ExposureApprovalState state = 8;
  string approver_subject_canonical_id = 9;
  string evidence_record_id = 10;          // OUTBOUND_GRANT_ISSUED
  bytes ed25519_signature = 11;
}

message AllowlistEntry {
  AllowlistEntryKind kind = 1;
  string value = 2;                        // FQDN, IP, CIDR, etc.
  repeated uint32 ports = 3;               // empty = any (rare; usually pinned)
  ProtocolFamily protocol = 4;
}

message RevokeOutboundRequest {
  string grant_id = 1;
  string reason = 2;
  string action_id = 3;
}
message RevokeOutboundResponse {
  bool revoked = 1;
  string evidence_record_id = 2;           // OUTBOUND_GRANT_REVOKED
}
```

### 6.4 `EvaluateConnection`

The synchronous decision RPC called by the kernel correlator at every `connect()` that is not loopback. Performance budget p95 < 500 µs.

```proto
message EvaluateConnectionRequest {
  string subject_canonical_id = 1;         // derived by correlator, NOT caller-asserted
  string remote_host = 2;                  // resolved IP or FQDN being connected to
  uint32 remote_port = 3;
  ProtocolFamily protocol = 4;
  bool inbound = 5;                        // false = outbound
  google.protobuf.Timestamp connect_at = 6;
}
message EvaluateConnectionResponse {
  EvaluateConnectionDecision decision = 1;
  string matched_grant_id = 2;             // expg:<ulid> or outg:<ulid> if granted; "" otherwise
  NetworkPolicyError error = 3;
}
enum EvaluateConnectionDecision {
  EVALUATE_CONNECTION_DECISION_UNSPECIFIED = 0;
  ALLOW = 1;
  DENY = 2;
  DEGRADE = 3;                              // allow at lower class (e.g., truncated allowlist)
}
```

### 6.5 `ListActiveExposures` / `ListActiveOutbound` / `StreamExposureState` / `GetNetworkPolicyInfo`

Diagnostic and live-state RPCs. `StreamExposureState` is consumed by L7.5's chrome banner and by L9 admin tooling.

```proto
message ListActiveExposuresRequest {}
message ListActiveExposuresResponse {
  repeated ExposureGrant grants = 1;
}

message ListActiveOutboundRequest {}
message ListActiveOutboundResponse {
  repeated OutboundGrant grants = 1;
}

message StreamExposureStateRequest {}
message ExposureStateEvent {
  string grant_id = 1;
  ExposureApprovalState state = 2;
  google.protobuf.Timestamp at = 3;
}

message GetNetworkPolicyInfoRequest {}
message GetNetworkPolicyInfoResponse {
  string schema_version = 1;               // "aios.network.v1alpha1"
  NetworkPosture current_posture = 2;
  uint32 active_exposures = 3;
  uint32 active_outbound_grants = 4;
  bool nftables_in_use = 5;
  bool iptables_fallback_active = 6;
  bool dns_over_tls_enforced = 7;
}

message NetworkPolicyError {
  NetworkPolicyErrorCode code = 1;
  string message = 2;
  string offending_field = 3;
}
```

## 7. Performance contract

| Operation                                     | p50      | p95      | p99      | Hard timeout |
| --------------------------------------------- | -------- | -------- | -------- | ------------ |
| `EvaluateConnection` (subject in fast path)   | < 100 µs | < 500 µs | < 2 ms   | 50 ms        |
| `EvaluateConnection` (FQDN allowlist resolve) | < 1 ms   | < 5 ms   | < 25 ms  | 500 ms       |
| `RequestExposure` (LAN, post-approval)        | < 10 ms  | < 50 ms  | < 200 ms | 1 s          |
| `RequestExposure` (PUBLIC, post-approval)     | < 50 ms  | < 200 ms | < 1 s    | 5 s          |
| `RevokeExposure` (kernel filter teardown)     | < 50 ms  | < 250 ms | < 1 s    | 5 s          |
| `GrantOutbound`                               | < 10 ms  | < 50 ms  | < 200 ms | 1 s          |
| `RevokeOutbound`                              | < 10 ms  | < 50 ms  | < 250 ms | 1 s          |
| Mid-flight grant revocation (full tear-down)  | < 100 ms | < 250 ms | < 1 s    | 5 s          |
| `SetNetworkPosture`                           | < 100 ms | < 500 ms | < 2 s    | 10 s         |

The 250 ms p95 mid-flight revocation budget is the **constitutional revocation deadline**: an operator-initiated revoke must tear active TCP connections within 250 ms. This is enforced by sending nftables `REJECT` with appropriate ICMP code + per-flow `conntrack -D` to drop established flows.

## 8. Adversarial robustness

### 8.1 Allowlist tampering

Every `OutboundGrant.allowlist` is part of the Ed25519-signed grant. Tampering by editing the on-disk grant fails signature verification at the next `EvaluateConnection`; the connection is denied; `OUTBOUND_OUTSIDE_MANIFEST` evidence is emitted because the subject has no valid grant. Persistent tampering attempts trigger `TAMPER_DETECTED` (per S3.1) with the offending file path and grant id.

### 8.2 Subject-id spoofing on connect()

The subject id in `EvaluateConnectionRequest` is **not caller-asserted**. It is derived by a kernel-side correlator implemented as netlink `NLMSG_CONNECT` events + an eBPF `cgroup/connect4` and `cgroup/connect6` probe that reads the originating `cgroup` and `pid`, looks them up in the L8 service's `pid → subject_id` table, and stamps the canonical id. A process cannot impersonate another subject by lying to L8.

The correlator's `pid → subject_id` table is built at process spawn time from S5.1's session+process bindings. A PID without a binding (e.g., a host-spawned process not under AIOS supervision) produces `SUBJECT_ID_CORRELATOR_FAILURE`, and the connection is denied — no fallback to "permissive when unknown".

### 8.3 IPv6 / IPv4 dual-stack bypass

Both stacks are evaluated per connection. nftables rules duplicate across the `inet` family table; the eBPF probe handles both `connect4` and `connect6`. A subject that cannot reach `1.2.3.4` is also unable to reach `::ffff:1.2.3.4` or `2001:db8::1.2.3.4` derivatives.

### 8.4 DNS rebinding

For each `HOST_FQDN` allowlist entry, the resolver pins a fixed IP set per the FQDN's TTL window. Subsequent name resolutions during that window return the pinned set. A response that returns IPs outside the pinned set is treated as drift and the FQDN entry transitions to `AWAITING_OPERATOR` for re-approval; mid-window connections to the original pinned IPs continue.

### 8.5 ARP spoofing on LAN

`LAN_SUBNET` grants pin to `(MAC, IP)` at activation. The pinning is kept as kernel `arp` table entries with `permanent` flag. ARP responses claiming a different MAC for a pinned IP emit `LAN_PEER_DRIFT_DETECTED` evidence and the grant transitions to `AWAITING_OPERATOR`.

### 8.6 Mid-flight grant revocation

Per the §7 budget, an operator-initiated `RevokeExposure` or `RevokeOutbound` must tear active connections within 250 ms p95. The implementation uses:

1. nftables rule removal (immediate; new SYNs blocked).
2. `conntrack -D` for established flows matching the grant.
3. Sending `RST` (TCP) or ICMP-PORT-UNREACHABLE (UDP) to break established flows.

The teardown is evidence-traced: `EXPOSURE_REVOKED` (or `OUTBOUND_GRANT_REVOKED`) records the elapsed time as a redacted observation field.

### 8.7 FQDN allowlist fan-out attack

An attacker registers a domain with 10000 A records and asks for it to be allowlisted. Mitigation: the I9 fan-out cap of 16 IPs per FQDN. Beyond 16, the entry is denied with `ALLOWLIST_FQDN_FANOUT_EXCEEDED` and evidence is emitted. The attacker cannot enumerate 10000 internal addresses by requesting one allowlist entry.

### 8.8 Manifest replacement attack

An attacker replaces an app's signed `network_outbound_manifest` on disk with a permissive one. Mitigation: manifest is referenced by `manifest_hash` (BLAKE3-128) inside the `OutboundGrant`. At each `EvaluateConnection`, the broker verifies the in-memory manifest hash matches the `OutboundGrant.manifest_hash`. Mismatch → connection denied with `OUTBOUND_OUTSIDE_MANIFEST` and `TAMPER_DETECTED`.

### 8.9 nftables → iptables fallback abuse

iptables has weaker semantics than nftables (no families, no atomic transactions). An attacker hopes the host has nftables but the L8 service has fallen back to iptables to gain holes. Mitigation: fallback is sticky for the boot session, emits FOREVER evidence at the moment of fallback, and the L7 chrome zone shows a persistent banner. The operator sees "AIOS is running on degraded firewall backend" as a non-dismissable warning.

## 9. Telemetry contract

All metrics MUST use bounded label cardinality. **Subject id, group id, user id, host, FQDN, IP, port are NEVER labels.** They appear only in evidence records.

| Metric                                       | Type      | Labels (closed)                                                               |
| -------------------------------------------- | --------- | ----------------------------------------------------------------------------- |
| `network_policy_evaluation_total`            | counter   | `result` (allow/deny/degrade), `error_code` (closed `NetworkPolicyErrorCode`) |
| `network_policy_evaluation_latency_seconds`  | histogram | `result`                                                                      |
| `network_active_exposures`                   | gauge     | `class` (closed `InboundExposureClass`)                                       |
| `network_active_outbound_grants`             | gauge     | `directive` (closed `OutboundDirective`)                                      |
| `network_outbound_connection_total`          | counter   | `result` (allow/deny)                                                         |
| `network_grant_revocation_total`             | counter   | `reason_class` (operator/ttl/policy/breach/posture_change)                    |
| `network_external_model_call_brokered_total` | counter   | `provider` (closed list: openai/anthropic/google/azure/local/other)           |
| `network_lan_subnet_drift_total`             | counter   | none                                                                          |
| `network_posture_state`                      | gauge     | `posture` (closed `NetworkPosture`)                                           |
| `network_iptables_fallback_active`           | gauge     | none (1 = fallback active, 0 = nftables in use)                               |
| `network_ai_direct_internet_denied_total`    | counter   | none                                                                          |
| `network_raw_socket_bypass_attempted_total`  | counter   | none                                                                          |
| `network_public_exposure_active`             | gauge     | none                                                                          |
| `network_public_exposure_heartbeat_total`    | counter   | none                                                                          |

Cardinality budget: ≤ 100 active label tuples per metric. With 5 `NetworkPosture` × 5 `OutboundDirective` × 4 `InboundExposureClass` × 16 error codes the worst-case product remains under budget.

## 10. Evidence record types (queued for S3.1)

The following record types are queued for the next S3.1 consolidation cycle. They are **closed**: adding or removing requires a versioned schema change. After consolidation, the `RecordType` vocabulary grows by 18 entries (current 87 → 105).

| Record type                             | Retention class | Trigger                                                                                                |
| --------------------------------------- | --------------- | ------------------------------------------------------------------------------------------------------ |
| `NETWORK_POSTURE_CHANGED`               | `FOREVER`       | Host `NetworkPosture` changes; carries `from`, `to`, `set_by_subject_canonical_id`, `action_id`        |
| `EXPOSURE_REQUESTED`                    | `STANDARD_24M`  | `RequestExposure` accepted into `AWAITING_OPERATOR`                                                    |
| `EXPOSURE_GRANTED`                      | `FOREVER`       | LAN or PUBLIC `ExposureGrant` reaches `ACTIVE`; carries `class`, `cidr_allow_list`, approver(s)        |
| `EXPOSURE_DENIED`                       | `EXTENDED_60M`  | `RequestExposure` denied by policy or invariant                                                        |
| `EXPOSURE_REVOKED`                      | `EXTENDED_60M`  | `RevokeExposure` succeeded; carries elapsed teardown time                                              |
| `EXPOSURE_TERMINATED_TTL_EXPIRED`       | `EXTENDED_60M`  | `ExposureGrant` reached `expires_at` while `ACTIVE` and was auto-terminated                            |
| `PUBLIC_EXPOSURE_HEARTBEAT`             | `STANDARD_24M`  | 5-minute heartbeat while `class = PUBLIC` and state `ACTIVE`                                           |
| `OUTBOUND_GRANT_ISSUED`                 | `STANDARD_24M`  | `GrantOutbound` succeeded; carries `directive`, `manifest_hash`                                        |
| `OUTBOUND_GRANT_REVOKED`                | `EXTENDED_60M`  | `RevokeOutbound` or auto-revoke after manifest breach                                                  |
| `OUTBOUND_OUTSIDE_MANIFEST`             | `FOREVER`       | A subject's connection attempt was outside its declared manifest                                       |
| `OUTBOUND_DEGRADED_TO_LOOPBACK_AUTO`    | `FOREVER`       | A subject's `OutboundDirective` was auto-degraded after repeated breaches                              |
| `ALLOWLIST_FQDN_FANOUT_EXCEEDED`        | `EXTENDED_60M`  | A `HOST_FQDN` resolved to > 16 IPs at evaluation time                                                  |
| `LAN_SUBNET_DRIFT_DETECTED`             | `STANDARD_24M`  | A `LAN_SUBNET`-pinned grant's CIDR drifted; grant transitioned to `AWAITING_OPERATOR`                  |
| `LAN_PEER_DRIFT_DETECTED`               | `EXTENDED_60M`  | A pinned `(MAC, IP)` peer's MAC changed; possible ARP spoofing                                         |
| `AI_DIRECT_INTERNET_DENIED`             | `FOREVER`       | An AI subject attempted a direct external connection without vault broker mediation                    |
| `EXTERNAL_MODEL_CALL_BROKERED`          | `STANDARD_24M`  | A vault-brokered external model call succeeded; carries `provider`, `action_id`, `vault_capability_id` |
| `BACKEND_DEGRADED_NFTABLES_TO_IPTABLES` | `FOREVER`       | nftables unavailable; iptables fallback chosen                                                         |
| `RAW_SOCKET_BYPASS_ATTEMPTED`           | `FOREVER`       | A subject attempted to open a raw / packet socket outside policy                                       |

Retention class distribution for the 18 additions: `FOREVER` × 7, `EXTENDED_60M` × 5, `STANDARD_24M` × 6.

Append authority: only the L8 NetworkPolicyService process may emit these record types. Forgery from any other subject is hard-denied at the S3.1 engine surface and emits a `TAMPER_DETECTED` record per S3.1 §11.

## 11. Cross-spec dependencies and follow-up queue

| Spec              | Direction | What this contract contributes / consumes                                                                                                                                            |
| ----------------- | --------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| S0.1              | consumer  | `RequestExposure`, `GrantOutbound`, `RevokeExposure`, `RevokeOutbound`, `SetNetworkPosture` flow as typed actions through the S0.1 lifecycle; `action_id` is recorded on every grant |
| S1.3              | consumer  | `network_outbound_manifest` is stored as an AIOS-FS object; `manifest_hash` is computed via S1.3 chunk discipline                                                                    |
| S2.1              | producer  | New closed query field `subject.network_outbound_directive` and `target.exposure_class` queued for next S2.1 consolidation                                                           |
| S2.3              | producer  | New closed condition fields `subject.network_outbound_directive`, `subject.ai_external_posture`, `target.exposure_class` queued for next S2.3 consolidation                          |
| S2.4              | producer  | Three new primitives queued: `network_subject_outbound_class(subject_id)`, `network_active_exposure_class(surface_id)`, `network_external_model_call_brokered_only(subject_id)`      |
| S3.1              | producer  | 18 new record types queued (§10)                                                                                                                                                     |
| S3.2              | consumer  | `SandboxProfile.network.mode` is the per-process floor; this contract enforces most-restrictive-wins between sandbox and subject directive                                           |
| S4.1              | consumer  | Per-group network namespace `aios-net-<group_id>`; cross-namespace traffic forbidden by default; INV-011 binding                                                                     |
| S5.1              | consumer  | Subject `is_ai`, `is_recovery_mode`, `primary_group_id` drive directive selection and AI cross-origin posture                                                                        |
| S5.3 (deferred)   | consumer  | Approval strength (STRONG required for PUBLIC); approval TTL caps (`TTL_LONG = 4h`)                                                                                                  |
| S5.4 (deferred)   | consumer  | Co-signer requirement for PUBLIC exposure                                                                                                                                            |
| L0 INV-006        | enforcer  | Localhost-default exposure FSM (§3.2) is the L8 implementation of INV-006                                                                                                            |
| L0 INV-008        | enforcer  | Default-deny rules at every grant decision and at every connection evaluation                                                                                                        |
| L0 INV-011        | enforcer  | Per-group network namespace + `CrossGroupAccessForbidden` hard-deny at `EvaluateConnection`                                                                                          |
| L0 INV-002        | enforcer  | AI subjects never receive `ALLOW_INTERNET`; vault-brokered external-model pattern is the only path                                                                                   |
| L0 (candidate)    | producer  | One candidate constitutional invariant `NETWORK_DEFAULT_DENY_OUTBOUND` queued for next L0 INV catalog revision                                                                       |
| L4.2 Vault Broker | consumer  | Holds external-model credentials; mediates AI external calls; INV-018 binding                                                                                                        |
| L7.5 Web renderer | consumer  | `WebExposureState` is renderer-side declaration; this contract is enforcer; `StreamExposureState` drives chrome banner                                                               |
| L8 (sibling)      | producer  | `NetworkPolicyService` is the network policy plane; the hardware graph (separate sub-spec) and DNS/VPN management (S8.3) consume this                                                |
| L9 Observability  | consumer  | Telemetry metrics (§9) and evidence records (§10) feed the L9 admin surface; chrome banner for degraded backends                                                                     |

### 11.1 Cross-spec follow-ups queued (NOT applied here)

- **S2.3** to add closed condition fields `subject.network_outbound_directive`, `subject.ai_external_posture`, `target.exposure_class` and a constitutional hard-deny candidate `OutboundDirectiveAIForbidden` (already implicit in this contract; promoting to S2.3 hard-deny chain is the consolidation target).
- **S2.4** to add primitives `network_subject_outbound_class(subject_id)`, `network_active_exposure_class(surface_id)`, `network_external_model_call_brokered_only(subject_id)`.
- **S3.1** to absorb the 18 new record types into the closed `RecordType` enum and the retention-class table.
- **S2.1** to add the closed query fields.
- **L0** to promote `NETWORK_DEFAULT_DENY_OUTBOUND` into the constitutional invariant catalog (next L0 revision).

## 12. Worked examples

### Example 1 — Operator opens evidence viewer (LOOPBACK exposure)

```text
Setup:
  Subject: family:alice (HUMAN_USER, primary_group_id = family)
  Web renderer (L7.5) starts; needs to bind a port for the chrome+stream client.

Sequence:
  L7.5 → L8.RequestExposure {
    surface_id: "wsurf:evidence_viewer",
    target_class: LOOPBACK,
    interface_name: "lo",
    cidr_allow_list: ["127.0.0.1/32", "::1/128"],
    requested_ttl: 24h,
    action_id: act_<ulid>,
    reason: "default loopback bind for evidence viewer"
  }

L8 evaluation:
  - InboundExposureClass = LOOPBACK is the constitutional default.
  - No policy approval required.
  - State: DRAFT → AWAITING_OPERATOR (auto) → GRANTED → ACTIVE within < 1 ms.
  - nftables rule generated: tcp dport <port> ip daddr 127.0.0.1 accept; rest drop.

Result:
  ExposureGrant returned with state = ACTIVE.
  Evidence: EXPOSURE_REQUESTED STANDARD_24M, EXPOSURE_GRANTED FOREVER (with class = LOOPBACK).
  L7.5 chrome zone shows no LAN/PUBLIC banner (loopback is the silent default).
  Outbound discipline for the evidence viewer is unaffected.
```

### Example 2 — AI agent attempts direct external API call (denied), then brokered call (allowed)

```text
Setup:
  Subject: family:agent:research-bot (is_ai = true, primary_group_id = family)
  AICrossOriginPosture: AI_VAULT_BROKERED_ONLY (operator pre-approved)
  Active OutboundGrant: directive = ALLOW_LOOPBACK_ONLY (default for AI)
  Vault holds OpenAI API key under capability id vault.external_model_credential.openai

Phase 1 — direct attempt:
  Agent's adapter naively does TCP connect("api.openai.com", 443).
  Kernel correlator stamps subject = "family:agent:research-bot".
  L8.EvaluateConnection {
    subject: "family:agent:research-bot",
    remote_host: <openai_ip>,
    remote_port: 443,
    protocol: TCP,
    inbound: false
  }
  Decision pipeline:
    - effective directive (sandbox ∩ subject) = ALLOW_LOOPBACK_ONLY
    - destination is not loopback → DENY
    - subject is_ai = true → emit AI_DIRECT_INTERNET_DENIED FOREVER
  Connection refused; SYN never leaves the host.
  Evidence: AI_DIRECT_INTERNET_DENIED carrying subject_canonical_id, remote_host (redacted to provider class "external"), action_id (if present).
  Telemetry: network_ai_direct_internet_denied_total += 1.

Phase 2 — typed action through broker:
  Agent submits action aios.network.external_model_call {
    target.provider: "openai",
    target.model_id: "gpt-5",
    target.request_hash: <BLAKE3-128>
  }
  S2.3 evaluates: requires capability external_model_invocation. Granted. Action approved.
  L4.2 vault broker (PID = broker_pid) opens TCP connect("api.openai.com", 443) on agent's behalf.
  Kernel correlator stamps subject = "_system:vault:broker" (broker is the originator, not the agent).
  L8.EvaluateConnection: broker subject has OutboundGrant to api.openai.com:443; allow.
  Connection succeeds. Broker performs MAC_GENERATE on the request, sends, reads response, returns to agent.
  Agent never sees the API key.
  Evidence:
    - EXTERNAL_MODEL_CALL_BROKERED STANDARD_24M (provider=openai, action_id, vault_capability_id)
    - Action runtime emits its own EXECUTION_* records for the action lifecycle (per S0.1).
  Telemetry: network_external_model_call_brokered_total{provider="openai"} += 1.

Constitutional outcome:
  AI never had network reach to api.openai.com. The credential was used without the AI seeing its bytes. INV-002, INV-003, INV-018 all bound.
```

### Example 3 — Operator wants to expose evidence viewer to LAN for a tablet

```text
Setup:
  Subject: family:alice (HUMAN_USER), tablet on 192.168.1.0/24.
  Renderer is currently EXPOSURE_LOOPBACK.
  Operator action: aios.web.GrantLANExposure → funnels to L8.RequestExposure(LAN).

Sequence:
  L8.RequestExposure {
    surface_id: "wsurf:evidence_viewer",
    target_class: LAN,
    interface_name: "eno1",
    cidr_allow_list: ["192.168.1.0/24"],
    requested_ttl: 4h,
    action_id: act_<ulid>,
    reason: "tablet read-only access for evidence review"
  }

  State: DRAFT → AWAITING_OPERATOR.
  Evidence: EXPOSURE_REQUESTED STANDARD_24M.
  S2.3 evaluates: requires HUMAN_USER + active session + interactive consent.
  S5.3 (deferred) prompts alice; alice consents.

  State: AWAITING_OPERATOR → GRANTED.
  Evidence: EXPOSURE_GRANTED FOREVER (class = LAN, cidr_allow_list, approver = family:alice, expires_at = now + 4h).
  nftables rule: tcp dport <port> ip saddr 192.168.1.0/24 accept; ip daddr 0.0.0.0/0 drop.
  State: GRANTED → ACTIVE within < 200 ms (per §7 budget).

Live state:
  StreamExposureState fires; L7.5 chrome adds <aios-lan-exposure-banner> showing "AIOS reachable on LAN: 192.168.1.42:9443".
  Heartbeat: PUBLIC_EXPOSURE_HEARTBEAT does NOT fire (LAN, not PUBLIC). LAN heartbeat is governed by L7.5 §5.4 (WEB_LAN_EXPOSURE_ACTIVE STANDARD_24M every 6h).

Forbidden direct LAN → PUBLIC:
  Operator later tries L8.RequestExposure(PUBLIC) for the same surface. L8 sees an ACTIVE LAN grant for that surface.
  Returns NetworkPolicyError {
    code: EXPOSURE_FORBIDDEN_TRANSITION,
    message: "must downgrade to LOOPBACK before escalating to PUBLIC",
    offending_field: "target_class"
  }
  Evidence: EXPOSURE_DENIED EXTENDED_60M.
  Operator must first L8.RevokeExposure(grant_id), then L8.RequestExposure(PUBLIC).

TTL expiry:
  4 hours later, grant transitions ACTIVE → TERMINATED.
  Evidence: EXPOSURE_TERMINATED_TTL_EXPIRED EXTENDED_60M.
  nftables rule removed; existing connections RST'd within 250 ms.
  L7.5 chrome banner removed via StreamExposureState.
```

## 13. Acceptance criteria

- [ ] All 9 enums in §4 are closed and have exactly the values declared.
- [ ] `NetworkPosture` lifecycle is documented; transitions to `PUBLIC_ROUTABLE` require recovery-mode + co-signer + STRONG approval.
- [ ] `OutboundDirective` defaults are `DENY_ALL` for AI subjects, `ALLOW_LOOPBACK_ONLY` for unprivileged service subjects.
- [ ] `InboundExposureClass = LOOPBACK` is the constitutional default; `LAN` requires policy approval; `PUBLIC` requires recovery-mode + co-signer + STRONG approval + FOREVER evidence + ≤ 4h TTL + 5-minute heartbeat.
- [ ] Forbidden direct `LAN → PUBLIC` transition is rejected with `EXPOSURE_FORBIDDEN_TRANSITION`.
- [ ] `ExposureApprovalState` FSM is closed with the exact transitions in §4.7; backward and self-loop transitions are rejected.
- [ ] `NetworkPolicyErrorCode` is closed with 16 entries.
- [ ] `AICrossOriginPosture` is closed with 3 values; AI subjects never granted `ALLOW_INTERNET` or arbitrary `ALLOW_LIST_ONLY`.
- [ ] `NetworkPolicyService` gRPC surface implemented with the RPCs in §6.
- [ ] Allowlist composition rules (append-only, FQDN fan-out ≤ 16, LAN_SUBNET drift detection, DoT resolver list constitutional) implemented per §5.4.
- [ ] Per-app `network_outbound_manifest` is signed, append-only at subject level, breach-degraded after 3 breaches in 24h.
- [ ] AI external-model-call brokered pattern (§5.7) is the only path for AI external calls; direct attempts emit `AI_DIRECT_INTERNET_DENIED` FOREVER.
- [ ] Backend integration: nftables primary, iptables fallback emits `BACKEND_DEGRADED_NFTABLES_TO_IPTABLES` FOREVER and is sticky for the boot session; per-group Linux network namespaces enforce INV-011.
- [ ] Bypass attempts (raw sockets, AF_PACKET, direct device access) are denied at sandbox capability + nftables ingress hook; emit `RAW_SOCKET_BYPASS_ATTEMPTED` FOREVER.
- [ ] Subject id at `EvaluateConnection` is derived by kernel correlator (netlink + eBPF), not caller-asserted.
- [ ] Mid-flight grant revocation tears active connections within 250 ms p95.
- [ ] Telemetry conforms to §9: subject id / group id / user id / host / FQDN / IP / port are NEVER labels.
- [ ] All 18 record types in §10 are emitted at the correct retention classes.
- [ ] All three worked examples in §12 produce the specified outcomes.
- [ ] L0 INV-002, INV-006, INV-008, INV-011 are bound by this contract; the candidate `NETWORK_DEFAULT_DENY_OUTBOUND` is queued for L0 catalog revision.

## 14. Open deferrals

These are intentionally out of scope for S8.1 and tracked elsewhere:

- **DNS / DoT mechanics** — resolver backend (systemd-resolved tuning), per-group resolver overrides, DNSSEC validation policy, mDNS / Avahi gating. Deferred to `03_dns_vpn_management.md` (S8.3).
- **VPN orchestration** — WireGuard tunnel lifecycle (peer enrollment, key rotation, tunnel up/down), TUN_VPN integration. Deferred to S8.3.
- **Hardware graph** — device detection, classification, lifecycle for network adapters. Deferred to `01_hardware_graph.md`.
- **Firmware trust** — network adapter firmware update classification. Deferred to `04_firmware_trust.md` (S8.4).
- **Multi-host network policy federation** — when AIOS becomes multi-host, exposure grants on host A may need awareness on host B. Deferred.
- **Per-flow QoS** — bandwidth shaping, latency budgets per `OutboundDirective`. Deferred.
- **WireGuard mesh as group transport** — using WG to bridge groups across hosts. Deferred to S8.3.
- **Network policy diff UI** — operator-facing visualization of "what changed" between two `NetworkPosture` snapshots. Deferred to L9 admin tooling.
- **External-model provider catalog** — closed list of allowed providers (OpenAI, Anthropic, Google, Azure, local) and per-provider capability shape. The `provider` enum used in §9 telemetry is treated as closed but its authoritative source is queued for an L5 cognitive-core sub-spec.
- **Per-capability outbound budgets** — per-day / per-hour byte and request budgets that bind to `OutboundGrant`. Deferred.
- **Subject-id correlator under cgroup-v1** — the eBPF cgroup/connect4 path requires cgroup-v2; pure cgroup-v1 hosts are not supported by this contract. Deferred (and effectively retired by S3.2's cgroup-v2 preference).
- **PUBLIC exposure TLS cert acquisition** — the public-CA cert chain acquisition path is delegated to L4.2 (vault) per S7.5 §5.5; full mechanics deferred.

## 15. See also

- [S0.1 — Action Envelope and Lifecycle](../XX_Cross_Cutting/01_action_envelope_lifecycle.md)
- [S2.3 — Policy Kernel](../L4_Policy_Identity_Vault/01_policy_kernel.md)
- [S2.4 — Verification Grammar](../L9_Observability_Admin_Operations/02_verification_grammar.md)
- [S3.1 — Evidence Log](../L9_Observability_Admin_Operations/01_evidence_log.md)
- [S3.2 — Sandbox Composition](../L6_Apps_Packages_Compatibility/04_sandbox_composition.md)
- [S4.1 — Namespace Layout](../L2_AIOS_FS/05_namespace_layout.md)
- [S5.1 — Identity Model](../L4_Policy_Identity_Vault/03_identity_model.md)
- [S7.5 — Web Renderer](../L7_Interaction_Renderers/05_web_renderer.md)
- [S8.2 — GPU Resource Model](05_gpu_resource_model.md)
- [L0 INV-006 — Web UI localhost default](../L0_Governance_Evidence_Safety/04_invariants.md)
- [L0 INV-008 — Default deny in policy](../L0_Governance_Evidence_Safety/04_invariants.md)
- [L0 INV-011 — Cross-group access forbidden by default](../L0_Governance_Evidence_Safety/04_invariants.md)
- [L0 INV-002 — AI proposes, never executes](../L0_Governance_Evidence_Safety/04_invariants.md)
- [Rev.1 §18 — Hardware and Network](../../001.AI-OS.NET--SPECREV.1/02_SPECIFICATION.md)
- [L8 Overview](00_overview.md)
- [Rev.2 Master Index](../00_MASTER_INDEX.md)

Status: REAL
Evidence: E1
