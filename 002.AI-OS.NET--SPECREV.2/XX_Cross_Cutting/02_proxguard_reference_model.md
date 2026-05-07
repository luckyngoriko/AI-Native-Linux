# ProxGuard Reference Model Notes (Rev.2)

| Field     | Value                                  |
| --------- | -------------------------------------- |
| Status    | `CONTRACT` reference note              |
| Phase tag | Rev.2 reference donor / system app candidate |
| Layers    | L3, L4, L6, L8, L9                     |
| Source    | Local ProxGuard code/spec inspection   |
| Evidence  | E1 artifact inspection; runtime tests not run |

## 1. Purpose

ProxGuard is treated in two roles:

1. Prototype donor for AIOS execution discipline.
2. Candidate AIOS system app for service deployment, DNS, gateway, and audit workflows.

It is not a core AIOS runtime dependency.

The useful pattern is:

```text
manifest
  -> simulation
  -> evidence
  -> release package
  -> inbox handoff
  -> production apply
  -> audit event
```

This maps closely to the AIOS model:

```text
intent
  -> action envelope
  -> policy decision
  -> simulated or live execution
  -> verification
  -> evidence receipt
```

## 2. Reusable concepts

### 2.0 ProxGuard as AIOS system app

ProxGuard may be packaged as an AIOS-native infrastructure application:

```text
/aios/apps/proxguard
  manifest
  private state
  requested capabilities
  sandbox profile
  verification plan
  rollback pointer
```

In this model ProxGuard is installed, updated, sandboxed, and removed like any other AIOS app. It does not receive raw host authority. It asks AIOS for capabilities and AIOS policy decides.

Candidate exported capabilities:

- `proxguard.service.simulate`
- `proxguard.service.deploy`
- `proxguard.service.restart`
- `proxguard.service.status`
- `proxguard.dns.plan`
- `proxguard.dns.apply`
- `proxguard.gateway.route`
- `proxguard.audit.read`

Example flow:

```text
human goal: "deploy this app behind HTTPS"
  -> AIOS intent
  -> action envelope
  -> policy decision
  -> ProxGuard app capability
  -> verification
  -> AIOS evidence receipt
```

The ProxGuard app may maintain its own internal audit log, but AIOS Evidence Log remains the system authority.

### 2.1 Manifest-driven control

ProxGuard's service manifest model is a useful seed for AIOS capability manifests.

AIOS should not copy the ProxGuard schema verbatim. Instead, it should adapt the idea into:

- `CapabilityManifest`
- target JSON Schema per action
- verification schema
- sandbox profile defaults
- rollback declaration
- simulation support declaration

### 2.2 Runtime adapter interface

ProxGuard's runtime adapter shape is directly relevant to L3:

- `plan`
- `validate`
- `deploy` / execute
- `restart`
- `stop`
- `status`
- `logs`

AIOS adapts this into typed capability adapters. The final AIOS execution runtime remains Rust-owned; ProxGuard Python code is only a reference implementation pattern.

### 2.3 Simulation-first execution

ProxGuard's simulation-first path validates the AIOS rule that unsafe actions must be previewable before live execution.

AIOS Rev.2 keeps this as:

- `dry_run=VALIDATE`
- `dry_run=SIMULATE`
- `dry_run=LIVE`

Destructive adapters must support simulation.

### 2.4 Deterministic policy

ProxGuard's deterministic `allow | warn | block` policy result is a useful seed for L4 Policy Kernel decisions.

AIOS extends this into:

- `ALLOW`
- `DENY`
- `APPROVAL_REQUIRED`
- `SIMULATION_REQUIRED`
- `DEGRADED_ALLOWED`
- `DEGRADED_DENIED`

The important reusable invariant is deterministic reason codes and canonical input hashes.

### 2.5 Inbox / release handoff

ProxGuard's release package and inbox watcher model is useful for separating proposal/approval from production execution.

AIOS may reuse the pattern as an implementation option for local or remote execution:

```text
approved action package
  -> sealed package hash
  -> executor inbox
  -> integrity verification
  -> apply
  -> evidence receipt
```

The AIOS executor must verify package identity, action hash, policy decision hash, and package hash before execution.

### 2.6 Evidence and audit split

ProxGuard separates simulation evidence from production audit. AIOS keeps the stronger Evidence Log contract, but the split is still useful:

- simulated evidence stream
- production evidence stream
- read-only joined operational view

This supports recovery, replay, and operator inspection without trusting the Cognitive Core.

### 2.7 DNS provider abstraction

ProxGuard's DNS provider abstraction is useful for L8 network operations.

Candidate AIOS capabilities:

- `dns.record.plan`
- `dns.record.apply`
- `dns.zone.inspect`
- `certificate.challenge.prepare`

Providers can include HE.net, Cloudflare, and manual instruction mode. Manual mode is important because not every provider has a reliable API.

### 2.8 Golden path testing

ProxGuard's golden path scripts are useful as a testing template:

```text
health
  -> simulate
  -> evidence persisted
  -> release created
  -> package submitted
  -> executor consumed package
  -> audit/evidence recorded
```

AIOS should define equivalent first proofs for:

- `service.restart`
- `package.install`
- `file.write` inside `/aios`
- `dns.record.plan`

## 3. App boundary requirements

When running as an AIOS app, ProxGuard must:

- declare all requested capabilities in its app manifest
- run under a composed sandbox profile
- store mutable state under its private `/aios/apps/proxguard/state` area
- expose public/LAN services only after L8 network policy approval
- access DNS provider credentials only through the AIOS Vault Broker
- emit AIOS evidence receipts for every privileged operation
- support simulation for destructive DNS, gateway, and deployment actions
- fail closed if AIOS policy, vault, sandbox, or evidence systems are unavailable

ProxGuard must not:

- mount Docker socket directly unless a policy-approved adapter profile grants it
- modify firewall, DNS, gateway, or service state outside typed AIOS capabilities
- store raw API tokens in its own config files
- bypass AIOS approvals through its own UI/API
- become required for boot, recovery, base package install, or Policy Kernel operation

## 4. Explicit non-goals

The following ProxGuard areas are not imported into AIOS Rev.2:

- ProxGuard product UI as the default AIOS UI
- pricing and billing flows
- Paddle integration
- SaaS workspace business model
- managed cloud provisioner
- NGINX/OpenResty gateway as core OS requirement
- ProxGuard branding or website content

These may remain separate product concerns.

## 5. Status and risk

The ProxGuard source tree is a useful local artifact, but its live runtime health is not established by this spec pass.

Therefore:

- Reference evidence grade: `E1`
- Runtime correctness: `UNKNOWN`
- Direct code reuse status: `DEFERRED`
- Conceptual reuse status: `CONTRACT`
- AIOS app candidate status: `CONTRACT`

AIOS may port concepts after tests prove the donor behavior and after the contracts are renamed into AIOS-native terminology. AIOS may package ProxGuard as an optional system app after its manifest, sandbox, vault access, network exposure policy, and evidence bridge are specified.

## 6. Mapping table

| ProxGuard concept              | AIOS target layer | AIOS interpretation                         |
| ------------------------------ | ----------------- | ------------------------------------------- |
| Service manifest               | L3, L5            | capability manifest and target schema       |
| Runtime adapter                | L3                | capability adapter                          |
| ProxGuard app package          | L6                | optional infrastructure system app          |
| DNS provider                   | L8                | network capability adapter                  |
| Policy engine                  | L4                | deterministic policy kernel seed            |
| Simulation service             | L3, L9            | dry-run execution plus evidence             |
| Release package                | L3, L9            | approved action package                     |
| Inbox watcher                  | L3                | isolated executor handoff option            |
| `sim.evidence_pack`            | L9                | simulated evidence stream                   |
| `prod.audit_event`             | L9                | production evidence stream                  |
| Golden path e2e tests          | L0, L9            | acceptance gate template                    |

## 7. Acceptance criteria for reuse

Before any ProxGuard-derived code becomes `REAL` in AIOS:

- The relevant donor component must have a passing test or smoke proof.
- The AIOS version must use AIOS names, action envelopes, status taxonomy, and evidence receipts.
- Policy and evidence must be preserved or made stricter.
- Shell execution must not become the primary execution interface.
- Secrets, billing code, and product-specific ProxGuard assumptions must be excluded.
- If packaged as an app, ProxGuard must pass install, update, rollback, sandbox, vault, network policy, and evidence bridge tests.
