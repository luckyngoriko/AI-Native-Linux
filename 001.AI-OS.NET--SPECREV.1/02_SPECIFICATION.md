# AI-Native Linux / Unified Cognitive Shell

Specification - Revision 1

Status: system specification, prototype contract, agent-readable engineering document.

> **Positioning note (added 2026-05-09, rev.2 era):** §1 below states "It is not another Linux distribution" — that was the rev.1 framing of AIOS as an operating _layer above_ Linux. **Rev.2 reframes AIOS as an AI-native Linux distribution** whose distinguishing component is the semantic operating layer described here. The architecture (L0–L10, AI-proposes-never-executes, evidence-first, recovery without cognition) is unchanged. See [`002.AI-OS.NET--SPECREV.2/01_executive_summary.md`](../002.AI-OS.NET--SPECREV.2/01_executive_summary.md) and [`002.AI-OS.NET--SPECREV.2/03_architecture_overview.md`](../002.AI-OS.NET--SPECREV.2/03_architecture_overview.md) for the current positioning. The body of this rev.1 specification is preserved verbatim as a frozen historical artifact.

This document is written as the canonical technical specification for AIOS: an AI-native Linux operating environment where human goals become typed, policy-checked, verified system actions.

It is not a marketing document and it does not define a calendar plan.

---

## 1. Core Definition

AIOS is a unified human-machine cognitive environment built above Linux.

It is not another Linux distribution. Linux remains the execution substrate: kernel, drivers, scheduler, memory manager, process isolation, filesystems, and syscall layer.

AIOS adds the semantic operating layer:

- intent understanding
- planning
- typed system actions
- policy decisions
- verification
- evidence logging
- persistent operational memory
- multi-agent coordination
- unified KDE/Web/CLI/Voice interaction surfaces

Core definition:

```text
AIOS is an AI-native Linux cognitive shell where goals become typed,
policy-checked, verified system actions.
```

The machine should understand goals instead of requiring the user to manually compose command sequences.

The AI layer must remain above kernel space.

---

## 2. Product Goal

The product goal is a unified cognitive operating environment.

The user can express a goal such as:

```text
prepare a Rust development environment
```

AIOS transforms it into:

```text
intent
-> plan
-> typed actions
-> policy decisions
-> runtime execution
-> verification
-> evidence
-> user-facing result
```

The same state and workflow must be available through:

- KDE desktop shell
- Web interface
- CLI
- future voice interface
- future mobile interface

These are renderers over one cognitive core, not separate products.

---

## 3. Non-Goals

Revision 1 does not define:

- a new Linux distribution
- a new kernel architecture
- AI running inside the Linux kernel
- unrestricted AI shell execution
- autonomous destructive administration
- replacement of Linux process isolation
- replacement of KDE Plasma
- full mobile implementation
- full voice implementation
- production readiness
- guaranteed compatibility with all Windows or Android applications
- security claims without evidence

Revision 1 does define:

- the system architecture
- the layer model
- the typed action model
- the Policy Kernel
- the Capability Runtime
- the AIOS-FS model
- recovery boundary
- renderer contracts
- app/package/compatibility contracts
- verification and evidence contracts
- first prototype acceptance criteria

---

## 4. Core Principles

1. One cognitive layer.
2. Many execution environments.
3. One unified interface model.
4. Linux remains the execution substrate.
5. AI operates above the kernel, never in kernel space.
6. AI never directly executes privileged shell commands.
7. System operations are typed actions.
8. Policy checks happen before execution.
9. Verification happens after execution.
10. Evidence is append-only.
11. Secrets are brokered, not revealed.
12. Recovery must not depend on AI.
13. The base OS is immutable by default.
14. `/root` remains a boring recovery island.
15. `/aios` is the AI-native system root.
16. KDE and Web are renderers over shared state.
17. Every capability has an owner.
18. Every state-changing capability has an acceptance gate.
19. No implementation claim is complete without evidence.
20. Failure states must be visible to the user.

---

## 5. High-Level Architecture

```text
Human
  |
  v
Unified Cognitive Shell
  |
  v
Cognitive Core
  |
  v
Semantic Runtime
  |
  v
Policy Kernel
  |
  v
Capability Runtime
  |
  v
Kernel Adaptation Layer
  |
  v
Linux / Cloud / Devices
```

Correct execution model:

```text
AI -> Action Plan -> Policy Check -> Typed Runtime -> Verified Result
```

Rejected execution model:

```text
AI -> sudo bash
```

---

## 6. Layer Model

AIOS uses a layered implementation and governance model.

Layer dependency rule:

```text
A layer may depend on its own layer and lower-numbered layers.
A layer must not require a higher-numbered layer for correctness.
```

| Layer | Name                                                  | Responsibility                                                                      |
| ----- | ----------------------------------------------------- | ----------------------------------------------------------------------------------- |
| L0    | Constitutional Truth / Governance / Evidence / Safety | status taxonomy, gates, evidence law, invariants                                    |
| L1    | Kernel and Host Bootstrap                             | Linux substrate, generic fallback kernel, recovery path, dedicated kernel candidate |
| L2    | AIOS-FS                                               | semantic object filesystem, `/aios`, versions, views, transactions                  |
| L3    | AIOS-SGR and Runtime                                  | desired machine state, service graph, runtime transitions                           |
| L4    | Policy / Identity / Vault                             | subjects, capabilities, approvals, secrets, policy packages                         |
| L5    | Cognitive Core and Model Governance                   | intent, planning, memory, model routing, agent coordination                         |
| L6    | Apps / Packages / Compatibility                       | AIOS packages, apps, Windows/Android/Linux compatibility                            |
| L7    | Interaction Renderers                                 | KDE, Web, CLI, Voice, Mobile, shared UI schema                                      |
| L8    | Network / Hardware / Devices                          | network policy, hardware graph, drivers, firmware, devices                          |
| L9    | Observability / Admin / Operations                    | health, logs, metrics, evidence viewer, recovery operations                         |
| L10   | Distribution / Ecosystem / Marketplace                | publishing, repositories, marketplace, external integrations                        |

Layer invariants:

- L1 recovery must not depend on L5 cognition.
- L2 filesystem truth must not depend on L7 UI.
- L3 runtime may ask L4 policy, but must not bypass it.
- L5 may propose actions, but must not execute them directly.
- L7 renderers must not own authoritative system state.

---

## 7. L0 Governance and Evidence

L0 owns truth.

Canonical statuses:

- `REAL`
- `PARTIAL`
- `SHELL`
- `CONTRACT`
- `DEFERRED`
- `BLOCKED`
- `UNKNOWN`
- `RETIRED`

Status meanings:

| Status     | Meaning                                                |
| ---------- | ------------------------------------------------------ |
| `REAL`     | implemented and verified with current evidence         |
| `PARTIAL`  | working but incomplete or partially verified           |
| `SHELL`    | placeholder exists but behavior is not implemented     |
| `CONTRACT` | interface or obligation exists, implementation pending |
| `DEFERRED` | intentionally outside current scope                    |
| `BLOCKED`  | cannot proceed until a named blocker clears            |
| `UNKNOWN`  | state is not known                                     |
| `RETIRED`  | superseded or intentionally removed                    |

Evidence grades:

| Grade | Meaning                                                      |
| ----- | ------------------------------------------------------------ |
| E0    | no evidence                                                  |
| E1    | file, folder, or artifact exists                             |
| E2    | build, typecheck, lint, or schema validation                 |
| E3    | unit, integration, smoke, or targeted verification           |
| E4    | end-to-end, recovery, release gate, or reproducible workflow |
| E5    | live operational, repeated, production-grade evidence        |

L0 truth rules:

- no capability is complete without evidence
- no capability is ownerless
- no state-changing operation is valid without policy decision
- no high-risk action is valid without approval or explicit policy
- no AI output is authoritative without verification
- no degraded state may be hidden from the user
- no layer may depend upward for correctness

Evidence receipt schema:

```json
{
  "receipt_id": "evr_01HX",
  "claim": "service restart action verified",
  "method": "verify.service_active",
  "result": "passed",
  "evidence_grade": "E3",
  "status": "REAL",
  "created_at": "2026-05-06T00:00:00Z"
}
```

---

## 8. Host Bootstrap and Recovery

AIOS installs on top of a stable Linux base.

Revision 1 preferred installer path:

- openSUSE Agama
- openSUSE transactional / MicroOS / Kalpa-style immutable base

Fallback installer path:

- Fedora Anaconda
- Fedora Atomic / Kinoite-style immutable KDE base

First boot flow:

```text
existing installer
-> generic Linux boot
-> AIOS Bootstrapper
-> create /aios volume
-> install AIOS runtime
-> install default local control model
-> configure AI provider mode
-> register recovery boot path
-> generate hardware map
-> build dedicated kernel candidate
-> verify and promote candidate only after health checks pass
```

Recovery boundary:

```text
/      recovery-safe immutable base
/root  human/operator recovery island
/aios  AI-native cognitive system root
```

Recovery rules:

- `/` must boot without `/aios`.
- `/root` must remain available for emergency repair.
- `/aios` may be mounted read-only for recovery.
- dedicated kernel failure must fall back to generic kernel.
- recovery must not require an LLM.
- recovery must not require Web UI.
- recovery must not require KDE session.

Dedicated kernel pipeline:

```text
hardware map
-> kernel source trust check
-> host-specific config generation
-> hardening profile
-> sandbox build
-> boot candidate
-> health verification
-> A/B promotion or rollback
```

AIOS does not fork Linux as a new kernel architecture. It builds a host-specific hardened kernel candidate from trusted Linux sources and promotes it only with rollback.

---

## 9. AIOS-FS

AIOS-FS is the native semantic filesystem mounted at `/aios`.

Definition:

```text
AIOS-FS is a semantic object filesystem where files are versioned objects,
paths are views, and every write has intent, policy, provenance, rollback,
and evidence.
```

Top-level layout:

```text
/aios
  /objects      content-addressed object storage
  /views        query-backed semantic views
  /apps         application writable state
  /users        user cognitive workspaces
  /agents       AI agent workspaces
  /projects     project projections
  /evidence     append-only evidence records
  /policies     filesystem policy metadata
  /sandboxes    isolated execution scratch space
  /system       AIOS-FS internal state
```

Core primitives:

- content-addressed chunks
- semantic objects
- immutable versions
- current pointers
- transaction journal
- semantic views
- object-level policy labels
- object-level provenance
- rollback pointers
- POSIX projections
- append-only evidence links

Object identity:

```text
object_id = stable logical identity
version_id = immutable version identity
chunk_id = content-addressed storage chunk
view_path = query-backed projection path
```

Object metadata example:

```json
{
  "object_id": "obj_01HXAIOS9A71",
  "kind": "source_file",
  "semantic_name": "main Rust entrypoint for cognitive runtime",
  "owner": "project:aios",
  "current_version": "ver_01HXAIOS20260506A",
  "policy_label": "project_source",
  "trust_level": "verified",
  "classification": {
    "content_type": "text/rust",
    "risk": "normal",
    "contains_secret": false
  },
  "evidence": ["evr_01HXBUILDOK"]
}
```

Version record example:

```json
{
  "version_id": "ver_01HXAIOS20260506A",
  "object_id": "obj_01HXAIOS9A71",
  "parent_versions": ["ver_01HXAIOS20260505Z"],
  "created_by": "agent:dev",
  "intent": "refine runtime contract",
  "chunks": ["chk_blake3_7f2a"],
  "content_digest": "blake3:...",
  "verification": ["schema.valid", "tests.pass"],
  "evidence_receipt": "evr_01HXVERSIONOK",
  "status": "verified"
}
```

Write flow:

```text
propose write
-> policy check
-> sandbox write
-> classify content
-> scan risk
-> create version
-> verify
-> move pointer
-> write evidence
```

AIOS-FS rules:

- committed data is never overwritten in place
- writes create new versions
- pointer moves promote state
- rollback moves a pointer to a previous verified version
- semantic views are not authoritative storage identity
- semantic indexes are rebuildable from object metadata
- recovery can inspect objects without Cognitive Core

Semantic view example:

```text
/aios/views/latest-stable/sdf-renderer
/aios/views/security/quarantined
/aios/views/recovery/rollback-targets
```

Semantic resolution result:

```json
{
  "request": "open latest stable sdf renderer",
  "candidates": [
    {
      "object_id": "obj_sdf_renderer",
      "version_id": "ver_sdf_renderer_stable",
      "view_path": "/aios/views/latest-stable/sdf-renderer",
      "confidence": 0.91,
      "reason": "latest verified stable renderer in active project"
    }
  ],
  "requires_confirmation": false
}
```

AIOS-FS recovery modes:

| Mode            | Behavior                                             |
| --------------- | ---------------------------------------------------- |
| `normal`        | read/write, policy enforced, semantic indexes active |
| `safe_readonly` | read-only mount, no pointer movement                 |
| `repair`        | explicit repair commands, no AI autonomy             |
| `quarantine`    | suspicious objects isolated from views               |
| `reindex`       | rebuild semantic and vector indexes from metadata    |

---

## 10. AIOS-SGR: Service Graph Runtime

AIOS-SGR owns desired and runtime machine state.

It is not a shell wrapper. It is a desired-state graph runtime.

AIOS-SGR manages:

- services
- one-shot jobs
- timers
- mounts
- devices
- app sessions
- agent workers
- model servers
- health checks
- rollback
- resource limits
- sandbox profiles
- approval gates

Unit manifest example:

```json
{
  "unit": "org.aios.dashboard",
  "kind": "service",
  "desired_state": "running",
  "artifact": "obj_dashboard_bin",
  "config": "obj_dashboard_config",
  "state_dir": "/aios/apps/org.aios.dashboard/state",
  "requires": ["aiosfs.mounted", "network.localhost"],
  "sandbox": {
    "network": "localhost_only",
    "filesystem": "app_private_write",
    "syscalls": "restricted"
  },
  "verification": ["process.running", "http.127.0.0.1:8080.ok"],
  "rollback": "previous_verified_artifact"
}
```

State transition flow:

```text
desired state changes
-> policy check
-> dependency graph solve
-> stage candidate
-> start in sandbox
-> verify health
-> promote or rollback
-> write evidence
```

AIOS-SGR rules:

- runtime transitions require policy
- health checks must be explicit
- failed promotion must roll back where possible
- state graph must be inspectable
- runtime correctness must not depend on LLM availability

---

## 11. Policy Kernel

The Policy Kernel is the operating constitution.

It decides whether a typed action is allowed, denied, requires approval, requires sandboxing, or requires authentication.

Allowed decisions:

- `allow`
- `deny`
- `requires_approval`
- `requires_sandbox`
- `requires_authentication`
- `requires_verification`
- `quarantine`

Default rule:

```text
If no rule matches, deny.
```

Rule precedence:

```text
emergency lockout
-> explicit deny
-> secret protection
-> destructive action checks
-> network/public exposure checks
-> privileged system mutation checks
-> scoped allow
-> default deny
```

Subject types:

- `human`
- `agent`
- `application`
- `service`
- `device`
- `workflow`
- `remote_operator`

Policy evaluation request:

```json
{
  "request_id": "polreq_01HX",
  "subject": "agent:dev",
  "action": "service.restart",
  "target": {
    "service": "nginx"
  },
  "reason": "apply updated local web configuration",
  "intent_id": "intent_01HX",
  "plan_id": "plan_01HX",
  "environment": "local",
  "risk": {
    "destructive": false,
    "network_exposure": false,
    "privileged": true,
    "secret_access": false
  }
}
```

Policy decision:

```json
{
  "request_id": "polreq_01HX",
  "decision": "requires_approval",
  "policy_id": "service.restart.nginx.approval_required",
  "reason": "nginx is a shared ingress service",
  "obligations": [
    "approval.human.local_operator",
    "verification.service_active",
    "evidence.write"
  ],
  "ttl_seconds": 300
}
```

Hard-denied action classes:

- raw secret read by AI agent
- recursive delete of `/home`, `/root`, `/`, or `/aios`
- disabling recovery boot path
- removing fallback kernel without verified replacement
- public exposure of service without explicit policy
- running foreign installers without sandbox
- modifying policy rules without governance approval
- altering evidence logs

Approval rules:

- approval must show concrete action, target, reason, and risk
- approval must be bound to one exact action request
- approval must expire
- approval must be recorded as evidence
- vague shell command approvals are not valid
- approval cannot override hard deny unless emergency override allows it

Emergency override:

- can be activated only by a human operator
- must be scoped
- must expire
- must write evidence
- cannot delete evidence
- cannot disable future evidence logging

Policies are versioned AIOS package objects.

Policy lifecycle:

```text
draft
-> schema validate
-> simulate
-> review
-> approve
-> stage
-> activate
-> monitor
-> rollback
```

Baseline policy tests:

- raw secret read by AI is denied
- brokered secret use can be approved
- recursive delete of `/home` is denied
- recursive delete of `/aios` is denied
- local service status is allowed
- shared ingress restart requires approval
- public network exposure requires approval
- compatibility installer requires sandbox
- evidence log mutation is denied
- emergency override requires human operator

---

## 12. Vault Broker

Secrets are capabilities, not files.

Rejected model:

```text
AI reads ~/.ssh/id_rsa
AI reads API_TOKEN
AI reads kubeconfig
```

Required model:

```text
AI requests operation
-> Policy Kernel evaluates capability
-> Vault Broker performs approved operation
-> secret material is not revealed
-> Evidence Log records operation without secret value
```

Secret classes:

- SSH keys
- API tokens
- certificates
- GPG and signing keys
- Wi-Fi credentials
- VPN keys
- cloud credentials
- kubeconfigs
- database passwords
- application secrets
- passkeys and FIDO references

Vault rules:

- raw secret read is denied by default
- use-without-reveal is the normal operation model
- permissions are purpose-specific
- permissions are target-specific
- grants should be short-lived where practical
- evidence must never contain secret values
- revocation must be supported
- rotation must be supported

Example:

```text
AI requests git push
-> Policy checks secret.use.ssh_key_for_git_push
-> Vault Broker asks for approval if required
-> ssh-agent performs authentication
-> AI never reads the private key
-> Evidence records actor, repo, operation, decision, and result
```

---

## 13. Typed Actions and Capability Runtime

AIOS operations are typed actions.

Action envelope:

```json
{
  "action_id": "act_01HX",
  "intent_id": "intent_01HX",
  "plan_id": "plan_01HX",
  "action": "service.restart",
  "version": "1.0",
  "subject": "agent:dev",
  "target": {
    "service": "nginx"
  },
  "reason": "apply updated local web configuration",
  "environment": "local",
  "risk": {
    "destructive": false,
    "privileged": true,
    "network_exposure": false,
    "secret_access": false
  },
  "verification": [
    {
      "type": "service.active",
      "target": "nginx"
    }
  ],
  "correlation_id": "corr_01HX"
}
```

Action lifecycle:

```text
created
-> policy_pending
-> approved | approval_pending | policy_denied
-> queued
-> executing
-> verifying
-> succeeded | failed | rolled_back
```

Capability Runtime API:

```proto
service CapabilityRuntime {
  rpc ValidateAction(ActionEnvelope) returns (ValidationResult);
  rpc EvaluatePolicy(ActionEnvelope) returns (PolicyDecision);
  rpc RequestApproval(ApprovalRequest) returns (ApprovalResult);
  rpc ExecuteAction(ApprovedAction) returns (ActionResult);
  rpc VerifyAction(VerificationRequest) returns (VerificationResult);
  rpc RollbackAction(RollbackRequest) returns (ActionResult);
  rpc GetActionStatus(ActionStatusRequest) returns (ActionStatus);
  rpc ListAdapters(ListAdaptersRequest) returns (AdapterList);
  rpc GetAdapterCapabilities(AdapterCapabilitiesRequest) returns (AdapterCapabilities);
}
```

API rules:

- `ExecuteAction` accepts only approved actions
- expired approvals must be rejected
- modified envelopes must be rejected
- state-changing calls must emit evidence
- adapters must not accept free-form shell commands as primary input
- unsupported actions fail closed

Initial capability families:

- filesystem
- AIOS-FS
- service management
- package management
- process management
- repository operations
- container operations
- verification
- network management
- secret broker operations
- desktop/session operations

Adapter manifest:

```json
{
  "adapter": "systemd.local",
  "version": "1.0.0",
  "actions": [
    "service.status",
    "service.start",
    "service.stop",
    "service.restart",
    "service.reload"
  ],
  "backend": "systemd",
  "requires": ["linux.systemd.available"],
  "verification": ["service.active", "service.enabled"]
}
```

Action result:

```json
{
  "action_id": "act_01HX",
  "status": "succeeded",
  "adapter": "systemd.local",
  "policy_decision": "poldec_01HX",
  "output": {
    "summary": "nginx restarted successfully",
    "changed": true,
    "raw_ref": "evidence://logs/act_01HX"
  },
  "verification": [
    {
      "type": "service.active",
      "status": "passed",
      "observed": "active"
    }
  ],
  "evidence_receipt": "evr_01HXACTIONOK"
}
```

---

## 14. Verification and Evidence

Verification proves that an action produced the intended result.

Verification must be:

- explicit
- typed
- attached to the action
- logged
- visible to the user

Verification types:

- `service.active`
- `service.enabled`
- `process.running`
- `port.open`
- `http.ok`
- `file.exists`
- `file.digest.matches`
- `schema.valid`
- `test.pass`
- `package.installed`
- `aiosfs.object.exists`
- `aiosfs.pointer.matches`
- `policy.simulation.pass`
- `kernel.boot.candidate.ok`
- `recovery.path.available`

Evidence log rules:

- evidence is append-only
- evidence cannot be modified by AI agents
- evidence records must reference action, policy decision, and verification
- sensitive values must be redacted
- failed operations are evidence
- denied operations are evidence
- rollback operations are evidence

Evidence receipt:

```json
{
  "receipt_id": "evr_01HXACTIONOK",
  "action_id": "act_01HX",
  "policy_decision": "poldec_01HX",
  "verification": [
    {
      "type": "service.active",
      "status": "passed"
    }
  ],
  "result": "success",
  "grade": "E3"
}
```

---

## 15. Cognitive Core

The Cognitive Core is responsible for cognition, not direct execution.

Required modules:

- Intent Engine
- Semantic Context Engine
- Planner / Orchestrator
- Capability Translator
- Policy Client
- Verification Assistant
- Persistent Memory
- System Knowledge Graph
- Agent Coordinator
- Evidence Logger
- Model Router

Intent object:

```json
{
  "intent_id": "intent_01HX",
  "raw_goal": "restart nginx and verify the site",
  "actor": "human:lucky",
  "context": {
    "project": "aios",
    "environment": "local"
  },
  "risk_hint": "privileged_service_operation"
}
```

Plan object:

```json
{
  "plan_id": "plan_01HX",
  "intent_id": "intent_01HX",
  "steps": [
    {
      "step": "check service status",
      "action": "service.status",
      "target": "nginx"
    },
    {
      "step": "restart service",
      "action": "service.restart",
      "target": "nginx",
      "policy": "requires_approval"
    },
    {
      "step": "verify web endpoint",
      "action": "verify.http_ok",
      "target": "https://localhost"
    }
  ]
}
```

Cognitive Core rules:

- may propose plans
- may explain policy decisions
- may summarize evidence
- may request typed actions
- must not execute privileged shell commands
- must not approve its own high-risk actions
- must not read raw secrets
- must degrade if external AI is unavailable

AI Provider Bootstrap modes:

- local default model only
- external API provider token
- local powerful model
- hybrid local and external

Model routing rules:

- boot and recovery cannot depend on external AI
- default local model handles basic offline control and status explanation
- powerful model handles deeper planning when available
- external model may be used for high-capability reasoning
- model calls must exclude secrets
- model routing decisions must be logged

---

## 16. Unified Cognitive Shell and Renderers

All renderers use the same cognitive state.

```text
                Cognitive Core
                       |
        +--------------+--------------+
        |                             |
   KDE Renderer                 Web Renderer
        |                             |
 Qt/QML Plasma                Next.js/WebAssembly
```

Supported renderer targets:

- KDE Plasma
- Web
- CLI
- future Voice
- future Mobile

Shared UI schema example:

```json
{
  "component": "approval_prompt",
  "id": "approval_network_expose",
  "title": "Expose dashboard on LAN?",
  "state_binding": "policy.approval.apr_01HX",
  "actions": ["approve_once", "deny", "approve_with_limits"]
}
```

Renderer rules:

- renderers submit intents or typed actions
- renderers do not own authoritative system state
- renderers do not bypass policy
- renderers display evidence
- renderers display denial reasons
- renderers display degraded states
- Web UI is localhost-only by default
- LAN or remote exposure requires policy approval

KDE renderer components:

- KRunner plugin
- Plasma widget
- tray entry
- notification bridge
- approval prompt
- evidence viewer
- command palette

Web renderer components:

- goal input
- plan viewer
- approval prompts
- action status stream
- evidence viewer
- AIOS-FS object browser
- service graph viewer
- policy explanation view

---

## 17. Application, Package, and Compatibility Model

AIOS applications are versioned application objects stored in AIOS-FS.

Application manifest:

```json
{
  "app": "org.aios.dashboard",
  "version": "1.0.0",
  "artifacts": {
    "binary": "obj_bin_dashboard_1",
    "config": "obj_cfg_dashboard_1"
  },
  "identity": "application:org.aios.dashboard",
  "capabilities": [
    "network.listen.localhost",
    "aiosfs.evidence.read.own",
    "ui.render.panel"
  ],
  "sandbox": {
    "filesystem": "app_private_write",
    "network": "localhost_only",
    "devices": [],
    "syscalls": "restricted"
  },
  "state_dir": "/aios/apps/org.aios.dashboard/state",
  "rollback": "previous_verified_version"
}
```

Application rules:

- every app has a manifest
- every app has an identity
- every app has private writable state
- broad filesystem writes are denied by default
- updates stage new versions before promotion
- rollback uses previous verified versions

Package object:

```text
signed object bundle
+ manifest
+ requested capabilities
+ install plan
+ verification plan
+ rollback pointer
```

Package types:

- application
- service
- AI model
- policy
- UI schema
- kernel artifact
- compatibility profile
- workflow template

Update flow:

```text
fetch package
-> verify signature
-> unpack staged objects
-> check requested capabilities
-> policy decision
-> install or upgrade in sandbox
-> verify
-> promote object pointers
-> write evidence
```

Compatibility targets:

- Linux native packages
- Flatpak
- AppImage
- containers
- Windows EXE/MSI through Wine/Proton-compatible runtimes
- Windows games through Proton-compatible runtimes
- Android APK through Waydroid-style runtime
- VM fallback for hard compatibility cases

Compatibility compiler flow:

```text
foreign artifact
-> static analysis
-> compatibility knowledge lookup
-> runtime plan
-> disposable sandbox install
-> telemetry and log analysis
-> profile generation
-> verification
-> AIOS app object
```

Compatibility rules:

- foreign installers never run unrestricted on the host
- every Windows app gets an isolated prefix
- Android apps get isolated per-app data
- filesystem access is through portals
- hard DRM, kernel drivers, or anti-cheat may require VM fallback or blocked status
- blocked applications must include explanation

---

## 18. Hardware and Network

AIOS-HDM is the Hardware and Driver Manager.

It owns the hardware graph:

- CPU and microcode
- GPU and accelerators
- storage controllers and disks
- network adapters
- audio devices
- Bluetooth
- USB and Thunderbolt
- printers and scanners
- sensors, battery, thermal devices
- firmware update paths
- removable device policy

Hardware lifecycle:

```text
detect device
-> identify vendor and model
-> determine driver and firmware requirements
-> classify risk
-> enable trusted components
-> verify device function
-> update hardware graph
-> feed kernel builder and policy runtime
```

AIOS-NP is the Network Policy Manager.

Default network posture:

```text
deny public exposure
local-first services
explicit LAN exposure
explicit remote exposure
per-app outbound policy
evidence for every network state change
```

Preferred backends:

- NetworkManager
- nftables
- systemd-resolved or compatible resolver backend
- WireGuard
- mDNS/Avahi only when policy allows

Network rules:

- services default to localhost-only
- listening on `0.0.0.0` requires approval
- opening firewall ports requires approval
- public exposure requires explicit approval
- DNS and VPN changes are logged
- per-app outbound access is declared

---

## 19. Security Requirements

AIOS security is policy-first and evidence-first.

Required enforcement primitives:

- SELinux where available
- AppArmor where SELinux is not default
- Landlock for unprivileged filesystem restrictions
- seccomp for syscall filtering
- cgroups for resource control
- namespaces for isolation
- TPM2 where available
- FIDO/passkey support where available

Security invariants:

- AI agents never run as unrestricted root
- raw secrets are not exposed to AI agents
- privileged actions require policy
- destructive actions require explicit treatment
- public network exposure requires approval
- recovery path cannot be removed without verified replacement
- evidence logs cannot be modified by AI agents
- compatibility installers run in sandbox
- policy changes require simulation and evidence

---

## 20. Observability and Failure Model

AIOS must be observable by default.

Required observability:

- action history
- policy decisions
- approvals and denials
- verification results
- service graph state
- AIOS-FS transaction journal
- evidence receipts
- resource pressure
- model routing decisions
- adapter failures
- recovery events

Preferred technologies:

- OpenTelemetry
- Prometheus
- Loki
- eBPF where appropriate
- structured logs
- append-only evidence log

Failure handling:

| Failure                      | Required Behavior                                |
| ---------------------------- | ------------------------------------------------ |
| dedicated kernel fails       | boot fallback generic kernel                     |
| `/aios` fails to mount       | boot recovery root, offer read-only/repair mode  |
| Policy Kernel unavailable    | fail closed, recovery runbook                    |
| LLM unavailable              | degrade cognition, keep runtime/recovery working |
| service update fails         | rollback previous verified artifact              |
| disk full                    | enter safe mode, preserve evidence               |
| trust failure                | block promotion, quarantine artifact             |
| network compromise suspected | disable exposure, preserve logs                  |

---

## 21. Technology Stack

Core runtime:

- Rust
- Tokio
- tonic gRPC
- serde
- tracing

Cognition:

- Python
- LangGraph or equivalent orchestration
- FastAPI where HTTP control surfaces are needed
- local model runtime through Ollama/vLLM-compatible providers
- optional external model providers through Vault Broker

UI:

- KDE Plasma
- Qt/QML
- TypeScript
- Next.js or equivalent Web renderer
- Tailwind/shadcn-compatible component discipline where useful

Storage:

- AIOS-FS
- SQLite for local metadata where appropriate
- PostgreSQL where service-grade relational state is required
- Qdrant or equivalent vector store where needed

Observability:

- OpenTelemetry
- Prometheus
- Loki
- eBPF

Stack philosophy:

```text
Rust owns execution.
Python owns cognition.
KDE owns native desktop interaction.
Web owns remote interaction surfaces.
Linux owns physics.
AIOS owns semantic operation.
```

---

## 22. Revision 1 MVP Scope

The MVP is a narrow golden path.

Golden path:

```text
Boot from recovery-safe root,
mount /aios,
create a versioned AIOS-FS object,
resolve it through a semantic view,
run one verified typed system action,
record the full evidence chain,
show the result in a renderer.
```

The MVP must prove:

- recovery-safe root separation
- AIOS-FS object creation
- version creation
- pointer promotion
- semantic view resolution
- typed action execution
- policy decision
- approval where required
- verification
- evidence logging
- renderer output

Prototype acceptance criteria:

- `/` remains bootable without `/aios`
- `/aios` mounts as the AIOS-FS volume
- AIOS-FS creates a semantic object
- AIOS-FS creates a new object version
- AIOS-FS promotes or rolls back an object pointer
- a user submits one text goal
- the system creates an intent object
- the system creates a plan
- the system emits typed actions
- policy allows, denies, or requests approval
- approved actions execute through adapters
- verification result is recorded
- evidence receipt is written
- result is visible to the user

---

## 23. Open Questions

Open questions for later revisions:

- exact gRPC message definitions
- exact policy language implementation
- exact AIOS-FS on-disk format
- exact kernel/userspace split for AIOS-FS
- first supported Linux base target
- exact local default control model
- exact KDE renderer implementation boundary
- exact Web gateway authentication design
- exact compatibility runtime versions
- package repository trust model
- marketplace governance model

---

## 24. Final Principle

```text
One cognitive layer
Many execution environments
One unified interface
Persistent machine cognition
Typed actions
Policy before execution
Verification after execution
Evidence forever
```
