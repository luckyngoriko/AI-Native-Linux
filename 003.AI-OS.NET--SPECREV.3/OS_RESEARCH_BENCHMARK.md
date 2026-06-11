# AI-OS.NET — Operating Systems Research Benchmark

> **Sprint/Context:** OS-RESEARCH-001  
> **Status:** Draft v1  
> **Date:** 2026-06-11  
> **Sources:** Wikipedia, architecture documentation, published papers  
> **Purpose:** Comprehensive survey of exotic-to-massive operating systems, extracting architectural patterns, design decisions, security models, and real-time/multimedia principles applicable to AIOS kernel design.

---

## Table of Contents

1. [Introduction](#introduction)
2. [BeOS — The Media OS](#beos--the-media-os)
3. [Haiku — BeOS Reborn](#haiku--beos-reborn)
4. [L4 Microkernel Family](#l4-microkernel-family)
5. [seL4 — Formally Verified Microkernel](#sel4--formally-verified-microkernel)
6. [Plan 9 from Bell Labs](#plan-9-from-bell-labs)
7. [Inferno — Plan 9's Portable Descendant](#inferno--plan-9s-portable-descendant)
8. [Singularity & Midori — Managed Code OSes](#singularity--midori--managed-code-oses)
9. [QNX — Commercial Real-Time Microkernel](#qnx--commercial-real-time-microkernel)
10. [Exokernel — MIT's End-to-End Kernel](#exokernel--mits-end-to-end-kernel)
11. [Fuchsia — Google's Capability OS](#fuchsia--googles-capability-os)
12. [Genode — Recursive Sandboxing Framework](#genode--recursive-sandboxing-framework)
13. [Cross-Cutting Architecture Patterns](#cross-cutting-architecture-patterns)
14. [AIOS Applicability Matrix](#aios-applicability-matrix)
15. [References](#references)

---

## Introduction

This document collects detailed architectural analysis of 12 operating systems spanning the spectrum from exotic research systems to massive commercial deployments. Each system offers distinct lessons for AIOS:

- **BeOS/Haiku:** Real-time multimedia architecture, pervasive multithreading
- **L4/seL4:** Microkernel performance proofs, formal verification, capability security
- **Plan 9/Inferno:** Namespace virtualization, everything-is-a-file taken to its extreme, distributed design
- **Singularity/Midori:** Language-based isolation, SIPs, managed-code kernels
- **QNX:** Commercial microkernel RTOS, industry-proven IPC, millions of deployments
- **Exokernel:** End-to-end principle, library OSes, application control
- **Fuchsia:** Modern capability-based microkernel, Rust kernel components
- **Genode:** Recursive sandboxing, component-based security

---

## BeOS — The Media OS

### Overview

| Property | Value |
|---|---|
| **Developer** | Be Inc. (Jean-Louis Gassée) |
| **Kernel type** | Monolithic (hybrid characteristics) |
| **Language** | C++ (kernel + userspace) |
| **First release** | 1995 (DR8) |
| **Last release** | 2000 (BeOS R5) |
| **Target** | Desktop multimedia workstation |
| **Status** | Discontinued; legacy in radio/TV broadcast |
| **Fate** | Apple acquisition failed; IP sold to Palm (2001) |

### Kernel Architecture

BeOS used a **monolithic kernel** written in C++ with pervasive multithreading. Key characteristics:

- **Symmetric Multiprocessing (SMP):** Supported from day one — unusual for the mid-1990s.
- **Preemptive multitasking** with real-time scheduling priorities.
- **Protected memory** — each application ran in its own address space.
- **Pervasive multithreading:** Every BWindow had its own thread. The kernel was designed around fine-grained concurrency.
- **64-bit journaling file system:** BFS (Be File System), designed by Dominic Giampaolo, supported extended attributes (metadata as database), 64-bit addressing, and was optimized for multimedia workloads (large sequential reads/writes).

### The Media Architecture (AIOs Relevance: CRITICAL)

BeOS's defining feature was its **real-time multimedia pipeline**. This architecture is the single most important pattern for AIOS's real-time AI inference streaming.

#### Media Kit Components

```
┌─────────────────────────────────────────────────────┐
│                   APPLICATION                       │
├─────────────────────────────────────────────────────┤
│  BMediaRoster  ◄── Media Node Registry & Mediator   │
├────────┬──────────────┬─────────────────────────────┤
│ Source │   Filter     │   Output                    │
│  Node  │    Node      │    Node                     │
│  (mic) │  (DSP)       │  (speaker)                  │
├────────┴──────────────┴─────────────────────────────┤
│              BBufferGroup (shared buffers)           │
├─────────────────────────────────────────────────────┤
│            media_server (global state)               │
│         media_addon_server (codec loading)           │
├─────────────────────────────────────────────────────┤
│                  BeOS Kernel                         │
│         (real-time scheduling, SMP)                  │
└─────────────────────────────────────────────────────┘
```

#### Node Pipeline Architecture

Media processing in BeOS was a **directed graph of BMediaNode** instances connected through the BMediaRoster:

1. **Source Nodes** (producers):
   - Capture from hardware (audio input, video capture card)
   - File readers
   - Generate output buffers, pass to downstream nodes

2. **Filter Nodes** (processors):
   - Audio effects, video codecs, format converters
   - Receive buffers from upstream, produce transformed buffers
   - The BeOS DSP pipeline: real-time audio effects chaining

3. **Output Nodes** (consumers):
   - Hardware playback (sound card, video output)
   - File writers
   - Network streams

#### Buffer Management (BBufferGroup / BBuffer)

The **buffer model** is the key insight:

```
┌─────────────────────────────────────────────┐
│         BBufferGroup (buffer pool)           │
│                                              │
│  ┌──────┐ ┌──────┐ ┌──────┐ ┌──────┐       │
│  │ Buf0 │ │ Buf1 │ │ Buf2 │ │ Buf3 │  ...  │
│  └──────┘ └──────┘ └──────┘ └──────┘       │
│                                              │
│  Owner: Consumer Node                        │
│  Trading: BBufferConsumer::BufferReceived()  │
└─────────────────────────────────────────────┘
```

**Key design decisions:**

- **Buffers owned by consumers.** The downstream node "owns" the buffer. When it's done, it passes ownership back upstream or recycles it.
- **Buffer trading, not copying.** Nodes exchange buffer ownership via `BBufferConsumer::BufferReceived()`. Zero-copy by design.
- **Buffer groups pre-allocated.** No heap allocation in the media path — all buffers pre-allocated at connection time.
- **Recycle semantics:** `BBuffer::Recycle()` returns the buffer to its origin pool.

#### Real-Time Scheduling

BeOS had **120 priority levels** (compared to Linux's 140 at the time) and dedicated real-time priority band:

- **Time-critical threads** (priority 100-120) ran before anything else, including the window server.
- **Real-time audio threads** could preempt the GUI — no audio dropouts from UI operations.
- **BMediaNode::Run()** — the processing loop — ran at real-time priority.

#### BeOS Legacy

BeOS R5 achieved cult status in professional audio:

- **TuneTracker** radio automation system — still running in hundreds of radio stations.
- **TASCAM SX-1** digital audio workstation — BeOS embedded.
- **RADAR 24/V/6** multitrack recorders by iZ Technology.
- **Roland Edirol DV-7** video editor.
- Became the multimedia foundation of **Palm OS Cobalt** after Palm acquired Be Inc. IP.

### AIOS Lessons from BeOS

| Pattern | BeOS Implementation | AIOS Application |
|---|---|---|
| **Pipeline graph model** | BMediaNode DAG, BMediaRoster | AI inference pipeline: token source → transformer → embedding → action |
| **Consumer-owned buffers** | BBuffer owned by downstream, traded | Capsule message buffers owned by recipient capability |
| **Zero-copy trading** | Buffer ownership transfer, no memcpy | Shared memory regions with ownership exchange |
| **Real-time scheduling** | 120 priorities, RT band preempts GUI | Neural network inference at RT priority, GUI at lower |
| **Pre-allocated buffers** | BBufferGroup at connection time | Pre-allocated tensor buffers, no allocation in hot path |
| **Per-window threads** | Every BWindow = one thread | Every capsule has its own executor thread |
| **Extended attribute FS** | BFS metadata for media files | Capsule metadata in filesystem-level xattrs |
| **SMP from day one** | Designed for multi-CPU | Designed for heterogeneous compute (CPU + GPU + NPU) |

---

## Haiku — BeOS Reborn

### Overview

| Property | Value |
|---|---|
| **Developer** | Haiku Project (community) |
| **Kernel type** | Hybrid (fork of NewOS) |
| **Language** | C++ |
| **First release** | 2009 (Alpha 1) |
| **Latest release** | Beta 5 (September 2024) |
| **License** | MIT |
| **Status** | Active development |

### Kernel Architecture

Haiku's kernel is forked from **NewOS**, written by Travis Geiselbrecht (ex-Be engineer, later wrote LK — Little Kernel — which became Fuchsia's Zircon kernel).

Key kernel features:

- **Hybrid kernel** — not purely monolithic, not purely microkernel.
- **VFS layer** — virtual filesystem with pluggable backends.
- **SMP support** with fine-grained locking.
- **app_server** — userspace compositor handling window management (similar to macOS's WindowServer).
- **VESA fallback** for graphics — works without GPU drivers.

### Compatibility Layer

- **Binary-compatible with BeOS R5** applications.
- **POSIX compatible** — can run many Linux/Unix applications.
- **X11 translation layer** — X11 apps run through an Xlib compatibility layer.
- **Wayland translation layer** — under active development.
- **Wine ported** (2022) — Windows applications run on Haiku.

### Package Management

Haiku innovated in package management:

- **PackageFS** — packages are mounted filesystems, not extracted archives. Installing a package mounts it; uninstalling unmounts it.
- **HaikuDepot** — package manager using **libsolv** (same SAT solver as openSUSE's zypper).
- **HVIF (Haiku Vector Icon Format)** — native vector icon format, extremely compact (typically < 1KB).

### Security Hardening

Since Beta 1, Haiku has added modern security features:

- **ASLR** (Address Space Layout Randomization)
- **DEP** (Data Execution Prevention)
- **SMAP** (Supervisor Mode Access Prevention)

### Active Ports

- **RISC-V** — active porting effort.
- **ARM** — active porting effort.

### AIOS Lessons from Haiku

| Pattern | Haiku Implementation | AIOS Application |
|---|---|---|
| **Package-as-filesystem** | PackageFS mounts packages | Capsule bundles mount as filesystems (capability-as-file) |
| **SAT-based dependency resolution** | libsolv for packages | Capability dependency resolution via SAT solving |
| **Binary compatibility** | Run BeOS R5 binaries unchanged | Run legacy capsule binaries via ABI compatibility layer |
| **Multi-ABI support** | POSIX + X11 + Wayland + Wine | Multiple capability runtime environments |

---

## L4 Microkernel Family

### Overview

| Property | Value |
|---|---|
| **Designer** | Jochen Liedtke |
| **Type** | Second-generation microkernel |
| **Core principle** | Minimality — IPC must be fast enough to be the only mechanism |
| **First release** | 1993 (L4) |
| **Predecessor** | L3 (1988, used at TÜV for security evaluations) |
| **Key innovation** | Hand-coded assembly IPC — 20× faster than Mach |

### The Liedtke Insight

Jochen Liedtke's foundational paper proved that **microkernels CAN be fast** — the conventional wisdom that "microkernels are inherently slow" was merely an artifact of first-generation designs (Mach).

His insight: **the microkernel must fit in the L1 cache.**

L4's kernel was so minimal that its entire hot path (IPC) fit in the processor's L1 instruction cache. This eliminated the primary source of microkernel overhead — cache pollution from kernel code.

### Kernel Mechanisms (Only Three)

L4's kernel provides exactly **three mechanisms**:

1. **Address spaces** — protection domains, page table management.
2. **Threads and scheduling** — execution contexts, priority scheduling.
3. **IPC** (Inter-Process Communication) — the only way threads in different address spaces interact.

**Everything else is in userspace:**
- Device drivers
- File systems
- Network stacks
- Memory management policy (pagers)
- Process management

### IPC Performance

- **L4 (original):** ~250 cycles for synchronous IPC on x86 (hand-tuned assembly).
- **Mach (first-gen):** thousands of cycles for the same operation.
- **L4Ka::Pistachio:** Portable C++ reimplementation, BSD licensed.
- **L4/Fiasco** (TU Dresden): Fully preemptible kernel, real-time capable.

### Commercial Deployment

- **OKL4:** Qualcomm's implementation. Ships in over **1.5 billion** mobile modems (baseband processors).
- **Apple Secure Enclave:** Uses an L4 derivative (sepOS) for the secure enclave processor in iPhones.
- **NIO SkyOS:** Chinese EV manufacturer uses seL4 in their vehicle OS.

### Variant Family Tree

```
L3 (1988)
 └─ L4 (1993) — Jochen Liedtke, hand-coded x86
     ├─ L4Ka::Pistachio — portable C++, BSD
     ├─ L4/Fiasco (TU Dresden) — fully preemptible, RT
     ├─ OKL4 (Qualcomm) — 1.5B+ deployments
     ├─ seL4 — formally verified
     └─ NOVA — virtualization-focused microhypervisor
```

### AIOS Lessons from L4

| Pattern | L4 Implementation | AIOS Application |
|---|---|---|
| **Minimal kernel surface** | Only 3 mechanisms | AIOS kernel: only capability dispatch, memory, scheduling |
| **L1 cache residency** | Hot path fits in L1 cache | AIOS kernel hot path must fit in CPU cache |
| **Userspace drivers** | All drivers in userspace | AIOS capsule drivers in userspace |
| **IPC as foundation** | Everything via IPC | Everything via capability message passing |
| **Policy in userspace** | Kernel never makes policy decisions | Kernel never makes AI policy decisions |

---

## seL4 — Formally Verified Microkernel

### Overview

| Property | Value |
|---|---|
| **Developer** | Data61 (CSIRO) / seL4 Foundation |
| **Type** | Third-generation microkernel |
| **Language** | C (verified), some Rust components |
| **Verification** | Formal proof of functional correctness (2009) |
| **Security model** | Capability-based access control |
| **First proof** | 2009 (world's first verified general-purpose OS kernel) |
| **Foundation** | Under Linux Foundation (2020) |
| **License** | GPLv2 (kernel), BSD (userland) |

### The Verification Achievement

seL4 is the **first general-purpose OS kernel to be formally verified for functional correctness.** The verification chain includes:

1. **Abstract specification** — what the kernel should do (Haskell prototype).
2. **Executable specification** — refined to C implementation level.
3. **C implementation** — the actual kernel code.
4. **Binary verification** — proof that the compiled binary matches the C code (translation validation).
5. **Machine model** — proof that the binary is correct on the ARM/x86/RISC-V hardware model.

This is an **end-to-end** proof: abstract spec → executable spec → C code → binary → hardware.

### Proof Properties Established

| Property | Description | Status |
|---|---|---|
| **Functional correctness** | Implementation refines specification | ✅ Proved (2009) |
| **Integrity** | Capability write cannot happen without possession of write capability | ✅ Proved (2013) |
| **Confidentiality** | Information flow only through authorized channels | ✅ Proved (2013) |
| **Translation validation** | Compiled binary matches C source | ✅ Proved |
| **WCET analysis** | Worst-case execution time bounds for kernel operations | ✅ Proved (2016) |
| **Time protection** | Timing side-channel elimination | Active research |

### Capability Security Model

seL4's security model is **pure capability-based access control:**

- **Every kernel object is accessed via a capability** (thread control blocks, address spaces, IPC endpoints, notifications, memory frames).
- **Capabilities are unforgeable tokens** — you cannot create a capability without proper authority.
- **Capability derivation** — capabilities can be restricted (fewer rights) but never expanded.
- **No ambient authority** — no concept of "root" or "superuser." Everything is explicit.

```
┌─────────────────────────────────────────────────┐
│              Process A                           │
│  ┌─────────────────────────────────┐            │
│  │ CNode (capability table)        │            │
│  │ ┌───────┬───────┬───────┐       │            │
│  │ │ EP_C  │ VSpace│ TCB_A │       │            │
│  │ │(send) │ (rw)  │ (ctrl)│       │            │
│  │ └───────┴───────┴───────┘       │            │
│  └─────────────────────────────────┘            │
│                                                   │
│  seL4_Call(EP_C, message) ──────────────────►   │
└─────────────────────────────────────────────────┘
                        │
                        ▼
┌─────────────────────────────────────────────────┐
│              Process B                           │
│  ┌─────────────────────────────────┐            │
│  │ CNode                           │            │
│  │ ┌───────┬───────┬───────┐       │            │
│  │ │ EP_C  │ VSpace│ TCB_B │       │            │
│  │ │(recv) │ (rw)  │ (ctrl)│       │            │
│  │ └───────┴───────┴───────┘       │            │
│  └─────────────────────────────────┘            │
│  seL4_ReplyRecv(EP_C, reply)                   │
└─────────────────────────────────────────────────┘
```

### Kernel Resource Management

Not only inter-process communication, but **kernel resources themselves** are capability-controlled:

| Resource | Control Capability |
|---|---|
| Create new thread | Must possess **Untyped Memory** capability |
| Map memory page | Must possess **Page** capability |
| Send message | Must possess **Endpoint** capability with Send right |
| Receive message | Must possess **Endpoint** capability with Receive right |
| Create notification | Must possess **Untyped Memory** capability |
| Delete object | Must possess capability with full authority |

This means the **kernel's own memory consumption** is controlled by user-level capabilities — no kernel memory exhaustion without user complicity.

### DARPA HACMS Project

The High-Assurance Cyber Military Systems (HACMS) project used seL4 to build an unhackable drone:

- seL4 ran the flight control software of a **Sikorsky UH-60 Black Hawk** variant (unmanned).
- Red team attempted to compromise the system — they could not.
- Demonstrated that formal verification + capability security = resilience against cyber attacks.

### Economics of Formal Verification

| Metric | seL4 | Traditional High-Assurance |
|---|---|---|
| **Cost per line** | ~$400 | ~$1,000 |
| **Total verified lines** | ~10,000 (kernel) | Varies |
| **Defects found** | 460+ during verification | Varies |
| **Residual bug density** | 0 (by proof) | Unknown |

Despite the massive upfront cost of formal methods, seL4 proved **cheaper per verified line** than traditional high-assurance development because the proofs catch bugs before they become expensive field failures.

### AIOS Lessons from seL4

| Pattern | seL4 Implementation | AIOS Application |
|---|---|---|
| **Formal verification** | End-to-end proof chain | Formal verification of AIOS recovery invariant |
| **Capability security** | No ambient authority | Capsule capabilities as unforgeable tokens |
| **Kernel resource control** | Capabilities control kernel objects | Capsule memory and CPU budgets via capabilities |
| **Integrity + confidentiality** | Proved information flow properties | Proved information flow between AI models |
| **WCET analysis** | Bounded kernel execution times | Bounded inference latency guarantees |
| **No root user** | No ambient superuser | No "root capsule" — everything explicit |
| **Untyped Memory model** | Capabilities retype raw memory | Capsule heap from capability-controlled memory |

---

## Plan 9 from Bell Labs

### Overview

| Property | Value |
|---|---|
| **Designers** | Rob Pike, Ken Thompson, Dennis Ritchie |
| **Type** | Distributed operating system |
| **Core ideas** | Per-process namespaces + 9P protocol |
| **First release** | 1992 (internal), 1995 (public) |
| **Kernel size** | ~18,000 lines of code |
| **License** | MIT (since 2014) |
| **Status** | Active forks (9front, Harvey, Jehanne) |

### The Two Core Ideas

Plan 9 was designed around two mutually reinforcing concepts:

#### 1. Per-Process Namespaces

Every process in Plan 9 has its **own view of the filesystem**. This is not chroot — it's a deeply composable mechanism:

```
Process A's namespace           Process B's namespace
─────────────────────           ─────────────────────
/bin  → /arch/amd64/bin         /bin  → /arch/arm64/bin
/net  → /net.alt                /net  → /net
/proc → /proc                   /proc → /proc
/dev  → /dev                    /dev  → /dev
/home → remote!/usr/alice       /home → /usr/bob
```

- **Union directories** — multiple directories mounted at the same point, files from all visible. Plan 9 invented this (later adopted by BSD/Linux as union mounts).
- **Private namespaces** — a web server process can mount its own `/net/tcp`, isolating from other processes.
- **Namespace as security boundary** — what a process cannot see, it cannot access.

#### 2. 9P Protocol — Filesystem as Universal Interface

9P (Styx in Inferno) is a **message-oriented filesystem protocol**. Every resource — hardware, network, processes, GUI — is accessed through file operations:

| Operation | Purpose |
|---|---|
| `attach` | Connect to a file server |
| `walk` | Navigate the name hierarchy |
| `open` | Open a file for I/O |
| `read` | Read data (synchronous or streaming) |
| `write` | Write data |
| `clunk` | Release a file handle |
| `stat` | Get file metadata |
| `wstat` | Set file metadata |
| `remove` | Delete a file |
| `create` | Create a new file |

### Everything Is a File (Extended to Its Logical Conclusion)

Plan 9 extended Unix's "everything is a file" philosophy across the entire system:

| Resource | Filesystem Path | Operations |
|---|---|---|
| **Processes** | `/proc/PID/ctl`, `/proc/PID/mem`, `/proc/PID/note` | `echo stop > /proc/42/ctl` (stop a process) |
| **Network** | `/net/tcp/0/ctl`, `/net/tcp/0/data` | `cat /net/tcp/0/data` (read TCP stream) |
| **Window system** | `/dev/mouse`, `/dev/cons`, `/dev/bitblt` | Write to `/dev/bitblt` to draw on screen |
| **Authentication** | Factotum (`/mnt/factotum`) | Auth credentials as files |
| **Hardware** | `/dev/sdC0/raw` (raw disk) | Direct hardware access |
| **Filesystem** | Fossil (`/active/fs`) | Versioned snapshots |

### Key Components

#### Fossil File System

- **Snapshots:** Instant, low-cost filesystem snapshots (`/archive/YYYY/MMDD`).
- **Versioned history:** Every file has a complete modification history.
- **Venti archival storage:** Deduplicated, append-only block storage.
- **Ephemeral and permanent storage:** Fossil = cache + Venti = archive.

#### Factotum — Authentication Agent

```
┌──────────────────────────────────────────┐
│                FACTOTUM                   │
│  ┌──────────────────────────────────────┐│
│  │ "I need to authenticate to server X" ││
│  └──────────────────────────────────────┘│
│              ↕ 9P protocol                │
│  ┌──────────────────────────────────────┐│
│  │ Credential store                     ││
│  │   key: proto=rsa serv=authdom        ││
│  │   key: proto=p9sk1 dom=example.com   ││
│  └──────────────────────────────────────┘│
└──────────────────────────────────────────┘
```

- Single authentication agent for the entire system.
- Applications never see credentials — they ask factotum to authenticate.
- 9P interface: read/write files in `/mnt/factotum` to manage authentication.

#### Plumber — System-Wide Hyperlinks

The plumber routes messages between applications based on rules:

```
echo 'click http://example.com' | plumb
```

- **Action patterns:** Define what happens when a URL, file path, or email is "plumbed."
- **System-wide hyperlinks:** Any application can send a message; the plumber dispatches it to the right handler.
- **Precursor to modern intents/URI schemes** on mobile platforms.

### UTF-8 Invention

**UTF-8 was invented here.** Ken Thompson designed UTF-8 on a placemat in a New Jersey diner with Rob Pike in September 1992. It was first implemented in Plan 9 and later became the dominant encoding of the World Wide Web.

### Current Forks

| Fork | Focus |
|---|---|
| **9front** | Active community fork. WiFi, USB, audio, emulators (NES, Game Boy), modern hardware support. |
| **Harvey OS** | 64-bit, SMP, rewritten in Go. |
| **Jehanne OS** | Modern Plan 9 successor by a former Plan 9 developer. Focus on simplicity and correctness. |

### AIOS Lessons from Plan 9

| Pattern | Plan 9 Implementation | AIOS Application |
|---|---|---|
| **Per-process namespace** | Every process has its own filesystem view | Every capsule has its own capability namespace |
| **9P protocol** | Everything as a file operation | Capability operations as uniform protocol |
| **Union directories** | Multiple sources visible at one mount point | Capability composition via union of namespaces |
| **Factotum authentication** | Central auth agent, apps never see secrets | Capsule identity service, capsules never hold credentials |
| **Plumber** | System-wide message routing | Inter-capsule message routing |
| **Fossil snapshots** | Instant filesystem snapshots | Capsule state snapshots for recovery |
| **Venti deduplication** | Content-addressable archival storage | Deduplicated capability state storage |
| **Private namespaces** | Isolation without virtualization overhead | Capability sandboxing without VM overhead |

---

## Inferno — Plan 9's Portable Descendant

### Overview

| Property | Value |
|---|---|
| **Designers** | Rob Pike, Phil Winterbottom, Sean Dorward |
| **Type** | Distributed OS / Virtual OS |
| **Language** | Limbo (application), C (kernel) |
| **VM** | Dis virtual machine |
| **Protocol** | Styx (9P2000 equivalent) |
| **Memory** | Runs in 1 MiB |
| **First release** | 1997 |
| **License** | MIT (since 2021) |

### Architecture

Inferno can run in two modes:

1. **Native:** Runs directly on hardware (kernel + Dis VM).
2. **Hosted:** Runs as an application on Linux, Windows, Plan 9, macOS, Solaris. In hosted mode, Inferno provides its own namespace — it's a **virtual operating system**.

```
┌────────────────────────────────────────────────┐
│              Limbo Application                  │
├────────────────────────────────────────────────┤
│              Dis Virtual Machine                │
├────────────────────────────────────────────────┤
│        Inferno Kernel (hosted or native)        │
├────────────────────────────────────────────────┤
│  Host OS  │  Native Hardware  │  Plan 9 Kernel  │
└───────────┴───────────────────┴────────────────┘
```

### Limbo Language

Limbo is a **concurrent, modular programming language** for the Dis VM:

- **CSP-based concurrency** (Communicating Sequential Processes) — channels as the primary communication mechanism.
- **Garbage collection:** Hybrid reference counting + **real-time coloring collector** (marks live objects while mutators continue running).
- **Strong typing** with structural subtyping.
- **Modules** loaded dynamically at runtime.
- **Built-in types:** `chan of T` (typed channels), `adt` (algebraic data types), `list of T`, tuples.

```limbo
# Example: Concurrent echo server in Limbo
implement Echo;

include "sys.m";   sys: Sys;
include "draw.m";

Echo: module {
    init: fn(ctxt: ref Draw->Context, args: list of string);
};

init(ctxt: ref Draw->Context, args: list of string) {
    sys = load Sys Sys->PATH;
    
    # Create a channel
    ch := chan of int;
    
    # Spawn a goroutine-like process
    spawn worker(ch);
    
    # Send work
    ch <-= 42;
}

worker(ch: chan of int) {
    n := <-ch;  # Receive from channel
    sys->print("got %d\n", n);
}
```

### Dis Virtual Machine

The Dis VM is a **register-based virtual machine** designed for:

- **Just-in-time compilation** (JIT-capable architecture).
- **Memory-efficient** bytecode format.
- **Garbage collection** integrated at the VM level.
- **Sandboxing** — the VM enforces type safety and memory safety.

### Portability Model

Inferno's portability is unique — it's portable across **both processors and host environments:**

```
         ┌──────────────────────┐
         │   Same Limbo binary  │
         │   runs everywhere    │
         └──────────────────────┘
                    │
    ┌───────────────┼───────────────┐
    ▼               ▼               ▼
┌────────┐    ┌──────────┐    ┌────────┐
│ x86    │    │  ARM     │    │  MIPS  │  ← Processors
│ Linux  │    │ Plan 9   │    │ Windows│  ← Host environments
└────────┘    └──────────┘    └────────┘
```

### AIOS Lessons from Inferno

| Pattern | Inferno Implementation | AIOS Application |
|---|---|---|
| **Portable bytecode** | Dis VM runs same binary everywhere | Capsule bytecode runs on any AIOS node |
| **Hosted mode** | OS runs as application on host | AIOS runs as nested capability runtime |
| **Concurrent language** | Limbo with CSP channels | Capsule DSL with channel-based communication |
| **Real-time GC** | Coloring collector, no stop-the-world | Real-time memory reclamation for inference |
| **1 MiB footprint** | Tiny memory requirement | Tiny capsule runtime footprint |
| **Module system** | Dynamic module loading | Dynamic capability loading |

---

## Singularity & Midori — Managed Code OSes

### Singularity (Microsoft Research)

| Property | Value |
|---|---|
| **Developer** | Microsoft Research |
| **Type** | Microkernel, managed-code OS |
| **Language** | Sing# (extended C#), C (kernel), assembly |
| **Isolation** | Software-Isolated Processes (SIPs) |
| **Status** | Research project (ended 2015) |

#### Core Innovation: Software-Isolated Processes (SIPs)

Singularity's radical idea: **no hardware memory protection between processes.** Instead:

- **All code is managed** (Sing#, compiled to CIL → x86 via Bartok compiler).
- **Type safety + static verification** guarantees that processes cannot access each other's memory.
- **SIPs share the same address space** — context switches cost nearly nothing.

```
┌────────────────────────────────────────────────────┐
│          SINGLE ADDRESS SPACE                       │
│                                                     │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐          │
│  │  SIP A   │  │  SIP B   │  │  SIP C   │          │
│  │ (driver) │  │  (FS)    │  │  (TCP)   │          │
│  └──────────┘  └──────────┘  └──────────┘          │
│                                                     │
│  Isolation: Type safety + static verification       │
│  Communication: Message passing via channels         │
│  No hardware MMU context switch on SIP transitions  │
└────────────────────────────────────────────────────┘
```

**Benefits:**
- **No TLB flush** on SIP switch (huge performance win).
- **Zero-copy message passing** — both SIPs see the same physical page.
- **Invariants are compiler-verified** — if it compiles, it's memory-safe.

**Cost:**
- Requires a trusted compiler (Bartok + Sing#), GC, and runtime — larger TCB than hardware isolation.
- Cannot run arbitrary code — only verified managed code.

### Midori (Microsoft)

| Property | Value |
|---|---|
| **Developer** | Microsoft (incubation) |
| **Type** | Microkernel, capability-based |
| **Language** | M# (custom C# variant) |
| **Security** | Capability-based |
| **Concurrency** | Distributed across nodes |
| **Status** | Research (ended 2015) |

Midori was the spiritual successor to Singularity, incubated as a potential future Windows kernel:

- **Written in M#** — a custom variant of C# with first-class support for async, immutability, and capabilities.
- **Capability-based security** — similar to seL4, all access through explicit capabilities.
- **Programs run across multiple nodes** — distributed by default, not as an afterthought.
- **Sandboxed applications** — every program runs in an isolated sandbox with explicit capabilities.
- **Migration paths from Windows** — Microsoft mapped several strategies for migrating existing Windows applications to Midori.

Midori concepts influenced later Microsoft projects including aspects of Windows Subsystem for Linux, the Windows sandbox, and Azure Sphere.

### AIOS Lessons from Singularity/Midori

| Pattern | Singularity/Midori Implementation | AIOS Application |
|---|---|---|
| **SIPs** | Software isolation, shared address space | Capsule isolation via Rust's type system + compile-time verification |
| **Zero-cost context switch** | No MMU involvement | Capsule context switch = function call |
| **Capability security** | Explicit per-process capabilities | Explicit per-capsule capabilities |
| **Distributed-native** | Programs span multiple nodes | Capsules designed for distributed execution |
| **Managed code kernel** | Invariants verified at compile time | AIOS properties verified by Rust borrow checker |
| **Bartok compiler** | Trusted compilation chain | Rust compiler as trusted compilation chain |

---

## QNX — Commercial Real-Time Microkernel

### Overview

| Property | Value |
|---|---|
| **Developer** | QNX Software Systems (now BlackBerry) |
| **Kernel type** | Microkernel (RTOS) |
| **Kernel name** | `procnto` |
| **Kernel size** | ~44,000 lines of code |
| **IPC mechanism** | `MsgSend` / `MsgReceive` / `MsgReply` |
| **First release** | 1982 (QNX 2) |
| **Latest release** | QNX 8.0 (December 2023) |
| **Deployments** | 275+ million vehicles |
| **Certification** | ISO 26262 ASIL-D (automotive safety) |
| **License** | Proprietary |

### Kernel Architecture: procnto

QNX's microkernel, `procnto`, is astonishingly minimal:

```
┌─────────────────────────────────────────────────┐
│                 procnto                           │
│  (ONLY these in kernel space)                    │
├─────────────────────────────────────────────────┤
│  • CPU scheduling                                │
│  • IPC (MsgSend / MsgReceive / MsgReply)         │
│  • Interrupt redirection (to userspace handlers) │
│  • Timers                                        │
└─────────────────────────────────────────────────┘
```

**Memory management is in userspace** — the `proc` manager handles virtual memory, not the kernel.

### IPC + Scheduling Integration

QNX's key insight: **tightly coupling IPC and scheduling** enables real-time performance:

```
┌─────────────────────────────────────────────────┐
│  Process A (high priority)                       │
│  MsgSend(channel, msg)                          │
│    │                                              │
│    │  Kernel: A is higher priority than B         │
│    │  → DON'T schedule A → schedule B instead     │
│    │  → CPU transferred directly to B             │
│    ▼                                              │
│  Process B (lower priority, waiting on channel)   │
│  MsgReceive(channel, &msg)                       │
│    │  Process message...                          │
│    │  MsgReply(channel, reply)                   │
│    │                                              │
│    │  Kernel: Reply sent → A is highest ready     │
│    │  → Schedule A back                           │
│    ▼                                              │
│  Process A resumes                                │
└─────────────────────────────────────────────────┘
```

- **MsgSend is synchronous** — the sender blocks until the receiver replies.
- **Priority inheritance** — if a low-priority process holds a resource a high-priority process needs, the low-priority process inherits the high priority until it releases the resource (prevents priority inversion).
- **Adaptive partition scheduling** — guarantee CPU percentage to partitions regardless of load.

### No Kernel Drivers

QNX has **zero kernel-mode drivers.** Every driver is a userspace process:

```
┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐
│  Disk    │  │  Network │  │  Audio   │  │  USB     │
│  Driver  │  │  Driver  │  │  Driver  │  │  Driver  │
└──────────┘  └──────────┘  └──────────┘  └──────────┘
      │              │              │              │
      └──────────────┴──────────────┴──────────────┘
                         │
                    procnto (IPC + scheduling)
                         │
                    Hardware Interrupts
```

- **Drivers crash = process crash** — restart it without kernel panic.
- **Driver can be updated live** — no reboot needed.
- **Any language** — drivers in C, C++, Rust, any language.

### Boot Image

QNX's **image filesystem** can include user programs in the boot image:

```
┌─────────────────────────────────────────┐
│  Boot Image (.ifs)                       │
│  ┌─────────────────────────────────────┐│
│  │ procnto (kernel)                    ││
│  │ proc (memory manager)               ││
│  │ devc-ser (serial driver)            ││
│  │ io-pkt (network stack)              ││
│  │ Application code                    ││
│  └─────────────────────────────────────┘│
└─────────────────────────────────────────┘
```

Everything from boot to application is a **linear address space** — no context switches during boot.

### Certification and Deployment

| Metric | Value |
|---|---|
| **ISO 26262 certification** | ASIL-D (highest automotive safety level) |
| **Vehicles deployed** | 275+ million |
| **IEC 61508** | SIL 3 (industrial safety) |
| **IEC 62304** | Class C (medical devices) |
| **POSIX compatibility** | Full PSE 54 profile |

### AIOS Lessons from QNX

| Pattern | QNX Implementation | AIOS Application |
|---|---|---|
| **Minimal kernel** | Only CPU, IPC, interrupts, timers | AIOS kernel: only capability dispatch, memory, scheduling |
| **MsgSend CPU transfer** | Priority-based direct CPU handoff | High-priority capsule messages preempt lower |
| **Userspace drivers** | Zero kernel drivers | All capsule drivers in userspace |
| **Driver crash recovery** | Restart process, not kernel | Restart capsule, not entire AIOS |
| **Adaptive partitions** | Guaranteed CPU budget | Guaranteed inference latency budget |
| **Boot image** | Kernel + userspace in single linear image | Recovery image: kernel + essential capsules |
| **Priority inheritance** | Prevents priority inversion | Prevents capsule priority inversion |

---

## Exokernel — MIT's End-to-End Kernel

### Overview

| Property | Value |
|---|---|
| **Designers** | Dawson Engler, M. Frans Kaashoek (MIT) |
| **Type** | Exokernel |
| **Core principle** | End-to-end — kernel provides protection + multiplexing, nothing else |
| **Applications** | Library operating systems (libOSes) |
| **Key insight** | Applications know best how to use resources |
| **Status** | Research concept, no commercial deployment |

### Architecture

An exokernel provides exactly **two functions:**

1. **Protection:** Ensure that no application can access another's resources.
2. **Multiplexing:** Allow multiple applications to share hardware safely.

**Everything else is in library operating systems (libOSes):**

```
┌────────────────────┐  ┌────────────────────┐  ┌────────────────────┐
│   Web Server App   │  │  Database App      │  │  Game App          │
│   (linked with     │  │  (linked with      │  │  (linked with      │
│    libOS-Web)      │  │   libOS-DB)        │  │   libOS-Game)      │
└────────────────────┘  └────────────────────┘  └────────────────────┘
                        │
        ┌───────────────┼───────────────┐
        ▼               ▼               ▼
┌─────────────────────────────────────────────────────┐
│                 EXOKERNEL                           │
│  • Secure bindings (protection)                     │
│  • Visible resource revocation (multiplexing)        │
│  • Expose hardware directly to applications          │
└─────────────────────────────────────────────────────┘
                        │
        ┌───────────────┼───────────────┐
        ▼               ▼               ▼
┌──────────┐    ┌──────────┐    ┌──────────┐
│   CPU    │    │  Memory  │    │   Disk   │
└──────────┘    └──────────┘    └──────────┘
```

### Application Control

Applications using libOSes have **direct control** over:

| Resource | What App Controls |
|---|---|
| **CPU** | Processor timeline — own scheduling policy, own quantum allocation |
| **Memory** | Physical page frames by number — own page replacement algorithm |
| **Disk** | Block access by physical address — own filesystem layout |
| **Network** | Programmable packet filter (DPF) — own protocol stack decisions |

### Cheetah Web Server

The canonical exokernel success story:

- **Cheetah HTTP server** implemented directly on the exokernel (no Unix underneath).
- Could achieve **8× throughput** of the same server on Unix for static file serving.
- Why? Because Cheetah knew its workload (mostly sequential reads of small files) and could optimize disk scheduling + buffer management for that specific pattern.

### Secure Bindings

The mechanism that makes exokernels safe:

- **Secure bindings** are hardware-protected resource access points.
- Example: a **secure binding to a disk block range** means the app can only read/write those specific blocks.
- The kernel verifies the binding but doesn't interpret it — the app interprets.

### AIOS Lessons from Exokernel

| Pattern | Exokernel Implementation | AIOS Application |
|---|---|---|
| **End-to-end principle** | Kernel only protects + multiplexes | AIOS kernel: only capability dispatch + memory isolation |
| **Library OSes** | App-specific operating systems | Capsule-specific runtime libraries |
| **Direct hardware access** | Apps control page tables, disk blocks | Capsules control tensor allocation, memory layout |
| **Secure bindings** | Hardware-enforced resource access | Capability tokens as secure bindings |
| **Visible revocation** | Apps see when resources are revoked | Capsules see when capabilities are revoked |
| **No abstraction in kernel** | Kernel never interprets resources | Kernel never interprets AI model structure |

---

## Fuchsia — Google's Capability OS

### Overview

| Property | Value |
|---|---|
| **Developer** | Google |
| **Kernel** | Zircon (microkernel) |
| **Type** | Capability-based operating system |
| **Languages** | Rust, C++, C, Dart, Go |
| **UI** | Flutter (Dart) |
| **Renderer** | Escher (Vulkan-based) |
| **First deployment** | Nest Hub (2021) |
| **License** | BSD, MIT, Apache 2.0 |
| **Status** | Active development, deployed to millions of devices |

### Zircon Kernel

Zircon is derived from **LK (Little Kernel)** — written by **Travis Geiselbrecht**, the same engineer who wrote NewOS (which became Haiku's kernel).

```
┌────────────────────────────────────────────────┐
│              Zircon Kernel                      │
├────────────────────────────────────────────────┤
│  Object-capability security model               │
│  Resource-as-object (not resource-as-file)      │
│  Mostly non-blocking syscalls                   │
│  Userspace drivers                              │
│  Written in C++ (with Rust components)          │
│  ~100 syscalls                                  │
└────────────────────────────────────────────────┘
```

Key Zircon properties:

- **Object-capability model:** Resources (processes, threads, memory, IPC channels, interrupts) are kernel objects accessed via handles (capabilities).
- **Handles have rights:** `ZX_RIGHT_READ`, `ZX_RIGHT_WRITE`, `ZX_RIGHT_EXECUTE`, `ZX_RIGHT_TRANSFER`, etc.
- **Resources as objects, not files:** Unlike Plan 9's "everything is a file," Fuchsia uses "everything is an object" — more natural for capability systems.
- **Most syscalls are non-blocking** — designed for asynchronous operation.

### Capability-Based Architecture

```
┌───────────────────────────────────────────────────┐
│  Process A                                        │
│  ┌─────────────────────────────────────────────┐  │
│  │ Handle Table (capabilities)                 │  │
│  │ ┌──────────┬──────────┬──────────┐         │  │
│  │ │ Channel  │  VMO     │  Job     │         │  │
│  │ │ (W,R)    │  (R)     │  (ctrl)  │         │  │
│  │ └──────────┴──────────┴──────────┘         │  │
│  └─────────────────────────────────────────────┘  │
│                                                    │
│  zx_channel_write(channel, data) ─────────────►  │
└───────────────────────────────────────────────────┘
                         │
                         ▼
┌───────────────────────────────────────────────────┐
│  Process B                                        │
│  ┌─────────────────────────────────────────────┐  │
│  │ Handle Table                                 │  │
│  │ ┌──────────┬──────────┬──────────┐         │  │
│  │ │ Channel  │  VMO     │  Thread  │         │  │
│  │ │ (R)      │  (W,R)   │  (ctrl)  │         │  │
│  │ └──────────┴──────────┴──────────┘         │  │
│  └─────────────────────────────────────────────┘  │
│  zx_channel_read(channel, &data)                  │
└───────────────────────────────────────────────────┘
```

### Component Architecture

Fuchsia's userland is built from **components** — the fundamental unit of software:

```
┌────────────────────────────────────────────────┐
│           Component Framework                   │
├────────────────────────────────────────────────┤
│  Capability routing between components          │
│  Components declare capabilities they use        │
│  Components declare capabilities they expose     │
│  Component manager enforces access control       │
│  Realm = isolated subtree of components          │
└────────────────────────────────────────────────┘
```

**Component manifest example:**

```json5
{
  "program": {
    "runner": "elf",
    "binary": "bin/my_app"
  },
  "use": [
    { "protocol": "fuchsia.net.http.Loader" },    // Capability I need
    { "directory": "config", "rights": ["r*"] }    // Directory I need
  ],
  "expose": [
    { "protocol": "fuchsia.my.Service" }           // Capability I provide
  ]
}
```

### Starnix — Linux Compatibility

Fuchsia runs Linux binaries through **Starnix**, a Linux ABI compatibility layer:

- Implements the Linux kernel interface in userspace.
- Linux programs run unmodified.
- NOT a VM — Starnix translates Linux syscalls to Fuchsia/Zircon equivalents.
- Similar to WSL1 (not WSL2 which is a VM).

### AIOS Lessons from Fuchsia

| Pattern | Fuchsia Implementation | AIOS Application |
|---|---|---|
| **Object-capability model** | Handles with rights | Capsule capabilities with access rights |
| **Component framework** | Declarative capability routing | Declarative capsule capability manifest |
| **Non-blocking syscalls** | Async-first kernel API | Async-first capsule message passing |
| **Starnix** | Linux ABI in userspace | Legacy application ABI in userspace capsule |
| **Realm isolation** | Isolated component subtrees | Capsule realm = isolated capability subtree |
| **Flutter UI** | Cross-platform UI framework | Capsule visualization via declarative UI |
| **Userspace drivers** | Zircon driver framework | Capsule drivers |

---

## Genode — Recursive Sandboxing Framework

### Overview

| Property | Value |
|---|---|
| **Developer** | Genode Labs (Dresden, Germany) |
| **Type** | OS framework (not a monolithic OS) |
| **Core idea** | Recursive sandboxing + explicit capabilities |
| **Kernels supported** | NOVA, seL4, Fiasco.OC, Linux, OKL4, Pistachio, hw kernel |
| **First release** | 2008 |
| **License** | AGPLv3 |
| **Status** | Active development, quarterly releases |

### Architecture: Component-Based with Recursive Sandboxing

Genode is not a kernel — it's a **framework for building operating systems from components.** It provides:

1. **Component model** — programs as isolated components.
2. **Capability-based security** — every resource access through explicit capabilities.
3. **Recursive sandboxing** — a parent component creates a child and controls what the child can access.

```
┌──────────────────────────────────────────────────────┐
│              Parent Component                         │
│  ┌────────────────────────────────────────────────┐  │
│  │               Child Sandbox                     │  │
│  │  ┌──────────────────────────────────────────┐  │  │
│  │  │            Grandchild Sandbox             │  │  │
│  │  │  ┌────────────────────────────────────┐  │  │  │
│  │  │  │        Great-Grandchild Sandbox     │  │  │  │
│  │  │  │  capabilities: {file_r, net_send}  │  │  │  │
│  │  │  └────────────────────────────────────┘  │  │  │
│  │  └──────────────────────────────────────────┘  │  │
│  └────────────────────────────────────────────────┘  │
│   capabilities: {block_dev, gpu, ...}                │
└──────────────────────────────────────────────────────┘
```

**Key principle: A parent can only grant its child a subset of its own capabilities.**

### Kernel Flexibility

Genode can run on top of multiple kernels:

| Kernel | When to Use |
|---|---|
| **seL4** | Highest security assurance, formal verification |
| **NOVA** | Virtualization-focused, hypervisor capabilities |
| **Fiasco.OC** | Real-time requirements |
| **Linux** | Development and testing |
| **hw kernel** | Bare-metal for minimal TCB |
| **OKL4** | ARM platforms |
| **Pistachio** | Legacy L4 compatibility |

### Sculpt — Desktop Distribution

**Sculpt OS** is Genode's pre-built general-purpose desktop distribution:

- Runs on standard PC hardware.
- Browser-based component management.
- Dynamic loading/unloading of components at runtime.
- File system as a component.
- GUI compositor as a component.

### AIOS Lessons from Genode

| Pattern | Genode Implementation | AIOS Application |
|---|---|---|
| **Recursive sandboxing** | Parents control children's capabilities | Capsule spawns sub-capsules with restricted rights |
| **Component isolation** | Every program in a dedicated sandbox | Every AI model in a dedicated capsule |
| **Explicit capabilities** | No ambient authority | No ambient authority in capsule system |
| **Multi-kernel support** | Same framework on seL4, Linux, NOVA | AIOS on multiple underlying isolation mechanisms |
| **Parent controls child** | Capability subset | Capsule spawns child with capability subset |
| **VirtualBox integration** | Run unmodified OS as component | Run legacy ML frameworks as capsule |

---

## Cross-Cutting Architecture Patterns

After analyzing all 12 operating systems, several universal patterns emerge:

### 1. The Capability Curve

```
Traditional OS    Microkernel    Capability µK    Verified µK
(Monolithic)        (Mach)         (seL4)         (seL4+proof)
    │                  │               │               │
    ▼                  ▼               ▼               ▼
┌─────────┐      ┌─────────┐     ┌─────────┐     ┌─────────┐
│ All     │      │ IPC     │     │ IPC     │     │ IPC     │
│ drivers │      │ sched   │     │ sched   │     │ sched   │
│ FS      │      │         │     │ caps    │     │ caps    │
│ network │  →   │ usersp  │  →  │ usersp  │  →  │ usersp  │
│ sched   │      │ drivers │     │ drivers │     │ drivers │
│ memory  │      │ FS      │     │ FS      │     │ FS      │
│ IPC     │      │ network │     │ network │     │ network │
│ ...     │      │ ...     │     │ ...     │     │ + proof │
└─────────┘      └─────────┘     └─────────┘     └─────────┘
    ↓                 ↓               ↓               ↓
  Fastest           Slower          Safe           Provably
  (no IPC)        (IPC cost)     (cap check)     Safe
```

The trend is inexorably toward smaller kernels with explicit capabilities — because **security and correctness matter more than raw throughput** in an AI-connected world.

### 2. Namespace / Capability Space as Security Boundary

Plan 9, Inferno, Fuchsia, Genode, and seL4 all converge on the same idea:

> **Security = what you can name**

- Plan 9: per-process namespace → can't see what you can't mount.
- seL4: capability table → can't access what you don't hold a capability to.
- Fuchsia: handle table with rights → can't use what you don't have a handle for.
- Genode: recursive sandboxing → can't access what parent didn't grant.

**AIOS pattern:** Every capsule has a CapabilityNamespace — a set of named, unforgeable capability tokens. Security is enforced by the kernel never granting access without a valid capability.

### 3. IPC as Universal Mechanism

From L4 to QNX to seL4 to Genode, the pattern is consistent:

> **IPC is the only cross-domain communication mechanism.**

- L4: threads communicate only via IPC.
- QNX: `MsgSend`/`MsgReceive`/`MsgReply` is the backbone.
- seL4: `seL4_Call` / `seL4_ReplyRecv` are the only cross-address-space operations.
- Fuchsia: channels are the primary IPC primitive.

**AIOS pattern:** Capsule-to-capsule communication exclusively via capability message passing. No shared state, no global variables, no memory sharing without capability exchange.

### 4. Real-Time Priority Inversion Prevention

Both BeOS and QNX solved priority inversion in real systems:

- **BeOS:** 120 priority levels with real-time band. RT threads preempt everything.
- **QNX:** Priority inheritance in MsgSend — if high-priority A sends to low-priority B, B inherits A's priority until it replies.

**AIOS pattern:** Capsule message passing with priority inheritance. Inference capsules at RT priority; UI capsules at normal priority. When UI capsule requests from inference capsule, inference inherits UI's priority for that request (or not — policy choice).

### 5. Userspace Drivers / Filesystems

QNX, L4, seL4, Fuchsia, Genode — all push drivers to userspace:

| System | Driver Location | Consequence |
|---|---|---|
| QNX | Userspace processes | Crash → restart; live update |
| L4 | Userspace servers | No kernel bloat |
| seL4 | Userspace processes | No kernel attack surface |
| Fuchsia | Userspace drivers | Sandboxed, capability-controlled |
| Genode | Components | Recursive sandboxing |

Only BeOS and Plan 9 kept drivers in-kernel (monolithic/hybrid approach).

**AIOS pattern:** All capsule frameworks run in userspace. The kernel is a capability dispatcher only. Drivers are capsules with hardware access capabilities.

### 6. Formal Methods at Scale

seL4 proved that formal verification of an OS kernel is achievable:

| Aspect | Cost | Benefit |
|---|---|---|
| Specification writing | High | Eliminates ambiguity |
| Proof development | High | Catches design bugs early |
| Proof maintenance | Medium | Survives refactoring |
| Bug prevention | $400/LOC vs $1000/LOC | Cheaper than field failures |
| Security assurance | Proof of integrity + confidentiality | DARPA couldn't hack it |

**AIOS pattern:** Formal verification of the AIOS recovery invariant (the core guarantee: "a failed capsule cannot contaminate other capsules"). Start with the recovery boundary proof, extend to capability integrity.

---

## AIOS Applicability Matrix

| OS | Capability Model | IPC Efficiency | Real-Time | Formal Verification | Multimedia Pipeline | Adoption Scale | AIOS Priority |
|---|---|---|---|---|---|---|---|
| **BeOS** | N/A (monolithic) | Thread-local (fast) | ✅ (120 levels) | ❌ | ✅✅✅ (Media Kit) | Millions (embedded) | CRITICAL |
| **Haiku** | N/A (hybrid) | Thread-local (fast) | Partial | ❌ | ✅✅ (BeOS compat) | Thousands (desktop) | HIGH |
| **L4 Family** | N/A (IPC-based) | ✅✅✅ (250 cycles) | ✅ (Fiasco) | ❌ | ❌ | Billions (OKL4) | HIGH |
| **seL4** | ✅✅✅ (capabilities) | ✅✅ (verified IPC) | ✅ (WCET) | ✅✅✅ (full proof) | ❌ | Millions (drones, cars) | CRITICAL |
| **Plan 9** | Namespace-based | 9P protocol | ❌ | ❌ | ❌ | Thousands (9front) | HIGH |
| **Inferno** | Namespace-based | Styx protocol | ❌ | ❌ | ❌ | Unknown | MEDIUM |
| **Singularity** | SIP isolation | Zero-copy (shared AS) | ❌ | Static verification | ❌ | Research only | MEDIUM |
| **Midori** | ✅✅ (capabilities) | N/A (research) | ❌ | Partial | ❌ | Research only | HIGH |
| **QNX** | N/A (process-based) | ✅✅✅ (MsgSend) | ✅✅✅ (ASIL-D) | ❌ | ✅ (media stack) | 275M+ vehicles | CRITICAL |
| **Exokernel** | Secure bindings | Bare metal | Possible | ❌ | ❌ | Research only | MEDIUM |
| **Fuchsia** | ✅✅ (object-capability) | ✅ (channels) | Partial | ❌ | ✅ (Flutter) | Millions (Nest) | HIGH |
| **Genode** | ✅✅✅ (recursive) | Kernel-dependent | Kernel-dependent | ✅ (on seL4) | ❌ | Unknown | HIGH |

### AIOS Priority Rankings

1. **CRITICAL (3 systems):**
   - **BeOS:** Multimedia pipeline architecture (node DAG, consumer-owned buffers, real-time scheduling)
   - **seL4:** Formal verification, capability security, kernel resource management
   - **QNX:** Commercial microkernel, IPC-scheduling integration, userspace drivers, 275M+ proof of viability

2. **HIGH (5 systems):**
   - **Haiku:** BeOS compatibility, PackageFS, SAT-based dependency resolution
   - **L4 Family:** Performance proof, 1.5B+ deployments, the Liedtke insight
   - **Plan 9:** Per-process namespaces, 9P, union directories, factotum, plumber
   - **Midori:** Managed-code kernel, capability security, distributed-native design
   - **Fuchsia:** Object-capability model, component framework, Starnix, Rust kernel components
   - **Genode:** Recursive sandboxing, multi-kernel support, parent-child capability control

3. **MEDIUM (3 systems):**
   - **Inferno:** Portable bytecode, Dis VM, Limbo concurrency, 1 MiB footprint
   - **Singularity:** SIPs, software isolation, zero-cost context switches
   - **Exokernel:** End-to-end principle, secure bindings, library OSes

---

## References

### Primary Sources

- Be Inc. *The Be Book* (BeOS API documentation)
- Jochen Liedtke. "On µ-Kernel Construction." *Proceedings of the 15th ACM Symposium on Operating Systems Principles*, 1995.
- Gerwin Klein et al. "seL4: Formal Verification of an OS Kernel." *Communications of the ACM*, 2010.
- Rob Pike, Dave Presotto, Sean Dorward, Bob Flandrena, Ken Thompson, Howard Trickey, Phil Winterbottom. "Plan 9 from Bell Labs." *Computing Systems*, 1995.
- Dawson Engler, M. Frans Kaashoek, James O'Toole Jr. "Exokernel: An Operating System Architecture for Application-Level Resource Management." *SOSP 1995*.
- Galen Hunt, James Larus. "Singularity: Rethinking the Software Stack." *Microsoft Research*, 2007.
- Norman Feske. "Genode OS Framework: A Component-Based Approach to Building Secure Systems." *Genode Labs*, 2008–present.

### Web Resources

- Haiku Project. https://www.haiku-os.org/
- seL4 Foundation. https://sel4.systems/
- 9front. https://9front.org/
- Fuchsia. https://fuchsia.dev/
- QNX. https://blackberry.qnx.com/
- Genode Labs. https://genode.org/
- Inferno (Vita Nuova). http://www.vitanuova.com/inferno/

---

## Document History

| Version | Date | Author | Changes |
|---|---|---|---|
| v1 | 2026-06-11 | AIOS Research Agent | Initial comprehensive survey: 12 operating systems, cross-cutting patterns, applicability matrix |

---

*End of OS Research Benchmark*
