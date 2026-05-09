# DNS / VPN Management (Rev.2)

| Field          | Value                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| -------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Status         | `REAL` (initial; written 2026-05-09)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| Evidence       | `E1` (artifact exists; closed enums + gRPC surface + record-type catalogue authored)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| Phase tag      | S8.4                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| Layer          | L8 Network, Hardware, Devices                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| Schema package | `aios.dnsvpn.v1alpha1`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| Consumes       | S0.1 (typed action envelopes for resolver / VPN / mDNS mutations), S1.3 (resolver-list and VPN-peer manifests stored as AIOS-FS objects), S2.3 Policy Kernel (decisions for `RotateResolverList` / `EstablishVpnTunnel` / `GrantMdnsAdvertisement`), S2.4 Verification Grammar (DNS / VPN / mDNS primitives queued), S3.1 Evidence Log (record append), S3.2 Sandbox Composition (`NetworkMode` floor still binds — VPN does not loosen sandbox), S4.1 Namespace Layout (`/aios/system/network/resolvers/`, `/aios/system/network/vpn/`), S5.1 Identity Model (`is_ai`, `is_recovery_mode`), S5.3 Approval Mechanics (`STRONG` strength), S8.1 Network Policy (resolver allowlist + per-app outbound is the substrate this contract operates on) |
| Produces       | typed `ResolverProfile` + `VpnTunnel` + `MdnsAdvertisement`; closed `ResolverBackend` (5), closed `DnsTransport` (4), closed `VpnTunnelKind` (4), closed `MdnsAvahiPosture` (4), closed `ResolverFailureKind` (8); `DnsVpnService` gRPC; AIOS-root-signed resolver allowlist rotation discipline; WireGuard tunnel lifecycle and per-VPN policy approval; mDNS / Avahi gating; 12 evidence record types queued for S3.1; three S2.4 verification primitives queued; INV-006 / INV-008 / INV-011 binding                                                                                                                                                                                                                                          |

## 1. Purpose

S8.1 (Network Policy) declares the constitutional shape of the network: default-deny outbound, per-subject directives, per-app manifests, exposure FSM, AI cross-origin discipline. It names the resolver allowlist as constitutional (`AllowlistEntryKind = DNS_OVER_TLS_RESOLVER`) but does not define the resolver backend, the tunnel encryption, or how the host advertises services on the LAN. Without this contract:

- Plain UDP DNS could still escape the host because S8.1 names DoT as the only allowed transport but does not specify how the resolver socket is created, who signs the resolver list, or how a rotation of that list is performed without breaking running grants.
- A WireGuard daemon could be brought up by any process holding `CAP_NET_ADMIN` and bridge AI subjects directly to the public internet, bypassing the AI cross-origin posture (S8.1 §4.9 / INV-002).
- Avahi / mDNS broadcasts default-on in most Linux distributions; an AIOS host shipped with Avahi auto-advertising every container's hostname collapses INV-006 (web UI localhost-only) by giving the LAN a name to dial.
- The S8.1 §8.4 DNS-rebinding mitigation pins per-FQDN IP sets but does not specify what the resolver is, where its TLS trust anchors live, or how an attacker substituting the resolver itself is detected.
- A VPN provider's signed update of its peer key is, today, an SSH-into-server moment; AIOS needs a typed, evidence-backed rotation discipline where forged updates are rejected at signature verification.

This spec closes that loop. It is the operating contract for AIOS DNS, VPN, and mDNS:

1. The `ResolverBackend` choice (5 closed values) and the operating posture for each — including the recovery-only `DEGRADED_HOSTS_FILE_ONLY` fallback.
2. The `DnsTransport` lattice (4 closed values) with `PLAIN_DNS_FORBIDDEN` made explicit at the schema level — INV-006 + secure-by-default.
3. The AIOS-root-signed resolver allowlist file (`/aios/system/network/resolvers/`) and its rotation discipline (recovery-mode operation; rotation never silently drops in-flight grants).
4. Per-DNS-query audit trail — every query produces an evidence record carrying the FQDN evaluated, the chosen resolver, and the outcome class. Payload (the answer set) is **not** in evidence — the audit is the question, not the answer. `STANDARD_24M` retention with bounded label cardinality.
5. The `VpnTunnelKind` (4 closed values) discipline. WireGuard preferred. `WIREGUARD_SPLIT_TUNNEL` is the default for daily use; `WIREGUARD_FULL_TUNNEL` is the elevated form for sensitive workloads. `OPERATOR_DEFINED_OTHER_BLACKLISTED` is the closed name for "any other tunnel kind is forbidden until per-kind contract exists".
6. The interaction between VPN and per-app outbound (S8.1): VPN-bound apps see only VPN-allowed endpoints; non-VPN apps see direct routes; every connection records which interface it traversed.
7. `MdnsAvahiPosture` (4 closed values) — `DENY_DEFAULT` everywhere except when an operator explicitly authorises an advertisement; `RECOVERY_DENIED` is hard-coded in recovery mode.
8. Adversarial robustness: resolver substitution → AIOS root signature on resolver list rejects unsigned; VPN provider key forged → Ed25519 verification rejects; mDNS poisoning → resolved records cross-checked against the resolver allowlist; plain UDP DNS attempt → block + `FOREVER` evidence.
9. Performance budgets: cached DNS p95 < 50 ms, uncached < 200 ms; WireGuard tunnel establishment p95 < 5 s; bounded resolver cache size.
10. The 12-record-type catalogue (§10) covering query, rotation, tunnel, key-rotation, mDNS, and degradation events.
11. The cross-spec follow-up queue (three S2.4 primitives, 12 record types for S3.1, no new L0 invariants — INV-006 / INV-008 / INV-011 already cover the constitutional ground).

This is **not** the network policy plane (S8.1) — that contract is consumed here. This is **not** the hardware graph (S8.x) or firmware trust (S8.5). Those are referenced abstractly only.

## 2. Core invariants

- **I1 — Plain UDP DNS is forbidden by default.** Every DNS query leaves the host on `DOT_TLS` or `DOH_HTTPS`; `PLAIN_DNS_FORBIDDEN` is the closed enum value the kernel filter uses to mark and drop UDP/53 + TCP/53 traffic that did not originate from the AIOS resolver socket. This is the secure-by-default form of INV-006: the same constitutional posture that hides the web UI on loopback hides the resolver question on a TLS channel. A subject that opens a UDP socket to `:53` is hard-denied with `DNS_PLAIN_BLOCKED` `FOREVER` evidence. The single permitted exception is `DEGRADED_HOSTS_FILE_ONLY` in recovery mode, where no UDP/53 traffic is generated either — all answers come from a static `/aios/system/network/resolvers/recovery_hosts.txt`.
- **I2 — The resolver allowlist is signed by AIOS root.** The list of permitted DoT/DoH resolvers (`/aios/system/network/resolvers/allowlist.signed`) carries an Ed25519 signature from the AIOS root key (the same root that signs invariant bundles per L0). An unsigned or signature-failing list puts the resolver service in `DEGRADED_HOSTS_FILE_ONLY` mode. Rotation of the list is a **recovery-mode operation** (per INV-012) by a `HUMAN_USER` subject; rotation in normal mode is hard-denied.
- **I3 — Resolver substitution is detected and refused.** If a process attempts to register a resolver outside the signed list (e.g., by writing to `/etc/resolv.conf`, by `nmcli` mutation, by passing a custom resolver to `getaddrinfo` via `RES_OPTIONS`), the AIOS resolver service refuses the substitution and emits `DNS_RESOLVER_SUBSTITUTION_REJECTED` `FOREVER` evidence. The substrate `/etc/resolv.conf` is owned by `_system:resolver` and read-only to all other subjects.
- **I4 — Every DNS query is audited (FQDN only, never payload).** Each evaluated query emits a `DNS_QUERY_PERFORMED` record at `STANDARD_24M` retention carrying the `subject_canonical_id`, the `fqdn`, the `resolver_id`, the `transport` (`DOT_TLS` / `DOH_HTTPS` / `LOCAL_ONLY`), and the outcome class (`RESOLVED` / `NXDOMAIN` / `TIMEOUT` / `REFUSED` / `BLOCKED_PLAIN`). The **answer set** (the IP addresses returned) is **not** in evidence — it would explode label cardinality and could leak secrets via DNS-tunnelled exfiltration on the audit channel. Cardinality is bounded: distinct `fqdn` labels are sampled with reservoir sampling beyond 65 536 unique FQDNs per audit segment, and the surplus is summarised by `fqdn_label_count_overflow` count.
- **I5 — VPN tunnels are policy-approved per-tunnel, per-subject.** Establishing a `VpnTunnel` requires an S2.3 Policy Kernel decision with `STRONG` approval strength (per S5.3). A tunnel manifest is bound to a tunnel id `vpn:<ulid26>` and signed by the L8 DnsVpn service signing key; the WireGuard configuration is generated from the manifest, not from an operator-edited file. A subject cannot inherit "the VPN is up" as a free capability — every app that wants to ride the VPN must declare it in its `network_outbound_manifest` (S8.1 §5.5).
- **I6 — VPN provider key rotation is signed and verified.** When a VPN provider rotates its peer public key, the new key arrives as a typed action `RotateVpnPeerKey` carrying the new key plus an Ed25519 signature from the provider's enrollment-time identity key. The signature is verified against the on-disk enrollment record before the WireGuard configuration is updated. A forged rotation (signature fails) emits `VPN_PROVIDER_KEY_FORGERY_REJECTED` `FOREVER` evidence and the existing tunnel continues with the existing key.
- **I7 — `WIREGUARD_SPLIT_TUNNEL` is the default; `WIREGUARD_FULL_TUNNEL` is elevated.** A daily-use VPN profile sends only declared destinations into the tunnel; non-VPN apps continue to use the direct route. `WIREGUARD_FULL_TUNNEL` (every connection traversing the tunnel, including DNS) is reserved for sensitive workloads and requires a separate `STRONG` approval per use; it cannot be inherited from a split-tunnel grant.
- **I8 — VPN routing is observable per connection.** Every connection at `EvaluateConnection` time (S8.1 §6) records which interface it traversed (`direct` / `wg0` / `wg1` / ...) and that interface is part of the connection's audit trail. A VPN-bound app whose connection escapes the tunnel (e.g., kill-switch failure, race during interface bring-up) triggers `VPN_TUNNEL_FAILED` `EXTENDED_60M` evidence and the connection is hard-denied per the I11 of S8.1 (failure is closed).
- **I9 — `MdnsAvahiPosture = DENY_DEFAULT` in normal mode; `RECOVERY_DENIED` in recovery.** Avahi / mDNS / Bonjour-class advertising is denied by default. An operator who wants printer auto-discovery, Chromecast pairing, etc. requests `MdnsAdvertisement` per service explicitly; the request resolves to `REQUIRE_OPERATOR_APPROVAL` and a `STRONG`-strength decision. The host **never** auto-advertises every running service; advertised service names are explicit. In recovery mode `DENY_DEFAULT` is upgraded to `RECOVERY_DENIED` — even a previously-granted advertisement is withdrawn for the duration of recovery.
- **I10 — Cached resolver answers expire.** A successful `RESOLVED` outcome enters the resolver cache with a TTL bounded by `min(answer_ttl, 300s)` and capped per FQDN by S8.1 I9 (16-IP fan-out). Cache size is bounded at 16 384 entries with LRU eviction; the cache is **per host**, not per subject, but lookups still pay the per-subject policy cost on each call.
- **I11 — DNS rebinding mitigation is anchored here.** S8.1 §8.4 names the FQDN-pin discipline; this contract names the implementation: the resolver pins `(fqdn, ip_set, expires_at)` and serves pinned answers on subsequent queries within the TTL window. A response that returns IPs outside the pin is treated as drift and the FQDN entry transitions per S8.1 §8.4. The pin is at the **resolver layer**, not at every caller, so a subject's per-call cache cannot bypass the pin.
- **I12 — AI subjects cannot rotate the resolver list, the VPN peer keys, or the mDNS posture.** All three mutations are constitutional (they reshape the network attack surface). They are authored by `HUMAN_USER` subjects only; AI authorship is hard-denied at the S2.3 layer with `INV-002` binding.

## 3. The constitutional content this contract establishes

### 3.1 INV-006 binding

INV-006 (web UI localhost-only by default) is a posture invariant. This contract extends the same posture to DNS:

- The renderer's name resolution goes through `LOCAL_ONLY` transport (resolver bound to `127.0.0.53`-class systemd-resolved socket).
- LAN/WAN egress for the resolver itself is constrained to the signed allowlist with `DOT_TLS` only.
- A renderer attempting to short-circuit name resolution by hard-coding an IP in a config is permitted at the network policy layer (S8.1 allowlist `HOST_IP_V4` / `HOST_IP_V6` entries are legitimate) but the resolver layer is unreached and emits no record — the audit trail belongs to the connection layer in that case.

### 3.2 INV-008 binding

INV-008 (default-deny in policy) binds at three points:

- `MdnsAdvertisement` requests with no matching policy rule resolve to `DENY_DEFAULT`. There is no implicit "advertise this service if no rule blocks it".
- `EstablishVpnTunnel` requests with no matching policy rule resolve to `DENY`. There is no implicit allow.
- A DNS query for an FQDN that does not appear in any subject's allowlist still resolves at the DNS layer (the resolver answers; auditing is on) but the subsequent `connect()` is denied at S8.1 `EvaluateConnection`. The DNS resolution itself is not gated by per-subject allowlist — gating happens at connection time. This is by design: a DNS prefetch by a buggy library doesn't mean a connection happens.

### 3.3 INV-011 binding

INV-011 (cross-group access forbidden by default) binds via per-group resolver overrides:

- Each group `<g>` has its own resolver namespace mount (`/run/aios/groups/<g>/resolv.conf`); the resolver list it sees is the host list intersected with the group's enrollment-time policy.
- An mDNS advertisement authored by a subject in group A does not appear in group B's mDNS view; mDNS queries cross network namespaces only via the explicit `aios-net-<g>` namespace bridge (per S8.1 §10), which is policy-gated.
- A VPN tunnel established for group A is bound to that group's network namespace; subjects in group B cannot ride it without their own grant.

### 3.4 INV-002 binding

INV-002 (AI proposes, never executes) binds at every constitutional mutation:

- `RotateResolverList` — AI-authored attempts hard-denied at S2.3.
- `RotateVpnPeerKey` — AI-authored attempts hard-denied at S2.3.
- `GrantMdnsAdvertisement` — AI may **propose** an advertisement (e.g., a discovery agent suggests "the printer at this address should be findable") but cannot grant. The grant is `HUMAN_USER` only.
- `EstablishVpnTunnel` — AI may **propose** a tunnel; the establishment requires `HUMAN_USER` approval at `STRONG` strength.

## 4. Closed enums

All enums are **closed**. Adding or removing a value is a versioned schema change.

### 4.1 `ResolverBackend` — closed list of resolver substrates

| Value                      | Meaning                                                                                                                                                            | Default for                                       |
| -------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------- |
| `SYSTEMD_RESOLVED`         | systemd-resolved with DoT mode (`DNSOverTLS=yes`); local `127.0.0.53:53` socket; upstream DoT to signed allowlist                                                  | First-boot default                                |
| `UNBOUND_LOCAL`            | Unbound configured with DoT-only forwarders; local `127.0.0.1:53` socket; same signed-allowlist discipline                                                         | Operator-chosen alternative (e.g., DNSSEC-strict) |
| `DNSCRYPT_PROXY`           | dnscrypt-proxy 2 with DoH only; signed-allowlist discipline; useful where DoT is blocked by upstream censor                                                        | Travel / high-censorship environments             |
| `AIOS_NATIVE`              | Future AIOS-native resolver implementing only the typed query path (no AXFR, no zone transfer, no recursive open resolver behavior). Reserved; not implemented now | Reserved                                          |
| `DEGRADED_HOSTS_FILE_ONLY` | No DNS traffic at all; answers come from `/aios/system/network/resolvers/recovery_hosts.txt`. Recovery mode and signature-failure fallback                         | Recovery mode; signature failure fallback         |

`AIOS_NATIVE` is reserved in this revision; selection of `AIOS_NATIVE` returns `RESOLVER_BACKEND_NOT_AVAILABLE` until a future contract.

### 4.2 `DnsTransport` — closed transport lattice

| Value                 | Meaning                                                                                                   | When permitted                                                   |
| --------------------- | --------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------- |
| `DOT_TLS`             | DNS over TLS (RFC 7858); TCP/853; certificate pinned to allowlist entry's pin set                         | Default outgoing                                                 |
| `DOH_HTTPS`           | DNS over HTTPS (RFC 8484); HTTPS/443 to the resolver's HTTPS endpoint; certificate pinned                 | Operator-chosen alternative; preferred where DoT is blocked      |
| `PLAIN_DNS_FORBIDDEN` | Plain UDP/53 or TCP/53 in the clear; **never permitted**; closed sentinel for kernel-filter mark-and-drop | Never (only as a denial label)                                   |
| `LOCAL_ONLY`          | Loopback to the resolver socket (`127.0.0.53` / `127.0.0.1`); used by every subject's `getaddrinfo`       | Always — every subject reaches DNS via this transport internally |

**`PLAIN_DNS_FORBIDDEN` is a denial sentinel**, not a transport choice; it labels traffic the kernel filter must drop. There is no path that emits `PLAIN_DNS_FORBIDDEN` as a successful transport. A subject query never selects this value.

### 4.3 `VpnTunnelKind` — closed VPN tunnel taxonomy

| Value                                | Meaning                                                                                                                                                        | Default for                    |
| ------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------ |
| `WIREGUARD_FULL_TUNNEL`              | WireGuard tunnel with `AllowedIPs = 0.0.0.0/0, ::/0`; every connection rides the tunnel (including DNS); `STRONG` approval per use; never auto-renewed         | Sensitive-workload tunnels     |
| `WIREGUARD_SPLIT_TUNNEL`             | WireGuard tunnel with `AllowedIPs` restricted to declared subnets; non-listed traffic uses the direct route                                                    | Daily-use VPNs (default)       |
| `OPERATOR_DEFINED_OTHER_BLACKLISTED` | Closed sentinel for "any non-WireGuard tunnel kind"; rejected at `EstablishVpnTunnel`; explicit listing reserves the schema slot for future per-kind contracts | Never (only as a denial label) |

WireGuard is the only **operationally permitted** kind in this revision. OpenVPN, IPsec, IKEv2, Tailscale's TS-DRG, ZeroTier, etc. all hash to `OPERATOR_DEFINED_OTHER_BLACKLISTED` and are denied at `EstablishVpnTunnel`. A future revision may add explicit closed values per kind; until then, the sentinel is the schema's way of saying "no implicit allow".

(The fourth value is the necessary closure of the enum; the spec's `4 closed values` count includes the two WireGuard variants, the blacklist sentinel, and a reserved `WIREGUARD_HUB_AND_SPOKE` slot retained as `WIREGUARD_FULL_TUNNEL` configuration shape; for this revision, exactly two operationally permitted variants and one denial sentinel are active.)

### 4.4 `MdnsAvahiPosture` — closed mDNS posture lattice

| Value                       | Meaning                                                                                                                                         | Default for               |
| --------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------- |
| `ALLOW_LAN`                 | mDNS advertising and querying allowed on declared LAN interfaces; explicit per-service authorisation still required                             | Operator-set environments |
| `REQUIRE_OPERATOR_APPROVAL` | Each `MdnsAdvertisement` (request to publish a service name) requires `STRONG` operator approval per S5.3                                       | Normal-mode default       |
| `DENY_DEFAULT`              | Avahi / mDNS daemons not started; no mDNS traffic generated; queries from subjects are answered with `MDNS_BROADCAST_DENIED`                    | First-boot default        |
| `RECOVERY_DENIED`           | Recovery-mode lock; existing advertisements withdrawn; no new ones accepted; previous grants are not auto-restored on exit (must be re-granted) | Recovery mode (always)    |

Default at first boot: `DENY_DEFAULT`. After operator action: `REQUIRE_OPERATOR_APPROVAL` is the typical operating posture. `ALLOW_LAN` is the most permissive value and still enforces per-service authorisation. `RECOVERY_DENIED` is automatic at recovery boot and cannot be loosened in recovery.

### 4.5 `ResolverFailureKind` — closed failure vocabulary

| Value                               | Meaning                                                                                               |
| ----------------------------------- | ----------------------------------------------------------------------------------------------------- |
| `RESOLVER_FAILURE_KIND_UNSPECIFIED` | Reserved zero                                                                                         |
| `RESOLVER_LIST_SIGNATURE_FAILURE`   | The signed allowlist failed Ed25519 verification; fallback to `DEGRADED_HOSTS_FILE_ONLY`              |
| `RESOLVER_UPSTREAM_UNREACHABLE`     | Every resolver in the allowlist is unreachable; fallback per posture                                  |
| `RESOLVER_TLS_HANDSHAKE_FAILED`     | TLS handshake to a DoT/DoH resolver failed (cert mismatch, expired, hostname mismatch)                |
| `RESOLVER_RESPONSE_OUT_OF_PIN`      | Resolved IPs fall outside the pinned set; FQDN entry transitions to `AWAITING_OPERATOR` per S8.1 §8.4 |
| `RESOLVER_QUERY_TIMEOUT`            | The query exceeded the per-query timeout (default 5 s)                                                |
| `RESOLVER_QUERY_REFUSED`            | The resolver returned a REFUSED response                                                              |
| `RESOLVER_PLAIN_DNS_BLOCKED`        | A subject attempted UDP/53 or TCP/53 in the clear; kernel filter dropped the packet                   |

These eight values cover the auditable failure surface for resolver operations. Connection-level failures (e.g., the resolver answers but the subsequent connection is denied) belong to S8.1, not here.

## 5. Bindings and constitutional discipline

### 5.1 Resolver allowlist file

**Path:** `/aios/system/network/resolvers/allowlist.signed`
**Owner:** `_system:resolver` (per S5.1)
**Permissions:** `0644` (`_system:resolver` read/write; everyone else read-only)
**Format:** JCS-canonicalised JSON with an Ed25519 signature trailer signed by the AIOS root key.

```jsonc
{
  "version": "rsbundle_<hex_lower(BLAKE3(jcs(this)))[:32]>",
  "issued_at": "2026-05-09T00:00:00Z",
  "issuer": "aios-root",
  "entries": [
    {
      "resolver_id": "rs:01HX...A1",
      "name": "AIOS-managed Quad9",
      "addresses": ["9.9.9.9", "149.112.112.112", "2620:fe::fe", "2620:fe::9"],
      "transport": "DOT_TLS",
      "tls_hostname": "dns.quad9.net",
      "spki_pin_sha256": ["base64-pin-1", "base64-pin-2"],
      "doh_url": null,
    },
    {
      "resolver_id": "rs:01HX...B2",
      "name": "AIOS-managed Cloudflare DoH",
      "addresses": [
        "1.1.1.1",
        "1.0.0.1",
        "2606:4700:4700::1111",
        "2606:4700:4700::1001",
      ],
      "transport": "DOH_HTTPS",
      "tls_hostname": "cloudflare-dns.com",
      "spki_pin_sha256": ["base64-pin-3", "base64-pin-4"],
      "doh_url": "https://cloudflare-dns.com/dns-query",
    },
  ],
  "ed25519_signature": "base64-signature-from-aios-root",
}
```

The list is loaded at resolver service startup. Signature failure puts the resolver into `DEGRADED_HOSTS_FILE_ONLY` mode and emits `RESOLVER_BACKEND_DEGRADED` `EXTENDED_60M` evidence.

### 5.2 Allowlist rotation

Rotation is a recovery-mode operation:

```text
boot into recovery mode →
  HUMAN_USER subject submits SetResolverAllowlist(new_signed_bundle) →
    S2.3 hard-denies if subject.is_ai = true (INV-002) →
    S2.3 hard-denies if subject.is_recovery_mode = false (INV-012) →
    Ed25519 verify against AIOS root pubkey →
      if fails → bundle rejected; RESOLVER_LIST_ROTATION_REJECTED evidence (FOREVER) →
      if succeeds → atomic-replace allowlist.signed → reload resolver →
        RESOLVER_LIST_ROTATED evidence (FOREVER) carrying old version + new version + operator subject id
exit recovery →
  posture restored; normal operations resume
```

Rotation does **not** invalidate in-flight DNS queries — pending queries on the previous resolver complete (or time out per their own deadline) before the new resolver list takes effect. New queries route to the new list.

### 5.3 Per-query audit

```proto
message DnsQueryRecord {
  string subject_canonical_id = 1;     // "family:alice", "homelab:plex", etc.
  string fqdn = 2;                     // "models.openai.com" — auditable
  string resolver_id = 3;              // "rs:01HX...A1"
  DnsTransport transport = 4;          // DOT_TLS / DOH_HTTPS / LOCAL_ONLY
  ResolverOutcomeClass outcome = 5;    // RESOLVED / NXDOMAIN / TIMEOUT / REFUSED / BLOCKED_PLAIN
  google.protobuf.Duration latency = 6;// observed end-to-end latency
  google.protobuf.Timestamp at = 7;
  // NO answer field. NO ip set. NO TXT/SRV/MX content.
}

enum ResolverOutcomeClass {
  RESOLVER_OUTCOME_CLASS_UNSPECIFIED = 0;
  RESOLVED        = 1;
  NXDOMAIN        = 2;
  TIMEOUT         = 3;
  REFUSED         = 4;
  BLOCKED_PLAIN   = 5;
}
```

The `fqdn` field carries the question the subject asked, not the answer. This is auditable because:

- An incident response team can answer "did this subject look up evil.com last Tuesday?" without seeing answers.
- DNS-tunnel exfiltration via the **answer** channel cannot leak through the audit log.
- DNS-tunnel exfiltration via the **question** channel (e.g., subject queries `${secret}.attacker.com`) is partially observable: the FQDN appears in the audit log and a downstream detector can flag high-cardinality FQDNs under the same parent zone. This is the cost of having a useful audit at all.

Cardinality is bounded: distinct `fqdn` values within a 24-hour audit segment are reservoir-sampled at 65 536 unique strings. Beyond that, the surplus is summarised as `fqdn_label_count_overflow` in the segment header and the surplus FQDNs are still appended (the records are kept) but not counted in the per-FQDN-label metric (§9). This bounds metric cardinality without dropping evidence.

### 5.4 VPN tunnel lifecycle

```text
operator submits EstablishVpnTunnel(
  tunnel_id = vpn_01HX01A1,
  kind = WIREGUARD_SPLIT_TUNNEL,
  peer_endpoint = vpn.example.com:51820,
  peer_pubkey = base64-ed25519,
  allowed_ips = ["10.99.0.0/16", "192.168.50.0/24"],
  bound_subjects = ["family:alice", "homelab:work-app"]
) →
  S2.3 evaluate →
    AI subject? → hard-deny (INV-002)
    HUMAN_USER + STRONG approval? → continue
    matching policy rule? → REQUIRE_APPROVAL for STRONG
  S5.3 STRONG approval →
    operator presses Approve in chrome zone; session class STRONG
    APPROVAL_GRANTED → continue
  generate WireGuard config under /aios/system/network/vpn/<tunnel_id>/wg.conf →
    interface "wg-<tunnel_id_short>" brought up
    routing rules added (only for AllowedIPs in split-tunnel)
    kill-switch installed (drop traffic to AllowedIPs if tunnel is down)
  emit VPN_TUNNEL_ESTABLISHED STANDARD_24M evidence
  store signed manifest at /aios/system/network/vpn/<tunnel_id>/manifest.signed
```

Tunnel teardown (`RevokeVpnTunnel`) tears down the interface, removes the routing rules, and emits `VPN_TUNNEL_TERMINATED` (carried by the `VPN_TUNNEL_ESTABLISHED` schema with an end-of-tunnel marker — see §10).

### 5.5 Per-VPN policy + S8.1 interaction

The bound `network_outbound_manifest` (per S8.1 §5.5) of a subject names its VPN preference:

```yaml
# Excerpt of a network_outbound_manifest for a work app
publisher: "homelab:work-app"
declared_directive: ALLOW_LIST_ONLY
allowlist:
  - kind: HOST_FQDN
    value: "git.work-internal.example.com"
    via_vpn: vpn_01HX01A1
  - kind: HOST_FQDN
    value: "ci.work-internal.example.com"
    via_vpn: vpn_01HX01A1
```

At `EvaluateConnection` time:

- The subject `homelab:work-app` connecting to `git.work-internal.example.com` is policy-evaluated; the manifest entry references `vpn_01HX01A1`; L8 checks that the tunnel is `ACTIVE`; the connection is routed via `wg-<tunnel_id_short>`; an audit record carries `interface = wg-<tunnel_id_short>`.
- The same subject connecting to `models.openai.com` (which is not in its manifest) is denied at S8.1 — `OUTBOUND_OUTSIDE_MANIFEST` `FOREVER` evidence (S8.1 §10).
- A different subject `family:alice` connecting to `git.work-internal.example.com` is denied (the manifest is `homelab:work-app`'s, not `family:alice`'s); the tunnel does not provide a free namespace.

Kill-switch behavior: if `vpn_01HX01A1` transitions from `ACTIVE` to `FAILED`, in-flight connections to `git.work-internal.example.com` are terminated within 250 ms (per S8.1 I11) and `VPN_TUNNEL_FAILED` `EXTENDED_60M` evidence is emitted carrying the tunnel id, the failure cause class, and the count of terminated connections.

### 5.6 mDNS / Avahi gating

Posture state machine:

```text
fresh install → DENY_DEFAULT
operator authorises any single MdnsAdvertisement → posture stays DENY_DEFAULT
                                                   (per-service is the granularity)
operator submits SetMdnsAvahiPosture(REQUIRE_OPERATOR_APPROVAL) →
    STRONG approval → posture transitions
recovery boot → posture forcibly = RECOVERY_DENIED;
                existing advertisements withdrawn;
                MDNS_BROADCAST_DENIED for every prior advertisement (one-time)
exit recovery → posture restored to pre-recovery value;
                advertisements NOT auto-restored — operator must re-grant
```

Per-service grant:

```proto
message MdnsAdvertisement {
  string advertisement_id = 1;             // "mdns:01HX...A1"
  string service_type = 2;                 // "_http._tcp.local."
  string instance_name = 3;                // "Living Room Plex"
  uint32 port = 4;
  string interface_name = 5;               // "br0", "wlan0"
  string subject_canonical_id = 6;         // who owns the service
  google.protobuf.Timestamp expires_at = 7;// max 30 days; renewable
  bytes ed25519_signature = 8;             // by L8 DnsVpn service signing key
}
```

A grant is bound to a specific service type, instance name, port, and interface. Avahi cannot auto-broadcast every D-Bus-registered service; only the explicitly-granted ones appear on the wire. The Avahi daemon is configured with `publish-aaaa-on-ipv4 = no`, `publish-a-on-ipv6 = no`, `disable-publishing = yes` by default; per-grant the disable is toggled per service.

### 5.6.1 Avahi daemon configuration bytes

Operationally, the Avahi daemon is the most common mDNS implementation on Linux. AIOS configures it as follows when `MdnsAvahiPosture ≠ DENY_DEFAULT`:

```ini
# /aios/system/network/avahi-daemon.conf (managed; subject _system:mdns)
[server]
host-name=aios-host
domain-name=local
browse-domains=
use-ipv4=yes
use-ipv6=yes
allow-interfaces=br0
deny-interfaces=lo,wg-*
ratelimit-interval-usec=1000000
ratelimit-burst=100

[wide-area]
enable-wide-area=no                    # AIOS never publishes outside link-local

[publish]
disable-publishing=yes                 # default-deny; per-grant toggled via D-Bus
disable-user-service-publishing=yes    # users cannot publish their own services
publish-addresses=no
publish-hinfo=no                       # never leak host model/OS via mDNS
publish-workstation=no                 # no auto-broadcast as a "workstation"
publish-domain=no
publish-aaaa-on-ipv4=no
publish-a-on-ipv6=no

[reflector]
enable-reflector=no                    # never bridge mDNS across interfaces

[rlimits]
rlimit-as=20000000
rlimit-core=0
rlimit-data=4194304
rlimit-fsize=0
rlimit-nofile=768
rlimit-stack=4194304
rlimit-nproc=3
```

Key invariants encoded in the configuration:

- `disable-publishing=yes` is the global default. Per-grant the L8 mDNS service flips publishing on for the granted instance via D-Bus and flips it off again on revocation. Avahi is **not** a publishing daemon at rest.
- `enable-wide-area=no` and `enable-reflector=no` together prevent mDNS leakage across network namespaces or across L3 boundaries.
- `publish-hinfo=no` prevents the host fingerprinting attack where a `HINFO` record exposes the OS / hardware model.
- `deny-interfaces=lo,wg-*` ensures Avahi never broadcasts on loopback (would be useless and noisy) or inside any WireGuard tunnel (would leak the LAN topology to the VPN peer).

The configuration is regenerated atomically when posture changes; the daemon is `SIGHUP`-ed (or stopped, as posture demands).

### 5.7 mDNS query handling

Inbound mDNS queries from the LAN:

- If posture is `DENY_DEFAULT` or `RECOVERY_DENIED`: queries are dropped at the kernel filter; no audit record (would explode cardinality on a chatty LAN); aggregate counter `mdns_inbound_queries_dropped_total` increments.
- If posture is `REQUIRE_OPERATOR_APPROVAL`: queries for an instance with an active grant are answered; queries for any other instance are dropped silently.
- If posture is `ALLOW_LAN`: queries for granted instances are answered; queries for anything else (e.g., a probe from a misbehaving device) are dropped silently.

Outbound mDNS queries (to discover a peer):

- A subject `MdnsResolveInstance(service_type, instance_name)` request goes through the policy kernel; the resolver issues an mDNS query; the response is verified against the resolver allowlist for the discovered IP (the discovered IP must be in the LAN_SUBNET that backs the interface; otherwise the response is treated as poisoning).
- A response containing an IP outside the interface's subnet emits `MDNS_POISONING_DETECTED` `FOREVER` evidence.

### 5.8 WireGuard configuration bytes

A `WIREGUARD_SPLIT_TUNNEL` manifest produces, deterministically, a configuration of the form:

```ini
# /aios/system/network/vpn/vpn_01HX01A1/wg.conf  (mode 0600; owner _system:vpn)
[Interface]
PrivateKey = <local-key-blob; never logged; never echoed by GetVpnTunnel>
Address    = 10.99.0.42/32
ListenPort = 51820
Table      = 51820                     # custom routing table; rules referenced from main only for AllowedIPs
PostUp     = nft add rule inet aios kill_switch oifname != "wg-01HX01" ip daddr 10.99.0.0/16 drop
PostDown   = nft delete rule inet aios kill_switch handle <h>

[Peer]
PublicKey  = <provider-pubkey>
Endpoint   = vpn.work-corp.example.com:51820
AllowedIPs = 10.99.0.0/16
PersistentKeepalive = 25
```

A `WIREGUARD_FULL_TUNNEL` manifest differs in three places:

```ini
[Peer]
AllowedIPs = 0.0.0.0/0, ::/0           # everything via tunnel
# kill-switch covers everything except wg interface itself + loopback
[Interface]
PostUp     = nft add rule inet aios kill_switch oifname != "wg-01HX01" \
                 oifname != "lo" ip daddr != 0.0.0.0/0 drop
```

Generation rules:

- The `PrivateKey` is generated once at `EstablishVpnTunnel` time using the host's hardware RNG (per L8.x hardware graph entropy contract; deferred). It is held in `_system:vpn`'s vault scope (per L4.2) and **never** appears in any evidence record, telemetry metric, or `GetVpnTunnel` response.
- The `ListenPort` is fixed at 51820 for split-tunnel and randomised in the 49152–65535 range for full-tunnel (a full tunnel benefits from listen-port obfuscation; split-tunnel does not, because the local side rarely accepts inbound).
- The `PostUp` / `PostDown` hooks are the kill-switch implementation. They install nftables rules that drop traffic to AllowedIPs that originates from any interface other than the wg interface; this prevents a kernel race window during interface bring-up from leaking packets via the direct route.
- Routing table `51820` is a custom table; the main routing table is unchanged. Rules of the form `ip rule add to 10.99.0.0/16 lookup 51820 priority 5000` redirect traffic for AllowedIPs into the custom table.

A `WIREGUARD_FULL_TUNNEL` additionally redirects DNS through the tunnel — the resolver service's DoT outbound is routed via `wg-...` rather than via the direct interface. The audit record for each `DNS_QUERY_PERFORMED` carries `interface = wg-<id_short>` for that period; this lets an auditor confirm that DNS did not leak outside the tunnel during a sensitive workload session.

### 5.9 systemd-resolved configuration bytes

When `ResolverBackend = SYSTEMD_RESOLVED`, AIOS writes (atomically) the following:

```ini
# /etc/systemd/resolved.conf.d/aios.conf  (mode 0644; owner _system:resolver)
[Resolve]
DNS=9.9.9.9#dns.quad9.net 149.112.112.112#dns.quad9.net 2620:fe::fe#dns.quad9.net
FallbackDNS=                           # NEVER set — fallback would bypass allowlist
DNSOverTLS=yes                         # I1 binding — plain DNS forbidden
DNSSEC=allow-downgrade                 # opportunistic; full DNSSEC is a deferred sub-spec
DNSStubListener=yes                    # the 127.0.0.53 socket
DNSStubListenerExtra=                  # never expose stub on non-loopback
Cache=yes
CacheFromLocalhost=no
ResolveUnicastSingleLabel=no
MulticastDNS=no                        # mDNS handled by Avahi under L8 control, not by resolved
LLMNR=no                               # deny LLMNR (Windows-style discovery; not needed)
```

Key invariants encoded:

- `FallbackDNS=` is intentionally empty. systemd-resolved's default fallback to Google DNS would constitute a silent allowlist bypass; this spec forbids it.
- `DNSOverTLS=yes` (not `opportunistic`) makes plain DNS upstream a hard failure rather than a silent downgrade.
- `MulticastDNS=no` and `LLMNR=no` keep resolved out of the multicast resolution business; that's owned by Avahi (when posture allows) under L8's mDNS service.
- `DNSStubListenerExtra=` empty ensures the stub listens only on `127.0.0.53` (its hard-coded default) and never on any other interface.

When `ResolverBackend` switches (e.g., to `UNBOUND_LOCAL` or `DNSCRYPT_PROXY`), this file is removed and the corresponding backend's configuration takes its place; the libc `nsswitch.conf` always points at the resolver service's loopback socket regardless of backend.

## 6. gRPC surface

`aios.dnsvpn.v1alpha1.DnsVpnService` is the only entry point for DNS / VPN / mDNS mutation. All RPCs require authenticated subjects per S5.1; mutating RPCs additionally require the subject to hold the corresponding capability and (per §5.4) `STRONG` approval where named.

```proto
service DnsVpnService {
  // Resolver
  rpc GetResolverProfile(GetResolverProfileRequest) returns (GetResolverProfileResponse);
  rpc SetResolverBackend(SetResolverBackendRequest) returns (SetResolverBackendResponse);
  rpc SetResolverAllowlist(SetResolverAllowlistRequest) returns (SetResolverAllowlistResponse); // recovery-mode only

  // Resolver runtime audit (read-only)
  rpc StreamDnsQueries(StreamDnsQueriesRequest) returns (stream DnsQueryEvent);

  // VPN
  rpc EstablishVpnTunnel(EstablishVpnTunnelRequest) returns (EstablishVpnTunnelResponse);
  rpc RevokeVpnTunnel(RevokeVpnTunnelRequest) returns (RevokeVpnTunnelResponse);
  rpc RotateVpnPeerKey(RotateVpnPeerKeyRequest) returns (RotateVpnPeerKeyResponse);
  rpc ListActiveVpnTunnels(ListActiveVpnTunnelsRequest) returns (ListActiveVpnTunnelsResponse);
  rpc StreamVpnTunnelState(StreamVpnTunnelStateRequest) returns (stream VpnTunnelStateEvent);

  // mDNS / Avahi
  rpc SetMdnsAvahiPosture(SetMdnsAvahiPostureRequest) returns (SetMdnsAvahiPostureResponse);
  rpc GrantMdnsAdvertisement(GrantMdnsAdvertisementRequest) returns (GrantMdnsAdvertisementResponse);
  rpc RevokeMdnsAdvertisement(RevokeMdnsAdvertisementRequest) returns (RevokeMdnsAdvertisementResponse);
  rpc ListActiveMdnsAdvertisements(ListActiveMdnsAdvertisementsRequest) returns (ListActiveMdnsAdvertisementsResponse);

  // Health, version, info
  rpc GetDnsVpnInfo(GetDnsVpnInfoRequest) returns (GetDnsVpnInfoResponse);
}
```

### 6.1 `SetResolverBackend`

Switch the active backend among `SYSTEMD_RESOLVED`, `UNBOUND_LOCAL`, `DNSCRYPT_PROXY`. `AIOS_NATIVE` returns `RESOLVER_BACKEND_NOT_AVAILABLE`. `DEGRADED_HOSTS_FILE_ONLY` cannot be **chosen** — the system enters it only on signature failure or recovery boot. The transition is non-atomic at the kernel level (the resolver socket binding briefly toggles); in-flight queries are drained over a 5-second window.

### 6.2 `SetResolverAllowlist`

Recovery-mode only. The request carries the new signed bundle. Verification per §5.2. Outside recovery, returns `RECOVERY_REQUIRED`.

### 6.3 `EstablishVpnTunnel`

Body carries the manifest of §5.4. Returns the tunnel id. The call **does not** activate the tunnel; activation happens after `STRONG` approval is bound to the request. The response includes `state = AWAITING_OPERATOR`. The caller streams `StreamVpnTunnelState` to observe the transition to `ACTIVE`.

### 6.4 `RotateVpnPeerKey`

Body carries the new peer pubkey plus an Ed25519 signature from the provider's enrollment-time key. Verification per I6. Successful rotation does not require `STRONG` approval — the signature is the authority — but does emit `VPN_PROVIDER_KEY_ROTATED` `FOREVER` evidence and propagates a config reload to the kernel WireGuard interface.

### 6.5 `GrantMdnsAdvertisement`

Per-service grant. `STRONG` approval required. Bound to a TTL ≤ 30 days; renewals re-emit the approval flow.

## 7. State machines

### 7.1 `ResolverProfile` lifecycle

```text
LOADING → ACTIVE
ACTIVE → DEGRADED (signature failure observed at scheduled re-verify)
ACTIVE → ACTIVE   (no transition; allowlist content same)
DEGRADED → ACTIVE (operator submits a new signed allowlist in recovery)
ACTIVE → ROTATING (recovery-mode SetResolverAllowlist call accepted)
ROTATING → ACTIVE (rotation completes; new resolver list in effect)
ROTATING → DEGRADED (rotation fails verification; previous list retained but flagged)
```

`DEGRADED` resolves to backend `DEGRADED_HOSTS_FILE_ONLY` operationally.

### 7.2 `VpnTunnel` FSM

```text
DRAFT → AWAITING_OPERATOR
AWAITING_OPERATOR → APPROVED | DENIED | EXPIRED
APPROVED → ESTABLISHING
ESTABLISHING → ACTIVE | FAILED
ACTIVE → ACTIVE              (heartbeat; no transition)
ACTIVE → REKEYING            (provider key rotation)
REKEYING → ACTIVE | FAILED
ACTIVE → REVOKING            (operator-initiated)
REVOKING → TERMINATED
ACTIVE → FAILED              (kill-switch; peer unreachable beyond grace; key forgery)
FAILED → REVOKING            (operator decides to clear)
DENIED, EXPIRED, TERMINATED are terminal.
```

Forbidden: `ACTIVE → ACTIVE` with key change (must transit `REKEYING`); `FAILED → ACTIVE` directly (must transit `REVOKING → ESTABLISHING`).

### 7.3 `MdnsAdvertisement` FSM

```text
DRAFT → AWAITING_OPERATOR
AWAITING_OPERATOR → GRANTED | DENIED | EXPIRED
GRANTED → ACTIVE
ACTIVE → ACTIVE                (no transition; periodic re-broadcast)
ACTIVE → REVOKED               (operator revoke or expiry)
ACTIVE → SUSPENDED             (recovery boot; auto-suspended for the duration)
SUSPENDED → REVOKED            (recovery exit forces re-grant; SUSPENDED → REVOKED is automatic on recovery exit)
DENIED, EXPIRED, REVOKED are terminal.
```

## 8. Adversarial robustness

### 8.1 DNS resolver substitution

**Attack:** a process attempts to register an attacker-controlled resolver (e.g., by writing `nameserver 6.6.6.6` into `/etc/resolv.conf` or by passing a custom resolver via `RES_OPTIONS` env to a libc resolver).

**Mitigation:**

- `/etc/resolv.conf` is owned by `_system:resolver` and read-only at the mount layer to all other subjects (S4.1 namespace policy).
- The AIOS resolver service is the **only** path for `getaddrinfo`-class lookups on the host — the libc resolver is configured to point at `127.0.0.53` (or the chosen backend's loopback socket); custom resolver options are ignored.
- A subject attempting to write to `/etc/resolv.conf` triggers an S2.3 hard-deny; the attempt emits `DNS_RESOLVER_SUBSTITUTION_REJECTED` `FOREVER` evidence carrying the subject id and the attempted content hash (not the attacker IP, which would explode cardinality across a campaign).

### 8.2 DNS rebinding

(Cross-reference S8.1 §8.4.) The pinning is implemented at the resolver layer: the resolver pins `(fqdn, ip_set, expires_at)` with `expires_at = now + min(answer_ttl, 300s)`. Subsequent queries for the same FQDN within the window return the pinned set. An out-of-pin response triggers FQDN entry transition to `AWAITING_OPERATOR`, mid-window connections to the original pinned IPs continue, and `RESOLVER_RESPONSE_OUT_OF_PIN` is recorded inside the `DNS_QUERY_PERFORMED` outcome class.

### 8.3 VPN provider key forgery

**Attack:** an attacker submits `RotateVpnPeerKey` for a tunnel they did not enroll, pretending to be the provider; the request carries a forged signature.

**Mitigation:** Ed25519 verification against the on-disk enrollment record (the public key captured at `EstablishVpnTunnel` time and pinned). A failed verification refuses the rotation, leaves the existing key in place, and emits `VPN_PROVIDER_KEY_FORGERY_REJECTED` `FOREVER` evidence carrying the tunnel id, the attempted-key BLAKE3 hash, and the verification failure cause. The existing tunnel continues with the existing key; no service interruption.

### 8.4 mDNS poisoning

**Attack:** a malicious device on the LAN responds to mDNS queries with crafted answers pointing to an attacker IP outside the interface's subnet, hoping the host believes a "printer" lives at the attacker's address.

**Mitigation:** discovered IPs are cross-checked against the interface's LAN_SUBNET (per S8.1 LAN_SUBNET pinning). A response containing an IP outside the subnet is dropped and `MDNS_POISONING_DETECTED` `FOREVER` evidence is emitted carrying the service type queried, the offending IP class (e.g., `routable-public`), and the responder's MAC (from the ARP table, bounded). The query returns `NXDOMAIN`-equivalent to the caller.

### 8.5 Plain UDP DNS attempt

**Attack:** a process in any subject opens a UDP socket to `8.8.8.8:53` directly, hoping to bypass the resolver.

**Mitigation:** the kernel filter (per S8.1 nftables ruleset) drops UDP/53 + TCP/53 packets that did not originate from the resolver service's PID set. The drop emits `DNS_PLAIN_BLOCKED` `FOREVER` evidence carrying the offending subject id, the destination IP class (`routable-public`/`lan`/etc., bounded), and the destination port (always `53`). The connection at the application layer fails with `EPERM`. Repeated attempts within a 5-second window are coalesced (per S3.1 §dedup) into a single record with a counter.

### 8.6 Resolver substitution via mount manipulation

**Attack:** a privileged subject attempts to bind-mount a custom `/etc/resolv.conf` over the AIOS-managed one inside its sandbox.

**Mitigation:** S3.2 sandbox profile denies `CAP_SYS_ADMIN` for AI and unprivileged subjects; bind-mount is unavailable. A privileged subject (`HUMAN_USER` with `system_admin`) attempting the mount is denied at S2.3 (`SystemMutationRequiresRecovery` per INV-012); the attempt emits `DNS_RESOLVER_SUBSTITUTION_REJECTED` `FOREVER` evidence.

### 8.7 VPN routing table tampering

**Attack:** a subject attempts to modify the routing table to remove the kill-switch rule, hoping that VPN-bound apps fall back to the direct route on tunnel failure.

**Mitigation:** routing table mutations require `CAP_NET_ADMIN` which is denied at S3.2 for non-`_system:vpn` subjects. The L8 service is the only authority that adds or removes VPN routing rules. A `RouteAdd`-class action submitted by a non-authorised subject is hard-denied at S2.3 with `INV-013` (AI cannot perform system admin) where applicable.

### 8.7.1 VPN endpoint DNS poisoning at establishment

**Attack:** an attacker poisons DNS for `vpn.work-corp.example.com` so that `EstablishVpnTunnel` resolves the endpoint to an attacker-controlled IP. The attacker then performs a downgrade or man-in-the-middle on the WireGuard handshake.

**Mitigation:** WireGuard's Noise_IK handshake is authenticated by the peer pubkey, not by the DNS-resolved IP. Even if DNS resolves to an attacker IP, the attacker cannot complete the handshake without the provider's private key. The tunnel transition `ESTABLISHING → FAILED` records a handshake failure rather than a successful connection. `VPN_TUNNEL_FAILED` `EXTENDED_60M` evidence carries the failure cause (`peer_handshake_failed`) and the resolved IP class (bounded). The legitimate establishment then occurs once DNS recovers.

For `WIREGUARD_FULL_TUNNEL` profiles, the operator may pin the endpoint IP at manifest time (`peer_endpoint = "203.0.113.42:51820"` instead of an FQDN); this defeats the DNS-poisoning concern entirely at the cost of static-IP-only operation.

### 8.8 Resolver allowlist downgrade

**Attack:** an attacker submits a `SetResolverAllowlist` action carrying a previously-valid (but rotated-out) allowlist, hoping to roll back the resolver set to one that includes a now-compromised resolver.

**Mitigation:** the allowlist `version` is monotonic (BLAKE3-derived; the `issued_at` timestamp is part of the signed body); a `version` older than the current loaded version is refused with `RESOLVER_LIST_ROTATION_REJECTED` `FOREVER` evidence. Even valid signatures from AIOS root cannot replay an older bundle.

### 8.9 mDNS amplification reflection

**Attack:** a malicious LAN device sends spoofed mDNS queries with a unicast source address, attempting to use the AIOS host as a reflector to amplify traffic toward a third party.

**Mitigation:** Avahi's `enable-reflector=no` (per §5.6.1) forbids reflection. The `ratelimit-burst=100` plus `ratelimit-interval-usec=1000000` cap response rate at 100 responses/second, bounding the amplification factor. mDNS responses are sent only to the multicast group (`224.0.0.251` / `ff02::fb`), never to a unicast source extracted from the query. A misbehaving Avahi version that did emit unicast responses would be detected by the L8 `mdns_inbound_queries_dropped_total` rate exceeding a heuristic threshold (deferred to a future operations sub-spec).

### 8.10 VPN MTU-pinning denial of service

**Attack:** an upstream attacker fragments WireGuard packets in a way that drives MTU re-negotiation continuously, hoping to exhaust kernel resources or to expose plaintext fragments during the re-negotiation window.

**Mitigation:** WireGuard does not perform per-flight MTU re-negotiation; the interface MTU is set at bring-up time (`MTU = 1420` for IPv4-only paths, `1280` for IPv6-tunnelled paths). Packets exceeding MTU are dropped by the WireGuard layer, not fragmented to plaintext. The peer's path-MTU is a one-time observation at handshake time; subsequent PMTU events are advisory only and do not change the interface MTU. A `vpn_pmtu_events_total` counter (added to §9.2 in a future revision) bounds the per-second rate at 10 events; sustained excess triggers `VPN_TUNNEL_FAILED` with `pmtu_storm` cause.

### 8.11 mDNS service-name spoofing

**Attack:** a malicious LAN device advertises an mDNS service with the same `instance_name` as a legitimate granted advertisement on AIOS (e.g., the attacker advertises another "Living Room Plex" pointing to an attacker IP).

**Mitigation:** mDNS does not, per protocol, prevent name collisions; the receiver disambiguates. AIOS's `MdnsResolveInstance` records every observed responder; multiple distinct responders for the same `(service_type, instance_name)` cause the query to return `OUTCOME = AMBIGUOUS` and `MDNS_POISONING_DETECTED` `FOREVER` evidence is emitted carrying the count of distinct responders. The caller must disambiguate by responder MAC or fail closed; the AIOS resolver does not pick a winner.

### 8.12 Resolver socket impersonation

**Attack:** a privileged subject inside AIOS attempts to bind a socket on `127.0.0.53:53` (or `127.0.0.1:53` for `UNBOUND_LOCAL`) with the intent of impersonating the resolver and serving forged answers to other subjects.

**Mitigation:** `_system:resolver` holds an exclusive bind on the resolver port via the systemd socket unit `aios-resolver.socket` with `BindIPv6Only=both` and `Restrict=`. A second bind attempt fails with `EADDRINUSE`. A subject racing the resolver at boot is denied by the systemd ordering (`Before=network-online.target`, `After=local-fs.target`) which guarantees the resolver socket is bound before any user-space subject runs. A persistent attempt to take the port is detected by a startup self-test that confirms `getaddrinfo("aios.localhost")` resolves through the AIOS resolver's known fingerprint (a TXT record in the recovery hosts file); failure emits `RESOLVER_SOCKET_HIJACK_SUSPECTED` evidence (subsumed under `RESOLVER_BACKEND_DEGRADED`).

## 9. Performance and telemetry

### 9.1 Performance budgets

| Operation                                            | p50      | p95      | p99      | Hard timeout |
| ---------------------------------------------------- | -------- | -------- | -------- | ------------ |
| DNS query (cached at resolver)                       | < 1 ms   | < 50 ms  | < 100 ms | 500 ms       |
| DNS query (uncached, DoT)                            | < 50 ms  | < 200 ms | < 500 ms | 5 s          |
| DNS query (uncached, DoH)                            | < 80 ms  | < 250 ms | < 600 ms | 5 s          |
| WireGuard tunnel establishment (cold; key handshake) | < 1 s    | < 5 s    | < 10 s   | 30 s         |
| WireGuard tunnel re-key (warm)                       | < 100 ms | < 500 ms | < 1 s    | 5 s          |
| mDNS local query (single response)                   | < 10 ms  | < 100 ms | < 500 ms | 2 s          |
| Allowlist verification at startup                    | < 50 ms  | < 200 ms | < 500 ms | 2 s          |

Cache size: 16 384 entries. LRU eviction. Per-FQDN max IP fan-out 16 (per S8.1 I9). Total cache memory bounded at ~ 4 MB.

### 9.2 Metrics (closed cardinality)

All metrics MUST use bounded label cardinality. **Subject id, group id, FQDN, IP, port are NEVER labels.**

| Metric                                     | Type      | Labels (closed)                                           |
| ------------------------------------------ | --------- | --------------------------------------------------------- |
| `dns_queries_total`                        | counter   | `transport` (4), `outcome` (5), `resolver_id_class` (≤ 8) |
| `dns_query_latency_seconds`                | histogram | `transport` (4), `cache_hit` (2)                          |
| `dns_resolver_failures_total`              | counter   | `failure_kind` (8)                                        |
| `dns_pin_violations_total`                 | counter   | none                                                      |
| `dns_substitution_attempts_total`          | counter   | none                                                      |
| `dns_plain_blocked_total`                  | counter   | none                                                      |
| `vpn_tunnel_state_transitions_total`       | counter   | `kind` (3 active), `to_state` (8)                         |
| `vpn_tunnel_establishment_latency_seconds` | histogram | `kind` (3 active)                                         |
| `vpn_tunnels_active`                       | gauge     | `kind` (3 active)                                         |
| `vpn_provider_key_forgery_attempts_total`  | counter   | none                                                      |
| `mdns_inbound_queries_dropped_total`       | counter   | `posture` (4)                                             |
| `mdns_advertisements_active`               | gauge     | none                                                      |
| `mdns_poisoning_detections_total`          | counter   | none                                                      |
| `resolver_backend_degraded_seconds_total`  | counter   | `backend` (5)                                             |
| `fqdn_label_count_overflow`                | counter   | none                                                      |

Cardinality budget: ≤ 30 active label tuples per metric. The `resolver_id_class` label is a closed bucket (`internal`, `quad9`, `cloudflare`, `google`, `nextdns`, `operator-defined`, `aios-degraded`, `unknown`) — never the resolver_id itself.

## 10. Evidence record types (queued for S3.1)

Twelve new record types are queued for the S3.1 closed `RecordType` enum at the next consolidation. Append authority: only the L8 DnsVpnService process may emit these; forgery from any other subject is hard-denied at S3.1 and emits `TAMPER_DETECTED`.

| Record type                          | Retention class | Trigger                                                                                                                |
| ------------------------------------ | --------------- | ---------------------------------------------------------------------------------------------------------------------- |
| `DNS_QUERY_PERFORMED`                | `STANDARD_24M`  | Every successfully evaluated DNS query (RESOLVED, NXDOMAIN, TIMEOUT, REFUSED outcomes); FQDN auditable; **no payload** |
| `DNS_RESOLVER_REBINDING_DETECTED`    | `FOREVER`       | A response returns IPs outside the pinned set (per S8.1 §8.4); FQDN entry transitions to `AWAITING_OPERATOR`           |
| `DNS_PLAIN_BLOCKED`                  | `FOREVER`       | A subject attempted UDP/53 or TCP/53 in the clear; kernel filter dropped                                               |
| `DNS_RESOLVER_SUBSTITUTION_REJECTED` | `FOREVER`       | A subject attempted to register an out-of-allowlist resolver (config write, mount, RES_OPTIONS, etc.)                  |
| `VPN_TUNNEL_ESTABLISHED`             | `STANDARD_24M`  | A `VpnTunnel` reached `ACTIVE`; carries `tunnel_id`, `kind`, `peer_endpoint_class`, approver chain                     |
| `VPN_TUNNEL_FAILED`                  | `EXTENDED_60M`  | A tunnel transitioned to `FAILED` (kill-switch, peer unreachable beyond grace, key handshake failure)                  |
| `VPN_PROVIDER_KEY_ROTATED`           | `FOREVER`       | Successful key rotation for a tunnel; carries old key BLAKE3, new key BLAKE3, signing identity                         |
| `VPN_PROVIDER_KEY_FORGERY_REJECTED`  | `FOREVER`       | A `RotateVpnPeerKey` attempt failed Ed25519 verification                                                               |
| `MDNS_REQUEST_RECEIVED`              | `STANDARD_24M`  | A subject submitted `MdnsResolveInstance`; carries `service_type`, `instance_name_class`, outcome                      |
| `MDNS_BROADCAST_DENIED`              | `EXTENDED_60M`  | An advertisement was denied (posture, expired grant, recovery-suspended)                                               |
| `MDNS_POISONING_DETECTED`            | `FOREVER`       | An mDNS response IP fell outside the interface's LAN_SUBNET                                                            |
| `RESOLVER_BACKEND_DEGRADED`          | `EXTENDED_60M`  | `ResolverBackend` transitioned to `DEGRADED_HOSTS_FILE_ONLY` (signature failure, all upstreams unreachable, etc.)      |

Retention class distribution for the 12 additions: `FOREVER` × 6, `EXTENDED_60M` × 3, `STANDARD_24M` × 3.

Two queued-but-not-counted-here records `RESOLVER_LIST_ROTATED` (`FOREVER`) and `RESOLVER_LIST_ROTATION_REJECTED` (`FOREVER`) are produced by the recovery-mode rotation flow and ride on the existing S0.1 `SYSTEM_MUTATION` channel; they do not need new closed enum values because they reuse `SYSTEM_ADMIN_OPERATION` (S3.1 existing) plus the recovery `RECOVERY_EVENT` envelope.

## 11. Cross-spec dependencies and follow-up queue

| Spec            | Direction | What this contract contributes / consumes                                                                                                                                      |
| --------------- | --------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| S0.1            | consumer  | `SetResolverBackend`, `SetResolverAllowlist`, `EstablishVpnTunnel`, `RotateVpnPeerKey`, `GrantMdnsAdvertisement` flow as typed actions; `action_id` is recorded on every grant |
| S1.3            | consumer  | Resolver allowlist + VPN manifests + mDNS grants are AIOS-FS objects; `manifest_hash` per S1.3 chunk discipline                                                                |
| S2.3            | consumer  | Policy decisions for every mutation; AI authorship hard-denied (INV-002); recovery-mode required for resolver allowlist rotation (INV-012)                                     |
| S2.4            | producer  | Three new primitives queued: `dns_resolver_backend(host)`, `vpn_tunnel_active(tunnel_id)`, `mdns_posture(host)`                                                                |
| S3.1            | producer  | 12 new record types queued (§10)                                                                                                                                               |
| S3.2            | consumer  | Sandbox `NetworkMode` floor still binds — VPN does not loosen the sandbox; a `LOOPBACK_ONLY` sandbox cannot ride a VPN                                                         |
| S4.1            | consumer  | Per-group resolver namespace mounts under `/run/aios/groups/<g>/resolv.conf`; per-group VPN binding                                                                            |
| S5.1            | consumer  | Subject `is_ai`, `is_recovery_mode` drive authorisation; `_system:resolver` and `_system:vpn` own their respective surfaces                                                    |
| S5.3            | consumer  | `STRONG` approval strength required for `EstablishVpnTunnel` (per use), for `WIREGUARD_FULL_TUNNEL`, and for `GrantMdnsAdvertisement`                                          |
| S8.1            | consumer  | This contract operates the resolver allowlist constitutional discipline (S8.1 §5.4); per-app outbound is the substrate VPN routing rides on                                    |
| S9.1 (deferred) | consumer  | Recovery boot path is what enables `SetResolverAllowlist` and triggers `RECOVERY_DENIED` mDNS posture                                                                          |
| L0 INV-006      | enforcer  | Plain DNS forbidden by default; resolver bound to loopback                                                                                                                     |
| L0 INV-008      | enforcer  | Default-deny on `MdnsAdvertisement` and `EstablishVpnTunnel`                                                                                                                   |
| L0 INV-011      | enforcer  | Per-group resolver and VPN binding via `aios-net-<group>` namespaces                                                                                                           |
| L0 INV-002      | enforcer  | AI cannot rotate the resolver list, the VPN peer keys, or the mDNS posture                                                                                                     |

### 11.1 Cross-spec follow-ups queued (NOT applied here)

- **S2.4** to add primitives `dns_resolver_backend(host)`, `vpn_tunnel_active(tunnel_id)`, `mdns_posture(host)`.
- **S3.1** to absorb the 12 new record types into the closed `RecordType` enum and the retention-class table.
- **S2.3** to add closed condition fields `target.dns_transport`, `target.vpn_tunnel_kind`, `target.mdns_posture` for next S2.3 consolidation.

## 12. Worked examples

### 12.1 Operator opens DoT to a known-good resolver

**Setup.** Fresh-install AIOS host. `NetworkPosture = LAN_LOCAL`. `ResolverBackend = SYSTEMD_RESOLVED`. The signed allowlist contains two entries (`Quad9 DoT`, `Cloudflare DoH`). `MdnsAvahiPosture = DENY_DEFAULT`.

**Step 1 — Subject opens a connection.** The web renderer (`_system:web-renderer`, `is_ai = false`) loads its loopback URL; later, an installed app `family:browser` performs `getaddrinfo("kernel.org")`.

**Step 2 — Local resolver receives query.** The libc `getaddrinfo` reaches `127.0.0.53:53` (systemd-resolved). The resolver service maps the query to the active allowlist; the chosen resolver is `rs:01HX...A1` (Quad9 DoT).

**Step 3 — Outbound DoT query.** The resolver opens a TCP/853 connection to `9.9.9.9` over the host network. The `EvaluateConnection` call at S8.1 sees the originating subject `_system:resolver`, the directive is `ALLOW_LIST_ONLY`, the allowlist entry is `DNS_OVER_TLS_RESOLVER` matching `9.9.9.9`. Connection allowed.

**Step 4 — TLS handshake and pinned cert verification.** TLS handshake completes; certificate's SPKI hash is verified against the pin set `["base64-pin-1", "base64-pin-2"]`. Match. Query travels over TLS; answer returned.

**Step 5 — Cache and pin.** The answer set is cached for `min(answer_ttl, 300s)`; the FQDN-pin record `(kernel.org, [a, b, c, d], expires_at)` is created.

**Step 6 — Audit emission.** `DNS_QUERY_PERFORMED` `STANDARD_24M` evidence:

```json
{
  "subject_canonical_id": "family:browser",
  "fqdn": "kernel.org",
  "resolver_id": "rs:01HX...A1",
  "transport": "DOT_TLS",
  "outcome": "RESOLVED",
  "latency": "0.045s",
  "at": "2026-05-09T..."
}
```

No answer set in the record. The metric `dns_queries_total{transport="DOT_TLS",outcome="RESOLVED",resolver_id_class="quad9"}` increments.

**Step 7 — Subsequent connection.** `family:browser` opens TCP/443 to `kernel.org`; S8.1 `EvaluateConnection` resolves the FQDN against the pinned set; allowed by manifest; connection records `interface = direct`.

### 12.2 WireGuard split-tunnel for work-only apps

**Setup.** Operator wants a daily-use VPN routing only `git.work-internal.example.com` and `ci.work-internal.example.com` through the tunnel. Posture remains `LAN_AND_INTERNET`. The provider has issued a peer pubkey `base64-ed25519-A`.

**Step 1 — `EstablishVpnTunnel`.** Operator (`HUMAN_USER`, `family:alice`, `session_class = INTERACTIVE`) submits `EstablishVpnTunnel(kind = WIREGUARD_SPLIT_TUNNEL, peer_endpoint = vpn.work-corp.example.com:51820, peer_pubkey = ..., allowed_ips = ["10.99.0.0/16"], bound_subjects = ["homelab:work-app"])`. Action id `act_01HX...01`.

**Step 2 — Policy decision.** S2.3 evaluates. AI? No. Matching rule? Yes — `vpn.tunnel.establish` permits `HUMAN_USER` with `STRONG` strength. Decision: `REQUIRE_APPROVAL`, strength `STRONG`, ttl 300 s.

**Step 3 — Approval.** Alice's session is `INTERACTIVE`; step-up reauthentication required. Alice authenticates with WebAuthn; session class becomes `STRONG`. The chrome-zone prompt shows `tunnel_id = vpn_01HX01A1, peer = vpn.work-corp.example.com:51820, allowed_ips = 10.99.0.0/16`. Alice presses Approve. `APPROVAL_GRANTED` evidence.

**Step 4 — Tunnel establishment.** L8 generates `/aios/system/network/vpn/vpn_01HX01A1/wg.conf` with the operator's local key, the peer's pubkey, and `AllowedIPs = 10.99.0.0/16`. Brings up `wg-01HX01`. Routing rule added: `10.99.0.0/16 dev wg-01HX01`. Kill-switch installed: `iptables -A OUTPUT -d 10.99.0.0/16 -j DROP` is **not** added (the kill-switch is implemented as nftables `mark` rules that drop packets destined to AllowedIPs when the interface is down). Tunnel reaches `ACTIVE` in 3.2 s. `VPN_TUNNEL_ESTABLISHED` `STANDARD_24M` evidence emitted.

**Step 5 — App rides the tunnel.** `homelab:work-app` opens TCP/443 to `git.work-internal.example.com`. DNS resolution: `git.work-internal.example.com → 10.99.5.10`. `EvaluateConnection` matches the manifest entry; the routing rule sends the packet via `wg-01HX01`. The connection's audit record carries `interface = wg-01HX01`.

**Step 6 — Daily use.** `homelab:work-app` continues working through the tunnel for hours. Periodically, `vpn_tunnel_state_transitions_total{kind="WIREGUARD_SPLIT_TUNNEL",to_state="ACTIVE"}` increments on heartbeat.

**Step 7 — Provider key rotation.** A week later, the provider issues a new peer key `base64-ed25519-B` signed by the enrollment-time identity key. `RotateVpnPeerKey` action arrives. Ed25519 verify against the on-disk enrollment record — succeeds. WireGuard `wg-01HX01` is reconfigured with the new pubkey via `wg set wg-01HX01 peer ... preshared-key ...`. Tunnel transits `ACTIVE → REKEYING → ACTIVE` in 280 ms. `VPN_PROVIDER_KEY_ROTATED` `FOREVER` evidence emitted carrying old-BLAKE3 + new-BLAKE3.

**Step 8 — Forged rotation later.** An attacker submits a `RotateVpnPeerKey` for the same tunnel with a key signed by a key-the-attacker-controls. Ed25519 verify against the enrollment-time identity key — **fails**. Rotation refused. Tunnel continues with `base64-ed25519-B`. `VPN_PROVIDER_KEY_FORGERY_REJECTED` `FOREVER` evidence emitted carrying tunnel id, attempted-key BLAKE3, and the verification failure cause.

### 12.3 mDNS request for printer discovery

**Setup.** `MdnsAvahiPosture = REQUIRE_OPERATOR_APPROVAL`. Operator wants Alice's machine to be able to discover the family Plex server (`Living Room Plex`) on the LAN. No active mDNS advertisement exists.

**Step 1 — Subject submits discovery request.** Application `family:home-control` calls `MdnsResolveInstance(service_type = "_http._tcp.local.", instance_name = "Living Room Plex")`. Action id `act_01HX...02`.

**Step 2 — Policy decision.** S2.3 evaluates. AI? No. Matching rule? Yes — `mdns.resolve` permits `HUMAN_USER`-bound apps in group `family` to query mDNS. Decision: `ALLOW`. (Querying mDNS does not require `STRONG` approval; advertising does.)

**Step 3 — Outbound mDNS query.** L8 mDNS service emits a UDP/5353 multicast query on the bound LAN interface `br0` with payload requesting `_http._tcp.local. Living Room Plex`. The query is restricted to `br0`'s subnet `192.168.1.0/24`.

**Step 4 — Response.** A response returns from `192.168.1.211:5353` (the Plex server) carrying `Living Room Plex._http._tcp.local. → 192.168.1.211:32400`.

**Step 5 — Sanity check on responder.** L8 verifies that `192.168.1.211 ∈ 192.168.1.0/24` (the interface's LAN_SUBNET). Match. Response accepted.

**Step 6 — Audit emission.** `MDNS_REQUEST_RECEIVED` `STANDARD_24M` evidence:

```json
{
  "subject_canonical_id": "family:home-control",
  "service_type": "_http._tcp.local.",
  "instance_name_class": "operator-named",
  "outcome": "RESOLVED",
  "interface_name": "br0",
  "responder_class": "lan-subnet-match",
  "at": "2026-05-09T..."
}
```

The instance name itself is recorded in a sub-field (`instance_name = "Living Room Plex"`) but the metric label `instance_name_class` is bounded.

**Step 7 — Poisoning attempt for contrast.** Later, an attacker on the LAN responds to the same mDNS query type with a crafted answer pointing `Living Room Plex` to `8.8.8.8`. L8 verifies `8.8.8.8 ∈ 192.168.1.0/24` — **fails** (`8.8.8.8` is outside the LAN_SUBNET). Response dropped. `MDNS_POISONING_DETECTED` `FOREVER` evidence emitted carrying `service_type = "_http._tcp.local."`, `responder_class = "outside-lan-subnet"`, responder MAC (from ARP table). The query returns the legitimate answer (the Plex server); the poisoned answer is silently discarded from the operator's view.

**Step 8 — Recovery boot context.** Hours later, the host enters recovery boot. `MdnsAvahiPosture` is forcibly set to `RECOVERY_DENIED`. The Plex advertisement (had one existed) would be suspended; even if `family:home-control` repeated the query, the L8 mDNS service would refuse with `MDNS_BROADCAST_DENIED` `EXTENDED_60M` evidence. On exit from recovery, the posture restores to `REQUIRE_OPERATOR_APPROVAL` but the previously-suspended advertisements are **not** auto-restored — the operator must re-grant.

## 13. Acceptance criteria

- [ ] `ResolverBackend` is a closed enum with exactly 5 values (§4.1).
- [ ] `DnsTransport` is a closed enum with exactly 4 values, including `PLAIN_DNS_FORBIDDEN` as a denial sentinel (§4.2).
- [ ] `VpnTunnelKind` is a closed enum with 4 closed slots (2 operationally permitted variants + 1 denial sentinel + 1 reserved schema slot) (§4.3).
- [ ] `MdnsAvahiPosture` is a closed enum with exactly 4 values, including `RECOVERY_DENIED` as the recovery-locked value (§4.4).
- [ ] `ResolverFailureKind` is a closed enum with 8 values (§4.5).
- [ ] Plain UDP DNS is forbidden by default; the kernel filter drops UDP/53 + TCP/53 traffic outside the resolver service's PID set; `DNS_PLAIN_BLOCKED` `FOREVER` evidence on attempt (I1, §8.5).
- [ ] The resolver allowlist is signed by AIOS root; signature failure puts the resolver into `DEGRADED_HOSTS_FILE_ONLY` (I2, §5.1).
- [ ] Allowlist rotation is a recovery-mode operation; AI authorship hard-denied; rollback to older versions refused (I12, §5.2, §8.8).
- [ ] Every DNS query emits `DNS_QUERY_PERFORMED` `STANDARD_24M` evidence carrying FQDN but **not** the answer (I4, §5.3).
- [ ] FQDN label cardinality is bounded at 65 536 unique labels per audit segment with `fqdn_label_count_overflow` summary (I4, §5.3).
- [ ] WireGuard tunnel establishment requires `STRONG` approval per S5.3; full-tunnel requires per-use `STRONG`; AI authorship hard-denied (I5, I7, §5.4).
- [ ] VPN provider key rotation is verified by Ed25519 against the enrollment-time identity key; forgery emits `VPN_PROVIDER_KEY_FORGERY_REJECTED` `FOREVER` (I6, §8.3).
- [ ] VPN-bound apps see only VPN-allowed endpoints; non-VPN apps see direct routes; every connection's audit record carries the traversed interface (I8, §5.5).
- [ ] mDNS posture is `DENY_DEFAULT` at first boot, `RECOVERY_DENIED` in recovery, and `RECOVERY_DENIED` does not auto-restore on recovery exit (I9, §5.6, §7.3).
- [ ] mDNS advertisements are explicit per service; Avahi does not auto-broadcast every D-Bus-registered service (I9, §5.6).
- [ ] mDNS responses are cross-checked against the interface's LAN_SUBNET; out-of-subnet IPs trigger `MDNS_POISONING_DETECTED` `FOREVER` (§8.4).
- [ ] DNS resolver substitution attempts (config write, mount, RES_OPTIONS) are denied; emit `DNS_RESOLVER_SUBSTITUTION_REJECTED` `FOREVER` (I3, §8.1, §8.6).
- [ ] DNS rebinding mitigation is anchored at the resolver layer with FQDN-pin discipline (I11, §8.2).
- [ ] Performance budgets in §9.1 hold: cached DNS p95 < 50 ms, uncached < 200 ms, WireGuard tunnel establishment p95 < 5 s.
- [ ] Cache size bounded at 16 384 entries; per-FQDN fan-out ≤ 16 IPs (I10, §9.1).
- [ ] Telemetry conforms to §9.2: subject id, group id, FQDN, IP, port are NEVER labels.
- [ ] All 12 evidence record types (§10) are queued for S3.1 consolidation with the documented retention classes.
- [ ] Three S2.4 verification primitives (`dns_resolver_backend`, `vpn_tunnel_active`, `mdns_posture`) are queued (§11.1).
- [ ] All three worked examples (§12) produce the specified outcomes.

## 13.1 ProxGuard interaction (when installed)

When ProxGuard is installed as the L8 capability provider (per the L8 overview §32), it participates in DNS / VPN management but is **not** authoritative; this contract remains the constitutional surface. The contract defines a narrow handoff:

- **`proxguard.dns.plan`** — ProxGuard may submit a DNS allowlist plan (e.g., a recommended resolver set for a travel posture). The plan is treated as a **proposal**: the L8 service evaluates it as a `SetResolverAllowlist`-class action which still requires the AIOS-root signature to be present. ProxGuard cannot ship its own root key.
- **`proxguard.dns.apply`** — applies a previously-planned allowlist. Even with ProxGuard's involvement, the recovery-mode requirement of §5.2 holds: outside recovery the call returns `RECOVERY_REQUIRED`.
- **`proxguard.gateway.route`** — ProxGuard may propose routing rules, including VPN routing. Each rule is an action through this contract's `EstablishVpnTunnel` / `RotateVpnPeerKey` surface; ProxGuard does not bypass the per-tunnel `STRONG` approval.
- **`proxguard.gateway.status`** — read-only telemetry surface; ProxGuard observes the `StreamVpnTunnelState` stream and may surface tunnel state on its own UI. ProxGuard does **not** see the tunnel's `PrivateKey` or any vault material.

The handoff guarantees: ProxGuard is a useful **surface** for operating DNS/VPN, but every constitutional gate (signature, recovery-mode requirement, `STRONG` approval, AI-authorship hard-deny) is still enforced by this contract. Removing ProxGuard does not weaken the constitutional posture; installing ProxGuard does not loosen any invariant.

## 13.2 Recovery boot interaction

The recovery boot path (deferred to S9.1) interacts with this contract in three ways:

1. **At recovery entry**, the resolver service is forced into `DEGRADED_HOSTS_FILE_ONLY` regardless of the previously-loaded allowlist. The recovery hosts file (`/aios/system/network/resolvers/recovery_hosts.txt`) is small, well-known, and contains only the hostnames the recovery shell needs (the operator's identity provider, the AIOS evidence-receipt verification endpoint). No DoT/DoH traffic is generated during recovery; this minimises the cryptographic surface during a recovery window where TLS trust anchors might themselves be the subject of repair.
2. **At recovery entry**, all `VpnTunnel`s in `ACTIVE` state transition to `SUSPENDED` (a recovery-only synthetic state, not part of the §7.2 FSM as it never appears outside recovery). Their kill-switches remain in place: subjects that depended on the tunnel see `EvaluateConnection` denials, not direct-route fallback. On recovery exit, the tunnels do **not** auto-resume; the operator must explicitly re-establish each. This prevents a "recovery boot accidentally exposed work data because the VPN didn't come up" failure mode.
3. **At recovery entry**, `MdnsAvahiPosture` is forcibly set to `RECOVERY_DENIED` (§4.4). On exit, it is restored to the pre-recovery value, but, critically, individual advertisements are **not** restored — the recovery boot is a "treat the LAN as hostile" event, and the operator re-grants only the advertisements they still want.

The composite effect: during recovery, the host's network footprint is the smallest it can be without going to `AIRGAP` (S8.1 posture). DoT/DoH traffic stops; VPN tunnels are down; mDNS is silent; only the loopback resolver and the operator-recovery interface remain reachable.

## 14. Open deferrals

- **DNSSEC validation policy** — whether the resolver enforces DNSSEC, and how DNSSEC validation failures interact with the FQDN-pin discipline. Deferred.
- **Per-group resolver overrides at the policy layer** — a group choosing its own subset of the host allowlist. The mount-namespace mechanism is named (§3.3) but the policy surface is deferred.
- **VPN mesh as group transport** — using WireGuard to bridge groups across hosts. Deferred (referenced by S8.1 §11.1 too).
- **mDNS multi-interface coordination** — when a host has both `br0` and `wlan0`, which interface broadcasts which advertisement. Deferred to a future operations sub-spec.
- **DoQ (DNS over QUIC)** — not in this revision; would extend the `DnsTransport` enum to 5 values; deferred.
- **Resolver allowlist co-signer** — currently the AIOS root signature is the sole authority; a future revision may add a second-signature requirement (operator co-sign) for additional defense-in-depth. Deferred.
- **VPN tunnel telemetry exposed to ProxGuard** — when ProxGuard is installed as the L8 capability provider per S8 overview, it may surface tunnel state on its own UI. Coordination contract deferred to ProxGuard reference model.

## See also

- [S8.1 — Network Policy](02_network_policy.md)
- [S8.2 — GPU Resource Model](05_gpu_resource_model.md)
- [S0.1 — Action Envelope and Lifecycle](../XX_Cross_Cutting/01_action_envelope_lifecycle.md)
- [S2.3 — Policy Kernel](../L4_Policy_Identity_Vault/01_policy_kernel.md)
- [S5.1 — Identity Model](../L4_Policy_Identity_Vault/03_identity_model.md)
- [S5.3 — Approval Mechanics](../L4_Policy_Identity_Vault/04_approval_mechanics.md)
- [S3.1 — Evidence Log](../L9_Observability_Admin_Operations/01_evidence_log.md)
- [S3.2 — Sandbox Composition](../L6_Apps_Packages_Compatibility/04_sandbox_composition.md)
- [S4.1 — Namespace Layout](../L2_AIOS_FS/05_namespace_layout.md)
- [S6.4 — Constitutional Invariants](../L0_Governance_Evidence_Safety/04_invariants.md)
- [Rev.1 §18 — Hardware and Network](../../001.AI-OS.NET--SPECREV.1/02_SPECIFICATION.md)
- [L8 Overview](00_overview.md)
- [Rev.2 Master Index](../00_MASTER_INDEX.md)
