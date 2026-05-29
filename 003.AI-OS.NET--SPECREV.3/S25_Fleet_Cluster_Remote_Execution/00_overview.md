# S25 - Fleet, Cluster, and Remote Execution

| Field     | Value                                                                                                                                                                                                                          |
| --------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Status    | `CONTRACT` (created 2026-05-29; awaiting implementation evidence)                                                                                                                                                              |
| Phase tag | S25                                                                                                                                                                                                                            |
| Layer     | Cross-cutting: L8, L4, L10 primary; L0, L9 anchoring                                                                                                                                                                           |
| Consumes  | S11.1 Repository Model + Trust Roots, S5.1 Identity Model, S8.1 Network Policy, S3.1 Evidence Log, S2.3 Policy Kernel, S16.1 Security Profile Matrix, S18 Kernel Personality and Portability Plane                             |
| Produces  | `FleetMembership`, `ClusterTrustRoot`, `RemoteWorkloadRouting`, `DistributedEvidenceLog`, `FederatedIdentityBundle`, `CrossOrgTrustDelegation`, federated `SubjectId` `(home_realm, local_id)`, fleet/cluster evidence records |

## 1. Responsibility

S25 defines how two to one hundred AIOS hosts cooperate as a fleet without any
host surrendering sovereignty. It owns four things the single-host contracts
deliberately leave open:

```text
1. cluster trust: how a host root extends into a cluster root (one extra hop)
2. fleet membership: how a host enrolls, what it agrees to, how it leaves
3. remote workload routing: a typed contract that lets S18 RUN_REMOTE exist
4. distributed evidence: a Merkle-DAG that preserves append-only under replication
```

S25 also owns the **federated identity model** (per DEC-R3-011): it redesigns the
single-host `SubjectId` into a realm-scoped `(home_realm, local_id)` pair with a
backward-compatible shim, and adds cross-organization trust delegation.

The governing rule of this entire plane is constitutional: **the host stays
sovereign.** A cluster root is a convenience for federated trust and routing; it
is never a super-administrator. A cluster root cannot weaken a host's security
profile, cannot mutate a host's evidence, cannot grant a capability a host's own
Policy Kernel would deny, and cannot become root on a member host. This is
codified as the new invariant **INV-026** (cluster root cannot override host
policy) and a hard-deny gate (§9).

Invariant links: INV-002, INV-008, INV-013, INV-014, INV-017, INV-024,
INV-026 (new), INV-027 (crypto-shred preserves evidence chain — consumed when
a federated subject is erased), INV-032 (new — see §13).

## 2. Product principle

A fleet should feel like one console and behave like many sovereign machines.

```text
operator picks a fleet goal
  -> cluster root resolves which hosts are in scope
  -> each target host evaluates the request against ITS OWN policy + profile
  -> hosts that allow it run a typed action locally and emit local evidence
  -> hosts that deny it return a typed blocked reason (not silence)
  -> the cluster aggregates local evidence into a Merkle-DAG view
  -> the operator sees one fleet result with per-host truth
```

The default answer to "apply this across the fleet" is never "the cluster root
SSHes in as root and runs a script." The default answer is: every host decides
for itself, every decision is typed, every mutation is local evidence, and the
cluster only federates trust, routing, and the evidence view.

## 3. Reference patterns

| Pattern                                                                                        | S25 use                                                                                             |
| ---------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------- |
| [WireGuard](https://www.wireguard.com/protocol/)                                               | Encrypted overlay transport between fleet hosts; hub-and-spoke default.                             |
| [Tailscale / headscale coordination model](https://tailscale.com/blog/how-tailscale-works)     | Optional mesh overlay and node-key coordination for sites that need full mesh.                      |
| [innernet](https://github.com/tonarino/innernet)                                               | Self-hosted WireGuard mesh CIDR model; admitted as a mesh option.                                   |
| [SPIFFE / SPIFFE ID](https://spiffe.io/docs/latest/spiffe-about/spiffe-concepts/)              | Realm-scoped workload identity; informs `(home_realm, local_id)` shape and trust-domain federation. |
| [SPIFFE Federation / trust bundles](https://spiffe.io/docs/latest/architecture/federation/)    | Cross-organization trust-bundle exchange; informs `CrossOrgTrustDelegation`.                        |
| [Certificate Transparency Merkle tree](https://datatracker.ietf.org/doc/html/rfc9162)          | Append-only Merkle log proofs; informs `DistributedEvidenceLog` inclusion/consistency proofs.       |
| [Git object DAG](https://git-scm.com/book/en/v2/Git-Internals-Git-Objects)                     | Content-addressed append-only DAG; informs evidence replication and merge.                          |
| [The Update Framework (TUF) roles](https://theupdateframework.github.io/specification/latest/) | Root/key-rotation discipline; informs `ClusterTrustRoot` rotation.                                  |

## 4. Cluster topology

Per DEC-R3-003 the cluster targets **2-100 hosts** (homelab through small
enterprise). The default overlay is hub-and-spoke WireGuard; a mesh overlay is
admitted for sites that need it.

```text
ClusterOverlayMode =
  HUB_AND_SPOKE          // default; coordinator host(s) relay; O(n) peer config
| FULL_MESH              // optional; innernet/headscale-style; O(n^2) peers
| HYBRID_RELAYED_MESH    // mesh where reachable, hub relay where NAT-blocked
```

```text
ClusterRole =
  CLUSTER_COORDINATOR    // holds cluster root pubkey, runs membership + DAG hub
| FLEET_MEMBER           // ordinary sovereign host
| FLEET_OBSERVER         // read-only console node; cannot route workloads
| RECOVERY_WITNESS       // offline/cold node holding a cluster-root recovery share
```

Unknown values of either enum are rejected by the cluster manifest loader.

Topology rules:

- The coordinator is a routing and trust convenience, not an authority. Losing
  the coordinator degrades fleet operations to per-host local operation; it
  never unlocks elevated control.
- `HUB_AND_SPOKE` is preferred up to ~100 hosts because pairwise WireGuard config
  is O(n^2). `FULL_MESH` is admitted only with an explicit operator reason.
- A `FLEET_OBSERVER` can read the aggregated DAG view but cannot enroll hosts,
  sign cluster artifacts, or route workloads.

## 5. Cluster trust root (one extra hop)

The single-host trust root from S11.1 (firmware-pinned root, extended in Rev.3
by the TPM dual chain of DEC-R3-002) is **not** replaced. S25 adds exactly one
hop above it: a **cluster root** that the host root chooses to recognize.

```text
firmware/TPM dual chain (DEC-R3-002)
  -> host trust root            (S11.1, per host, sovereign)
    -> host recognizes cluster root        <-- the ONE extra hop S25 adds
      -> cluster-signed fleet artifacts (membership, routing, DAG checkpoints)
```

```yaml
cluster_trust_root:
  cluster_id: "clr_<ULID>"
  cluster_root_pubkey: "ed25519:..."
  signing_scheme: ED25519 # ED25519 | ED25519_THRESHOLD
  threshold: # present only for ED25519_THRESHOLD
    k: 2
    n: 3
    share_holders:
      ["host:coordinator-a", "host:coordinator-b", "RECOVERY_WITNESS:cold-1"]
  rotation:
    rotation_index: 4
    previous_root_pubkey: "ed25519:..."
    rotation_signed_by_quorum: true
  recognized_by_host:
    host_id: "host:<ULID>"
    host_root_signature: "ed25519:..." # the host signs that it recognizes this cluster root
    recognized_scope: FLEET_TRUST_ONLY # see ClusterTrustScope below
  bound_realm: "realm:acme-eu"
```

```text
ClusterTrustScope =
  FLEET_TRUST_ONLY        // cluster root may sign membership + routing + DAG checkpoints only
| FLEET_TRUST_AND_REPO    // additionally a fleet-shared signed package mirror (S11.1 mirror semantic)
```

`ClusterTrustScope` is closed; unknown values are rejected by the trust-root
loader. There is no scope value that grants the cluster root host-administration
rights — by construction. The host's recognition signature is what makes a
cluster root meaningful to that host; a host can revoke recognition at any time
and immediately fall back to sovereign single-host trust.

Cluster root key rotation follows TUF-style discipline: a rotation is valid only
if signed by the current root (or quorum for the threshold scheme) and recorded
in evidence as `CLUSTER_ROOT_SIGNED` with `rotation_index` strictly increasing.

## 6. Fleet membership

```yaml
fleet_membership:
  membership_id: "flm_<ULID>"
  host_id: "host:<ULID>"
  cluster_id: "clr_<ULID>"
  state: ENROLLED
  enrolled_at: "<rfc3339>"
  overlay:
    mode: HUB_AND_SPOKE
    wireguard_pubkey: "wg:..."
    overlay_addr: "100.x.y.z/32"
    coordinator_peers: ["host:coordinator-a"]
  posture_floor:
    min_security_profile: SECURE_DEFAULT # host MUST be at or above this to stay ENROLLED
    require_tpm_dual_chain: false
  host_sovereignty:
    host_root_id: "host:<ULID>"
    host_policy_supremacy: true # constitutional; cannot be set false
    cluster_overridable: false # constitutional; cannot be set true
  evidence:
    enroll_receipt: "evr_..."
    last_attestation_receipt: "evr_..."
```

Membership lifecycle FSM:

```text
DISCOVERED
  -> INVITED           (coordinator issues a signed invite; operator-initiated)
  -> ATTESTING         (host presents TPM/firmware posture per DEC-R3-002)
  -> ENROLLED          (host root signs recognition; cluster root counter-signs)
  -> SUSPENDED         (posture floor failed or attestation stale; routing paused)
  -> QUARANTINED       (drift/compromise signal; overlay isolated, evidence kept)
  -> WITHDRAWN         (host or operator revokes recognition; sovereign single-host)
  -> EXPELLED          (cluster revokes membership; host returns to single-host)
```

```text
FleetMembershipState =
  DISCOVERED | INVITED | ATTESTING | ENROLLED
| SUSPENDED | QUARANTINED | WITHDRAWN | EXPELLED
```

Closed enum; unknown values rejected by the membership loader. Constitutional
edges:

- A host always retains a unilateral `-> WITHDRAWN` edge. No cluster signature is
  required to leave; leaving never destroys local evidence.
- `host_policy_supremacy` and `cluster_overridable` are fixed constants in the
  schema. A manifest that sets `host_policy_supremacy: false` or
  `cluster_overridable: true` is rejected as malformed (INV-026).
- A host below its `min_security_profile` is auto-`SUSPENDED` (routing paused),
  never auto-weakened — S25 cannot lower a profile to keep a host in the fleet
  (INV-026 + S16.1 downgrade discipline).

## 7. Federated identity

Single-host `SubjectId` (S5.1) is the canonical id `<group_part>:<kind_segment>`.
For a fleet it must carry an originating realm. S25 redesigns the subject id into
a realm-scoped pair while preserving every existing single-host id.

```text
FederatedSubjectId = (home_realm, local_id)

home_realm ::= "realm:" <realm_label>          realm_label = [a-z][a-z0-9_-]{0,62}
local_id   ::= the S5.1 canonical subject id, verbatim
```

Backward-compatibility shim (mandatory): every pre-existing single-host
`local_id` is interpreted as `(realm:default, local_id)`. On the wire and in
evidence the federated form is rendered `realm:<label>:<local_id>`; the legacy
form `realm:default:<local_id>` is therefore exactly the old id with a stable
prefix. The shim is loss-free and reversible:

```text
legacy:     family:alice
federated:  realm:default:family:alice         (default realm)
federated:  realm:acme-eu:family:alice          (foreign realm)
```

```yaml
federated_identity_bundle:
  bundle_id: "fib_<ULID>"
  home_realm: "realm:acme-eu"
  cluster_id: "clr_<ULID>"
  realm_root_pubkey: "ed25519:..." # who signs subjects in this realm
  subjects:
    - federated_id: "realm:acme-eu:family:alice"
      kind: HUMAN_USER # reuses S5.1 SubjectKind, unchanged
      is_ai: false # set by issuing realm, never self-declared (S5.1 I6)
      bound_local_id: "family:alice"
  shim:
    default_realm_for_legacy: "realm:default"
    legacy_ids_loss_free: true
  signature: "ed25519:..."
```

Resolution rule: when a federated subject acts on a host, the host's identity
service (S5.1) resolves `(home_realm, local_id)` to a local authority decision.
A foreign-realm subject only gains the rights the **local** host's Policy Kernel
grants to that realm via an explicit `CrossOrgTrustDelegation`. A foreign subject
is never silently mapped onto a powerful local subject. `is_ai` is preserved
across realms and remains set by the issuer (S5.1 invariant I6); a foreign AI
subject is still an AI subject on the receiving host and is still subject to
INV-002/INV-013 there.

## 8. Cross-organization trust delegation

```yaml
cross_org_trust_delegation:
  delegation_id: "ctd_<ULID>"
  from_realm: "realm:acme-eu"
  to_realm: "realm:partner-eu"
  direction: INBOUND_ACCEPT # this host's realm accepts subjects FROM to_realm
  accepted_subject_kinds: [HUMAN_USER] # closed: subset of S5.1 SubjectKind
  granted_capability_ceiling:
    max_security_profile_reachable: SECURE_DEFAULT # foreign subjects capped here
    forbid_admin_actions: true
    forbid_ai_subjects: false
  scope:
    realms_path_max_hops: 1 # no transitive delegation beyond 1 hop by default
    expiry: "<rfc3339>"
    revocable: true
  signed_by:
    home_realm_root: "ed25519:..."
    accepting_host_root: "ed25519:..." # the accepting host must also sign (sovereignty)
```

```text
TrustDelegationDirection =
  INBOUND_ACCEPT | OUTBOUND_VOUCH | BIDIRECTIONAL
```

Closed enum; unknown values rejected by the delegation loader. Rules:

- Delegation is capped: a foreign realm can never receive a higher capability
  ceiling than the **accepting host** is willing to sign. The accepting host root
  signature is mandatory (sovereignty).
- Delegation is non-transitive by default (`realms_path_max_hops: 1`). Extending
  the hop count is an explicit, evidenced operator decision.
- Delegation never grants admin actions unless `forbid_admin_actions: false` is
  explicitly signed by the host root, and even then the host Policy Kernel and
  security profile still decide each action.

## 9. Remote workload routing (the contract S18 RUN_REMOTE needs)

S18 (Kernel Personality and Portability Plane) defines workload backends that may
include a remote target. S25 gives `RUN_REMOTE` a real contract so it is not a
hand-wave: routing a workload to another host is itself a typed action governed
by **both** hosts.

```yaml
remote_workload_routing:
  routing_id: "rwr_<ULID>"
  workload_ref: "capsule:<id> | kernel-candidate:<id> | driver-lab-job:<id>"
  origin_host: "host:<ULID>"
  target_host: "host:<ULID>"
  reason: HARDWARE_AFFINITY # see RemoteRoutingReason
  routing_class: SANDBOXED_CAPSULE # see RemoteRoutingClass
  subject: "realm:acme-eu:family:alice"
  required_target_floor:
    min_security_profile: SECURE_DEFAULT
    sandbox_floor: STRONGEST_VIABLE
  origin_decision:
    policy_decision_id: "pd_..." # origin host S2.3 must allow egress of this workload
    egress_grant_id: "og_..." # S8.1 OutboundGrant on the overlay
  target_decision:
    policy_decision_id: "pd_..." # target host S2.3 must independently allow ingress + run
    sandbox_profile_id: "sbx_..." # S3.2 sandbox the target will enforce
  evidence:
    routed_receipt: "evr_..." # REMOTE_WORKLOAD_ROUTED, emitted on BOTH hosts
    result_receipt: "evr_..."
```

```text
RemoteRoutingReason =
  HARDWARE_AFFINITY        // target has the GPU/NPU/device the workload needs
| CAPACITY_OFFLOAD         // origin is saturated; target has headroom
| ISOLATION_REQUIRED       // run the risky workload off the operator's primary host
| KERNEL_PERSONALITY_MATCH // target runs the kernel backend S18 selected
| RECOVERY_FAILOVER        // origin degraded; run elsewhere to keep service

RemoteRoutingClass =
  SANDBOXED_CAPSULE        // S17 app capsule shipped to target sandbox
| MICROVM_JOB              // S18 microVM/WASI backend on target
| DRIVER_LAB_JOB           // S19 driver lab run on a host that owns the device
| KERNEL_BUILD_JOB         // S18 kernel candidate built on a beefier target
| BLOCKED_ROUTE            // routing denied; carries a typed reason
```

Both enums are closed; unknown values rejected by the routing loader. The
two-sided decision is constitutional:

- The **origin** host must allow the workload to leave (S2.3 policy + S8.1
  outbound grant on the overlay). AI may _propose_ a route; it cannot approve it.
- The **target** host must _independently_ allow the workload to arrive and run,
  under its own profile and its own sandbox floor (S3.2). A cluster root cannot
  force a target to accept a workload (INV-026).
- A workload routed remotely runs in a sandbox at least as strong as the policy
  floor of the _stricter_ of the two hosts. Routing never relaxes isolation.
- `RUN_REMOTE` never carries a raw shell command; it carries a typed workload
  reference, exactly as S18/S17/S19 require for local execution.

## 10. Distributed evidence log

The single-host evidence log (S3.1) is a linear BLAKE3 hash chain
(`previous_receipt_hash`). Under fleet replication a strict linear chain across
hosts is impossible without a global lock, so S25 generalizes the per-host chain
into a **Merkle-DAG** while preserving append-only (INV-014).

```yaml
distributed_evidence_log:
  dag_id: "dag_<ULID>"
  cluster_id: "clr_<ULID>"
  host_chains: # each host keeps its own linear S3.1 chain unchanged
    - host_id: "host:a"
      head_receipt_hash: "blake3:..."
      sealed_segment_count: 42
  dag_nodes: # content-addressed; a node references its parents
    - node_id: "blake3:<digest>"
      host_id: "host:a"
      covers_segment: "seg_..."
      parents:
        ["blake3:<prev-on-host-a>", "blake3:<last-replicated-from-host-b>"]
  checkpoints: # periodic cluster-root-signed Merkle roots
    - checkpoint_id: "chk_<ULID>"
      merkle_root: "blake3:..."
      signed_by_cluster_root: "ed25519:..."
      inclusion_proof_scheme: RFC9162_LIKE
  consistency:
    append_only: true # INV-014 preserved (see rules)
    fork_detection: ENABLED
```

Rules that preserve INV-014 under replication:

- **Local chains are unchanged.** Each host keeps its own S3.1 linear BLAKE3
  chain exactly as today. The DAG only _references_ sealed local nodes; it never
  rewrites a host's chain. A host's evidence remains readable in recovery without
  the Cognitive Core (S3.1 invariant 5) and without the cluster.
- **Append-only across hosts** means: a DAG node, once published, is referenced
  by content hash and never edited. A correction is a _new_ node referencing the
  old one (the S3.1 "corrections are new records" rule, lifted to the DAG).
- **Replication is additive merge.** Receiving a peer's sealed nodes adds DAG
  edges; it can never delete or supersede a local node. A fork (two nodes
  claiming the same host+sequence) is itself recorded as a `FORK_DETECTED`
  evidence event, never silently resolved.
- **Cluster-root checkpoints are notarization, not authority.** A signed Merkle
  root lets any host prove a record was included by a point in time
  (CT-style inclusion proof). The checkpoint cannot mutate or revoke any
  underlying record. A missing or unsigned checkpoint degrades verification
  confidence; it never grants the cluster root write access to host evidence.
- **AI cannot mutate the DAG.** The S3.1 hard-deny `hd.evidence_log_mutation`
  extends to DAG nodes and checkpoints. AI subjects can read and request
  replication; they cannot author checkpoints or rewrite parents (INV-014).

## 11. Security profile gates

| Profile          | Fleet rule                                                                                                                                                                                                    |
| ---------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `DEV_RELAXED`    | Enrollment allowed with warning; mesh overlay permitted; cross-org delegation allowed loosely; checkpoints optional.                                                                                          |
| `SECURE_DEFAULT` | Enrollment requires host-root recognition signature; hub-and-spoke default; delegation requires accepting-host signature; periodic checkpoints required.                                                      |
| `STIG_ALIGNED`   | Enrollment requires TPM dual-chain attestation (DEC-R3-002); foreign realms capped at `SECURE_DEFAULT` ceiling; remote routing requires both-host policy decisions; checkpoints signed and retained extended. |
| `AIRGAP_HIGH`    | No live cross-org delegation; cluster confined to the local signed realm; overlay on local transport only; remote routing only to attested in-realm hosts; checkpoints exported to offline audit bundle.      |

Profile interaction rules:

- A fleet operation targets the **stricter** of origin/target profiles for any
  cross-host effect. The fleet never runs a workload under a weaker posture than
  either participating host requires.
- Joining a fleet can never lower a host's profile. If membership would require a
  weaker posture, the host is `SUSPENDED`, not weakened (INV-026 + S16.1).
- `FIPS_STRICT` (S16.5 overlay) on any participating host forces the overlay
  transport and checkpoint signatures onto the validated crypto provider for that
  host's contribution.

## 12. Hard denies

The Policy Kernel (S2.3) must deny these across the fleet plane:

| Policy id                               | Denied action                                                                             |
| --------------------------------------- | ----------------------------------------------------------------------------------------- |
| `hd.s25.cluster_override_host_policy`   | Cluster root issues a decision a member host's Policy Kernel would deny (INV-026).        |
| `hd.s25.cluster_weaken_profile`         | Cluster operation lowers a member host's `SecurityProfile`.                               |
| `hd.s25.cluster_mutate_host_evidence`   | Cluster root edits, deletes, or reorders a host's S3.1 chain or DAG node (INV-014).       |
| `hd.s25.cluster_become_root`            | Cluster root obtains a root/admin subject on a member host.                               |
| `hd.s25.ai_author_checkpoint`           | AI subject authors or signs a DAG checkpoint.                                             |
| `hd.s25.ai_approve_routing`             | AI subject approves remote workload routing (it may only propose).                        |
| `hd.s25.foreign_subject_admin`          | Foreign-realm subject performs an admin action beyond its signed ceiling.                 |
| `hd.s25.transitive_delegation_unsigned` | Trust delegation chained beyond `realms_path_max_hops` without explicit signed extension. |
| `hd.s25.silent_legacy_id_collision`     | A federated id maps onto a different local subject than its loss-free shim resolution.    |

## 13. New invariants proposed by S25

- **INV-026 (consumed; defined in `04_invariants.md`)** — A cluster root cannot
  override host policy: any cluster-issued effect on a member host is gated by
  that host's own Policy Kernel and security profile.
- **INV-032 (proposed by S25)** — _Federated identity is loss-free and
  non-escalating._ Every legacy single-host subject id resolves to exactly one
  federated `(realm:default, local_id)` and back without ambiguity, and a
  foreign-realm subject can never resolve to a more privileged local subject than
  its signed `CrossOrgTrustDelegation` ceiling permits. Listed in the return
  manifest for registration in `04_invariants.md`.

INV-027 (crypto-shred preserves the evidence chain) is _consumed_, not
introduced: erasing a federated subject's personal data via S16.9 crypto-shred
destroys the per-subject key while the host chain and the DAG node referencing it
remain intact and verifiable.

## 14. Evidence records

S25 adds these record types:

```text
FLEET_HOST_ENROLLED
FLEET_HOST_SUSPENDED
FLEET_HOST_WITHDRAWN
CLUSTER_ROOT_SIGNED
CLUSTER_ROOT_ROTATED
REMOTE_WORKLOAD_ROUTED
REMOTE_WORKLOAD_RESULT
EVIDENCE_DAG_REPLICATED
EVIDENCE_DAG_CHECKPOINT_SIGNED
EVIDENCE_DAG_FORK_DETECTED
FEDERATED_IDENTITY_RESOLVED
CROSS_ORG_DELEGATION_GRANTED
CROSS_ORG_DELEGATION_REVOKED
HOST_POLICY_OVERRIDE_DENIED
```

Minimum fields for `HOST_POLICY_OVERRIDE_DENIED`:

```text
host_id
cluster_id
cluster_root_pubkey
attempted_effect            // the cluster-issued effect that was rejected
host_policy_decision_id     // the local S2.3 decision that denied it
denied_policy_id            // e.g. hd.s25.cluster_override_host_policy
security_profile
subject                     // federated id of the requesting cluster actor
evidence_receipt_id
```

Minimum fields for `REMOTE_WORKLOAD_ROUTED`:

```text
routing_id
workload_ref
origin_host
target_host
reason
routing_class
subject
origin_policy_decision_id
target_policy_decision_id
target_sandbox_profile_id
security_profile_effective    // the stricter of origin/target
evidence_receipt_id
```

## 15. Non-goals

- Do not let a cluster root become a super-administrator. The host is sovereign;
  the cluster federates trust, routing, and evidence views only (INV-026).
- Do not silently bypass, weaken, or override a member host's security profile or
  Policy Kernel decision.
- Do not edit, delete, reorder, or rewrite any host's evidence under replication.
  The DAG is additive and append-only (INV-014).
- Do not claim global SSO certification or treat federation as legal trust
  attestation between organizations.
- Do not require mesh networking for small fleets, and do not promise the model
  scales beyond 100 hosts in Rev.3.
- Do not let AI author DAG checkpoints, approve remote routing, or escalate a
  foreign realm.
- Do not break single-host operation: a host with no cluster runs exactly as
  S5.1/S3.1 specify, and the federated id shim is loss-free.

## 16. Acceptance criteria

S25 is `REAL` only when:

1. `FleetMembership`, `ClusterTrustRoot`, `RemoteWorkloadRouting`,
   `DistributedEvidenceLog`, `FederatedIdentityBundle`, and
   `CrossOrgTrustDelegation` parse and reject unknown enum values.
2. A host recognizes a cluster root via its own host-root signature and can
   unilaterally withdraw recognition, returning to sovereign single-host trust.
3. The cluster trust chain adds exactly one hop above the S11.1 host root; no
   schema field can grant the cluster root host-administration rights.
4. The membership FSM enforces auto-`SUSPENDED` (not auto-weaken) when a host
   falls below its `min_security_profile`.
5. Every legacy single-host `SubjectId` resolves loss-free to
   `(realm:default, local_id)` and back (INV-032).
6. A foreign-realm subject is capped at its signed `CrossOrgTrustDelegation`
   ceiling and never resolves to a more privileged local subject.
7. `RemoteWorkloadRouting` requires _independent_ policy decisions on both origin
   and target hosts, with no raw shell payload, and runs under the stricter
   sandbox floor.
8. The `DistributedEvidenceLog` preserves each host's linear S3.1 chain unchanged,
   merges peers additively, records forks rather than resolving them silently,
   and never lets the cluster root mutate a host record (INV-014).
9. A cluster-issued effect that a member host's Policy Kernel would deny emits
   `HOST_POLICY_OVERRIDE_DENIED` and is rejected (INV-026).
10. AI subjects cannot author DAG checkpoints, approve remote routing, sign the
    cluster root, or escalate a foreign realm.

## 17. See also

- [S11.1 Repository Model + Trust Roots](../../002.AI-OS.NET--SPECREV.2/L10_Distribution_Ecosystem_Marketplace/01_repository_model.md)
- [S5.1 Identity Model](../../002.AI-OS.NET--SPECREV.2/L4_Policy_Identity_Vault/03_identity_model.md)
- [S8.1 Network Policy](../../002.AI-OS.NET--SPECREV.2/L8_Network_Hardware_Devices/02_network_policy.md)
- [S3.1 Evidence Log Architecture](../../002.AI-OS.NET--SPECREV.2/L9_Observability_Admin_Operations/01_evidence_log.md)
- [S2.3 Policy Kernel](../../002.AI-OS.NET--SPECREV.2/L4_Policy_Identity_Vault/01_policy_kernel.md)
- [S16.1 Security Profile Matrix](../S16_Security_Hardening_Compliance/01_security_profile_matrix.md)
- [S18 Kernel Personality and Portability Plane](../S18_Kernel_Personality_Portability/00_overview.md)
- [Rev.3 Design Decisions (DEC-R3-003, DEC-R3-011)](../02_design_decisions.md)
