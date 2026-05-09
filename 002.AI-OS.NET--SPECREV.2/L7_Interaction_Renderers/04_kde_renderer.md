# KDE Plasma Renderer (Rev.2)

| Field          | Value                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| -------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Status         | `CONTRACT` (initial; written 2026-05-10)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| Phase tag      | S7.4                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| Layer          | L7 Interaction Renderers                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| Schema package | `aios.renderer.kde.v1alpha1`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| Consumes       | **Imports vocabulary from**: S5.1 identity (`recovery_mode` flag — type-level), S8.2 GPU Resource Model (per-group `VkDevice` shape, dmabuf brokering capability schema, `gpu_capability_class` closed enum — type-level shape co-defined with L8; renderer requests a `GpuCapabilityBinding` at runtime), L0 INV-019..INV-022 (closed-id reference). **Peer (intra-L7)**: S7.1 Surface + Composition (KDE column of §4.3 mapping), S7.2 Shared UI Schema (closed `NodeKind` 19 values), S7.3 Visual Language (KDE mapping §8.1). **Note (architectural)**: S8.2 is a higher-numbered-layer reference at L8; vocabulary import only — could be relocated to a cross-cutting GPU-capability contract in W12+. |
| Produces       | typed KDE renderer service contract; KWin Wayland integration protocol; deterministic compilation rules from S7.2 `NodeKind` to Qt/QML primitives; visual token compilation per S7.3 §8.1; recovery shell rendering rules                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |

## 1. Purpose

S7.1 fixes _what surfaces are_ and _which zones they may live in_. S7.2 fixes _what an AIOS UI tree is structurally_. S7.3 fixes _which semantic tokens every renderer reads_. None of those specs commit to a concrete renderer.

This spec is the **KDE renderer**: the binding contract that says, when AIOS UI is rendered on a KDE Plasma session, **this is exactly what compiles to what**. It defines:

1. The exact set of Wayland protocols the renderer speaks and how each S7.1 `CompositionZone` maps onto KWin's compositor primitives.
2. The deterministic compilation table from every S7.2 `NodeKind` (19 values) to a Qt/QML primitive — including the GPU-bearing kinds (`VISUALIZATION`, `STREAM`, `SURFACE_EMBED`) which compile through `wgpu` against the per-group `VkDevice` issued by S8.2.
3. How S7.3 visual tokens become `QPalette`, `QFont`, `QPropertyAnimation`, `KIconLoader` lookups, and `QML Component` recipes.
4. The KDE Plasma platform-integration surface (KRunner, Plasma widgets, KDE notifications, system tray, KWin scripting) the renderer is allowed to touch — and what it is **not** allowed to touch.
5. The recovery-shell rendering rules — a separate KWin session/layer with no `APP_SURFACE`/`STREAM_SURFACE`, locked to the AIOS_RECOVERY theme.
6. The renderer's adversarial robustness against KWin scripting injection, Wayland protocol forgery, Qt theme override, chrome impersonation, and recovery escape.

Out of scope: any concrete Qt code, QML source, or KWin script source; any concrete hex colors, font names, or icon glyphs (those belong to L7.3 stage 3); any other renderer (Web, CLI, Voice, Mobile have their own contracts).

The KDE renderer **does not** reimplement compositing. AIOS does not write its own Wayland compositor. KWin remains the compositor; the AIOS KDE renderer is a Wayland client + a small set of KWin scripts + a signed asset bundle — all sitting between AIOS and KWin.

## 2. Core invariants

- **I1 — Renderer owns no authoritative state.** The KDE renderer is a target binding. Authoritative state (objects, evidence, policy decisions, sandbox profiles, surfaces, UI trees, visual tokens) lives in AIOS-FS, the policy kernel, the surface service, the UI schema service, and the visual language service. The renderer reads; it does not become a source of truth.
- **I2 — Trust-bearing UI rides the overlay layer.** Every AIOS chrome element (`SECURITY_INDICATOR`, `APPROVAL_PROMPT`, `EVIDENCE_LINK`, recovery banner) compiles into a `wlr-layer-shell` surface on the `overlay` layer, with `keyboard-interactivity = on-demand` and `exclusive-zone = 0`. The overlay layer is the only Wayland layer that survives fullscreen — this is how INV-020 is enforced at the compositor.
- **I3 — Cross-group `VkDevice` isolation.** Every `APP_SURFACE`, `STREAM_SURFACE`, `VISUALIZATION` payload, and `wgpu` canvas is bound to the per-group `VkDevice` issued by S8.2. The renderer never opens a Vulkan instance directly; it requests a `GpuCapabilityBinding` from S8.2 and uses the device handle the binding provides. dmabuf import calls are checked against the binding's authorized peer set.
- **I4 — `APP_SURFACE` cannot promote to `CHROME` zone.** The renderer parses the requested `wlr-layer-shell` namespace at surface creation and refuses any `overlay` layer claim from a non-AIOS-chrome client. Only the AIOS chrome service's signed identity is permitted on the `overlay` layer. Rejected attempts emit `KDE_LAYER_SHELL_REJECTED` evidence (per §10).
- **I5 — Recovery shell is a separate compositor session.** A recovery-mode session runs as a distinct KWin Wayland session (separate `WAYLAND_DISPLAY`, separate KWin process, separate user) and exclusively renders `AIOS_SURFACE` instances. `APP_SURFACE` and `STREAM_SURFACE` creation is rejected at the surface service (S7.1 §I6); the renderer never sees those kinds during recovery. Recovery surfaces are also blocked from being composed alongside normal-mode surfaces — the recovery KWin session does not accept clients outside the recovery shell.
- **I6 — Constitutional icons load from a root-signed asset bundle.** `ICON_CONSTITUTIONAL_*` glyphs (S7.3 §4.5) are loaded via `KIconLoader` from `/aios/system/themes/<theme_id>/icons/` whose contents are content-addressed and AIOS-root-signed. The renderer never falls back to the system icon theme (Plasma's `breeze`, `oxygen`, etc.) for constitutional icons. If the signed bundle is absent or its hashes mismatch, the renderer enters degraded fallback (§I7).
- **I7 — Fail-closed degraded fallback.** When KWin or Wayland is unavailable, when `wgpu` device acquisition fails, or when the constitutional icon bundle fails verification, the renderer enters **text-only degraded mode**: it renders `AIOS_SURFACE` content as plain Qt `QLabel`/`QListView` widgets without GPU-accelerated chrome, without theme animations, with the recovery shield prominently shown, and emits `KDE_RENDERER_DEGRADED` evidence (FOREVER retention; degradation is a tamper-relevant event). It does not silently render with diminished trust indicators.
- **I8 — KWin scripts are signed.** Any KWin script the renderer loads at session start (e.g., per-window AIOS chrome injection, fullscreen-promotion-block) is loaded only from `/aios/system/renderers/kde/kwin-scripts/` whose contents are AIOS-root-signed. Scripts from `~/.local/share/kwin/scripts/` or system-installed locations are never loaded by the AIOS renderer.
- **I9 — Theme override priority is AIOS-bound.** The Plasma global theme governs Plasma's own surfaces. The AIOS theme (per S7.3) governs AIOS surfaces and overrides Plasma global tokens for those surfaces only. The renderer applies AIOS tokens via `QQuickStyle` overrides scoped to the `AIOSWindow` QML component tree; Plasma's tokens cannot leak into AIOS chrome.

## 3. KWin integration protocol

### 3.1 Wayland protocols used

The renderer is a Wayland client; it speaks the following protocols:

| Protocol                             | Purpose in the renderer                                                                                                                      |
| ------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------- |
| `wl_compositor`                      | Surface allocation                                                                                                                           |
| `xdg-shell`                          | Top-level windows for `CONTENT`-zone `AIOS_SURFACE` instances and host windows                                                               |
| `wlr-layer-shell-unstable-v1`        | `BACKGROUND` / `OVERLAY` / `CHROME` zones bound to `background` / `top` / `overlay` layers respectively (S7.1 §4.3 KDE column)               |
| `wp-presentation-time`               | Frame timing measurement; deadline-miss detection emits `KDE_FRAME_DROPPED`                                                                  |
| `linux-dmabuf-v1`                    | dmabuf import for `APP_SURFACE` (game/wgpu app textures) and `STREAM_SURFACE` (decoded video frames)                                         |
| `xdg-output-unstable-v1`             | Per-monitor logical geometry (resolution, scale factor) for multi-monitor topology (§3.4)                                                    |
| `wlr-foreign-toplevel-management-v1` | Detection and overlay of non-AIOS top-level windows (legacy Linux apps not running through AIOS Capability Runtime); used for chrome overlay |
| `wp-fractional-scale-v1`             | Per-surface fractional scaling (`Dimensions.scale_factor` from S7.1)                                                                         |
| `wp-viewporter`                      | Surface viewport/cropping for `STREAM` payload subsurfaces                                                                                   |

The renderer **does not** speak any KWin-private extension. KWin-side AIOS-specific behavior (e.g., per-window chrome injection) is implemented as **KWin scripts**, not as new Wayland protocols. This keeps the KDE-side change surface auditable and upgrade-safe.

### 3.2 `CompositionZone` → KWin layer mapping (canonical)

| `CompositionZone` | KWin / Wayland primitive                                                                                | Z-order semantics                                                                              |
| ----------------- | ------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------- |
| `BACKGROUND`      | `wlr-layer-shell` `background` layer; `exclusive-zone = 0`; `keyboard-interactivity = none`             | Below all xdg-shell windows                                                                    |
| `CONTENT`         | `xdg-shell` top-level windows for `AIOS_SURFACE`; `xdg-shell` + `wl_subsurface` for `APP_SURFACE` hosts | Normal stacking; KWin manages within the layer                                                 |
| `OVERLAY`         | `wlr-layer-shell` `top` layer; `exclusive-zone = 0`; `keyboard-interactivity = on-demand`               | Above `CONTENT`, below `CHROME`                                                                |
| `CHROME`          | `wlr-layer-shell` `overlay` layer; `exclusive-zone = 0`; `keyboard-interactivity = on-demand`           | Always topmost — survives `xdg-shell` fullscreen (this is why INV-020 holds during fullscreen) |

Per-zone allowed `SurfaceKind` is enforced at the surface service (S7.1 §4.1); the renderer does not re-enforce, but it does **observe** — a request to create a Wayland surface on the `overlay` layer from a non-AIOS-chrome client identity is rejected at the renderer with `KDE_LAYER_SHELL_REJECTED` evidence and never reaches KWin.

### 3.3 KWin scripting hooks

The AIOS KDE renderer ships a small bundle of signed KWin scripts loaded at session start:

- **`aios-fullscreen-block`** — when a normal-mode application enters fullscreen, ensures the `overlay`-layer chrome surfaces remain visible. Without this script, KWin's default fullscreen behavior may attempt to claim the entire output; with it, the AIOS chrome `overlay` layer continues to receive frames.
- **`aios-foreign-toplevel-overlay`** — for legacy Linux applications running outside AIOS Capability Runtime (loaded via `wlr-foreign-toplevel-management-v1`), detects their top-level windows and ensures the AIOS chrome overlay covers the appropriate canonical chrome rectangles, defending S7.1 §12.4 chrome impersonation.
- **`aios-recovery-shell-isolate`** — when KWin starts under the recovery user, refuses to accept clients from outside the recovery shell process tree (defense in depth around I5).

These scripts are loaded only from the signed path (`/aios/system/renderers/kde/kwin-scripts/`); KWin's standard scripting path (`~/.local/share/kwin/scripts/`) is not consulted by the renderer.

### 3.4 Multi-monitor handling

In rev.2, each connected monitor has its own zone stack (`BACKGROUND` / `CONTENT` / `OVERLAY` / `CHROME`). The renderer subscribes to `xdg-output-unstable-v1` and:

- Maintains one chrome `overlay` surface per monitor.
- Allows `CONTENT` surfaces to migrate between monitors via user gesture (KWin handles the move; the renderer follows the new `xdg_output`).
- On hot-plug add: the new monitor receives a fresh chrome `overlay` surface within 200 ms (p95).
- On hot-plug remove: surfaces are migrated to the primary monitor (KWin policy); the renderer recomposes within 500 ms (p95).

Surface-to-monitor pinning, surface-spans-multiple-monitors, and per-monitor theme variants are deferred (§14).

## 4. `NodeKind` → Qt/QML compilation table

Every value of S7.2's closed `NodeKind` enum (19 declared kinds plus `NODE_KIND_UNSPECIFIED`) compiles to a deterministic Qt/QML primitive. The renderer rejects unknown kinds with `KdeUnknownNodeKind` and emits `KDE_RENDER_FAILED` evidence — this exists in addition to S7.2's `UnknownNodeKind` (the renderer is the place where the compilation failure becomes concrete).

| `NodeKind`           | Qt / QML primitive                                                                                                                                                                                         | Notes                                                                                                                        |
| -------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------- |
| `CONTAINER`          | `Item { }` in QML; `QHBoxLayout` / `QVBoxLayout` / `QGridLayout` for `STACK_HORIZONTAL` / `STACK_VERTICAL` / `GRID`                                                                                        | `LayoutMode` selects the layout class                                                                                        |
| `DIVIDER`            | `Frame { }` with `HLine` or `VLine` shape                                                                                                                                                                  | Spacing / color from S7.3 tokens                                                                                             |
| `SPACER`             | `Item { }` with `Layout.preferredWidth` / `Layout.preferredHeight` or `Layout.fillWidth: true` for `flexible = true`                                                                                       | Honors `SPACING_*` tokens                                                                                                    |
| `TEXT`               | `Label { }` (Qt Quick Controls) or `QLabel`                                                                                                                                                                | `emphasis` → italic; `strong` → bold; typography token from S7.3 `TYPOGRAPHY_BODY_*`                                         |
| `HEADING`            | `Label { }` with `font` overridden by `TYPOGRAPHY_HEADING_LG/MD/SM` based on `level`                                                                                                                       | `level 1..6` mapped: 1→DISPLAY_LG, 2→HEADING_LG, 3→HEADING_MD, 4→HEADING_SM, 5/6→BODY_LG with bold weight                    |
| `INLINE_CODE`        | `Label { }` with monospace family; `TYPOGRAPHY_CODE_SM` token                                                                                                                                              | Inline within parent text flow                                                                                               |
| `CODE_BLOCK`         | `QPlainTextEdit` (read-only) wrapped in `QQuickWidget`, with `KSyntaxHighlighting::Repository` for `language_hint`                                                                                         | Read-only; selection enabled; copy supported; no execution affordance                                                        |
| `CARD`               | `QFrame { frameShape: StyledPanel }` with shadow via `QGraphicsDropShadowEffect`; QML `Pane` / custom `AIOSCard`                                                                                           | Title bound to `CardPayload.title` via `Label`; children compose into the card body                                          |
| `LIST`               | `QListView` with `QAbstractListModel` delegate; QML `ListView`                                                                                                                                             | `view_ref` source binds to S2.1 view via a Qt model adapter; `inline` source uses a static model                             |
| `TABLE`              | `QTableView` with `QAbstractTableModel`; sortable headers honor `TableColumn.sortable`                                                                                                                     | `view_ref` binds to S2.1 view; pagination uses S2.1 cursor semantics                                                         |
| `FORM`               | `QFormLayout` with field-kind-specific widgets; submit button bound to `action_template_ref`                                                                                                               | `FIELD_*` enum drives widget choice (see §4.1); submit constructs S0.1 envelope                                              |
| `ACTION_BUTTON`      | `QPushButton` (or QML `Button`); `destructive = true` triggers `MOTION_DURATION_DELIBERATE` + danger color                                                                                                 | Click submits S0.1 envelope keyed to `action_template_ref`                                                                   |
| `VISUALIZATION`      | `QQuickItem` subclass embedding a `wgpu` render target via `QRhiTexture` handoff; backed by per-group `VkDevice`                                                                                           | `wgpu` Rust crate compiled with `vulkan` backend on Linux; same source code targets `wasm32 + WebGPU` for the Web renderer   |
| `STREAM`             | GStreamer pipeline → `dmabuf` → `wl_subsurface`; hardware decode where available                                                                                                                           | `STREAM` rejected in recovery mode at surface service (S7.1 §3.3); the renderer never sees recovery-mode streams             |
| `SURFACE_EMBED`      | `wl_subsurface` parented to the host `xdg-shell` window; the embedded `surface_id` resolves to an S7.1 `APP_SURFACE`                                                                                       | The app's own renderer draws into the dmabuf-backed texture; the AIOS renderer composites and overlays chrome                |
| `SECURITY_INDICATOR` | QML `AIOSSecurityIndicator` component on `wlr-layer-shell` `overlay` layer; subject + action + evidence link rendered                                                                                      | `is_trust_bearing = true`; AI subjects cannot author this kind (refused at S7.2 signing)                                     |
| `APPROVAL_PROMPT`    | `KDialog`-shaped QML `AIOSApprovalDialog` with `request_hash` displayed in monospace; modal where `ApprovalChoice` requires synchronous decision; non-modal otherwise                                      | Approval bound per S0.1 §4 binding rules                                                                                     |
| `EVIDENCE_LINK`      | `QPushButton` with link icon (`ICON_CONSTITUTIONAL_EVIDENCE_CHAIN`); click opens evidence viewer in a new `CONTENT` surface                                                                                | The viewer is itself an `AIOS_SURFACE` rendering an evidence record subtree                                                  |
| `AGENT_MESSAGE`      | QML `AIOSAgentMessage` component bound to the `COMPONENT_AGENT_MESSAGE` recipe (S7.3 §4.6); INV-021 distinct treatment via `COLOR_ACTION_AI` + `TYPOGRAPHY_AI_ORIGIN` + `ICON_CONSTITUTIONAL_AI_INDICATOR` | All three axes (hue, typography, pattern) are present per S7.3 §7.2; the renderer applies them at compile time, not at theme |

### 4.1 `FormFieldKind` → Qt widget table

S7.2's closed `FormFieldKind` enum compiles to:

| `FormFieldKind`        | Qt widget                                                                                   |
| ---------------------- | ------------------------------------------------------------------------------------------- |
| `FIELD_TEXT`           | `QLineEdit`                                                                                 |
| `FIELD_TEXT_MULTILINE` | `QPlainTextEdit`                                                                            |
| `FIELD_NUMBER`         | `QSpinBox` / `QDoubleSpinBox` (renderer chooses based on payload metadata; defaults to int) |
| `FIELD_BOOLEAN`        | `QCheckBox`                                                                                 |
| `FIELD_ENUM`           | `QComboBox` populated from `enum_choices`                                                   |
| `FIELD_OBJECT_REF`     | `AIOSObjectPicker` QML component (S1.3 object id picker; queries S2.1 for namespace scope)  |
| `FIELD_DATE`           | `QDateEdit`                                                                                 |

### 4.2 `AccessibilityRole` → `QAccessible::Role`

S7.2's `AccessibilityRole` maps onto Qt's `QAccessible::Role` enumeration (e.g., `ROLE_BUTTON` → `QAccessible::Button`, `ROLE_DIALOG` → `QAccessible::Dialog`). Mapping is 1:1 for all 18 values. Screen readers (Orca on Linux) read the `accessibility_label` and `accessibility_description`.

### 4.3 Compilation result vocabulary

```proto
enum KdeCompilationResultCode {
  KDE_COMPILATION_RESULT_CODE_UNSPECIFIED = 0;
  KDE_COMPILED = 1;
  KDE_UNKNOWN_NODE_KIND = 2;
  KDE_KIND_NOT_SUPPORTED_IN_RECOVERY = 3;
  KDE_GPU_BINDING_UNAVAILABLE = 4;
  KDE_DMABUF_IMPORT_FAILED = 5;
  KDE_LAYER_SHELL_REJECTED = 6;
  KDE_KWIN_SCRIPT_REJECTED = 7;
  KDE_QML_LOAD_FAILED = 8;
  KDE_THEME_TOKEN_UNRESOLVED = 9;
  KDE_FOREIGN_TOPLEVEL_DETECTED = 10;
  KDE_RENDERER_INTERNAL = 11;
}
```

Closed enum — 11 declared values. Adding a value requires a versioned spec change.

## 5. Visual token compilation (S7.3 §8.1 binding)

The renderer materializes the active S7.3 `Theme` into Qt and QML primitives at theme load. Token tables resolve once at load (cached) and re-resolve on `SwitchTheme`.

| Token family       | Compilation                                                                                                                                                                                                                                 |
| ------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `ColorTokenName`   | `QPalette` entries on the AIOS root QML scope; QML `PropertyChanges` apply per-component overrides. Specific role mapping: `COLOR_TEXT_PRIMARY` → `QPalette::WindowText` (AIOS scope), `COLOR_SURFACE_CONTENT` → `QPalette::Base`, etc.     |
| `TypographyToken`  | `QFont(family, size, weight)` registered in `QQuickStyle` overrides; `italic` toggles `QFont::setItalic`; `letter_spacing_em` maps to `QFont::setLetterSpacing(QFont::PercentageSpacing, ...)`; `line_height_multiple` applied at QML layer |
| `SpacingTokenName` | Resolved at theme load into pixel values per current scale factor; bound to `Layout.margins` / `Layout.spacing` / `Layout.preferredHeight` for SPACER nodes; CLI ignores spacing — KDE honors it                                            |
| `MotionTokenName`  | `QPropertyAnimation` `duration` and `easingCurve` per token; `MOTION_DURATION_INSTANT` always compiles to `duration = 0`; `MOTION_DURATION_DELIBERATE` triggers the destructive-action confirmation cadence                                 |
| `IconTokenName`    | `KIconLoader::loadIcon(name)` against the AIOS theme bundle path; constitutional icons (`ICON_CONSTITUTIONAL_*`) load only from `/aios/system/themes/<theme_id>/icons/` with content-hash verification. System icon themes never consulted  |
| `ComponentName`    | QML `Component` templates per `ComponentRecipe`; constitutional components (`COMPONENT_SECURITY_BANNER`, `COMPONENT_RECOVERY_SHIELD`, etc.) ship with the renderer and are not theme-overridable                                            |

### 5.1 Scope of override

AIOS theme tokens override Plasma global theme tokens **only within AIOS surfaces**. The renderer applies overrides via a custom `QQuickStyle` that is scoped to the `AIOSWindow` QML root component tree. Plasma's own surfaces (the panel, the kickoff menu, system-tray applets that are not AIOS-authored) continue to use Plasma's global theme. This binds the I9 priority rule.

### 5.2 Reduced motion

The renderer honors KDE's reduced-motion accessibility setting (`KGlobalSettings::animationsEnabled()`). When reduced motion is active and the theme's `MotionValue.reduced_motion_zero = true`, all motion durations collapse to 0 ms, with the constitutional exception that `MOTION_DURATION_INSTANT` is always 0 anyway and `MOTION_DURATION_DELIBERATE` for destructive operations is preserved at its theme-supplied value (a deliberate slowdown that the user opted into via the destructive flag is not an animation in the accessibility sense).

## 6. KDE Plasma platform integration

### 6.1 KRunner integration

The renderer ships a KRunner plugin (`aios-runner`) that activates when the user opens KRunner (default `Alt+Space`). Typed input is treated as an AIOS goal; pressing Enter creates an `AIOS_SURFACE` in the `CONTENT` zone with the goal-entry UI tree (a `FORM` node bound to the canonical "submit goal" S0.1 action template). KRunner's other native runners (file search, calculator, command run) remain available.

The plugin runs in the user's KRunner process, but the goal-entry tree is signed by AIOS and the form submission goes through the standard `Capability Runtime` lifecycle. KRunner does not gain elevated privilege from this.

### 6.2 Plasma widget (system tray)

The renderer ships a Plasma applet (`aios-tray`) that:

- Always present in the system tray when an AIOS session is active (this is constitutional per INV-020 — chrome is always visible; the tray applet is one chrome anchor).
- Shows the active subject's `COLOR_ACTION_HUMAN`/`COLOR_ACTION_AI`/`COLOR_ACTION_RECOVERY` indicator at a glance.
- Click opens an `AIOS_SURFACE` mini-dashboard (recent actions, pending approvals, evidence link).
- Recovery-mode tray applet uses the recovery aesthetic — a different glyph, different color, different prefix (`[RECOVERY]`).

### 6.3 KDE notifications

Notifications dispatched by AIOS use `org.freedesktop.Notifications` for portability and `KNotification` (KDE's richer notification API) when richer rendering is needed (interactive buttons, persistent stickers, evidence-link buttons). All notifications carry an `AIOS:` source prefix and the active subject's badge.

Notification rendering compiles into the `OVERLAY` zone (above `CONTENT`, below `CHROME`). Notifications cannot promote to the `CHROME` zone — only the constitutional `SECURITY_INDICATOR` / `EVIDENCE_LINK` chrome elements live there.

### 6.4 KDE Activities

The renderer optionally maps each KDE Activity to an AIOS context (e.g., `family-life` Activity ↔ `family` group as primary; `homelab-work` Activity ↔ `homelab` group as primary). When the user switches Activities, the AIOS session's `primary_group_id` follows. This is a convenience binding, not a policy binding — the policy kernel still evaluates the cross-group permissions; switching Activities does not grant access.

### 6.5 Plasma global theme

The renderer reads but does not write Plasma's global theme. Plasma's theme governs Plasma surfaces; AIOS theme governs AIOS surfaces. The two coexist; AIOS scope is enforced via §5.1.

## 7. Recovery shell rendering

### 7.1 Separate KWin session

A recovery shell is a distinct compositor session — separate KWin process, separate `WAYLAND_DISPLAY` socket, separate user (system-managed `_recovery` user; not the operator's regular user). The recovery shell is reached via the recovery boot path defined in L1 (and is not subject to the cognitive core).

The recovery KWin session loads only the renderer's recovery components: chrome, recovery shell content, operator-credential dialog. KWin under the recovery user refuses clients from outside the recovery shell process tree (KWin script `aios-recovery-shell-isolate`, §3.3).

### 7.2 Auto-activated AIOS_RECOVERY theme

On recovery shell start, the renderer activates `theme_aios_recovery` (S7.3 §4.7 / §6.3). The user **cannot** switch theme during recovery; the recovery theme is constitutional and root-signed. Recovery theme tokens satisfy §7.3 cross-theme distinctness from every other available theme.

The recovery aesthetic includes:

- `COLOR_BOUNDARY_RECOVERY` as a distinct accent applied to chrome, banner, and the recovery shield component.
- `TYPOGRAPHY_RECOVERY` as the body and heading typeface — a different family from the default theme so even monochrome displays can distinguish recovery from normal.
- `ICON_CONSTITUTIONAL_RECOVERY_LOCK` prominent in chrome.
- `COMPONENT_RECOVERY_SHIELD` rendered persistently in the chrome zone alongside `COMPONENT_SECURITY_BANNER`.

### 7.3 Surface kind constraints

`APP_SURFACE`, `STREAM_SURFACE`, `SURFACE_EMBED` are rejected at the surface service (S7.1 §I6) for recovery-mode subjects. If the renderer receives a UI tree containing those kinds during recovery — which should never happen if S7.2 validation is correct, but defense in depth — the renderer rejects the tree at compilation with `KDE_KIND_NOT_SUPPORTED_IN_RECOVERY` and emits `KDE_RECOVERY_KIND_REJECTED_AT_RENDERER` evidence (FOREVER retention; queued for S3.1 consolidation).

### 7.4 Operator credential entry

Operator credential entry uses a dedicated recovery dialog rendered as a `QDialog` running in a recovery-shell-private process; the dialog does not run inside the operator's regular Plasma session. This is consistent with the L1 + L4 recovery boundary — credential prompts in recovery are isolated from any normal-mode subject's process tree.

## 8. Performance contract

| Operation                                                                       | p50      | p95      | p99      | Hard timeout  |
| ------------------------------------------------------------------------------- | -------- | -------- | -------- | ------------- |
| Per-frame compositor pass at 60 Hz (renderer-side cost; KWin owns its own)      | < 2 ms   | < 6 ms   | < 12 ms  | 16 ms (60 Hz) |
| Schema-tree-to-Qt compilation (≤ 1 000 nodes)                                   | < 20 ms  | < 50 ms  | < 200 ms | 2 s           |
| Schema-tree-to-Qt compilation (≤ 10 000 nodes)                                  | < 100 ms | < 200 ms | < 800 ms | 5 s           |
| KRunner search response (goal-entry surface ready to type)                      | < 50 ms  | < 100 ms | < 200 ms | 2 s           |
| `KDialog` approval prompt show (from `APPROVAL_PROMPT` node arrival to visible) | < 100 ms | < 200 ms | < 500 ms | 2 s           |
| Recovery shell first paint (from session start to chrome visible)               | < 1 s    | < 2 s    | < 5 s    | 10 s          |
| Theme switch (`SwitchTheme` end-to-end, cached tokens)                          | < 100 ms | < 300 ms | < 1 s    | 5 s           |
| Hot-plug monitor add (chrome surface ready on new monitor)                      | < 100 ms | < 200 ms | < 500 ms | 2 s           |

Frame budget violations emit `KDE_FRAME_DROPPED` evidence (rate-limited per §10) with a severity classification. Persistent violations transition the renderer to degraded mode.

Failure modes — all fail closed:

- `KDE_GPU_BINDING_UNAVAILABLE` → the relevant surface stays `DRAFT`; the user sees a "GPU capability unavailable" placeholder; chrome remains operational.
- `KDE_QML_LOAD_FAILED` → the affected subtree fails to render; the rest of the tree continues; `KDE_RENDER_FAILED` evidence emitted.
- `KDE_THEME_TOKEN_UNRESOLVED` → fallback token used; `KDE_TOKEN_FALLBACK_USED` warning evidence (STANDARD_24M).
- KWin or Wayland disconnect → `KDE_RENDERER_DEGRADED` (§I7); recovery instructions shown; `KDE_RENDERER_DEGRADED` evidence (FOREVER retention).

## 9. Adversarial robustness

### 9.1 App spawning a top-level that mimics AIOS chrome

A Linux application running as `APP_SURFACE` could attempt to spawn its own `xdg-shell` top-level with content visually mimicking AIOS chrome. The KWin script `aios-foreign-toplevel-overlay` (loaded via `wlr-foreign-toplevel-management-v1`) detects every non-AIOS top-level and ensures the AIOS chrome `overlay` layer covers the canonical chrome rectangles regardless of the imitator's window content. Constitutional chrome wins on the overlay layer.

### 9.2 Wayland protocol forgery — fake `wlr-layer-shell` `overlay` claim

A malicious client connects to KWin and requests an `wlr-layer-shell` surface on the `overlay` layer. The AIOS chrome service refuses any `overlay` layer claim from a client identity that is not the AIOS chrome service itself (verified via the Wayland client's authenticated PID + AIOS subject mapping). The rejected attempt emits `KDE_LAYER_SHELL_REJECTED` evidence with the offending subject id (FOREVER retention; cross-zone violation is tamper-relevant).

### 9.3 KWin scripting injection

A malicious user-installed KWin script in `~/.local/share/kwin/scripts/` or a system path attempts to inject code that affects AIOS surfaces. The renderer **does not load** any KWin script outside `/aios/system/renderers/kde/kwin-scripts/` (per I8). Detection of an unauthorized script in the KWin scripting directory at session start emits `KDE_KWIN_SCRIPT_REJECTED` evidence.

The renderer cannot prevent users from running unauthorized KWin scripts in their normal Plasma session — KDE's scripting mechanism is user-controlled. What the renderer guarantees is that AIOS chrome surfaces and AIOS recovery surfaces are not accessible to those scripts: chrome lives on the `overlay` layer which is not exposed to user-script manipulation, and the recovery shell runs under a separate KWin session.

### 9.4 Qt theme override that violates INV-021

A user installs a Qt theme (via `qt5ct` or `qt6ct`) that visually unifies AI and human action coloring, attempting to defeat the AI/human distinction. The renderer's AIOS theme application (§5.1) overrides Plasma global theme **and** any external Qt theme for AIOS surfaces. AIOS chrome and `AGENT_MESSAGE` rendering use the AIOS-resolved tokens directly; the user's Qt theme cannot bleed into AIOS surfaces.

If a user theme attempts to install token values that would loosen INV-021 distinction, the theme is rejected at S7.3 theme validation (`ThemeInvariantViolated`); it never reaches the renderer.

### 9.5 Recovery escape via faked normal-mode chrome

A normal-mode application attempts to render a fake recovery-mode banner to confuse the operator into believing the system is in recovery. The fake banner is an `APP_SURFACE` content; the AIOS chrome `overlay` layer always renders **above** any `APP_SURFACE`, and the chrome shows the actual session's recovery state (which is `false` for a normal session). The fake banner is overlaid by the real chrome and its claimed recovery status is contradicted at the constitutional chrome layer.

Additionally, the recovery shell runs as a separate KWin session (§I5); a normal-mode application cannot reach the recovery shell's surfaces or its KWin instance — there is no shared compositor where a normal-mode fake recovery surface could appear in the same screen as the real recovery shell.

### 9.6 dmabuf import from a wrong-group peer

An `APP_SURFACE` in group A receives a dmabuf handle from a peer in group B (e.g., via a misconfigured IPC). The renderer's dmabuf import path checks the dmabuf's originating `VkDevice` against the surface's `GpuCapabilityBinding` authorized peer set (S8.2). Mismatch fails the import with `KDE_DMABUF_IMPORT_FAILED` and emits `CROSS_SURFACE_READ_DENIED` evidence (S7.1 §9; FOREVER retention).

### 9.7 Constitutional icon hash mismatch at runtime

Disk-resident constitutional icon assets are mutated by an attacker between theme load and surface render. Each icon load through `KIconLoader` re-verifies the content hash for `ICON_CONSTITUTIONAL_*` tokens against the theme bundle's recorded hash; mismatch triggers degraded mode (§I7) and `KDE_RENDERER_DEGRADED` evidence with the specific icon name.

### 9.8 Renderer-impersonation attack

A non-AIOS process attempts to advertise itself as the AIOS KDE renderer to KWin (e.g., to claim the `overlay` layer or load AIOS-signed KWin scripts). Renderer identity is established via the renderer's identity-service-issued signing key (per S5.1); the AIOS chrome service's Wayland client connection is authenticated with this key. KWin scripts are loaded only from the AIOS-root-signed path. There is no "claim to be the renderer" affordance.

## 10. Cross-spec dependencies

| Spec       | Direction         | What this spec contributes / consumes                                                                                                                                      |
| ---------- | ----------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| S7.1       | consumer          | Compiles `Surface` model into KWin Wayland primitives per §4.3 KDE column; honors `CompositionZone` z-order; honors recovery-mode kind restriction                         |
| S7.2       | consumer          | Compiles every `NodeKind` (19 values) to Qt/QML primitives per §4 of this spec; honors `is_ai_origin` / `is_trust_bearing` / `recovery_only` flags via L7.3 token bindings |
| S7.3       | consumer          | Realizes §8.1 KDE token mapping; constitutional component recipes load as QML `Component` templates; AIOS theme overrides scoped per §5.1                                  |
| S8.2       | consumer          | Per-group `VkDevice` from `GpuCapabilityBinding`; dmabuf import authorized-peer set; pixel-readback isolation enforced at the device layer                                 |
| S5.1       | consumer          | `subject.recovery_mode` drives recovery-shell session selection; `subject.kind` drives chrome AI/human badge (paired with S7.3 INV-021 tokens)                             |
| S0.1       | consumer          | Form / `ACTION_BUTTON` / `APPROVAL_PROMPT` submissions construct S0.1 action envelopes with the active session subject as `submitter`                                      |
| S2.1       | consumer          | `LIST` / `TABLE` `view_ref` binds to S2.1 query views via Qt model adapters; cursor pagination per S2.1 §11.3                                                              |
| S3.1       | producer (queued) | New record types listed below to be folded into S3.1's vocabulary at the next S3.1 consolidation cycle                                                                     |
| L0 INV-020 | enforcer          | Chrome on `wlr-layer-shell` `overlay` layer is always topmost (KWin-enforced); KWin script `aios-fullscreen-block` defends fullscreen claim                                |
| L0 INV-021 | enforcer          | All three S7.3 axes (hue, typography, pattern) materialized in QML for `AGENT_MESSAGE` and `is_ai_origin = true` nodes; AIOS theme overrides Plasma + external Qt themes   |
| L0 INV-022 | enforcer          | Recovery shell as separate KWin session; AIOS_RECOVERY theme auto-activated; constitutional component `COMPONENT_RECOVERY_SHIELD` always present                           |

### 10.1 Queued S3.1 record-type touch-ups

The following record types are queued for the next S3.1 consolidation cycle (do not touch S3.1 in this contract):

| Record type                              | Retention class | Carries                                                                                                   |
| ---------------------------------------- | --------------- | --------------------------------------------------------------------------------------------------------- |
| `KDE_RENDERER_STARTED`                   | STANDARD_24M    | renderer build hash, schema versions consumed (`aios.surface.v1alpha1`, `aios.ui.v1alpha1`, …)            |
| `KDE_RENDERER_DEGRADED`                  | FOREVER         | reason class (`kwin_unreachable` / `wgpu_init_failed` / `constitutional_icon_hash_mismatch` / …), context |
| `KDE_FRAME_DROPPED`                      | STANDARD_24M    | severity class (occasional/persistent), `zone`, frame deadline overage bucket                             |
| `KDE_LAYER_SHELL_REJECTED`               | FOREVER         | offending client subject id, attempted layer (`overlay` / `top` / `background`)                           |
| `KDE_KWIN_SCRIPT_LOADED`                 | STANDARD_24M    | script name, content hash                                                                                 |
| `KDE_KWIN_SCRIPT_REJECTED`               | FOREVER         | rejected script source path, reason (`unsigned_path` / `hash_mismatch`)                                   |
| `KDE_RECOVERY_SHELL_STARTED`             | FOREVER         | recovery operator subject id, session boot id                                                             |
| `KDE_RECOVERY_KIND_REJECTED_AT_RENDERER` | FOREVER         | offending tree id, attempted `NodeKind`, attempted `SurfaceKind`                                          |
| `KDE_PLASMA_THEME_OVERRIDDEN`            | STANDARD_24M    | scope (`aios_window_tree`), Plasma global theme id, AIOS theme id                                         |
| `KDE_RENDER_FAILED`                      | EXTENDED_60M    | tree id, offending node id, `KdeCompilationResultCode`                                                    |
| `KDE_TOKEN_FALLBACK_USED`                | STANDARD_24M    | token name, token family                                                                                  |

All records carry `namespace_scope` per S3.1 §23 cross-group privacy ceiling.

## 11. Golden fixtures

### Fixture 1 — AIOS evidence viewer in CONTENT zone

```text
Tree: CARD with HEADING "Evidence evr_01HXYZB...", TABLE bound to S2.1 view of evidence chain rows.

Surface: AIOS_SURFACE in CONTENT zone (S7.1 fixture 1 outcome).
Renderer KDE compiles:
  - xdg-shell top-level window for the surface
  - QFrame (CARD) with QGraphicsDropShadowEffect for shadow
  - QLabel (HEADING) with TYPOGRAPHY_HEADING_MD font
  - QTableView (TABLE) bound to a QAbstractTableModel adapter over the S2.1 view
  - AIOS theme tokens: COLOR_SURFACE_CONTENT background, COLOR_TEXT_PRIMARY text, etc.

Expected outcome:
  Surface visible in CONTENT zone. AIOS chrome on wlr-layer-shell overlay layer remains above.
  No theme tokens unresolved; KDE_RENDERER_STARTED already emitted at session start.
```

### Fixture 2 — Bevy game fullscreen with chrome above

```text
Surface: APP_SURFACE in CONTENT zone owned by family:app:com.example.game:i-01.
The game running through AIOS Capability Runtime is granted GPU_FULL_3D class on family-group VkDevice.

Game enters fullscreen via xdg-shell fullscreen request.
KWin enlarges the game's wl_surface to fill the output.

Without aios-fullscreen-block KWin script: KWin's default fullscreen behavior may unmap top-layer surfaces.
With it: the wlr-layer-shell `overlay` chrome layer keeps receiving frames and remains visible.

Expected outcome:
  Game pixels fill CONTENT zone.
  AIOS chrome (SECURITY_INDICATOR with subject = family:app:com.example.game, current_action_id, evidence_record_id;
    EVIDENCE_LINK to the active action) visible at top.
  INV-020 satisfied during fullscreen.
```

### Fixture 3 — Cross-group VkDevice isolation on the same display

```text
Two surfaces present on the same monitor:
  surface_a = AIOS_SURFACE in family group's evidence-viewer (GPU_BASIC_2D on family VkDevice)
  surface_b = APP_SURFACE in homelab group's app (GPU_FULL_3D on homelab VkDevice)

Rendering:
  KWin composites both surfaces.
  AIOS renderer never imports a dmabuf from family VkDevice into homelab's surface, or vice versa.
  Compute shaders inside surface_b cannot read surface_a's framebuffer because the VkDevice
  authorized peer set forbids cross-group access.

Expected outcome:
  Both surfaces render correctly on the same display.
  Pixel-readback attempts across the boundary fail; CROSS_SURFACE_READ_DENIED (S7.1 §9; FOREVER retention) emitted.
  No KDE-side leakage; KWin sees the surfaces as opaque buffers, no cross-buffer access path.
```

### Fixture 4 — Recovery shell distinct from normal

```text
System enters recovery mode (L1 recovery boot path).

KWin recovery session starts under _recovery user.
Renderer activates theme_aios_recovery automatically (user theme switch refused).

Compilation differences from a normal session:
  - TYPOGRAPHY_BODY_MD and TYPOGRAPHY_HEADING_MD use the recovery family (different from any normal-mode theme)
  - COLOR_BOUNDARY_RECOVERY accent applied to chrome and recovery banner
  - COMPONENT_RECOVERY_SHIELD present persistently in CHROME zone
  - Operator credential dialog rendered as a separate QDialog process under _recovery user
  - APP_SURFACE / STREAM_SURFACE / SURFACE_EMBED rejected at surface service; renderer never sees them

Expected outcome:
  Visual identity of recovery is unmistakably different from any normal-mode theme.
  KDE_RECOVERY_SHELL_STARTED evidence emitted (FOREVER retention).
  Operator can complete recovery actions; cognitive core is offline (per L1 invariant).
```

### Fixture 5 — KRunner Alt+Space invocation

```text
User in normal-mode session presses Alt+Space.
KRunner opens; aios-runner plugin offered as a runner.
User types: "delete invoices older than 90 days"
Press Enter:
  - aios-runner constructs an AIOS_SURFACE in CONTENT zone with a goal-entry FORM tree
    bound to the canonical "submit goal" S0.1 action template
  - The form's submitter = active session's subject (the human user)

Expected outcome:
  Goal-entry surface visible within p95 < 100 ms.
  Form submission produces an S0.1 action envelope; cognitive core proposes a typed action plan;
  policy kernel evaluates; user approves via APPROVAL_PROMPT; capability runtime executes.
  KRunner returns to its idle state.
```

### Fixture 6 — Notification with constitutional icon

```text
A typed action verifies successfully (state: succeeded).
Action runtime dispatches a notification: "Action act_01HXYZ... verified. Receipt: evr_..."
Notification renders into OVERLAY zone via `org.freedesktop.Notifications`.

Constitutional icon: ICON_CONSTITUTIONAL_TRUST_CHECK loaded via KIconLoader from
/aios/system/themes/<theme_id>/icons/. Hash verified against the theme bundle.

Expected outcome:
  Notification visible in OVERLAY (above CONTENT, below CHROME).
  Icon glyph matches AIOS_DEFAULT canonical hash.
  Click on the EVIDENCE_LINK button in the notification opens the evidence viewer in a new CONTENT surface (Fixture 1).
```

### Fixture 7 — App attempts wlr-layer-shell `overlay` claim

```text
An APP_SURFACE-owning client (a malicious or misbehaving app) connects to KWin and requests
a wlr-layer-shell surface on the `overlay` layer.

AIOS chrome service intercepts at the renderer level (Wayland client identity check: the
requesting client's PID maps to a non-AIOS-chrome subject). The renderer rejects the request;
KWin never receives the layer-shell surface request from this client.

Expected outcome:
  Layer-shell creation refused.
  KDE_LAYER_SHELL_REJECTED evidence emitted (FOREVER retention) carrying the offending subject id
  and attempted layer = `overlay`.
  The app may continue to operate normally on its existing CONTENT-zone surface.
```

### Fixture 8 — KWin / Wayland disconnect → degraded mode

```text
During an active session, KWin crashes (or the Wayland socket becomes unreachable).
The renderer's Wayland event loop receives a connection-lost event.

Renderer enters degraded fallback mode (per I7):
  - Switch to a Qt-only X11/headless rendering path for the AIOS dashboard
  - Constitutional icons remain (loaded from the AIOS root-signed bundle)
  - Theme animations disabled; tokens collapse to neutral fallback values
  - Recovery shield prominently shown ("Compositor unavailable; system in degraded display mode.
    Recovery shell available via Ctrl+Alt+F2 console.")
  - APP_SURFACE / STREAM_SURFACE / VISUALIZATION nodes show placeholder cards explaining unavailability

Expected outcome:
  KDE_RENDERER_DEGRADED evidence emitted (FOREVER retention) with reason = "kwin_unreachable".
  User retains text-only access to AIOS state and to the recovery path.
  No silent loss of trust indicators; the degraded banner is itself a trust signal.
```

## 12. Telemetry contract

All metrics MUST use bounded label cardinality. **`window_id`, `surface_id`, `subject_canonical_id`, `group_id`, `namespace_path`, `tree_id`, `node_id`, `theme_id`, `client_pid` are NEVER labels.**

| Metric                                       | Type      | Labels (closed)                                                                       |
| -------------------------------------------- | --------- | ------------------------------------------------------------------------------------- |
| `kde_renderer_session_total`                 | counter   | `result` (started/degraded/exited), `mode` (normal/recovery)                          |
| `kde_renderer_compilation_total`             | counter   | `kind` (NodeKind value), `result`, `error_code` (KdeCompilationResultCode)            |
| `kde_renderer_compilation_duration_seconds`  | histogram | `node_count_class` (1/16/256/1k/10k)                                                  |
| `kde_renderer_active_surfaces`               | gauge     | `zone`, `kind` (SurfaceKind)                                                          |
| `kde_renderer_frame_duration_seconds`        | histogram | `zone_count_class` (1/4/16/64), `monitor_class` (single/multi)                        |
| `kde_renderer_frame_dropped_total`           | counter   | `severity_class` (occasional/persistent), `zone`                                      |
| `kde_renderer_layer_shell_rejected_total`    | counter   | `attempted_layer` (overlay/top/background)                                            |
| `kde_renderer_kwin_script_loaded_total`      | counter   | `result` (loaded/rejected), `reason_class` (signed_ok/unsigned_path/hash_mismatch)    |
| `kde_renderer_token_resolve_total`           | counter   | `family` (color/typography/spacing/motion/icon), `result` (resolved/fallback)         |
| `kde_renderer_dmabuf_import_total`           | counter   | `result` (success/cross_group_denied/format_unsupported)                              |
| `kde_renderer_recovery_kind_rejected_total`  | counter   | `attempted_kind` (NodeKind value)                                                     |
| `kde_renderer_theme_override_scope_total`    | counter   | `scope` (aios_window_tree)                                                            |
| `kde_renderer_degraded_total`                | counter   | `reason_class` (kwin_unreachable/wgpu_init_failed/icon_hash_mismatch/qml_load_failed) |
| `kde_renderer_monitor_topology_change_total` | counter   | `event` (added/removed/scale_changed)                                                 |

Cardinality budget: ≤ 200 active label tuples per metric.

## 13. Acceptance criteria

- [ ] Schema package `aios.renderer.kde.v1alpha1` published; service `KdeRenderer` and the `KdeCompilationResultCode` closed enum (11 declared values) match Appendix A.
- [ ] Every S7.2 `NodeKind` (19 values) has a deterministic Qt/QML primitive mapping per §4.
- [ ] `CompositionZone` → KWin layer mapping per §3.2 honored end-to-end; `overlay` layer reserved exclusively for AIOS chrome.
- [ ] AIOS theme overrides Plasma global theme and any external Qt theme **only within AIOS surfaces** (§5.1).
- [ ] Constitutional icons load only from AIOS-root-signed bundle; runtime hash mismatch triggers degraded mode.
- [ ] KWin scripts loaded only from `/aios/system/renderers/kde/kwin-scripts/` (signed); unauthorized-path attempts emit `KDE_KWIN_SCRIPT_REJECTED`.
- [ ] Recovery shell runs as a separate KWin session with `_recovery` user; AIOS_RECOVERY theme auto-active; user theme switch refused; `APP_SURFACE`/`STREAM_SURFACE`/`SURFACE_EMBED` rejected.
- [ ] `wlr-layer-shell` `overlay` claims from non-AIOS-chrome clients rejected with `KDE_LAYER_SHELL_REJECTED` evidence (FOREVER retention).
- [ ] `wgpu` GPU canvas bound to the per-group `VkDevice` via S8.2 `GpuCapabilityBinding`; cross-group dmabuf imports denied at the device layer.
- [ ] KWin / Wayland unavailability triggers degraded fallback per §I7; `KDE_RENDERER_DEGRADED` evidence (FOREVER) emitted; recovery instructions shown.
- [ ] `MOTION_DURATION_INSTANT` always compiles to 0 ms; `MOTION_DURATION_DELIBERATE` preserved across reduced-motion settings.
- [ ] Performance contract (§8) measurable; deadline-miss bands feed `kde_renderer_frame_dropped_total` per §12.
- [ ] All 8 golden fixtures (§11) produce the specified outcomes.
- [ ] Telemetry conforms to §12; `window_id` / `surface_id` / `subject_canonical_id` / `group_id` / `namespace_path` / `tree_id` / `node_id` / `theme_id` / `client_pid` NEVER appear as labels.
- [ ] L0 INV-019 / INV-020 / INV-021 / INV-022 honored at the renderer per §10 enforcer rows.

## 14. Open deferrals

- **Plasma Mobile.** This spec is desktop Plasma only; KDE Plasma Mobile (phone/tablet form factor) is a separate renderer surface, deferred.
- **X11 fallback.** The renderer currently assumes Wayland; an X11 fallback path is deferred. Degraded mode (§I7) provides a text-only fallback that does not require Wayland.
- **Multi-output `CHROME` strategy beyond per-monitor stacks.** Surface-spans-multiple-monitors, per-monitor chrome variants, ultrawide-display chrome behavior — deferred.
- **HDR / wide-gamut color profiles.** Per-surface color management, ICC profile honoring, HDR tone mapping. Deferred to a per-renderer color-management spec.
- **VRR (variable refresh rate).** Per-surface frame pacing hints to KWin's VRR support. Deferred.
- **Surface migration between renderers.** Moving an active session from KDE to Web mid-flight (taking the same UI tree to a different target). Deferred (already deferred at S7.1; reaffirmed here).
- **Renderer-level a11y plugin protocols.** Custom a11y bridges (e.g., for non-Orca screen readers, for AAC devices). The current contract uses Qt's built-in a11y bridge.
- **KDE Plasma 7+.** This contract targets Plasma 6 (current at spec time). Plasma 7 protocol changes, if/when announced, go into a versioned bump.
- **GPU-accelerated CHROME zone.** Currently chrome surfaces are CPU-rendered Qt widgets on a `wlr-layer-shell` `overlay` surface. GPU acceleration of chrome is deferred (a complexity vs. trust-surface trade-off).
- **Per-Activity theme.** Different AIOS themes per KDE Activity. Deferred (current contract: one active theme per session).

## 15. See also

- [S7.1 — Surface + Composition Model](01_surface_composition.md)
- [S7.2 — Shared UI Schema](02_shared_ui_schema.md)
- [S7.3 — Visual Language](03_visual_language.md)
- [S8.2 — GPU Resource Model](../L8_Network_Hardware_Devices/05_gpu_resource_model.md)
- [S5.1 — Identity Model](../L4_Policy_Identity_Vault/03_identity_model.md)
- [S6.4 — Constitutional Invariants (incl. INV-019..INV-022)](../L0_Governance_Evidence_Safety/04_invariants.md)
- [S2.1 — Query/View Language](../L2_AIOS_FS/02_query_view_language.md)
- [S0.1 — Action Envelope and Lifecycle](../XX_Cross_Cutting/01_action_envelope_lifecycle.md)
- [S3.1 — Evidence Log](../L9_Observability_Admin_Operations/01_evidence_log.md)
- [L7 Overview](00_overview.md)
- [Rev.2 Master Index](../00_MASTER_INDEX.md)

## Appendix A — Full Proto IDL

```proto
syntax = "proto3";
package aios.renderer.kde.v1alpha1;

import "google/protobuf/timestamp.proto";
import "google/protobuf/duration.proto";

// ============================================================================
// Service
// ============================================================================

service KdeRenderer {
  // Render a UI tree into the KDE renderer; returns once the tree is composed
  // (or rejected). The tree itself is the S7.2 UITree (referenced by id; the
  // renderer fetches it from the UISchemaService).
  rpc RenderTree(RenderTreeRequest) returns (RenderTreeResponse);

  // Open a recovery shell session (called by L1 recovery boot path; not by
  // normal-mode subjects).
  rpc OpenRecoveryShell(OpenRecoveryShellRequest) returns (OpenRecoveryShellResponse);

  // Introspection: renderer identity, capabilities, supported schema versions.
  rpc GetRendererInfo(GetRendererInfoRequest) returns (GetRendererInfoResponse);

  // Register a renderer-internal composition extension (e.g., per-app chrome
  // injection script bundle). Restricted to AIOS root identity.
  rpc RegisterCompositionExtension(RegisterCompositionExtensionRequest)
      returns (RegisterCompositionExtensionResponse);
}

// ============================================================================
// Closed enums
// ============================================================================

enum KdeCompilationResultCode {
  KDE_COMPILATION_RESULT_CODE_UNSPECIFIED = 0;
  KDE_COMPILED = 1;
  KDE_UNKNOWN_NODE_KIND = 2;
  KDE_KIND_NOT_SUPPORTED_IN_RECOVERY = 3;
  KDE_GPU_BINDING_UNAVAILABLE = 4;
  KDE_DMABUF_IMPORT_FAILED = 5;
  KDE_LAYER_SHELL_REJECTED = 6;
  KDE_KWIN_SCRIPT_REJECTED = 7;
  KDE_QML_LOAD_FAILED = 8;
  KDE_THEME_TOKEN_UNRESOLVED = 9;
  KDE_FOREIGN_TOPLEVEL_DETECTED = 10;
  KDE_RENDERER_INTERNAL = 11;
}

enum KdeRendererMode {
  KDE_RENDERER_MODE_UNSPECIFIED = 0;
  KDE_MODE_NORMAL = 1;
  KDE_MODE_RECOVERY = 2;
  KDE_MODE_DEGRADED = 3;     // text-only fallback (per §I7)
}

enum KdeWaylandLayer {
  KDE_WAYLAND_LAYER_UNSPECIFIED = 0;
  KDE_LAYER_BACKGROUND = 1;
  KDE_LAYER_BOTTOM = 2;
  KDE_LAYER_TOP = 3;
  KDE_LAYER_OVERLAY = 4;
  KDE_LAYER_XDG_SHELL = 5;       // not a layer-shell value; xdg-shell top-level
  KDE_LAYER_XDG_SUBSURFACE = 6;  // wl_subsurface child of an xdg-shell window
}

enum KdeDegradationReason {
  KDE_DEGRADATION_REASON_UNSPECIFIED = 0;
  KDE_DEGRADATION_KWIN_UNREACHABLE = 1;
  KDE_DEGRADATION_WGPU_INIT_FAILED = 2;
  KDE_DEGRADATION_CONSTITUTIONAL_ICON_HASH_MISMATCH = 3;
  KDE_DEGRADATION_QML_LOAD_FAILED = 4;
  KDE_DEGRADATION_THEME_VERIFICATION_FAILED = 5;
}

// ============================================================================
// Capabilities and info
// ============================================================================

message KdeRendererCapabilities {
  bool supports_wayland = 1;
  bool supports_xdg_shell = 2;
  bool supports_wlr_layer_shell = 3;
  bool supports_dmabuf = 4;
  bool supports_presentation_time = 5;
  bool supports_foreign_toplevel_management = 6;
  bool supports_fractional_scaling = 7;
  bool supports_viewporter = 8;
  bool supports_kwin_scripting = 9;
  bool supports_krunner_plugin = 10;
  bool supports_plasma_widget = 11;
  bool supports_kde_notifications = 12;
  bool supports_kde_activities = 13;
  bool supports_wgpu_vulkan = 14;
  bool supports_gstreamer_dmabuf = 15;
  bool supports_ksyntax_highlighting = 16;
  uint32 max_concurrent_monitors = 17;
}

message KdeRendererInfo {
  string renderer_id = 1;                 // always "kde"
  string renderer_build_hash = 2;         // BLAKE3 of the renderer binary
  string surface_schema_version = 3;      // "aios.surface.v1alpha1"
  string ui_schema_version = 4;           // "aios.ui.v1alpha1"
  string visual_schema_version = 5;       // "aios.visual.v1alpha1"
  string kde_renderer_schema_version = 6; // "aios.renderer.kde.v1alpha1"
  string plasma_version_observed = 7;     // e.g., "6.2.4"
  string kwin_version_observed = 8;
  KdeRendererMode mode = 9;
  KdeRendererCapabilities capabilities = 10;
}

// ============================================================================
// RenderTree
// ============================================================================

message RenderTreeRequest {
  string tree_id = 1;                     // S7.2 uit_<ulid>; renderer fetches the tree
  string session_id = 2;                  // active rendering session
  string target_surface_id = 3;           // S7.1 surf_<ulid>; the AIOS_SURFACE that will host this tree
}

message RenderTreeResponse {
  oneof result {
    RenderTreeAccepted accepted = 1;
    RenderTreeError error = 2;
  }
}

message RenderTreeAccepted {
  string render_id = 1;                   // rnd_<ulid>; opaque renderer reference
  uint32 nodes_compiled = 2;
  uint32 nodes_dropped = 3;               // recovery / target_renderer_id branches dropped (per S7.2 §8 / §11.5)
  KdeNodeMappingSummary mapping_summary = 4;
}

message KdeNodeMappingSummary {
  // Aggregate per-NodeKind compilation counts; useful for telemetry verification.
  // NodeKind value -> count compiled by this render call.
  map<uint32, uint32> per_kind_compiled_count = 1;
}

message RenderTreeError {
  KdeCompilationResultCode code = 1;
  string message = 2;
  string offending_node_id = 3;           // empty for whole-tree errors
  string offending_token_name = 4;        // populated for KDE_THEME_TOKEN_UNRESOLVED
  string l8_denial_reason = 5;            // populated for KDE_GPU_BINDING_UNAVAILABLE
}

// ============================================================================
// OpenRecoveryShell
// ============================================================================

message OpenRecoveryShellRequest {
  string operator_subject_canonical_id = 1; // _system:operator-<id>; per S5.1
  string boot_id = 2;                       // L1 boot id
  string recovery_reason = 3;               // free-text operator-supplied
}

message OpenRecoveryShellResponse {
  oneof result {
    RecoveryShellOpened opened = 1;
    RecoveryShellError error = 2;
  }
}

message RecoveryShellOpened {
  string recovery_session_id = 1;           // rsh_<ulid>
  string recovery_wayland_socket = 2;       // path to recovery-only WAYLAND_DISPLAY socket
  string active_theme_id = 3;               // always theme_aios_recovery (signed)
}

enum RecoveryShellErrorCode {
  RECOVERY_SHELL_ERROR_CODE_UNSPECIFIED = 0;
  RECOVERY_OPERATOR_NOT_RESOLVED = 1;
  RECOVERY_THEME_VERIFICATION_FAILED = 2;
  RECOVERY_KWIN_LAUNCH_FAILED = 3;
  RECOVERY_CONSTITUTIONAL_ICON_BUNDLE_MISSING = 4;
  RECOVERY_RENDERER_INTERNAL = 5;
}

message RecoveryShellError {
  RecoveryShellErrorCode code = 1;
  string message = 2;
}

// ============================================================================
// GetRendererInfo
// ============================================================================

message GetRendererInfoRequest {}

message GetRendererInfoResponse {
  KdeRendererInfo info = 1;
}

// ============================================================================
// RegisterCompositionExtension (root-only)
// ============================================================================

message RegisterCompositionExtensionRequest {
  string extension_id = 1;                  // e.g., "aios-fullscreen-block"
  bytes content = 2;                        // KWin script source
  bytes content_hash = 3;                   // BLAKE3 of content
  bytes ed25519_signature = 4;              // signed by AIOS root key
}

message RegisterCompositionExtensionResponse {
  bool registered = 1;
  string error_message = 2;                 // empty on success
}
```
