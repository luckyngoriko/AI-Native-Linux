# S16.2 — SELinux MAC Policy Plane

| Field     | Value                                                                                                                                       |
| --------- | ------------------------------------------------------------------------------------------------------------------------------------------- |
| Status    | `CONTRACT` (created 2026-05-28; awaiting implementation evidence)                                                                           |
| Phase tag | S16.2                                                                                                                                       |
| Layer     | L1/L4/L6/L9/L10 cross-cutting                                                                                                               |
| Consumes  | S16.1 Security Profile Matrix, S2.3 Policy Kernel, S3.2 Sandbox Composition, S12.2 Package Model, S3.1 Evidence Log, S9.1 Recovery Boundary |
| Produces  | AIOS SELinux domains, labels, policy bundle lifecycle, AVC evidence records                                                                 |

## 1. Purpose

SELinux is the primary kernel-enforced mandatory access control backend for
`STIG_ALIGNED` and `AIRGAP_HIGH`. It does not replace the AIOS Policy Kernel.
It enforces the host boundary underneath policy decisions.

Invariant links: INV-003, INV-005, INV-008, INV-011, INV-017, INV-018.

## 2. Domain model

| Domain             | Purpose                                                                                    |
| ------------------ | ------------------------------------------------------------------------------------------ |
| `aios_policy_t`    | Policy Kernel process; reads signed policy bundles and emits policy decisions.             |
| `aios_vault_t`     | Vault Broker; owns secret material and exposes use-without-reveal operations.              |
| `aios_evidence_t`  | Evidence Log writer; append-only storage owner.                                            |
| `aios_sandbox_t`   | Sandbox Composer/enforcer; applies namespaces, seccomp, cgroups, Landlock, SELinux labels. |
| `aios_runtime_t`   | Capability Runtime and adapters; executes approved typed actions only.                     |
| `aios_renderer_t`  | KDE/Web/CLI/Mobile renderers; UI only, no direct policy/evidence mutation.                 |
| `aios_agent_t`     | AI agent processes; strictest filesystem, network, ptrace, device, and secret boundary.    |
| `aios_recovery_t`  | Recovery services and shell; only active in recovery/first-boot path.                      |
| `aios_package_t`   | Package/App Control Plane installer; writes only declared scopes after policy approval.    |
| `aios_container_t` | Local container supervision process; no Docker socket exposure by default.                 |
| `aios_k8s_t`       | Kubernetes profile control process; kubeconfig and CRI access are explicitly labeled.      |
| `aios_rt_t`        | RT session controller; manages admitted RT workloads only.                                 |

AIOS-owned services must not run as `unconfined_t` in `SECURE_DEFAULT`,
`STIG_ALIGNED`, or `AIRGAP_HIGH`.

## 3. Label namespaces

| Label family    | Examples                                                                     | Rule                                                     |
| --------------- | ---------------------------------------------------------------------------- | -------------------------------------------------------- |
| System config   | `aios_policy_conf_t`, `aios_mac_policy_conf_t`                               | Read by policy/recovery; mutation recovery-gated.        |
| Evidence        | `aios_evidence_log_t`, `aios_evidence_export_t`                              | Append by evidence writer; read through brokered API.    |
| Vault           | `aios_vault_store_t`, `aios_vault_socket_t`                                  | Only vault domain owns raw secret store.                 |
| App scopes      | `aios_app_code_t`, `aios_app_data_t`, `aios_app_cache_t`, `aios_app_state_t` | Bound to App Control Plane and workspace MCS category.   |
| Runtime sockets | `aios_runtime_sock_t`, `aios_policy_sock_t`, `aios_evidence_sock_t`          | Domain-specific access only.                             |
| Recovery        | `aios_recovery_conf_t`, `aios_recovery_exec_t`                               | Available to recovery path; not writable by normal apps. |
| Kubernetes      | `aios_kubeconfig_t`, `aios_cri_sock_t`                                       | Human/admin approval and profile gates required.         |
| Containers      | `aios_container_image_t`, `aios_container_volume_t`                          | Rootless default; device/socket grants explicit.         |

## 4. MCS/MLS mapping

AIOS maps high-level isolation into SELinux categories:

```text
group_id/workspace_id -> MCS category set
privacy_class         -> MLS-like level where supported
```

Required category separations:

| AIOS boundary             | SELinux treatment                                       |
| ------------------------- | ------------------------------------------------------- |
| Work vs Gaming            | Different MCS categories.                               |
| Admin vs normal user apps | Different MCS categories and stricter type allow rules. |
| Lab/untrusted apps        | Disposable category set; no cross-workspace read.       |
| AI agent domains          | Agent-specific categories; no raw secret/data read.     |
| Airgap workspace          | Dedicated categories plus network policy denial.        |

If the platform cannot enforce MLS/MCS correctly, `STIG_ALIGNED` promotion must
fail unless an explicit exception records the compensating control.

## 5. Policy bundle type

SELinux policy modules are AIOS packages of kind `MAC_POLICY_BUNDLE`.

```yaml
mac_policy_bundle:
  bundle_id: "aios.selinux.core"
  version: "2026.05.rev3"
  format: cil
  target_profiles: [SECURE_DEFAULT, STIG_ALIGNED, AIRGAP_HIGH]
  modules:
    - name: aios_core
      hash: "sha256:..."
      domains: [aios_policy_t, aios_vault_t, aios_evidence_t]
  signatures:
    - signer: aios-release
      signature: "..."
  rollback:
    previous_bundle_id: "aios.selinux.core@2026.04"
  compatibility:
    selinux_policy_version_min: "33"
    requires_mcs: true
```

## 6. Lifecycle

```text
policy source
  -> compile CIL/module
  -> static validation
  -> forbidden allow-rule scan
  -> sign bundle
  -> stage bundle
  -> recovery-gated install for system domains
  -> load
  -> emit MAC_POLICY_LOADED
  -> run scanner
  -> promote or rollback
```

Production rule:

- `audit2allow` output may be used as a diagnostic hint.
- `audit2allow` output must not be installed directly in production.
- Every allow rule must have a control rationale or app/workload capability
  reference.

## 7. Forbidden allow patterns

The policy build must reject:

| Pattern                                                | Reason                                  |
| ------------------------------------------------------ | --------------------------------------- |
| `aios_agent_t` reading `aios_vault_store_t`            | Raw secret disclosure.                  |
| `aios_renderer_t` writing policy/evidence/vault stores | UI cannot mutate authority.             |
| App/container domains writing boot/recovery paths      | Recovery chain protection.              |
| App domains broad-reading all home directories         | Workspace isolation failure.            |
| Any AIOS domain becoming `unconfined_t`                | STIG profile violation.                 |
| Non-recovery domains loading MAC policy                | Policy mutation must be recovery-gated. |
| Container domains accessing Docker socket by default   | Root-equivalent bypass.                 |

## 8. AVC evidence translation

Every relevant AVC denial involving AIOS domains emits an evidence record:

```text
MAC_AVC_DENIAL
  domain
  target_type
  permission
  path_redacted
  pid
  app_id
  workspace_id
  security_profile
  selinux_mode
  policy_bundle_hash
  related_action_id
  decision: deny | exception_requested | policy_bug
```

Noise control:

- Repeated identical denials may be coalesced.
- Denials involving vault, evidence, policy, recovery, boot, or AI agents are
  never silently dropped.

## 9. Runtime invariants

1. `STIG_ALIGNED` and `AIRGAP_HIGH` require `selinux=1 enforcing=1`.
2. `SECURE_DEFAULT` may temporarily tolerate permissive only with evidence and
   visible warning.
3. Policy reload always emits `MAC_POLICY_LOADED`.
4. Policy rollback always emits `MAC_POLICY_ROLLED_BACK`.
5. Recovery can disable a broken policy only by entering degraded recovery mode
   with evidence.
6. App Control Plane cannot install a package that requires forbidden MAC
   relaxation unless routed to VM or blocked.

## 10. Non-goals

The SELinux MAC plane does not attempt the following:

- It does not replace or duplicate the AIOS Policy Kernel (S2.3). SELinux
  enforces the host boundary underneath policy decisions; it does not make
  typed-action authorization, approval, or capability decisions.
- `audit2allow`-generated policy is never auto-installed. Its output is a
  diagnostic hint only; every allow rule still requires a control rationale
  and the signed-bundle lifecycle of §6.
- MAC labels and MCS categories do not grant cross-group, cross-workspace, or
  cross-privacy-class access that the Policy Kernel denies. A permissive MAC
  label cannot widen authority beyond a policy decision.
- It does not authenticate or identify users/agents (S2.3 identity). SELinux
  confines already-running domains; it does not establish who they are.
- It is not a substitute for the sandbox composition layer (S3.2). SELinux is
  one enforced layer alongside namespaces, seccomp, cgroups, and Landlock, not
  a replacement for them.

## 11. Acceptance criteria

S16.2 is `REAL` only when:

1. AIOS services run under AIOS-specific domains in test images.
2. `aios_agent_t` cannot read vault/evidence/policy raw stores.
3. `aios_renderer_t` cannot write policy/evidence/vault stores.
4. `aios_package_t` can write only declared install scopes.
5. AVC denials emit `MAC_AVC_DENIAL` evidence.
6. `STIG_ALIGNED` promotion fails if SELinux is absent, permissive, or
   unconfined AIOS services exist.
7. Policy bundle install and rollback are both recovery-gated and evidenced.
