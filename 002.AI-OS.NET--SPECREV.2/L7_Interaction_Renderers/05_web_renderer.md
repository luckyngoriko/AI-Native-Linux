# Web Renderer (Rev.2)

| Field          | Value                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| -------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Status         | `CONTRACT` (initial; written 2026-05-10)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| Phase tag      | S7.5                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| Layer          | L7 Interaction Renderers                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| Schema package | `aios.renderer.web.v1alpha1`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| Consumes       | **Imports vocabulary from**: S5.1 identity (recovery-mode flag — type-level), S8.2 GPU Resource Model (per-origin `GPUAdapter` shape, group-bound origin scheme — type-level shape co-defined with L8), L0 INV-006 (web localhost default — closed-id reference), INV-019..INV-022 (closed-id reference). **Peer (intra-L7)**: S7.1 Surface + Composition, S7.2 Shared UI Schema, S7.3 Visual Language. **Note (architectural)**: S8.2 is a higher-numbered-layer reference at L8; vocabulary import only — could be relocated to a cross-cutting GPU-capability contract in W12+. |
| Produces       | typed Web renderer service contract; DOM + Web Components compilation rules from S7.2 NodeKinds; WebGPU canvas hosting for surface nodes via `wgpu` compiled to wasm; visual token mapping per S7.3 §8.2; recovery page rendering rules; localhost-default exposure policy with explicit LAN/public escalation FSM                                                                                                                                                                                                                                                                 |

## 1. Purpose

This contract fixes how an AIOS UI Schema tree (S7.2) referencing AIOS Surfaces (S7.1) and bound to a Visual Theme (S7.3) is rendered on a web browser. The Web renderer is one of several L7 renderers (alongside KDE in S7.4 and the deferred CLI / Voice / Mobile renderers); none owns authoritative state, all read from the same upstream services.

This is the **operator surface for AIOS over a browser**. It is not a general-purpose web app framework. It binds three structural commitments:

1. **Localhost by default (INV-006).** The renderer service binds only to `127.0.0.1` and `::1` unless an explicit, evidence-backed exposure escalation has occurred.
2. **Hybrid DOM + WebGPU compilation.** Structural NodeKinds compile to DOM and Web Components; GPU-bearing NodeKinds (`VISUALIZATION`, `STREAM`, `SURFACE_EMBED`) compile to `<canvas>` + WebGPU contexts driven by the same `wgpu` crate that powers the KDE renderer (Rust source compiled to `wasm32-unknown-unknown` for the web target).
3. **Per-origin GPU sandboxing (S8.2 §I4).** Each active group's `APP_SURFACE` runs inside a per-group iframe whose origin is `https://<group_id>.aios.localhost:<port>`, so the browser's own same-origin policy delivers a distinct `GPUAdapter` per group; AIOS verifies origin equals `ScopeBinding.group_id` at composition.

This spec is a **binding contract**, not an implementation plan. It says "when AIOS UI is rendered on Web, this is exactly what compiles to what." No source code, no build steps, no timeline.

## 2. Core invariants

- **I1 — Renderer does not own authoritative state.** The Web renderer reads schema trees, surface records, theme bundles, and S2.1 view subscriptions from upstream services. It does not write back; user actions submitted from `FORM` / `ACTION_BUTTON` go through the Capability Runtime (S0.1 → S2.3), never directly mutate AIOS-FS.
- **I2 — Trust-bearing UI lives in a closed shadow root at z-index 9999.** AIOS chrome (`SECURITY_INDICATOR`, `APPROVAL_PROMPT`, `EVIDENCE_LINK`, recovery banner) is rendered into a closed shadow DOM whose host element has `position: fixed` and z-index 9999. Page-level Fullscreen API requests cannot hide this shadow root. Apps with JavaScript access to the page DOM cannot reach into the closed shadow root. Binds INV-020.
- **I3 — Localhost by default per INV-006.** The renderer's HTTP/HTTPS listener binds to `127.0.0.1:<port>` and `[::1]:<port>` only. LAN exposure requires policy approval + `WEB_EXPOSURE_GRANTED` `FOREVER` evidence + `WEB_LAN_EXPOSURE_ACTIVE` `STANDARD_24M` heartbeat while exposure is active. Public exposure additionally requires recovery-mode authorization.
- **I4 — Per-origin `GPUAdapter` per S8.2.** Every `APP_SURFACE` is hosted in an `<iframe>` whose origin is `https://<group_id>.aios.localhost:<port>`. The renderer rejects compositions where iframe origin's `<group_id>` token does not match the surface's `ScopeBinding.group_id`.
- **I5 — Page-level fullscreen cannot hide chrome.** The renderer rejects requests for the page document itself to enter fullscreen; only individual `<canvas>` elements inside CONTENT-zone surfaces may enter element-fullscreen, and the chrome shadow root remains positioned above them via the browser's fullscreen-ancestor stacking rules. Binds S7.1 §6.4.
- **I6 — Constitutional icons load only from the AIOS root-signed asset bundle.** Icon SVGs reference `/aios/system/themes/<theme_id>/icons/sprite.svg`. The CSP `img-src` and `default-src` directives forbid icon loads from any other origin. Binds S7.3 §11.2 against substitution.
- **I7 — App-controlled JavaScript cannot reach the shadow root.** Chrome shadow roots are constructed as **closed** (`mode: "closed"`); the `shadowRoot` accessor returns `null` for all but the renderer's own code. Page DOM cannot replace, restyle, or read trust-bearing chrome.
- **I8 — No client-side authoritative state.** IndexedDB and Service Worker caches hold only **ephemeral derived data** (last-known view snapshots for offline indication, theme assets, schema-tree fragments under explicit retention bound). Authoritative state is always re-fetched from AIOS-FS via gRPC-Web at session resume. The recovery page disables Service Worker entirely.
- **I9 — HTTPS mandatory.** Loopback uses a self-signed cert generated at AIOS install with SAN `localhost`, `127.0.0.1`, `::1`, and `*.aios.localhost`. LAN exposure requires a policy-approved cert (Let's Encrypt or CA-issued). Plain HTTP is never served; HTTP requests on the bound port return `421 Misdirected Request` with an HSTS `upgrade-insecure-requests` hint.
- **I10 — Renderer-signed DOM construction path.** Every node attached to AIOS chrome zones is appended via the renderer's signed construction path; a tree-signing service (S7.2 §I4) hash is verified before any subtree is rendered. DOM mutations injected by extensions or the page outside this path are detected by a periodic chrome-zone integrity check and trigger `WEB_EXTENSION_INTERFERENCE` evidence.
- **I11 — Recovery page is a different origin.** Recovery-mode rendering is served from `https://recovery.localhost:<port>`; it shares no origin, no Service Worker scope, no IndexedDB partition with the normal-mode origin `https://aios.localhost:<port>`. Browser same-origin policy enforces the boundary.

## 3. Schema NodeKind compilation table

The Web renderer compiles every closed `NodeKind` from S7.2 §3 to a fixed mapping. Renderers do not invent compilations; the table is authoritative.

| NodeKind             | Web compilation                                                                                                                                                                                                                                                                                                                                              | Notes                                                                           |
| -------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------- |
| `CONTAINER`          | `<div>` with CSS `display: flex` (STACK\_\*), `flex-wrap: wrap` (FLOW_WRAP), `display: grid` (GRID), `overflow: auto` (SCROLL\_\*)                                                                                                                                                                                                                           | LayoutHints map directly to CSS variables                                       |
| `DIVIDER`            | `<hr>` with `aria-orientation` per layout                                                                                                                                                                                                                                                                                                                    | Spacing token applied as margin                                                 |
| `SPACER`             | `<div>` sized via SPACING token; `flex: 1` when SpacerPayload.flexible                                                                                                                                                                                                                                                                                       | Inert; `aria-hidden="true"`                                                     |
| `TEXT`               | `<span>` (inline) or `<p>` (block by accessibility role); applies COLOR_TEXT_PRIMARY by default; emphasis → `<em>`, strong → `<strong>`                                                                                                                                                                                                                      | Honors AccessibilityRole                                                        |
| `HEADING`            | `<h1>` ... `<h6>` per HeadingPayload.level                                                                                                                                                                                                                                                                                                                   | Typography token TYPOGRAPHY_HEADING\_\* per level                               |
| `INLINE_CODE`        | `<code>` with TYPOGRAPHY_CODE_SM                                                                                                                                                                                                                                                                                                                             | No syntax highlighting (inline)                                                 |
| `CODE_BLOCK`         | `<pre><code>` with TYPOGRAPHY_CODE_MD; optional Shiki/Prism highlighting bound to language_hint                                                                                                                                                                                                                                                              | Highlighter runs in a Web Worker so syntax pass cannot block main thread        |
| `CARD`               | `<aios-card>` Web Component composing slotted header / body / actions                                                                                                                                                                                                                                                                                        | Uses CSS Container Queries for responsive layout                                |
| `LIST`               | `<ul>` (unordered) or `<ol>` (ordered); for view-bound lists, virtual-scroller (e.g., custom element backed by `IntersectionObserver` + windowing); subscribes to S2.1 view via gRPC-Web                                                                                                                                                                     | First page materialized; further pages paged via cursor                         |
| `TABLE`              | `<aios-table>` Web Component wrapping `<table>` with virtualized rows; sortable headers via `aria-sort`; filterable headers expose `<input>` per column                                                                                                                                                                                                      | View-bound tables subscribe to S2.1 view; row reuse preserves `node_id`         |
| `FORM`               | `<form>` with field-kind-specific elements: FIELD_TEXT → `<input type="text">`, FIELD_TEXT_MULTILINE → `<textarea>`, FIELD_NUMBER → `<input type="number">`, FIELD_BOOLEAN → `<input type="checkbox">`, FIELD_ENUM → `<select>` or `<input type="radio">` group, FIELD_OBJECT_REF → `<aios-object-picker>` Web Component, FIELD_DATE → `<input type="date">` | Submit handler constructs S0.1 envelope and sends via gRPC-Web                  |
| `ACTION_BUTTON`      | `<button>` wrapped in `<aios-action-button>` carrying action_template_ref; destructive class applies COLOR_SEMANTIC_DANGER + MOTION_DURATION_DELIBERATE confirm gesture                                                                                                                                                                                      | Click submits S0.1 envelope                                                     |
| `VISUALIZATION`      | `<canvas>` with WebGPU context obtained via `navigator.gpu.requestAdapter()` (per-origin per S8.2); wgpu-wasm renders chart/graph/topology                                                                                                                                                                                                                   | Canvas size = layout-resolved CSS size × devicePixelRatio                       |
| `STREAM`             | `<video>` with MSE/WebCodecs for low-latency streams; for transformed streams, `<canvas>` + WebCodecs decode into WebGPU texture                                                                                                                                                                                                                             | Stream payload kind determines pipeline                                         |
| `SURFACE_EMBED`      | `<iframe sandbox="allow-scripts" src="https://<group_id>.aios.localhost:<port>/surface/<surface_id>">`; cross-origin iframe boundary delivers per-origin `GPUAdapter`                                                                                                                                                                                        | App's wasm/wgpu code runs inside the iframe and renders into its own `<canvas>` |
| `SECURITY_INDICATOR` | `<aios-security-indicator>` Web Component appended into the chrome closed shadow root; renders subject_canonical_id + current_action_id + evidence_record_id + ICON_CONSTITUTIONAL_SECURITY_SHIELD                                                                                                                                                           | Always in CHROME zone; never authored by AI subjects (S7.2 §7.1)                |
| `APPROVAL_PROMPT`    | `<aios-approval-prompt>` Web Component as modal `<dialog>`; for destructive prompts, `<dialog>` `cancel` event default-prevented (no Esc dismiss); MOTION_DURATION_DELIBERATE confirm                                                                                                                                                                        | Renders into chrome shadow root; submitter is session subject (S7.2 §I9)        |
| `EVIDENCE_LINK`      | `<aios-evidence-link>` Web Component opening evidence viewer in a new CONTENT surface (new tab `target="_blank"` with `rel="noopener noreferrer"`, or modal); shows evidence_record_id                                                                                                                                                                       | Constitutional icon ICON_CONSTITUTIONAL_EVIDENCE_CHAIN                          |
| `AGENT_MESSAGE`      | `<aios-agent-message>` Web Component bearing INV-021 distinct treatment via theme tokens: `color.action.ai`, `typography.ai_origin`, ICON_CONSTITUTIONAL_AI_INDICATOR badge                                                                                                                                                                                  | All descendants inherit `data-ai-origin="true"` for CSS selector matching       |

The mapping is a function: given a valid `Node`, exactly one Web compilation is selected. Any kind absent from the table is undefined behavior and is rejected with `UNKNOWN_NODE_KIND` (S7.2 RenderTreeErrorCode).

### 3.1 NodeKind family compilation summary

| Family        | Primary primitive        | GPU? | Trust-bearing?        |
| ------------- | ------------------------ | ---- | --------------------- |
| Structural    | `<div>` / `<hr>`         | no   | no                    |
| Text          | semantic tags            | no   | no                    |
| Composite     | `<aios-card/list/table>` | no   | no                    |
| Interaction   | `<form>` / `<button>`    | no   | no                    |
| Live / GPU    | `<canvas>` + WebGPU      | yes  | no                    |
| Trust-bearing | Web Components in shadow | no   | yes                   |
| AI-origin     | `<aios-agent-message>`   | no   | no (but distinct CSS) |

### 3.2 NodeKinds rejected on browsers without WebGPU

When `navigator.gpu` is absent, the renderer enters **degraded mode** (§7.2). NodeKinds that require GPU (`VISUALIZATION`, `STREAM` with hardware-decode requirement, `SURFACE_EMBED` whose surface declares `gpu_capability_class` higher than `GPU_PASSIVE_DISPLAY`) are replaced with a placeholder `<aios-degraded-surface>` Web Component bearing label "Open in a WebGPU-compatible browser." Structural and text NodeKinds continue to render normally. The renderer emits `WEB_RENDERER_DEGRADED` evidence on entry.

## 4. Visual token compilation (per S7.3 §8.2)

Theme tokens compile to CSS custom properties + Web Component internals. The mapping is fixed per token family.

### 4.1 Color tokens

Every closed `ColorTokenName` from S7.3 §4.1 maps to a CSS custom property of the same name in lower kebab-case under `--color-*`:

```text
COLOR_ACTION_AI         → --color-action-ai
COLOR_ACTION_HUMAN      → --color-action-human
COLOR_TRUST_VERIFIED    → --color-trust-verified
... (31 properties + UNSPECIFIED ignored)
```

Variables are applied at `:root` for the page's normal-mode origin, and re-declared inside each chrome shadow root so chrome zone enforcement is independent of page-level CSS overrides. Theme switch emits a `<style>` element replacement inside `:root` and inside every chrome shadow root atomically.

### 4.2 Typography tokens

Every closed `TypographyTokenName` from S7.3 §4.2 maps to a `@font-face` plus a custom-property bundle:

```text
TYPOGRAPHY_BODY_MD       → --typography-body-md-family,
                            --typography-body-md-size,
                            --typography-body-md-weight,
                            --typography-body-md-italic,
                            --typography-body-md-line-height,
                            --typography-body-md-letter-spacing
... (17 typography roles)
```

Typeface assets are served from `/aios/system/themes/<theme_id>/fonts/<asset>.woff2` with `font-display: swap`. The CSP `font-src` directive is `'self'` only.

### 4.3 Spacing tokens

Every closed `SpacingTokenName` from S7.3 §4.3 maps to a single CSS custom property:

```text
SPACING_NONE  → --spacing-none
SPACING_XS    → --spacing-xs
... (9 spacing slots)
```

`gap_logical_pixels` and `padding_logical_pixels` from S7.2 LayoutHints resolve through the spacing scale at compile time.

### 4.4 Motion tokens

Every closed `MotionTokenName` from S7.3 §4.4 maps to two custom properties (duration + easing):

```text
MOTION_DURATION_INSTANT  → --motion-duration-instant   (constitutional 0 ms)
MOTION_DURATION_FAST     → --motion-duration-fast
MOTION_EASING_LINEAR     → --motion-easing-linear
... (11 motion slots)
```

`MOTION_DURATION_INSTANT` is hard-fixed to `0ms` regardless of theme value. The Web Component for `SECURITY_INDICATOR` uses `--motion-duration-instant`; an attempt to override this via inline style is overridden by `!important` in the chrome stylesheet and noted via `WEB_THEME_INJECTION_BLOCKED`.

The CSS `@media (prefers-reduced-motion: reduce)` query collapses durations to 0 ms when the theme's `MotionValue.reduced_motion_zero = true`.

### 4.5 Icon tokens

Constitutional icons are loaded from `/aios/system/themes/<theme_id>/icons/sprite.svg` as a single SVG sprite sheet referenced by `<svg><use href="...#icon-id" />`. The CSP `img-src` directive is `'self'`; `default-src` does not include external CDNs. User-theme icon substitution per S7.3 §7.3 happens at theme validation, before serving — the Web renderer never decides icon authenticity at render time.

### 4.6 Constitutional component recipes

The component recipes from S7.3 §4.6 (e.g., `COMPONENT_SECURITY_BANNER`, `COMPONENT_RECOVERY_SHIELD`, `COMPONENT_AI_SUBJECT_BADGE`) compile to specific custom elements:

```text
COMPONENT_SECURITY_BANNER     → <aios-security-banner>
COMPONENT_RECOVERY_SHIELD     → <aios-recovery-shield>
COMPONENT_AI_SUBJECT_BADGE    → <aios-ai-subject-badge>
COMPONENT_HUMAN_SUBJECT_BADGE → <aios-human-subject-badge>
COMPONENT_TRUST_INDICATOR     → <aios-trust-indicator>
COMPONENT_EVIDENCE_LINK_TILE  → <aios-evidence-link-tile>
COMPONENT_TAMPER_WARNING      → <aios-tamper-warning>
COMPONENT_AGENT_MESSAGE       → <aios-agent-message>
COMPONENT_APPROVAL_PROMPT     → <aios-approval-prompt>
COMPONENT_AUDIT_TRAIL         → <aios-audit-trail>
COMPONENT_ACTION_ENVELOPE_VIEW → <aios-action-envelope-view>
COMPONENT_GROUP_HEADER        → <aios-group-header>
COMPONENT_INBOX_TILE          → <aios-inbox-tile>
```

Custom elements are registered with the browser's `CustomElementRegistry`. Constitutional component recipes (those with `constitutional = true` in `ComponentRecipe`) cannot be re-registered by app code; the renderer holds the registry handle and rejects re-definition attempts with `WEB_CONSTITUTIONAL_ELEMENT_REREGISTER_BLOCKED` evidence (queued for S3.1).

## 5. Localhost-default exposure model

This is the implementation surface for INV-006. The exposure FSM is closed.

### 5.1 Exposure states (closed enum)

```proto
enum WebExposureState {
  WEB_EXPOSURE_STATE_UNSPECIFIED = 0;
  EXPOSURE_LOOPBACK = 1;       // 127.0.0.1 + ::1 only — default; INV-006-conformant
  EXPOSURE_LAN = 2;             // bound to specific LAN interfaces; requires WEB_EXPOSURE_GRANTED FOREVER + ACTIVE STANDARD_24M
  EXPOSURE_PUBLIC = 3;          // bound to public interface(s); requires recovery-mode authorization + dedicated firewall record
  EXPOSURE_RECOVERY = 4;        // recovery page bound to https://recovery.localhost only
}
```

Closed enum, 4 values plus `UNSPECIFIED`.

### 5.2 Exposure transitions (FSM)

```text
EXPOSURE_LOOPBACK    → EXPOSURE_LAN        (requires policy approval + WEB_EXPOSURE_GRANTED)
EXPOSURE_LOOPBACK    → EXPOSURE_PUBLIC     (requires recovery-mode + dual approval + firewall record)
EXPOSURE_LAN         → EXPOSURE_LOOPBACK   (revocation; emits WEB_EXPOSURE_REVOKED)
EXPOSURE_PUBLIC      → EXPOSURE_LOOPBACK   (revocation; emits WEB_EXPOSURE_REVOKED)
EXPOSURE_PUBLIC      → EXPOSURE_LAN        (downgrade; emits WEB_EXPOSURE_REVOKED + grant of LAN)
EXPOSURE_RECOVERY    is orthogonal — recovery page always binds to https://recovery.localhost regardless of normal exposure
```

Forbidden: `EXPOSURE_LAN → EXPOSURE_PUBLIC` (must downgrade to LOOPBACK first, then escalate to PUBLIC). This forces every escalation to pass through the constitutional ground state.

### 5.3 Loopback (default)

The renderer service binds:

- `127.0.0.1:<port>` (IPv4 loopback)
- `[::1]:<port>` (IPv6 loopback)

No other interfaces. The TLS cert is the AIOS-install-time self-signed cert with SAN containing `localhost`, `127.0.0.1`, `::1`, `aios.localhost`, `*.aios.localhost`, `recovery.localhost`. The cert is signed by the AIOS root key per S6.1 vault broker (deferred); for now the cert is generated at install and stored at `/aios/system/web/tls/`.

The loopback state is INV-006-conformant by construction; the verification primitive `port_open(host="0.0.0.0", port=N)` returns `FAILED` because the renderer never binds `0.0.0.0`.

### 5.4 LAN exposure

Granted only on:

1. A submitted action of kind `aios.web.GrantLANExposure` carrying `interface_name`, `bind_address`, `cidr_allow_list`, with a non-empty intent.
2. A policy decision (S2.3) that returns `APPROVED` with explicit human approval (the operator is `HUMAN_USER`).
3. Issuance of `WEB_EXPOSURE_GRANTED` evidence at retention `FOREVER` carrying the granted parameters.

On grant, the renderer rebinds to the specified interface(s) within the performance budget (§8). The exposure is **not** persistent across renderer restarts unless `WEB_EXPOSURE_GRANTED.persistent = true` is set in the granted body. While LAN-exposed, the renderer emits a heartbeat `WEB_LAN_EXPOSURE_ACTIVE` `STANDARD_24M` evidence record on a renderer-defined cadence (default every 6 hours).

The chrome surface acquires a persistent `<aios-lan-exposure-banner>` element informing the operator that AIOS is reachable on LAN; the banner is rendered into the chrome shadow root and cannot be dismissed.

### 5.5 Public exposure

Public exposure is rare and gated harder:

1. Action of kind `aios.web.GrantPublicExposure`.
2. Active subject must be in **recovery mode** (per S5.1 §7).
3. Two human approvers (per S2.3 quorum constraint).
4. Explicit firewall configuration record `WEB_PUBLIC_EXPOSURE_FIREWALL_RECORDED` (FOREVER) carrying allowed source CIDRs and rate limits.
5. `WEB_PUBLIC_EXPOSURE_GRANTED` evidence at retention `FOREVER`.

While public-exposed, the chrome shows an unmistakable banner via `<aios-public-exposure-banner>` and the renderer additionally enforces `Strict-Transport-Security: max-age=63072000; includeSubDomains; preload`. The cert MUST chain to a public CA (Let's Encrypt or equivalent); self-signed certs are rejected for public exposure.

### 5.6 Revocation

Revocation is a single-action: any operator with the appropriate capability submits `aios.web.RevokeExposure`. The renderer rebinds to loopback within the performance budget and emits `WEB_EXPOSURE_REVOKED` `STANDARD_24M` evidence. The `WEB_LAN_EXPOSURE_ACTIVE` heartbeat ceases on the next cycle. The `<aios-lan-exposure-banner>` / `<aios-public-exposure-banner>` is removed from chrome.

## 6. Recovery page rendering

Recovery rendering is a structural mode, not a theme switch. The recovery page is its own browser document at its own origin.

### 6.1 Origin and scope

- Normal-mode origin: `https://aios.localhost:<port>` (or `https://<group_id>.aios.localhost:<port>` for per-group APP_SURFACE iframes).
- Recovery-mode origin: `https://recovery.localhost:<port>`.
- The browser's same-origin policy isolates the two: no shared cookies, no shared IndexedDB, no shared Service Worker scope. A normal-mode session cannot navigate into the recovery page without an explicit operator action that invalidates the normal-mode session.

### 6.2 Auto-active recovery theme

When the renderer serves the recovery origin, it loads `theme_kind = AIOS_RECOVERY` automatically (per S7.3 §6.3). The recovery theme is root-signed and cannot be overridden. The page's `:root` carries the recovery token bundle; the chrome shadow root acquires the recovery shield prominently.

### 6.3 NodeKinds permitted in recovery

Per S7.1 §I6 + S7.2 §8 + INV-022:

- Allowed: `CONTAINER`, `DIVIDER`, `SPACER`, `TEXT`, `HEADING`, `INLINE_CODE`, `CODE_BLOCK`, `CARD`, `LIST` (inline source only), `TABLE` (inline source only), `FORM`, `ACTION_BUTTON`, `SECURITY_INDICATOR`, `APPROVAL_PROMPT`, `EVIDENCE_LINK`.
- Rejected: `VISUALIZATION` (no GPU in recovery shell), `STREAM` (no live media in recovery), `SURFACE_EMBED` (no APP_SURFACE in recovery), `AGENT_MESSAGE` (no AI cognition in recovery — recovery is L1 territory and must not depend on L5).

The renderer rejects forbidden kinds at validation with `RECOVERY_KIND_FORBIDDEN_ON_WEB`; emits `WEB_RECOVERY_KIND_REJECTED` evidence at `FOREVER` retention.

### 6.4 No Service Worker on recovery

The recovery origin does not register a Service Worker. Network requests are direct. IndexedDB is not used. The recovery page is fully stateless on the client; every reload re-fetches authoritative state from upstream services.

### 6.5 Recovery sign-in

The recovery sign-in form is a dedicated `<form>` rendered from a tree whose root is `recovery_only = true`, issued by `_system:service:recovery-shell`. Operator credentials are entered into FIELD_TEXT and FIELD_TEXT_MULTILINE (for hardware-token responses) per the L4 identity model recovery flow. The form submits to the L4 identity service over gRPC-Web on the recovery origin only.

### 6.6 Recovery page lifecycle evidence

Loading the recovery page emits `WEB_RECOVERY_PAGE_LOADED` `EXTENDED_60M` evidence carrying a privacy-preserving session id and the user agent fingerprint class (browser kind only — see telemetry §12). Exiting recovery (operator logout or session timeout) emits `WEB_RECOVERY_PAGE_EXITED` `EXTENDED_60M`.

## 7. Browser support matrix

The renderer targets browsers with stable WebGPU support at adoption time. The matrix is closed and curated at theme + renderer bundle release; older browsers are not partially supported.

### 7.1 Supported

| Browser kind    | Minimum                                                                               | Notes                                                               |
| --------------- | ------------------------------------------------------------------------------------- | ------------------------------------------------------------------- |
| Chromium-based  | Stable channel current at AIOS release with WebGPU enabled (`navigator.gpu` non-null) | Includes Chrome, Edge, Brave, Vivaldi                               |
| Firefox         | Stable channel current with WebGPU enabled                                            | `dom.webgpu.enabled` true; renderer detects via `navigator.gpu`     |
| Safari (WebKit) | Stable channel current with WebGPU enabled                                            | macOS / iPad OS / iOS; renderer respects WebKit origin-frame quirks |

### 7.2 Degraded mode

Browsers that load the page but lack WebGPU enter `BROWSER_KIND_OTHER` degraded mode. Structural NodeKinds render normally; GPU-bearing NodeKinds (§3.2) show placeholders. The renderer emits `WEB_RENDERER_DEGRADED` evidence and the chrome shows a non-dismissable hint to upgrade.

### 7.3 Unsupported

Internet Explorer, legacy Edge (EdgeHTML), pre-WebGPU browser builds, and any user agent whose feature detection fails to find both `customElements`, `navigator.gpu` (for full mode) or `customElements` alone (for degraded), receive HTTP `426 Upgrade Required` with a static landing page describing supported browsers. No JavaScript executes on these browsers.

### 7.4 Closed `BrowserKind` enum

```proto
enum BrowserKind {
  BROWSER_KIND_UNSPECIFIED = 0;
  BROWSER_KIND_CHROMIUM = 1;
  BROWSER_KIND_FIREFOX = 2;
  BROWSER_KIND_SAFARI = 3;
  BROWSER_KIND_OTHER = 4;        // detected modern but unrecognized; degraded heuristics
}
```

Closed, 4 values plus `UNSPECIFIED`. Telemetry uses this enum as a label, never the raw User-Agent string.

## 8. Performance contract

| Operation                                                      | p50      | p95      | p99      | Hard timeout |
| -------------------------------------------------------------- | -------- | -------- | -------- | ------------ |
| First Contentful Paint (FCP) on loopback                       | < 0.7 s  | < 1.5 s  | < 2.5 s  | 10 s         |
| Largest Contentful Paint (LCP) on loopback                     | < 1.0 s  | < 2.0 s  | < 3.5 s  | 10 s         |
| Interaction to Next Paint (INP)                                | < 80 ms  | < 200 ms | < 500 ms | 2 s          |
| Cumulative Layout Shift (CLS) — chrome must not shift          | < 0.05   | < 0.10   | < 0.20   | enforced     |
| WebGPU surface initialization (`requestAdapter` → first frame) | < 50 ms  | < 100 ms | < 250 ms | 2 s          |
| Schema-tree-to-DOM compilation (per 1 000 nodes)               | < 20 ms  | < 50 ms  | < 150 ms | 1 s          |
| Service Worker registration (normal-mode origin)               | < 100 ms | < 200 ms | < 500 ms | 2 s          |
| Theme load + style application                                 | < 50 ms  | < 100 ms | < 300 ms | 2 s          |
| Exposure rebind (loopback ↔ LAN)                               | < 200 ms | < 500 ms | < 1 s    | 5 s          |
| Chrome shadow root construction                                | < 5 ms   | < 20 ms  | < 50 ms  | 200 ms       |

CLS for the chrome zone is constitutional: chrome must not shift after first paint. A measured CLS > 0.10 in chrome triggers a `WEB_RENDERER_CLS_BREACH` `STANDARD_24M` evidence record (queued for S3.1) and a renderer-side alert.

Failure modes — fail closed:

- `WEB_RENDERER_INTERNAL` → caller receives error; alert emitted.
- `WEBGPU_UNAVAILABLE` → degraded mode (§7.2); GPU NodeKinds replaced.
- `THEME_NOT_LOADED` → fallback theme applied; `WEB_THEME_FALLBACK_USED` evidence.
- `EXPOSURE_REBIND_FAILED` → previous binding retained; alert emitted; revocation request held in pending state.

## 9. Adversarial robustness

### 9.1 Page-level Fullscreen API request

An app's JavaScript calls `document.documentElement.requestFullscreen()`, hoping to take the entire viewport and hide chrome. The renderer's CSP `permissions-policy` directive declares `fullscreen=()` (empty allowlist) at the page level; `requestFullscreen` on the document is denied. Element-level fullscreen on a `<canvas>` inside CONTENT-zone surfaces is permitted via `permissions-policy` `fullscreen=(self "https://*.aios.localhost:*")`, but the chrome shadow root remains positioned above the fullscreen element by browser stacking rules (z-index 9999, `position: fixed`, attached at the document root).

### 9.2 Clickjacking via iframe embedding from outside

Headers on every response include `X-Frame-Options: DENY` (legacy) and CSP `frame-ancestors 'self'` (modern). External pages cannot iframe AIOS surfaces. SURFACE_EMBED iframes between `aios.localhost` subdomains are permitted because they share the parent origin family.

### 9.3 App attempts to break out of `<iframe>` SURFACE_EMBED

The SURFACE_EMBED iframe carries `sandbox="allow-scripts"` only — no `allow-same-origin`, no `allow-top-navigation`, no `allow-popups`. App code inside the iframe cannot access the parent document, cannot navigate the top frame, cannot open windows. Cross-origin nature gives a separate `GPUAdapter`. The CSP within the iframe further restricts.

### 9.4 Theme injection via custom CSS

The renderer ignores user-supplied stylesheets. Themes load only from S7.3 signed bundles. CSS `@import` inside themes is forbidden by validation. The CSP `style-src` is `'self' 'sha256-<theme-css-hash>'`; inline styles are accepted only with hash matches; `unsafe-inline` is never set. An attempted `<style>` injection from page DOM into the chrome shadow root is impossible (closed shadow root), and into the page DOM does not affect chrome.

### 9.5 AIOS chrome impersonation in page DOM

An app draws fake security indicators in CONTENT zone to trick the operator. Defenses:

- Constitutional icons load only from `/aios/system/themes/<theme_id>/icons/sprite.svg` (CSP `img-src 'self'`); page CSP forbids loading constitutional sprite asset URLs from app origins.
- The real chrome shadow root is always on top (z-index 9999); fake indicators in CONTENT zone are visually subordinate.
- Operators learn the visual language: the constitutional `COMPONENT_TRUST_INDICATOR` always sits in chrome, never in content.
- Tree-signing service refuses to sign trees that contain `SECURITY_INDICATOR` authored by AI (S7.2 §7.1); a non-AI app trying to render `SECURITY_INDICATOR` in CONTENT can do so only with a content-zone surface, where the chrome `SECURITY_INDICATOR` overpaints any conflicting content.

### 9.6 Service Worker interception of evidence requests

The Service Worker (normal-mode origin only) registers handlers only for theme assets, schema-tree fragments under explicit retention, and last-known view snapshots. Evidence record fetches (`/api/evidence/...`) and capability runtime calls bypass the Service Worker via `cache: "no-store"` and explicit `bypass: true` flag in the SW configuration. The recovery origin has no Service Worker.

### 9.7 Cross-origin GPU exfiltration

Per S8.2 §I4: `family.aios.localhost` and `homelab.aios.localhost` are distinct origins; the browser allocates distinct `GPUAdapter` instances on `navigator.gpu.requestAdapter()`. Cross-origin texture sharing is denied by the browser. dmabuf passing across origins is not a concept on Web; surface bytes never leave the iframe.

### 9.8 Browser extension injection

Browser extensions can inject DOM into any page including AIOS. Defenses:

- The chrome shadow root is closed; extensions cannot read or modify it via the `shadowRoot` accessor.
- A periodic chrome-zone integrity check walks the chrome shadow tree and verifies hashes against the renderer-signed construction path. Mismatches trigger `WEB_EXTENSION_INTERFERENCE` evidence at `STANDARD_24M` retention with the offending node count (no DOM excerpts; bounded label).
- The operator is shown an `<aios-extension-warning>` chrome banner when interference is detected.
- Critical actions (those flagged `destructive = true`) require approval through `<dialog>` modal that captures focus; extensions cannot programmatically click `ACCEPT` because the modal traps focus and the `dialog::backdrop` blocks pointer events from outside.

### 9.9 Content Security Policy summary

The normal-mode page CSP is fixed. Closed; not theme-overridable.

```text
default-src 'self';
script-src 'self' 'wasm-unsafe-eval';
style-src 'self' 'sha256-<theme-css-hash>';
img-src 'self' data:;
font-src 'self';
connect-src 'self' https://*.aios.localhost:*;
frame-src 'self' https://*.aios.localhost:*;
frame-ancestors 'self';
form-action 'self';
base-uri 'self';
object-src 'none';
worker-src 'self';
manifest-src 'self';
upgrade-insecure-requests;
```

Recovery-mode CSP additionally drops `worker-src` (no Service Worker) and tightens `connect-src` to the recovery origin only.

### 9.10 IndexedDB / Service Worker DoS

A misbehaving page tries to fill IndexedDB until quota errors. The renderer enforces a per-origin quota (default 50 MiB ephemeral cache) and evicts on LRU. Service Worker cache enforces a similar bound. Quota breach emits `WEB_CLIENT_STORAGE_QUOTA_BREACH` `STANDARD_24M` evidence (queued for S3.1).

## 10. Cross-spec dependencies

| Spec       | Direction | What this spec contributes / consumes                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| ---------- | --------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| S0.1       | consumer  | `FORM` and `ACTION_BUTTON` in the Web compilation produce S0.1 envelopes; `submitter` is the active session subject.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| S2.1       | consumer  | View subscriptions for `LIST` / `TABLE` are gRPC-Web streams; cursor semantics for paging.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| S2.3       | consumer  | Exposure-grant actions go through the policy kernel; LAN/PUBLIC require explicit approval.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| S3.1       | producer  | Ten new record types queued for the next S3.1 consolidation cycle: `WEB_RENDERER_STARTED` STANDARD_24M, `WEB_RENDERER_DEGRADED` STANDARD_24M, `WEB_LAN_EXPOSURE_GRANTED` FOREVER, `WEB_LAN_EXPOSURE_ACTIVE` STANDARD_24M, `WEB_EXPOSURE_REVOKED` STANDARD_24M, `WEB_EXTENSION_INTERFERENCE` STANDARD_24M, `WEB_FULLSCREEN_REQUESTED` STANDARD_24M, `WEB_THEME_INJECTION_BLOCKED` STANDARD_24M, `WEB_PUBLIC_EXPOSURE_GRANTED` FOREVER, `WEB_RECOVERY_PAGE_LOADED` EXTENDED_60M. Plus secondary candidates: `WEB_RECOVERY_PAGE_EXITED` EXTENDED_60M, `WEB_RECOVERY_KIND_REJECTED` FOREVER, `WEB_THEME_FALLBACK_USED` STANDARD_24M, `WEB_CLIENT_STORAGE_QUOTA_BREACH` STANDARD_24M, `WEB_RENDERER_CLS_BREACH` STANDARD_24M, `WEB_PUBLIC_EXPOSURE_FIREWALL_RECORDED` FOREVER, `WEB_CONSTITUTIONAL_ELEMENT_REREGISTER_BLOCKED` STANDARD_24M. |
| S5.1       | consumer  | Recovery mode flag drives §6 distinct-origin rendering; subject identity drives `<aios-security-indicator>` content.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| S6.1       | consumer  | Vault Broker (deferred) ultimately holds LAN/public exposure cert chains; in this spec the cert acquisition path is delegated to vault.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| S7.1       | consumer  | Composition zones BACKGROUND / CONTENT / OVERLAY / CHROME map to DOM portals + shadow root per §4.3 of S7.1; surface lifecycle FSM consumed.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| S7.2       | consumer  | NodeKind compilation per §3; tree signature verification per S7.2 §I4; trust-bearing authorship rule honored at compilation (chrome rejects unsigned trust-bearing nodes).                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| S7.3       | consumer  | Token compilation per §4 (Web column of S7.3 §8.2); constitutional component recipes registered as custom elements; theme load via Web's `LoadTheme` flow.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| S8.2       | consumer  | Per-origin `GPUAdapter` per S8.2 §I4; iframe origin = `https://<group_id>.aios.localhost:<port>`; `gpu_capability_class` from L8 binds `<canvas>` allocation.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| L0 INV-006 | enforcer  | Localhost-default exposure FSM (§5) is the implementation of INV-006 on Web.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| L0 INV-019 | enforcer  | Cross-renderer visual continuity preserved by consuming S7.3 token mapping verbatim.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| L0 INV-020 | enforcer  | Closed shadow root + z-index 9999 + `permissions-policy fullscreen=()` + CSP `frame-ancestors 'self'` keep chrome unbreakable.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| L0 INV-021 | enforcer  | `<aios-agent-message>` distinct CSS bound to `is_ai_origin`; theme tokens differentiate.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                |
| L0 INV-022 | enforcer  | Distinct recovery origin + auto-active recovery theme + restricted NodeKind set.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| S2.4       | producer  | New verification primitive `web_renderer_bound_to(host, port) ∈ {LOOPBACK, LAN, PUBLIC}` queued for next S2.4 consolidation; primitive `web_chrome_z_index_at_least(value)` queued.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| S3.2       | consumer  | App's sandbox profile constrains the per-iframe permissions in SURFACE_EMBED (queued for the S3.2 follow-up that adds web sandbox vector).                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |

## 11. Golden fixtures

### Fixture 1 — Loopback-only operator session

```text
Setup:
  Renderer started; no exposure-grant evidence on file.
  Operator alice opens https://aios.localhost:9443 in Firefox.

Expected:
  Renderer state: EXPOSURE_LOOPBACK.
  Bind addresses observed: 127.0.0.1:9443, [::1]:9443. No 0.0.0.0 binding.
  S2.4 verification primitive port_open(host="0.0.0.0", port=9443) → FAILED.
  WEB_RENDERER_STARTED evidence emitted (STANDARD_24M).
  No WEB_LAN_EXPOSURE_* evidence emitted.
  Chrome banner: absent (no exposure indicator).
```

### Fixture 2 — NeuroCAD app surface in per-group iframe

```text
Setup:
  Group: homelab. Subject: homelab:app:bg.iconys.neurocad:i-01 (APPLICATION).
  NeuroCAD's render core is a Rust + wgpu binary compiled to wasm32-unknown-unknown.
  S7.1 surface created: SURFACE_KIND = APP_SURFACE, zone = CONTENT,
     gpu_capability_class = GPU_RICH_2D + co-binding GPU_COMPUTE_HEAVY.

UI tree contains a SURFACE_EMBED node referencing the surface_id.

Expected:
  Web compilation produces an <iframe> with src = "https://homelab.aios.localhost:9443/surface/<surface_id>",
     sandbox = "allow-scripts" only.
  The iframe's origin "homelab.aios.localhost" delivers a per-group GPUAdapter (per S8.2 §I4).
  A different group (family) opening a different iframe gets a different GPUAdapter — no cross-group GPU memory sharing.
  AIOS chrome shadow root in the parent frame remains visible above the iframe's content;
     INV-020 satisfied even if the iframe enters element-fullscreen on its own <canvas>.
```

### Fixture 3 — Page-level fullscreen does not hide chrome

```text
Setup:
  Operator views a SURFACE_EMBED of a Bevy game APP_SURFACE.
  Game JavaScript inside the iframe calls element.requestFullscreen() on its own <canvas>.

Expected:
  Element-level fullscreen permitted (permissions-policy allows fullscreen for self + *.aios.localhost).
  The parent document's chrome shadow root, with position: fixed and z-index 9999,
     remains stacked above the fullscreen <canvas> per browser stacking rules
     for fullscreen-ancestor elements with higher stacking context.
  No WEB_FULLSCREEN_REQUESTED denial; INV-020 satisfied.

Adversarial twist:
  Page-level code calls document.documentElement.requestFullscreen().
  permissions-policy "fullscreen=()" denies; promise rejects.
  WEB_FULLSCREEN_REQUESTED evidence emitted (STANDARD_24M) noting denial.
```

### Fixture 4 — Theme load with custom CSS rejected

```text
Setup:
  A user uploads a theme bundle that includes a custom CSS file with @import url("https://attacker.example/font.css");

Expected:
  Theme validation per S7.3 §6.2 rejects: custom @import is forbidden.
  ConstitutionalIconAltered or ThemeInvariantViolated raised at S7.3 layer, not at the Web renderer.
  Web renderer never loads the theme; previous theme remains active.
  WEB_THEME_INJECTION_BLOCKED evidence (STANDARD_24M) NOT emitted at this stage — the rejection is
     upstream at theme validation. WEB_THEME_INJECTION_BLOCKED applies to attempts to inject CSS
     into the live page after theme load.
```

### Fixture 5 — LAN exposure approval flow

```text
Setup:
  Operator alice (HUMAN_USER) submits action: aios.web.GrantLANExposure
     interface_name = "eno1", bind_address = "192.168.1.42",
     cidr_allow_list = ["192.168.1.0/24"].

Policy:
  S2.3 evaluates; rule "LAN exposure requires HUMAN_USER + explicit operator confirmation" matches;
     decision = APPROVED with approval_required = false (alice is the operator).

Capability runtime:
  ExecuteAction → ValidateAction → succeeded.
  WEB_EXPOSURE_GRANTED evidence (FOREVER) emitted carrying granted parameters.

Web renderer:
  Receives grant; rebinds to 127.0.0.1:9443, [::1]:9443, 192.168.1.42:9443 within 500 ms.
  S2.4 verification primitive port_open(host="192.168.1.42", port=9443) → SUCCESS.
  Chrome adds <aios-lan-exposure-banner>: "AIOS reachable on LAN: 192.168.1.42:9443".
  Heartbeat WEB_LAN_EXPOSURE_ACTIVE STANDARD_24M emitted every 6 hours while exposed.
  Telemetry web_exposure_state{state="lan"} = 1.
```

### Fixture 6 — Recovery page distinct origin

```text
Setup:
  Operator navigates to https://recovery.localhost:9443/.

Expected:
  Renderer serves the recovery shell.
  Origin: recovery.localhost — distinct from aios.localhost; no shared cookies / IndexedDB / SW.
  Auto-loaded theme: theme_kind = AIOS_RECOVERY (per S7.3 §6.3); recovery typography active;
     recovery shield prominent in CHROME zone.
  Service Worker NOT registered.
  IndexedDB NOT used.
  A submitted UI tree containing SURFACE_EMBED is rejected with RECOVERY_KIND_FORBIDDEN_ON_WEB.
  WEB_RECOVERY_PAGE_LOADED evidence emitted (EXTENDED_60M) carrying browser_kind = BROWSER_KIND_FIREFOX.
  Operator credential entry via dedicated <aios-recovery-signin-form> on the recovery origin only.
```

### Fixture 7 — Browser without WebGPU enters degraded mode

```text
Setup:
  Operator opens https://aios.localhost:9443 in a Firefox build with dom.webgpu.enabled = false.

Expected:
  Feature detection: navigator.gpu === undefined.
  Renderer enters degraded mode.
  Schema NodeKinds CONTAINER, TEXT, HEADING, FORM, LIST, TABLE, CARD render normally.
  NodeKinds VISUALIZATION, STREAM, SURFACE_EMBED with capability_class > GPU_PASSIVE_DISPLAY
     replaced by <aios-degraded-surface> placeholder showing
     "Open in a WebGPU-compatible browser."
  WEB_RENDERER_DEGRADED evidence emitted (STANDARD_24M).
  Chrome banner shown; not dismissable.
  Telemetry web_render_mode{mode="degraded"} = 1.
```

### Fixture 8 — Browser extension injects DOM into AIOS page

```text
Setup:
  Operator has a generic ad-blocker extension installed.
  Extension content script injects <div id="ad-blocker-banner">...</div> into the page DOM.

Expected:
  Renderer's chrome-zone integrity check (interval-driven; default every 2 s) walks the chrome shadow root.
  Chrome shadow root is closed; ad-blocker's content script cannot read or modify it (extensions can sometimes
     access closed shadow roots in privileged contexts; the check still detects unauthorized children
     by hash mismatch).
  If the extension somehow injects into chrome (e.g., via debugger API), hash mismatch detected;
     offending node removed by renderer; <aios-extension-warning> chrome banner shown.
  WEB_EXTENSION_INTERFERENCE evidence emitted (STANDARD_24M) carrying browser_kind and a bounded
     count_class label (1 / 2-10 / >10).
  Page DOM injection (outside chrome) is ignored — ad-blocker's banner sits below chrome z-index.
  Operator's chrome remains intact; trust-bearing UI unaffected.
```

## 12. Telemetry contract

All metrics use bounded label cardinality. **node_id, surface_id, subject_canonical_id, group_id, user_id, evidence_record_id, view_ref, theme_id are NEVER labels.**

| Metric                                  | Type      | Labels (closed)                                                              |
| --------------------------------------- | --------- | ---------------------------------------------------------------------------- |
| `web_renderer_started_total`            | counter   | `result` (success / error), `error_code`                                     |
| `web_render_mode_active`                | gauge     | `mode` (full / degraded / recovery)                                          |
| `web_browser_kind_active`               | gauge     | `browser_kind` (chromium / firefox / safari / other)                         |
| `web_node_compile_total`                | counter   | `kind` (S7.2 NodeKind enum), `result` (compiled / dropped / placeholder)     |
| `web_node_compile_duration_seconds`     | histogram | `kind`, `node_count_class` (1 / 16 / 256 / 1k / 10k)                         |
| `web_webgpu_init_duration_seconds`      | histogram | `result` (success / unavailable)                                             |
| `web_exposure_state`                    | gauge     | `state` (loopback / lan / public / recovery)                                 |
| `web_exposure_transition_total`         | counter   | `from_state`, `to_state`, `result` (granted / denied / revoked)              |
| `web_lan_exposure_active_seconds_total` | counter   | none                                                                         |
| `web_chrome_integrity_check_total`      | counter   | `result` (intact / interference)                                             |
| `web_extension_interference_total`      | counter   | `browser_kind`, `dropped_frame_severity` (low / medium / high)               |
| `web_fullscreen_request_total`          | counter   | `target` (page / element), `result` (allowed / denied)                       |
| `web_theme_injection_blocked_total`     | counter   | `vector` (style_tag / inline_style / import / shadow_root_attempt)           |
| `web_recovery_page_session_total`       | counter   | `result` (loaded / kind_rejected / signin_completed / exited)                |
| `web_form_submit_total`                 | counter   | `result` (accepted / rejected), `error_code`                                 |
| `web_action_button_click_total`         | counter   | `destructive` (true / false)                                                 |
| `web_view_subscription_total`           | counter   | `kind` (list / table), `result` (success / scope_violation / view_not_found) |
| `web_client_storage_bytes_used`         | gauge     | `store` (idb / sw_cache)                                                     |
| `web_cls_chrome`                        | histogram | none                                                                         |
| `web_lcp_seconds`                       | histogram | `mode`                                                                       |
| `web_inp_seconds`                       | histogram | `mode`                                                                       |
| `web_fullscreen_denied_total`           | counter   | `kind` (page / element)                                                      |

Cardinality budget: ≤ 200 active label tuples per metric.

## 13. Acceptance criteria

- [ ] Web renderer service binds to loopback only (`127.0.0.1` + `[::1]`) by default; `port_open(host="0.0.0.0", port=N)` returns `FAILED`. INV-006 satisfied.
- [ ] Every closed S7.2 NodeKind has exactly one Web compilation per §3; unknown kinds rejected with `UNKNOWN_NODE_KIND`.
- [ ] Trust-bearing chrome (`SECURITY_INDICATOR`, `APPROVAL_PROMPT`, `EVIDENCE_LINK`) renders into a closed shadow root at `position: fixed; z-index: 9999`; CSP `permissions-policy: fullscreen=()` denies page-level fullscreen.
- [ ] Per-origin `GPUAdapter` per S8.2: iframe origin format `https://<group_id>.aios.localhost:<port>`; renderer rejects compositions where origin's `<group_id>` does not match `ScopeBinding.group_id`.
- [ ] Constitutional icons load only from `/aios/system/themes/<theme_id>/icons/sprite.svg`; CSP `img-src 'self'` enforced; no external icon hosts permitted.
- [ ] Chrome shadow roots are constructed `mode: "closed"`; page DOM cannot reach them via the `shadowRoot` accessor.
- [ ] Service Worker registered on normal-mode origin only; recovery origin has no Service Worker; IndexedDB unused on recovery.
- [ ] HTTPS mandatory; HTTP requests return `421 Misdirected Request`; LAN exposure requires policy-approved cert; public exposure requires public-CA cert.
- [ ] Closed shadow-root integrity check runs on a renderer-defined cadence; mismatches emit `WEB_EXTENSION_INTERFERENCE` evidence.
- [ ] Recovery page served from `https://recovery.localhost:<port>`; auto-active `AIOS_RECOVERY` theme; restricted NodeKind set per §6.3; `RECOVERY_KIND_FORBIDDEN_ON_WEB` rejection on forbidden kinds.
- [ ] Exposure FSM (§5.2) closed; transitions covered by approval-bound actions; `WEB_EXPOSURE_GRANTED` `FOREVER` and `WEB_LAN_EXPOSURE_ACTIVE` `STANDARD_24M` heartbeat emitted while LAN-exposed; `WEB_EXPOSURE_REVOKED` `STANDARD_24M` on revocation.
- [ ] Performance budgets (§8) measured and enforced; `WEB_RENDERER_CLS_BREACH` evidence on chrome-zone CLS > 0.10.
- [ ] Browser support matrix (§7) enforced; unsupported browsers receive `426 Upgrade Required`; degraded mode for browsers without WebGPU emits `WEB_RENDERER_DEGRADED`.
- [ ] All eight golden fixtures (§11) produce the specified outcomes.
- [ ] Telemetry conforms to §12; node / surface / subject / group / user / evidence / view / theme identifiers never appear as labels.
- [ ] `BrowserKind`, `WebExposureState` enums are closed and have exactly the values declared.
- [ ] L0 INV-006 (web localhost default) implemented via §5 exposure FSM.
- [ ] L0 INV-020 (chrome unbreakable) implemented via closed shadow root + z-index 9999 + CSP `permissions-policy fullscreen=()` + `frame-ancestors 'self'`.
- [ ] L0 INV-021 (AI vs human visual distinction) implemented via S7.3 token compilation in `<aios-agent-message>` and AI-origin CSS selector matching.
- [ ] L0 INV-022 (recovery aesthetic distinct) implemented via §6 distinct origin + auto-active recovery theme + NodeKind restriction.

## 14. Open deferrals

- **Service Worker offline cache contents.** Current spec restricts SW to ephemeral cache (themes, schema fragments, view snapshots). The exact eviction policy, cache key strategy, and invalidation protocol on theme switch / schema upgrade are deferred to a Web renderer operations follow-up.
- **Push notifications.** Web Push for action notifications (e.g., approval prompts arriving while AIOS tab is backgrounded) deferred. Requires careful integration with S2.3 approval flow and FOREVER-graded evidence.
- **Federated multi-window sessions.** Multiple tabs of AIOS open simultaneously, sharing session state via BroadcastChannel + identity service. Deferred.
- **Drag and drop across surfaces.** Cross-surface DnD (between AIOS surfaces, between AIOS and external pages) deferred to a per-renderer interaction spec.
- **WebRTC for peer-to-peer streams.** Live A/V streams between operators in different groups deferred; depends on S8.X NetworkStream model.
- **Progressive Web App install.** "Install AIOS as desktop app" via PWA manifest deferred; the manifest schema, install evidence, and uninstall flow need design.
- **WebTransport / HTTP/3.** Migration from gRPC-Web (HTTP/1.1 / HTTP/2) to WebTransport (HTTP/3) for view subscriptions deferred. The Web renderer currently mandates gRPC-Web.
- **Accessibility tree details for WebGPU surfaces.** A `<canvas>` carries no inherent accessibility tree; how AIOS_SURFACE / APP_SURFACE expose internal structure to assistive tech (ARIA Virtual Trees, AOM) is deferred.
- **Mobile browser viewport-meta + safe-area handling.** iOS Safari notch handling, on-screen keyboard interaction, Android viewport quirks deferred.
- **Per-action confirmation gestures beyond the modal.** Slow-confirm sliders, multi-step gestures, biometric reauthentication on Web (WebAuthn) for destructive actions deferred.
- **gRPC-Web cert pinning.** Browser cert pinning for the loopback cert (so a malicious local CA cannot MitM the page) is deferred; current spec relies on the per-browser cert trust dialog at first install.
- **Private namespace via URL paths.** Currently `<group_id>.aios.localhost:<port>` is the per-group origin via subdomain. An alternative URL-path-based scheme for browsers that block wildcard local subdomains is deferred.

## 15. See also

- [S7.1 — Surface + Composition Model](01_surface_composition.md)
- [S7.2 — Shared UI Schema](02_shared_ui_schema.md)
- [S7.3 — Visual Language](03_visual_language.md)
- [S8.2 — GPU Resource Model](../L8_Network_Hardware_Devices/05_gpu_resource_model.md)
- [S5.1 — Identity Model](../L4_Policy_Identity_Vault/03_identity_model.md)
- [S6.4 — Constitutional Invariants (incl. INV-006, INV-019..INV-022)](../L0_Governance_Evidence_Safety/04_invariants.md)
- [S2.3 — Policy Kernel](../L4_Policy_Identity_Vault/01_policy_kernel.md)
- [S3.1 — Evidence Log](../L9_Observability_Admin_Operations/01_evidence_log.md)
- [S0.1 — Action Envelope and Lifecycle](../XX_Cross_Cutting/01_action_envelope_lifecycle.md)
- [L7 Overview](00_overview.md)
- [Rev.2 Master Index](../00_MASTER_INDEX.md)

## Appendix A — Full Proto IDL

```proto
syntax = "proto3";
package aios.renderer.web.v1alpha1;

import "google/protobuf/timestamp.proto";
import "google/protobuf/duration.proto";

// ============================================================================
// Service
// ============================================================================

service WebRenderer {
  // Compile + render an S7.2 UI tree into the renderer's session DOM. The session
  // is identified by the active session_id; chrome / content / overlay zones
  // are populated per S7.1 composition rules.
  rpc RenderTree(RenderTreeRequest) returns (RenderTreeResponse);

  // Open the recovery shell at https://recovery.localhost:<port>. Closed-origin
  // navigation; emits WEB_RECOVERY_PAGE_LOADED.
  rpc OpenRecoveryPage(OpenRecoveryPageRequest) returns (OpenRecoveryPageResponse);

  // Register a custom element bound to a constitutional or non-constitutional
  // S7.3 component recipe. Constitutional registrations require AIOS root
  // signature; non-constitutional may be theme-supplied.
  rpc RegisterCustomElement(RegisterCustomElementRequest) returns (RegisterCustomElementResponse);

  // Grant LAN or PUBLIC exposure. Caller MUST have approved S0.1 envelope; the
  // RPC is the post-approval execution hook.
  rpc GrantExposure(GrantExposureRequest) returns (GrantExposureResponse);

  // Revoke any non-loopback exposure; rebind to loopback.
  rpc RevokeExposure(RevokeExposureRequest) returns (RevokeExposureResponse);

  // Renderer self-description.
  rpc GetRendererInfo(GetRendererInfoRequest) returns (GetRendererInfoResponse);

  // Live exposure state stream — clients (e.g., the chrome shadow root) subscribe
  // to be notified of LAN/PUBLIC banner state changes.
  rpc StreamExposureState(StreamExposureStateRequest) returns (stream ExposureStateUpdate);
}

// ============================================================================
// Closed enums
// ============================================================================

enum WebExposureState {
  WEB_EXPOSURE_STATE_UNSPECIFIED = 0;
  EXPOSURE_LOOPBACK = 1;
  EXPOSURE_LAN = 2;
  EXPOSURE_PUBLIC = 3;
  EXPOSURE_RECOVERY = 4;
}

enum BrowserKind {
  BROWSER_KIND_UNSPECIFIED = 0;
  BROWSER_KIND_CHROMIUM = 1;
  BROWSER_KIND_FIREFOX = 2;
  BROWSER_KIND_SAFARI = 3;
  BROWSER_KIND_OTHER = 4;
}

enum WebRenderMode {
  WEB_RENDER_MODE_UNSPECIFIED = 0;
  RENDER_MODE_FULL = 1;          // WebGPU available, all NodeKinds compile fully
  RENDER_MODE_DEGRADED = 2;      // no WebGPU; GPU NodeKinds replaced with placeholders
  RENDER_MODE_RECOVERY = 3;      // recovery origin; restricted NodeKind set
}

enum WebRenderTreeErrorCode {
  WEB_RENDER_TREE_ERROR_CODE_UNSPECIFIED = 0;
  TREE_SIGNATURE_INVALID = 1;
  SCHEMA_VERSION_UNSUPPORTED = 2;
  UNKNOWN_NODE_KIND = 3;
  KIND_PAYLOAD_MISMATCH = 4;
  RECOVERY_KIND_FORBIDDEN_ON_WEB = 5;
  SURFACE_REF_NOT_RESOLVABLE = 6;
  ORIGIN_GROUP_MISMATCH = 7;
  WEBGPU_UNAVAILABLE = 8;
  THEME_NOT_LOADED = 9;
  WEB_RENDERER_INTERNAL = 10;
  CSP_VIOLATION = 11;
}

enum ExposureGrantErrorCode {
  EXPOSURE_GRANT_ERROR_CODE_UNSPECIFIED = 0;
  EXPOSURE_GRANT_NOT_APPROVED = 1;
  EXPOSURE_GRANT_INVALID_INTERFACE = 2;
  EXPOSURE_GRANT_CERT_UNAVAILABLE = 3;
  EXPOSURE_GRANT_REBIND_FAILED = 4;
  EXPOSURE_GRANT_FORBIDDEN_TRANSITION = 5;       // e.g., LAN → PUBLIC without LOOPBACK
  EXPOSURE_GRANT_PUBLIC_REQUIRES_RECOVERY = 6;
}

// ============================================================================
// Renderer info
// ============================================================================

message WebRendererCapabilities {
  bool webgpu_supported = 1;
  bool service_worker_supported = 2;
  bool indexeddb_supported = 3;
  uint32 max_concurrent_canvases = 4;
  uint64 client_storage_quota_bytes = 5;        // ephemeral; aggregate IDB + SW cache
  repeated string supported_visualization_kinds = 6;     // mirrors S7.2 GetSchemaInfoResponse
  repeated string supported_stream_kinds = 7;
}

message WebRendererInfo {
  string renderer_id = 1;                       // "web"
  string schema_version = 2;                    // "aios.renderer.web.v1alpha1"
  string ui_schema_version = 3;                 // "aios.ui.v1alpha1"
  string surface_schema_version = 4;            // "aios.surface.v1alpha1"
  string visual_schema_version = 5;             // "aios.visual.v1alpha1"
  WebExposureState current_exposure_state = 6;
  WebRenderMode current_mode = 7;
  BrowserKind active_browser_kind = 8;          // populated by client-side feature detection
  WebRendererCapabilities capabilities = 9;
}

// ============================================================================
// RenderTree
// ============================================================================

message RenderTreeRequest {
  bytes ui_tree_jcs = 1;                        // signed S7.2 UITree, JCS-canonical bytes
  string session_id = 2;
  string requesting_origin = 3;                 // browser origin observed; verified server-side
}

message RenderTreeResponse {
  oneof result {
    RenderTreeAccepted accepted = 1;
    RenderTreeError error = 2;
  }
}

message RenderTreeAccepted {
  string render_id = 1;                         // wrnd_<ulid>
  uint32 nodes_compiled = 2;
  uint32 nodes_dropped = 3;                     // recovery-only branches in normal mode, etc.
  uint32 nodes_placeholder = 4;                 // GPU NodeKinds replaced in degraded mode
  WebRenderMode mode = 5;
}

message RenderTreeError {
  WebRenderTreeErrorCode code = 1;
  string message = 2;
  string offending_node_id = 3;                 // empty for whole-tree errors
}

// ============================================================================
// Recovery page
// ============================================================================

message OpenRecoveryPageRequest {
  string session_id = 1;                        // optional; new recovery session created if empty
  string operator_subject_canonical_id = 2;
}

message OpenRecoveryPageResponse {
  oneof result {
    OpenRecoveryPageAccepted accepted = 1;
    string error_message = 2;
  }
}

message OpenRecoveryPageAccepted {
  string recovery_origin = 1;                   // "https://recovery.localhost:<port>"
  string evidence_record_id = 2;                // WEB_RECOVERY_PAGE_LOADED
}

// ============================================================================
// Custom element registration
// ============================================================================

message RegisterCustomElementRequest {
  string component_name = 1;                    // S7.3 ComponentName enum value name
  bool constitutional = 2;
  bytes recipe_jcs = 3;                         // signed S7.3 ComponentRecipe bytes
}

message RegisterCustomElementResponse {
  oneof result {
    string registered_tag = 1;                  // e.g., "aios-security-banner"
    string error_message = 2;
  }
}

// ============================================================================
// Exposure
// ============================================================================

message ExposureBindAddress {
  string interface_name = 1;
  string bind_address = 2;
  uint32 port = 3;
}

message GrantExposureRequest {
  WebExposureState target_state = 1;            // EXPOSURE_LAN or EXPOSURE_PUBLIC
  repeated ExposureBindAddress bind_addresses = 2;
  repeated string cidr_allow_list = 3;          // for PUBLIC, empty means deny-all by default
  string approval_evidence_record_id = 4;       // S3.1 evidence record proving approval
  bool persistent = 5;
}

message GrantExposureResponse {
  oneof result {
    GrantExposureAccepted accepted = 1;
    GrantExposureError error = 2;
  }
}

message GrantExposureAccepted {
  WebExposureState new_state = 1;
  repeated ExposureBindAddress active_bind_addresses = 2;
  string grant_evidence_record_id = 3;          // WEB_LAN_EXPOSURE_GRANTED or WEB_PUBLIC_EXPOSURE_GRANTED
}

message GrantExposureError {
  ExposureGrantErrorCode code = 1;
  string message = 2;
}

message RevokeExposureRequest {
  string reason = 1;
}

message RevokeExposureResponse {
  WebExposureState new_state = 1;               // always EXPOSURE_LOOPBACK on success
  string revoke_evidence_record_id = 2;         // WEB_EXPOSURE_REVOKED
}

// ============================================================================
// Renderer info RPC
// ============================================================================

message GetRendererInfoRequest {}
message GetRendererInfoResponse { WebRendererInfo info = 1; }

// ============================================================================
// Streaming exposure state
// ============================================================================

message StreamExposureStateRequest { string session_id = 1; }

message ExposureStateUpdate {
  google.protobuf.Timestamp at = 1;
  WebExposureState state = 2;
  repeated ExposureBindAddress active_bind_addresses = 3;
  string evidence_record_id = 4;                // last grant/revoke evidence id
}
```
