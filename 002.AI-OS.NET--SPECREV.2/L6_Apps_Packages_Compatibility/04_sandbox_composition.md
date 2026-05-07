# Sandbox Composition Language (Rev.2)

| Field     | Value                                |
| --------- | ------------------------------------ |
| Status    | `CONTRACT` draft                     |
| Phase tag | S3.2                                 |
| Layer     | L6 Apps, Packages, Compatibility     |
| Consumes  | L4 policy decisions, L2 object paths, L3 adapter metadata |
| Produces  | applied sandbox profiles             |

## 1. Purpose

AIOS needs one sandbox profile language that can compile to Linux enforcement tools: namespaces, cgroups, seccomp, Landlock, SELinux/AppArmor, bind mounts, portals, Wine prefixes, Waydroid containers, and VM fallback.

## 2. Core invariant

The composed sandbox is the most restrictive combination of:

```text
adapter default
application manifest
user/request hint
policy requirement
runtime safety floor
```

Less restrictive requests cannot override stricter policy.

## 3. Profile shape

```yaml
profile_id: host-service-control
version: 1
filesystem:
  root: read_only
  allow_write:
    - /aios/runtime/actions/{action_id}
network:
  mode: host_limited
  allow:
    - localhost
process:
  seccomp: service-control
  no_new_privileges: true
resources:
  cpu_weight: 100
  memory_max: 512MiB
secrets:
  mode: broker_only
evidence:
  capture_stdout: redacted
  capture_stderr: redacted
```

## 4. Composition rules

| Field type      | Composition rule                         |
| --------------- | ---------------------------------------- |
| booleans        | restrictive wins                         |
| allow lists     | intersection unless policy says union    |
| deny lists      | union                                    |
| resource limits | lower limit wins                         |
| filesystem      | read-only by default; writes explicit    |
| network         | deny by default; endpoints explicit      |
| secrets         | broker-only unless human recovery mode   |

## 5. Enforcement backends

| Backend       | Role                                      |
| ------------- | ----------------------------------------- |
| namespaces    | process, mount, network isolation         |
| cgroups       | resource limits                           |
| seccomp       | syscall filtering                         |
| Landlock      | unprivileged path restrictions            |
| SELinux/AppArmor | MAC policy where available             |
| portals       | user-mediated desktop/file access         |
| containers    | app/runtime isolation                     |
| VM fallback   | hard isolation for hostile compatibility  |

Backend availability is host-specific. If required enforcement is unavailable, action fails closed unless policy explicitly allows degraded sandboxing.

## 6. Compatibility runtimes

Windows and Android applications receive composed profiles:

- Wine/Proton: isolated prefix per app, no broad home access, brokered file portals.
- Waydroid: per-app data isolation, network profile, clipboard/file portal mediation.
- VM fallback: explicit storage shares, network policy, evidence bridge.

## 7. Acceptance criteria

- Profiles are declarative.
- Composition is deterministic.
- Policy can only make profiles stricter.
- Missing required enforcement fails closed.
- Applied profile id is recorded in S0.1 execution.
- Foreign apps never run unrestricted on host.

