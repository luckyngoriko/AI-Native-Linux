# Visual Language (Rev.2)

| Field          | Value                                                                                                                                                                                                                    |
| -------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Status         | `CONTRACT` (initial; written 2026-05-10)                                                                                                                                                                                 |
| Phase tag      | S7.3                                                                                                                                                                                                                     |
| Layer          | L7 Interaction Renderers                                                                                                                                                                                                 |
| Schema package | `aios.visual.v1alpha1`                                                                                                                                                                                                   |
| Consumes       | L0 INV-019..INV-022 (constitutional bindings), S5.1 identity (subject kind for AI/human distinction), S7.1 surface composition, S7.2 shared UI schema (node flags `is_ai_origin` / `is_trust_bearing` / `recovery_only`) |
| Produces       | closed token taxonomy, distinctive AIOS component vocabulary, theme contract, multi-axis distinction model, cross-renderer mapping rules consumed by L7.4 KDE renderer, L7.5 Web renderer, L7.6 CLI renderer             |

## 1. Purpose

The constitutional layer of the AIOS visual identity is locked at L0 (INV-019..INV-022). The structural grammar is locked at S7.2 (UI schema). This spec is the layer between them: a **semantic token system** that every renderer binds to and that themes — concrete bundles of values — populate.

This is **stage 2 of the three-stage visual plan**:

```text
Stage 1  (DONE — L0 INV-019..022)   constitutional invariants — what visual identity MUST do
Stage 2  (this spec — S7.3)          semantic token taxonomy — the variables every renderer reads
Stage 3  (post-spec, post-prototype)  concrete theme — the actual hex codes / typeface / icon set
```

This spec defines **no concrete hex code, no specific typeface, no specific icon glyph**. Those are stage 3, deliberately deferred until a renderer prototype exists and visual choices can be tested against real surface contact. What this spec fixes:

1. The closed token taxonomy (color, typography, spacing, motion, icon).
2. The constitutional constraints that any concrete theme must satisfy — `ΔE` distinctness, multi-axis distinction, recovery cross-theme distinctness.
3. The distinctive AIOS component vocabulary (SecurityBanner, RecoveryShield, AISubjectBadge, etc.) — recognizable patterns that combine S7.2 schema nodes with S7.3 token bindings.
4. The signed `Theme` bundle format and validation rules.
5. The cross-renderer mapping — how the same tokens compile to Qt palette + QML styles on KDE, CSS custom properties + shader uniforms on Web, ANSI sequences on CLI.
6. The multi-axis distinction model (color / pattern / typography / outline) so that AI-vs-human and recovery-vs-normal distinctions remain visible even when a single visual axis fails (colorblind users, monochrome displays, accessibility profiles).

## 2. Core invariants

- **I1 — Closed token vocabulary.** `ColorTokenName`, `TypographyTokenName`, `SpacingTokenName`, `MotionTokenName`, `IconTokenName`, `ComponentName`, `DistinctionAxis` are closed enums. Adding a token is a versioned spec change.
- **I2 — Themes are signed.** A `Theme` is a content-addressed bundle (`theme_<hex_lower(BLAKE3(jcs(theme)))[:32]>`) signed by AIOS root or a designated theme publisher. Unsigned or signature-failing themes are rejected at load.
- **I3 — Constitutional token constraints are non-negotiable.** Any concrete theme that fails `ΔE` distinctness, multi-axis distinction, or recovery-cross-theme distinctness is rejected at theme-validation time with `ThemeInvariantViolated`. Themes do not load partially.
- **I4 — Multi-axis distinction.** Every constitutional distinction (AI/human, recovery/normal, trust-verified/unverified) is encoded across at least two `DistinctionAxis` values. This binds INV-021 and INV-022 against single-axis-failure modes (colorblindness, monochrome displays, restrictive accessibility profiles).
- **I5 — Stage 3 deferred.** Concrete hex codes, specific typeface names, specific icon glyphs are NOT in this spec. Themes that supply them are the stage-3 artifact.
- **I6 — Recovery theme is constitutional.** The recovery theme is shipped with AIOS, signed by root, cannot be overridden by any user choice, activates automatically when `subject.recovery_mode = true`. User themes cannot use the recovery token namespace.
- **I7 — Trust tokens are exclusive.** `color.trust.*`, `icon.constitutional.*`, and `component.RecoveryShield`/`SecurityBanner` cannot be authored by AI subjects (binds INV-020 at theme-authorship layer; mirrors S7.2 trust-bearing-node discipline).

## 3. Three-stage visual plan (recap)

| Stage | Where           | Artifact                                                 | What it locks                                       |
| ----- | --------------- | -------------------------------------------------------- | --------------------------------------------------- |
| 1     | L0 INV-019..022 | Four constitutional invariants                           | Visual identity is a constitution, not decoration   |
| 2     | S7.3 (this)     | Semantic token taxonomy + theme contract + components    | Variables that every renderer reads                 |
| 3     | Post-prototype  | Concrete theme bundle (hex codes, typeface, icon glyphs) | Actual aesthetic — chosen with eyes on real surface |

A renderer compiled against stage 2 with no theme attached produces a structurally correct tree using **fallback tokens** (per §4.7) — neutral colors, default typeface, monotone icons. The fallback is intentionally austere; it is not a default aesthetic.

## 4. Token taxonomy (closed)

### 4.1 Color tokens

```proto
enum ColorTokenName {
  COLOR_TOKEN_NAME_UNSPECIFIED = 0;

  // Action provenance — binds INV-021
  COLOR_ACTION_HUMAN = 1;
  COLOR_ACTION_AI = 2;
  COLOR_ACTION_SYSTEM = 3;
  COLOR_ACTION_RECOVERY = 4;

  // Surface backgrounds
  COLOR_SURFACE_BACKGROUND = 10;
  COLOR_SURFACE_CONTENT = 11;
  COLOR_SURFACE_OVERLAY = 12;
  COLOR_SURFACE_CHROME = 13;

  // Text
  COLOR_TEXT_PRIMARY = 20;
  COLOR_TEXT_SECONDARY = 21;
  COLOR_TEXT_MUTED = 22;
  COLOR_TEXT_INVERSE = 23;
  COLOR_TEXT_CODE = 24;
  COLOR_TEXT_AUDIT = 25;          // log-line text style

  // Semantic state
  COLOR_SEMANTIC_SUCCESS = 30;
  COLOR_SEMANTIC_WARNING = 31;
  COLOR_SEMANTIC_DANGER = 32;
  COLOR_SEMANTIC_INFO = 33;

  // Trust posture — binds INV-020
  COLOR_TRUST_VERIFIED = 40;
  COLOR_TRUST_UNVERIFIED = 41;
  COLOR_TRUST_DEGRADED = 42;
  COLOR_TRUST_DENIED = 43;

  // Evidence retention class
  COLOR_EVIDENCE_PERMANENT = 50;   // FOREVER retention; visually distinguished from standard
  COLOR_EVIDENCE_EXTENDED = 51;
  COLOR_EVIDENCE_STANDARD = 52;

  // Boundaries (group / scope / recovery)
  COLOR_BOUNDARY_GROUP = 60;
  COLOR_BOUNDARY_SCOPE = 61;
  COLOR_BOUNDARY_RECOVERY = 62;     // recovery-only namespace; binds INV-022

  // Border / outline
  COLOR_BORDER_DEFAULT = 70;
  COLOR_BORDER_FOCUS = 71;
  COLOR_BORDER_DIVIDER = 72;
}
```

Closed. 31 named color slots plus `UNSPECIFIED`. Themes supply concrete values; renderers consume by name.

### 4.2 Typography tokens

```proto
enum TypographyTokenName {
  TYPOGRAPHY_TOKEN_NAME_UNSPECIFIED = 0;

  // Display / hierarchy
  TYPOGRAPHY_DISPLAY_LG = 1;
  TYPOGRAPHY_DISPLAY_MD = 2;
  TYPOGRAPHY_DISPLAY_SM = 3;
  TYPOGRAPHY_HEADING_LG = 4;
  TYPOGRAPHY_HEADING_MD = 5;
  TYPOGRAPHY_HEADING_SM = 6;

  // Body
  TYPOGRAPHY_BODY_LG = 10;
  TYPOGRAPHY_BODY_MD = 11;
  TYPOGRAPHY_BODY_SM = 12;
  TYPOGRAPHY_CAPTION = 13;

  // Specialized
  TYPOGRAPHY_CODE_LG = 20;
  TYPOGRAPHY_CODE_MD = 21;
  TYPOGRAPHY_CODE_SM = 22;
  TYPOGRAPHY_AUDIT = 23;            // dense log-line; monospace-leaning
  TYPOGRAPHY_RECOVERY = 24;         // recovery-only typography; visually distinct from all normal tokens
  TYPOGRAPHY_AI_ORIGIN = 25;         // AI-origin distinguishing typography (binds INV-021 typography axis)
}
```

Closed. 17 typography roles. A typography token resolves to:

```proto
message TypographyValue {
  string family = 1;            // theme-supplied; e.g., "Inter Variable"
  uint32 size_logical_pixels = 2;
  uint32 weight = 3;            // 100..900
  bool italic = 4;
  float line_height_multiple = 5;
  float letter_spacing_em = 6;
}
```

Themes supply the values. Renderers compile to:

- KDE: `QFont(family, size, weight)` + `QFont::setItalic(...)`.
- Web: CSS `font-family`/`font-size`/`font-weight`/`font-style`/`line-height`/`letter-spacing` custom properties.
- CLI: ignored (terminal has no typography in the strict sense; bold + italic ANSI are the only proxy and used only for `TYPOGRAPHY_RECOVERY` distinction and `TYPOGRAPHY_AI_ORIGIN`).

### 4.3 Spacing tokens

```proto
enum SpacingTokenName {
  SPACING_TOKEN_NAME_UNSPECIFIED = 0;
  SPACING_NONE = 1;
  SPACING_XXS = 2;
  SPACING_XS = 3;
  SPACING_SM = 4;
  SPACING_MD = 5;
  SPACING_LG = 6;
  SPACING_XL = 7;
  SPACING_XXL = 8;
  SPACING_HUGE = 9;
}
```

Closed. 9 named slots plus `UNSPECIFIED`. Theme supplies one base unit (8px is the default suggestion at stage 3, but stage 2 does not commit) and a multiplier table per slot (e.g., XS = 0.5×, SM = 1×, MD = 2×, LG = 3×, XL = 5×, XXL = 8×, HUGE = 13×).

The spacing system is the only layout grid AIOS exposes. S7.2 layout modes (STACK, FLOW, GRID) consume spacing tokens for their gap/padding values; renderer-specific CSS / Qt margins / terminal whitespace columns are computed from the same token.

### 4.4 Motion tokens

```proto
enum MotionTokenName {
  MOTION_TOKEN_NAME_UNSPECIFIED = 0;

  // Durations (for animations)
  MOTION_DURATION_INSTANT = 1;        // 0 ms; mandatory for security indicators
  MOTION_DURATION_FAST = 2;
  MOTION_DURATION_NORMAL = 3;
  MOTION_DURATION_SLOW = 4;
  MOTION_DURATION_DELIBERATE = 5;     // intentionally slow for destructive operations

  // Easings
  MOTION_EASING_LINEAR = 10;
  MOTION_EASING_STANDARD = 11;
  MOTION_EASING_DECELERATE = 12;
  MOTION_EASING_ACCELERATE = 13;
  MOTION_EASING_CRITICAL = 14;        // sharp, attention-grabbing for warnings
}
```

Closed. 11 motion slots.

`MOTION_DURATION_INSTANT` is constitutional: any animation that delays the appearance of a security indicator, recovery banner, or trust state change MUST resolve to 0 ms. Renderers reject themes that map `MOTION_DURATION_INSTANT` to a non-zero value with `ThemeInvariantViolated`.

`MOTION_DURATION_DELIBERATE` is the recommended duration for confirmation buttons on destructive operations (delete, retire, override). It slows the user down on purpose.

A theme supplies a `MotionValue` per token:

```proto
message MotionValue {
  uint32 duration_ms = 1;
  string easing_curve_id = 2;       // matches one of MOTION_EASING_*
  bool reduced_motion_zero = 3;     // honor OS reduced-motion preference; collapse to 0 when set
}
```

### 4.5 Icon tokens

```proto
enum IconTokenName {
  ICON_TOKEN_NAME_UNSPECIFIED = 0;

  // Constitutional icons — cannot be customized by user themes
  ICON_CONSTITUTIONAL_SECURITY_SHIELD = 1;
  ICON_CONSTITUTIONAL_RECOVERY_LOCK = 2;
  ICON_CONSTITUTIONAL_AI_INDICATOR = 3;
  ICON_CONSTITUTIONAL_HUMAN_INDICATOR = 4;
  ICON_CONSTITUTIONAL_EVIDENCE_CHAIN = 5;
  ICON_CONSTITUTIONAL_TRUST_CHECK = 6;
  ICON_CONSTITUTIONAL_TAMPER_WARNING = 7;
  ICON_CONSTITUTIONAL_GROUP_BOUNDARY = 8;

  // Semantic icons — themes may customize within constraints
  ICON_SEMANTIC_SUCCESS = 20;
  ICON_SEMANTIC_WARNING = 21;
  ICON_SEMANTIC_DANGER = 22;
  ICON_SEMANTIC_INFO = 23;

  // Action icons — fully customizable by themes
  ICON_ACTION_APPROVE = 40;
  ICON_ACTION_REJECT = 41;
  ICON_ACTION_DEFER = 42;
  ICON_ACTION_VIEW_EVIDENCE = 43;
  ICON_ACTION_OPEN = 44;
  ICON_ACTION_CLOSE = 45;
  ICON_ACTION_RETIRE = 46;
}
```

Closed. 24 named icons plus `UNSPECIFIED`. Three categories:

- **Constitutional icons** (8): glyphs are part of the AIOS root-signed theme and cannot be replaced by user themes. They carry constitutional meaning (AI subject indicator, recovery lock, evidence chain marker).
- **Semantic icons** (4): user themes may customize but must preserve recognizability — a `SUCCESS` icon must remain recognizable as success-equivalent.
- **Action icons** (7): user themes fully customize.

A theme supplies an `IconValue` per token:

```proto
message IconValue {
  string aiosfs_pointer = 1;            // S1.3 object pointer to the SVG/PNG/glyph asset
  string vector_glyph_id = 2;            // optional: a font-glyph reference for icon-font themes
  string description = 3;                // a11y label
  bytes content_hash = 4;                // hex_lower(BLAKE3(asset))[:32]
}
```

### 4.6 Component vocabulary

```proto
enum ComponentName {
  COMPONENT_NAME_UNSPECIFIED = 0;

  // Constitutional components — render rules cannot be overridden by themes
  COMPONENT_SECURITY_BANNER = 1;
  COMPONENT_RECOVERY_SHIELD = 2;
  COMPONENT_AI_SUBJECT_BADGE = 3;
  COMPONENT_HUMAN_SUBJECT_BADGE = 4;
  COMPONENT_TRUST_INDICATOR = 5;
  COMPONENT_EVIDENCE_LINK_TILE = 6;
  COMPONENT_TAMPER_WARNING = 7;

  // Recognizable AIOS components — themes adjust appearance within constraints
  COMPONENT_AGENT_MESSAGE = 20;
  COMPONENT_APPROVAL_PROMPT = 21;
  COMPONENT_AUDIT_TRAIL = 22;
  COMPONENT_ACTION_ENVELOPE_VIEW = 23;
  COMPONENT_GROUP_HEADER = 24;
  COMPONENT_INBOX_TILE = 25;
}
```

Closed. 14 named components plus `UNSPECIFIED`. A `Component` is a recipe — a specific S7.2 schema node tree with specific token bindings — that produces a recognizable AIOS pattern across all renderers. The component vocabulary makes "an evidence link looks like an evidence link" a contract, not a style accident.

Each component has a `ComponentRecipe`:

```proto
message ComponentRecipe {
  ComponentName name = 1;
  string schema_node_template = 2;     // S7.2 NodeKind subtree pattern (JSON-serialized)
  repeated TokenBinding token_bindings = 3;
  repeated DistinctionAxis required_axes = 4;
  bool constitutional = 5;              // if true, themes cannot override rendering rules
  string version = 6;                    // e.g., "v1alpha1"
}

message TokenBinding {
  string node_path = 1;                 // path within the schema_node_template
  oneof token {
    ColorTokenName color = 2;
    TypographyTokenName typography = 3;
    SpacingTokenName spacing = 4;
    MotionTokenName motion = 5;
    IconTokenName icon = 6;
  }
}
```

### 4.7 Fallback tokens

For every token name, the spec defines a **fallback value** that renderers use when no theme is loaded. Fallbacks are intentionally austere (neutral grayscale colors, system default monospace, 8px base spacing, 0 ms motion, monotone glyphs). They are not a default aesthetic; they are the "no theme" rendering, used only at first boot before a theme bundle loads.

The fallback is itself a signed `Theme` (`theme_aios_fallback`) shipped with AIOS root.

## 5. Distinction axes — multi-axis distinction model

`DistinctionAxis` is the closed enum that names the visual dimensions across which constitutional distinctions are encoded:

```proto
enum DistinctionAxis {
  DISTINCTION_AXIS_UNSPECIFIED = 0;
  AXIS_HUE = 1;                     // color hue (CIE LCH or RGB)
  AXIS_PATTERN = 2;                 // iconography, glyph differences, hatching
  AXIS_TYPOGRAPHY = 3;              // font weight, italic, family, size
  AXIS_OUTLINE = 4;                 // border style, weight, dash pattern
  AXIS_OPACITY = 5;                 // alpha; rare
  AXIS_POSITION = 6;                // location on screen (e.g., chrome zone vs content zone)
}
```

The constitutional bindings (§7) list which axes each must encode. A theme that fails to encode the required axes for a binding is rejected.

## 6. Theme contract

### 6.1 `Theme` message

```proto
message Theme {
  string theme_id = 1;                  // theme_<hex_lower(BLAKE3(jcs(theme)))[:32]>
  string theme_kind = 2;                // closed enum: AIOS_FALLBACK / AIOS_DEFAULT / AIOS_RECOVERY / USER_THEME / PUBLISHER_THEME
  string display_name = 3;
  string issuer = 4;                    // canonical_subject_id of theme author
  google.protobuf.Timestamp issued_at = 5;
  bytes ed25519_signature = 6;          // theme-publisher signature

  repeated ColorTokenValue colors = 10;
  repeated TypographyTokenValue typography = 11;
  repeated SpacingTokenValue spacing = 12;
  repeated MotionTokenValue motion = 13;
  repeated IconTokenValue icons = 14;

  string a11y_profile_id = 20;          // optional: link to a sibling theme for accessibility variants
  bool supports_reduced_motion = 21;
  bool supports_high_contrast = 22;
  bool supports_colorblind = 23;        // multi-pattern colorblind palette
}

enum ThemeKind {
  THEME_KIND_UNSPECIFIED = 0;
  AIOS_FALLBACK = 1;             // shipped fallback; used pre-theme-load
  AIOS_DEFAULT = 2;              // root-signed default theme
  AIOS_RECOVERY = 3;             // root-signed recovery theme; constitutional, cannot be replaced
  USER_THEME = 4;                // user-authored, must be signed by user's identity
  PUBLISHER_THEME = 5;            // third-party publisher, must be signed by publisher with valid trust chain
}
```

### 6.2 Theme validation

A theme is loaded through `LoadTheme`:

```text
1. Verify Ed25519 signature against theme_kind's expected key:
     AIOS_FALLBACK / AIOS_DEFAULT / AIOS_RECOVERY → AIOS root key
     USER_THEME → user's identity service signing key
     PUBLISHER_THEME → publisher key with trust chain to AIOS root
   Fail → ThemeSignatureInvalid.

2. Verify token completeness: every token in the closed taxonomy has a value.
   Fail → ThemeIncomplete.

3. Verify ΔE distinctness for constitutional pairs (§7.1).
   Fail → ThemeInvariantViolated with the failing pair.

4. Verify multi-axis distinction (§7.2).
   Fail → ThemeInvariantViolated with the failing axis-count.

5. Verify recovery cross-theme distinctness (§7.3).
   Fail → ThemeInvariantViolated.

6. Verify constitutional component recipes are not overridden.
   Fail → ThemeOverridesConstitutional.

7. Verify constitutional icon glyphs match the AIOS_RECOVERY/AIOS_DEFAULT canonical hashes (USER_THEME and PUBLISHER_THEME only).
   Fail → ConstitutionalIconAltered.

8. On all checks passing, register theme as available; if user-selected as active, activate.
   Emit THEME_LOADED evidence (STANDARD_24M).
```

### 6.3 User theme selection

A user's theme preference lives at `/aios/groups/<g>/users/<u>/prefs/theme` (per S4.1 §6 user reserved subdirs). The user picks one of the available themes (AIOS_DEFAULT, USER_THEME, PUBLISHER_THEME instances). The recovery theme is **never user-selectable**; it activates automatically when `subject.recovery_mode = true`.

Theme switch emits `THEME_SWITCHED` evidence (STANDARD_24M, queued for S3.1 vocabulary follow-up) carrying old + new theme_id and the active subject.

## 7. Constitutional bindings

### 7.1 ΔE distinctness for constitutional color pairs

For every concrete theme, the following color pairs must satisfy `ΔE(CIEDE2000) ≥ 25` to ensure perceptual distinctness across contrast types:

| Pair                                                    | Why                                                     |
| ------------------------------------------------------- | ------------------------------------------------------- |
| `COLOR_ACTION_AI` vs `COLOR_ACTION_HUMAN`               | INV-021 — operator must distinguish AI vs human actions |
| `COLOR_ACTION_RECOVERY` vs all normal-mode actions      | INV-022 — recovery never blends with normal             |
| `COLOR_BOUNDARY_RECOVERY` vs `COLOR_BOUNDARY_GROUP`     | INV-022 — recovery boundary is not "just another group" |
| `COLOR_TRUST_VERIFIED` vs `COLOR_TRUST_UNVERIFIED`      | INV-020 — trust state visually unambiguous              |
| `COLOR_TRUST_VERIFIED` vs `COLOR_TRUST_DEGRADED`        | INV-020                                                 |
| `COLOR_EVIDENCE_PERMANENT` vs `COLOR_EVIDENCE_STANDARD` | retention class must be visually communicated           |

`ΔE ≥ 25` is "noticeable at a glance under normal viewing conditions" in CIEDE2000. The threshold may be tightened via signed bundle update; it cannot be loosened.

### 7.2 Multi-axis distinction requirements

A constitutional binding must encode distinction across **at least two axes** so that single-axis-failure modes (colorblindness, monochrome, high-contrast accessibility) preserve the distinction.

| Binding                                    | Required axes (≥)                                                                   |
| ------------------------------------------ | ----------------------------------------------------------------------------------- |
| AI action vs Human action (INV-021)        | 2 — typically `AXIS_HUE` + `AXIS_PATTERN` (icon) and/or `AXIS_TYPOGRAPHY`           |
| Recovery vs Normal mode (INV-022)          | 3 — `AXIS_HUE` + `AXIS_PATTERN` + `AXIS_TYPOGRAPHY` (mandatory recovery typography) |
| Trust verified vs unverified (INV-020)     | 2 — `AXIS_HUE` + `AXIS_PATTERN` (constitutional icon)                               |
| AIOS chrome zone vs content zone (INV-020) | 2 — `AXIS_HUE` + `AXIS_POSITION` (always at top)                                    |

A theme that encodes only one axis for any binding is rejected with `ThemeInvariantViolated` and the specific binding name.

### 7.3 Recovery cross-theme distinctness

The recovery theme's tokens must be distinguishable from every other theme's tokens by at least:

- `COLOR_BOUNDARY_RECOVERY`: ΔE ≥ 30 from every other color in every other available theme.
- `TYPOGRAPHY_RECOVERY`: a different `family` than every other theme's `TYPOGRAPHY_BODY_MD` or `TYPOGRAPHY_HEADING_MD`.
- `ICON_CONSTITUTIONAL_RECOVERY_LOCK`: a glyph not used elsewhere in the system.

These checks run when a new theme is loaded against the recovery theme as the reference. A theme whose colors creep too close to recovery is rejected.

## 8. Cross-renderer mapping

### 8.1 KDE (`L7.4`)

```text
Color tokens         → Qt palette + QML PropertyChanges
Typography tokens    → QFont via QQuickStyle theme overrides
Spacing tokens       → QML Layout.margins / Layout.spacing computed at theme load
Motion tokens        → QPropertyAnimation duration + easing curve
Icon tokens          → KIconLoader with theme-bound icon set; constitutional icons load from AIOS
                       root-signed asset bundle, never from system icon theme
Components           → QML ComponentRecipe templates that compose Qt widgets per the recipe
```

### 8.2 Web (`L7.5`)

```text
Color tokens         → CSS custom properties (e.g., --color-action-ai: ...) applied via :root
                        and overridden in shadow roots for chrome-zone enforcement
Typography tokens    → CSS @font-face + custom property bundles
Spacing tokens       → CSS custom properties consumed by layout primitives
Motion tokens        → CSS transition / animation duration + easing function variables
Icon tokens          → SVG sprite sheet served from /aios/system/themes/<theme_id>/icons/
                        Constitutional icons reference the root-signed asset
Components           → Web Components (custom elements) whose internals consume the CSS variables
```

### 8.3 CLI (`L7.6`)

```text
Color tokens         → ANSI 24-bit truecolor escape sequences (with 256-color fallback for legacy)
                        Constitutional distinctions (AI/human, recovery/normal, trust)
                        ALSO encoded in box-drawing characters (axis 2) and prefix glyphs
                        (axis 3) so monochrome terminals preserve distinction
Typography tokens    → bold / italic / underline ANSI codes; family ignored
Spacing tokens       → column counts and blank-line counts
Motion tokens        → duration_ms governs progress-indicator refresh; instant is 0 ms
Icon tokens          → Unicode glyph or short ASCII tag (e.g., [AI], [HUMAN], [SECURITY])
Components           → Templates of ANSI-bordered text blocks per ComponentRecipe
```

### 8.4 Voice / Mobile (`L7.6`/`L7.7` — deferred)

Voice renderer maps tokens onto verbal cadence and earcons (short audio cues distinguishing AI/human, etc.). Mobile renderer maps onto platform-native primitives (Material 3 on Android, UIKit on iOS) within the constitutional constraints.

## 9. Determinism contract

```text
GIVEN
  active theme_id        = T
  visual catalog version = visbundle_V
  active session         = (subject, primary_group_id, recovery_mode)
  schema tree            = signed UI subtree from S7.2

THEN
  (T, V, session, tree) → identical token resolutions for every node.
```

The visual catalog version `visbundle_<hex>` is the JCS-canonical hash of the closed token taxonomy + component vocabulary at a point in time. Bundle version increments when a new token is added (versioned spec change).

## 10. Performance contract

| Operation                          | p50      | p95      | p99      | Hard timeout |
| ---------------------------------- | -------- | -------- | -------- | ------------ |
| `LoadTheme` (validate + register)  | < 10 ms  | < 50 ms  | < 200 ms | 2 s          |
| `ResolveTokens` (per node, cached) | < 10 µs  | < 50 µs  | < 200 µs | 5 ms         |
| `ResolveTokens` (per node, fresh)  | < 100 µs | < 500 µs | < 2 ms   | 50 ms        |
| `ApplyTheme` (full re-render)      | < 50 ms  | < 200 ms | < 1 s    | 10 s         |
| `SwitchTheme`                      | < 100 ms | < 500 ms | < 2 s    | 10 s         |

Failure modes — all fail closed:

- `ThemeSignatureInvalid` → theme rejected; previous theme remains active.
- `ThemeInvariantViolated` → theme rejected; renderer continues with previous.
- `VisualCatalogMismatch` → renderer in degraded mode rendering with fallback theme only until catalog reconciled.
- `ThemeServiceInternal` → fail closed; alert emitted.

## 11. Adversarial robustness

### 11.1 Theme forgery

Themes are Ed25519-signed. Forged themes fail signature verification and are rejected at load. The signature key set per `ThemeKind` is fixed (AIOS root, identity service, designated publisher chain) and cannot be extended at runtime without a recovery-mode invariant-bundle update.

### 11.2 Constitutional-icon substitution

USER_THEME and PUBLISHER_THEME are validated against the canonical hashes of constitutional icon assets shipped with AIOS root. A theme that supplies a different glyph for `ICON_CONSTITUTIONAL_SECURITY_SHIELD` is rejected with `ConstitutionalIconAltered`. AIOS_DEFAULT and AIOS_RECOVERY (root-signed) define the canonical hashes.

### 11.3 ΔE-bypass attack

A theme picks colors that are pairwise ΔE ≥ 25 in CIEDE2000 but are nearly indistinguishable to certain CVD types (e.g., red/green for protan/deutan colorblindness). The multi-axis distinction (§7.2) defends against this — even when hue collapses, the distinction remains in pattern, typography, or position axes. Themes that DO encode ≥ 2 axes pass; themes that rely solely on hue fail validation.

### 11.4 Recovery-namespace squatting

A user theme attempts to use `COLOR_BOUNDARY_RECOVERY` for normal-mode purposes, hoping to mimic recovery aesthetic and confuse the operator. Validation catches this in §7.3 — the ΔE ≥ 30 cross-theme check ensures no normal theme's tokens come close to the recovery theme's reserved values.

### 11.5 Token-flood DoS

A theme bundle with abnormally large icon assets (multi-megabyte SVGs) is rate-limited at load: `LoadTheme` rejects bundles where any single icon asset exceeds 256 KiB, where total theme bundle exceeds 8 MiB, or where any single CSS-variable value exceeds 4 KiB. `ThemeBundleTooLarge` fails the load.

### 11.6 Constitutional-component override

A theme attempts to ship a `COMPONENT_SECURITY_BANNER` recipe that drops the trust indicator. Validation catches this — constitutional components carry `constitutional = true` flag in `ComponentRecipe` and themes cannot override their `schema_node_template` field. `ThemeOverridesConstitutional` fails the load.

### 11.7 Constitutional-token loosening via theme update

A signed theme attempts to map `MOTION_DURATION_INSTANT` to 200 ms (so security indicators animate in slowly, giving the eye time to miss them on quick context switches). Validation catches this — `MOTION_DURATION_INSTANT` is constitutionally fixed at 0 ms. `ThemeInvariantViolated` fails the load.

## 12. Cross-spec dependencies

| Spec       | Direction | What this spec contributes                                                                                                                                                                           |
| ---------- | --------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| L0 INV-019 | enforcer  | Token taxonomy + theme contract are the implementation layer; renderer obligations are theme conformance                                                                                             |
| L0 INV-020 | enforcer  | Constitutional `COLOR_TRUST_*` tokens, `ICON_CONSTITUTIONAL_*` icons, `COMPONENT_SECURITY_BANNER` lock the trust indicator at the visual layer                                                       |
| L0 INV-021 | enforcer  | `COLOR_ACTION_AI` vs `COLOR_ACTION_HUMAN` ΔE distinctness + multi-axis distinction encoded in §7                                                                                                     |
| L0 INV-022 | enforcer  | `COLOR_BOUNDARY_RECOVERY`, `TYPOGRAPHY_RECOVERY`, `ICON_CONSTITUTIONAL_RECOVERY_LOCK` reserved namespace; cross-theme distinctness in §7.3                                                           |
| S5.1       | consumer  | `subject.is_ai`, `subject.recovery_mode` drive token selection at render time                                                                                                                        |
| S7.1       | consumer  | Composition zones (BACKGROUND/CONTENT/OVERLAY/CHROME) consume `COLOR_SURFACE_*` tokens                                                                                                               |
| S7.2       | consumer  | Schema node flags (`is_ai_origin`, `is_trust_bearing`, `recovery_only`) drive token bindings; component recipes consume schema NodeKinds                                                             |
| S2.1       | producer  | New closed query field `target.theme_kind` and `target.theme_id`                                                                                                                                     |
| S3.1       | producer  | New evidence record types queued for follow-up: `THEME_LOADED` STANDARD_24M, `THEME_REJECTED` EXTENDED_60M (carries failure code), `THEME_SWITCHED` STANDARD_24M, `THEME_INVARIANT_VIOLATED` FOREVER |
| S2.4       | producer  | New verification primitives queued: `theme_satisfies_invariants(theme_id)`, `theme_constitutional_icons_intact(theme_id)`                                                                            |
| L7.4 KDE   | consumer  | Implements §8.1 Qt/QML mapping                                                                                                                                                                       |
| L7.5 Web   | consumer  | Implements §8.2 CSS custom properties + Web Components mapping                                                                                                                                       |
| L7.6 CLI   | consumer  | Implements §8.3 ANSI + box-drawing + glyph mapping                                                                                                                                                   |

## 13. Golden fixtures

### Fixture 1 — Valid normal-mode theme

```text
Theme {
  theme_kind: AIOS_DEFAULT,
  colors: {
    COLOR_ACTION_HUMAN: theme-supplied hue X (theme decides),
    COLOR_ACTION_AI:    theme-supplied hue Y, ΔE(X,Y) = 38 in CIEDE2000,
    COLOR_TRUST_VERIFIED: ΔE 30 from COLOR_TRUST_UNVERIFIED,
    ...
  },
  typography: { TYPOGRAPHY_AI_ORIGIN: { italic: true } },  // typography axis carries distinction
  ...
}

LoadTheme result:
  signature verified
  ΔE distinctness passes (X vs Y = 38; trust pair = 30)
  multi-axis distinction passes (AI vs human encoded in HUE + TYPOGRAPHY = 2 axes)
  THEME_LOADED evidence emitted (STANDARD_24M)
```

### Fixture 2 — Theme rejected for indistinct AI/human

```text
Theme {
  colors: {
    COLOR_ACTION_HUMAN: hue X,
    COLOR_ACTION_AI:    hue Y, ΔE(X,Y) = 12,    # too close
  },
  typography: { TYPOGRAPHY_AI_ORIGIN: { italic: false } },  # no typography axis
  icons: { ICON_CONSTITUTIONAL_AI_INDICATOR matches default }  # pattern axis present but ΔE alone failed
}

LoadTheme result:
  ΔE check failed for COLOR_ACTION_AI / COLOR_ACTION_HUMAN
  ThemeInvariantViolated with binding "AI vs Human action"
  THEME_REJECTED evidence emitted
```

### Fixture 3 — Colorblind-resilient theme

```text
Theme {
  supports_colorblind: true,
  a11y_profile_id: "deutan-safe",
  colors: { /* protan/deutan-safe palette: ΔE high in luminance not just hue */ },
  typography: { TYPOGRAPHY_AI_ORIGIN: { italic: true, weight: 700 } },
  icons: { ICON_CONSTITUTIONAL_AI_INDICATOR: distinct glyph from human },
}

Theme passes validation:
  AI vs human distinction encoded in:
    - hue (works for non-CVD)
    - typography (italic + bold)
    - pattern (distinct icon)
  3 axes — exceeds the required ≥ 2; survives both protan and deutan failure modes.
```

### Fixture 4 — Recovery theme constitutional

```text
Boot into recovery mode.
Theme service: AIOS_RECOVERY auto-activates regardless of user preference.
User attempts SwitchTheme to AIOS_DEFAULT during recovery.
SwitchTheme rejected with RecoveryThemeImmutable.
Recovery theme tokens (COLOR_BOUNDARY_RECOVERY, TYPOGRAPHY_RECOVERY) verified against
all available themes — ΔE ≥ 30, font family different.
```

### Fixture 5 — Constitutional icon substitution rejected

```text
USER_THEME ships:
  ICON_CONSTITUTIONAL_SECURITY_SHIELD: aiosfs_pointer to a custom heart-shaped icon
                                       content_hash: <not matching canonical>

LoadTheme:
  step 7 detects content_hash mismatch with AIOS_DEFAULT canonical hash
  ConstitutionalIconAltered failure
  THEME_REJECTED evidence emitted
```

### Fixture 6 — Animation timing attempt against INV-020

```text
USER_THEME ships:
  motion: { MOTION_DURATION_INSTANT: { duration_ms: 200 } }

LoadTheme:
  validation rejects: MOTION_DURATION_INSTANT must be 0 ms
  ThemeInvariantViolated with token name MOTION_DURATION_INSTANT
  THEME_REJECTED evidence emitted; previous theme remains
```

### Fixture 7 — Recovery-namespace squatting

```text
USER_THEME ships:
  COLOR_BOUNDARY_GROUP: hue X
  COLOR_BOUNDARY_RECOVERY: same hue X (or ΔE < 30 from AIOS_RECOVERY's recovery boundary)

LoadTheme:
  cross-theme distinctness check (§7.3) detects ΔE < 30 from AIOS_RECOVERY
  ThemeInvariantViolated with binding "recovery cross-theme distinctness"
  THEME_REJECTED
```

### Fixture 8 — Constitutional component override attempt

```text
USER_THEME ships:
  components: [
    {
      name: COMPONENT_SECURITY_BANNER,
      schema_node_template: <without trust indicator child>,
      constitutional: false,  # tries to claim non-constitutional
    }
  ]

LoadTheme:
  step 6 — COMPONENT_SECURITY_BANNER is constitutional in the system catalog;
  cannot be redeclared as non-constitutional. ThemeOverridesConstitutional.
  THEME_REJECTED.
```

## 14. Telemetry contract

All metrics MUST use bounded label cardinality. **theme_id, owner_subject_canonical_id, group_id are NEVER labels.**

| Metric                                                | Type      | Labels (closed)                                                                                                                         |
| ----------------------------------------------------- | --------- | --------------------------------------------------------------------------------------------------------------------------------------- |
| `visual_theme_load_total`                             | counter   | `theme_kind`, `result` (success/error), `error_code`                                                                                    |
| `visual_theme_load_duration_seconds`                  | histogram | `theme_kind`, `result`                                                                                                                  |
| `visual_theme_active`                                 | gauge     | `theme_kind`                                                                                                                            |
| `visual_theme_switch_total`                           | counter   | none                                                                                                                                    |
| `visual_theme_invariant_violation_total`              | counter   | `binding` (closed enum: ai_human / recovery_normal / trust / chrome_zone / motion_instant / constitutional_icon / cross_theme_recovery) |
| `visual_token_resolve_duration_seconds`               | histogram | `token_class` (color/typography/spacing/motion/icon), `cache` (hit/miss)                                                                |
| `visual_recovery_theme_activated_total`               | counter   | none                                                                                                                                    |
| `visual_constitutional_icon_alteration_attempt_total` | counter   | `icon_token` (closed enum, 8 values)                                                                                                    |

## 15. Acceptance criteria

- [ ] All token-name enums (`ColorTokenName`, `TypographyTokenName`, `SpacingTokenName`, `MotionTokenName`, `IconTokenName`, `ComponentName`, `DistinctionAxis`) are closed.
- [ ] `MOTION_DURATION_INSTANT` is constitutionally pinned at 0 ms; non-zero theme values rejected.
- [ ] `Theme` is Ed25519-signed; signature failure rejects load.
- [ ] ΔE distinctness ≥ 25 in CIEDE2000 enforced at load for all constitutional pairs in §7.1.
- [ ] Multi-axis distinction (≥ 2 axes for AI/human, trust, chrome zone; ≥ 3 axes for recovery/normal) enforced in §7.2.
- [ ] Recovery cross-theme distinctness (`ΔE ≥ 30`) enforced in §7.3.
- [ ] Constitutional icons cannot be replaced (canonical hash check).
- [ ] Constitutional component recipes cannot be overridden by themes.
- [ ] Recovery theme auto-activates in recovery mode and is not user-selectable.
- [ ] Theme bundle size and per-asset size limits prevent token-flood DoS.
- [ ] Cross-renderer mapping in §8 produces visually equivalent rendering across KDE / Web / CLI within accessibility and target capability.
- [ ] All eight golden fixtures (§13) produce the specified outcomes.
- [ ] Telemetry conforms to §14 cardinality bounds.
- [ ] L0 INV-019, INV-020, INV-021, INV-022 are each implemented by an enumerated section in §7.

## 16. Open deferrals

- **Concrete theme bundle (`AIOS_DEFAULT`).** Specific hex codes, the chosen typeface family, the icon glyph set — stage 3 of the visual plan. Deferred until first renderer prototype exists; will be authored against real surface contact.
- **Concrete theme bundle (`AIOS_RECOVERY`).** Same — deferred to stage 3 with explicit recovery-aesthetic constraints.
- **Theme marketplace** — third-party theme distribution, ratings, signed publisher chain. Deferred to L10 marketplace.
- **Per-tenant theming** — themes scoped per tenant in a future multi-tenant Rev. Deferred per S4.1 Q1 (`groups/` not `tenants/` in Rev.2).
- **Animated icon support** — Lottie / animated SVG. Deferred; `IconValue` schema is static-asset-only in Rev.2.
- **High-DPI / variable-DPI tokens** — current contract assumes per-token logical pixel sizing. DPI-adaptive scaling deferred.
- **Color-spectrum coverage** — additional palette tokens for specialized scenarios (heatmaps, charts with > 8 categories). Deferred.
- **Localization-aware typography** — bilingual typography tokens (e.g., a Cyrillic-and-Latin paired family with consistent x-height). Deferred to typeface decisions in stage 3.
- **Sound design tokens** — earcons, alert tones, agent-voice prosody. Deferred to L7 voice renderer when refined.
- **Theme inheritance / cascading** — a theme that extends another. Currently themes are flat. Deferred.
- **Per-component theme overrides** — fine-grained per-component customization within a theme. Deferred; Rev.2 themes are token-set-only.

## Appendix A — Full Proto IDL

```proto
syntax = "proto3";
package aios.visual.v1alpha1;

import "google/protobuf/timestamp.proto";

// ============================================================================
// Service
// ============================================================================

service VisualLanguageService {
  rpc LoadTheme(LoadThemeRequest) returns (LoadThemeResponse);
  rpc SwitchTheme(SwitchThemeRequest) returns (SwitchThemeResponse);
  rpc ListThemes(ListThemesRequest) returns (ListThemesResponse);
  rpc GetActiveTheme(GetActiveThemeRequest) returns (GetActiveThemeResponse);
  rpc ResolveTokens(ResolveTokensRequest) returns (ResolveTokensResponse);
  rpc ValidateTheme(ValidateThemeRequest) returns (ValidateThemeResponse);
  rpc GetVisualLanguageInfo(GetVisualLanguageInfoRequest) returns (GetVisualLanguageInfoResponse);
}

// ============================================================================
// Closed enums
// ============================================================================

enum ColorTokenName {
  COLOR_TOKEN_NAME_UNSPECIFIED = 0;
  COLOR_ACTION_HUMAN = 1;
  COLOR_ACTION_AI = 2;
  COLOR_ACTION_SYSTEM = 3;
  COLOR_ACTION_RECOVERY = 4;
  COLOR_SURFACE_BACKGROUND = 10;
  COLOR_SURFACE_CONTENT = 11;
  COLOR_SURFACE_OVERLAY = 12;
  COLOR_SURFACE_CHROME = 13;
  COLOR_TEXT_PRIMARY = 20;
  COLOR_TEXT_SECONDARY = 21;
  COLOR_TEXT_MUTED = 22;
  COLOR_TEXT_INVERSE = 23;
  COLOR_TEXT_CODE = 24;
  COLOR_TEXT_AUDIT = 25;
  COLOR_SEMANTIC_SUCCESS = 30;
  COLOR_SEMANTIC_WARNING = 31;
  COLOR_SEMANTIC_DANGER = 32;
  COLOR_SEMANTIC_INFO = 33;
  COLOR_TRUST_VERIFIED = 40;
  COLOR_TRUST_UNVERIFIED = 41;
  COLOR_TRUST_DEGRADED = 42;
  COLOR_TRUST_DENIED = 43;
  COLOR_EVIDENCE_PERMANENT = 50;
  COLOR_EVIDENCE_EXTENDED = 51;
  COLOR_EVIDENCE_STANDARD = 52;
  COLOR_BOUNDARY_GROUP = 60;
  COLOR_BOUNDARY_SCOPE = 61;
  COLOR_BOUNDARY_RECOVERY = 62;
  COLOR_BORDER_DEFAULT = 70;
  COLOR_BORDER_FOCUS = 71;
  COLOR_BORDER_DIVIDER = 72;
}

enum TypographyTokenName {
  TYPOGRAPHY_TOKEN_NAME_UNSPECIFIED = 0;
  TYPOGRAPHY_DISPLAY_LG = 1;
  TYPOGRAPHY_DISPLAY_MD = 2;
  TYPOGRAPHY_DISPLAY_SM = 3;
  TYPOGRAPHY_HEADING_LG = 4;
  TYPOGRAPHY_HEADING_MD = 5;
  TYPOGRAPHY_HEADING_SM = 6;
  TYPOGRAPHY_BODY_LG = 10;
  TYPOGRAPHY_BODY_MD = 11;
  TYPOGRAPHY_BODY_SM = 12;
  TYPOGRAPHY_CAPTION = 13;
  TYPOGRAPHY_CODE_LG = 20;
  TYPOGRAPHY_CODE_MD = 21;
  TYPOGRAPHY_CODE_SM = 22;
  TYPOGRAPHY_AUDIT = 23;
  TYPOGRAPHY_RECOVERY = 24;
  TYPOGRAPHY_AI_ORIGIN = 25;
}

enum SpacingTokenName {
  SPACING_TOKEN_NAME_UNSPECIFIED = 0;
  SPACING_NONE = 1;
  SPACING_XXS = 2;
  SPACING_XS = 3;
  SPACING_SM = 4;
  SPACING_MD = 5;
  SPACING_LG = 6;
  SPACING_XL = 7;
  SPACING_XXL = 8;
  SPACING_HUGE = 9;
}

enum MotionTokenName {
  MOTION_TOKEN_NAME_UNSPECIFIED = 0;
  MOTION_DURATION_INSTANT = 1;
  MOTION_DURATION_FAST = 2;
  MOTION_DURATION_NORMAL = 3;
  MOTION_DURATION_SLOW = 4;
  MOTION_DURATION_DELIBERATE = 5;
  MOTION_EASING_LINEAR = 10;
  MOTION_EASING_STANDARD = 11;
  MOTION_EASING_DECELERATE = 12;
  MOTION_EASING_ACCELERATE = 13;
  MOTION_EASING_CRITICAL = 14;
}

enum IconTokenName {
  ICON_TOKEN_NAME_UNSPECIFIED = 0;
  ICON_CONSTITUTIONAL_SECURITY_SHIELD = 1;
  ICON_CONSTITUTIONAL_RECOVERY_LOCK = 2;
  ICON_CONSTITUTIONAL_AI_INDICATOR = 3;
  ICON_CONSTITUTIONAL_HUMAN_INDICATOR = 4;
  ICON_CONSTITUTIONAL_EVIDENCE_CHAIN = 5;
  ICON_CONSTITUTIONAL_TRUST_CHECK = 6;
  ICON_CONSTITUTIONAL_TAMPER_WARNING = 7;
  ICON_CONSTITUTIONAL_GROUP_BOUNDARY = 8;
  ICON_SEMANTIC_SUCCESS = 20;
  ICON_SEMANTIC_WARNING = 21;
  ICON_SEMANTIC_DANGER = 22;
  ICON_SEMANTIC_INFO = 23;
  ICON_ACTION_APPROVE = 40;
  ICON_ACTION_REJECT = 41;
  ICON_ACTION_DEFER = 42;
  ICON_ACTION_VIEW_EVIDENCE = 43;
  ICON_ACTION_OPEN = 44;
  ICON_ACTION_CLOSE = 45;
  ICON_ACTION_RETIRE = 46;
}

enum ComponentName {
  COMPONENT_NAME_UNSPECIFIED = 0;
  COMPONENT_SECURITY_BANNER = 1;
  COMPONENT_RECOVERY_SHIELD = 2;
  COMPONENT_AI_SUBJECT_BADGE = 3;
  COMPONENT_HUMAN_SUBJECT_BADGE = 4;
  COMPONENT_TRUST_INDICATOR = 5;
  COMPONENT_EVIDENCE_LINK_TILE = 6;
  COMPONENT_TAMPER_WARNING = 7;
  COMPONENT_AGENT_MESSAGE = 20;
  COMPONENT_APPROVAL_PROMPT = 21;
  COMPONENT_AUDIT_TRAIL = 22;
  COMPONENT_ACTION_ENVELOPE_VIEW = 23;
  COMPONENT_GROUP_HEADER = 24;
  COMPONENT_INBOX_TILE = 25;
}

enum DistinctionAxis {
  DISTINCTION_AXIS_UNSPECIFIED = 0;
  AXIS_HUE = 1;
  AXIS_PATTERN = 2;
  AXIS_TYPOGRAPHY = 3;
  AXIS_OUTLINE = 4;
  AXIS_OPACITY = 5;
  AXIS_POSITION = 6;
}

enum ThemeKind {
  THEME_KIND_UNSPECIFIED = 0;
  AIOS_FALLBACK = 1;
  AIOS_DEFAULT = 2;
  AIOS_RECOVERY = 3;
  USER_THEME = 4;
  PUBLISHER_THEME = 5;
}

enum LoadThemeErrorCode {
  LOAD_THEME_ERROR_CODE_UNSPECIFIED = 0;
  THEME_SIGNATURE_INVALID = 1;
  THEME_INCOMPLETE = 2;
  THEME_INVARIANT_VIOLATED = 3;
  THEME_OVERRIDES_CONSTITUTIONAL = 4;
  CONSTITUTIONAL_ICON_ALTERED = 5;
  THEME_BUNDLE_TOO_LARGE = 6;
  RECOVERY_THEME_IMMUTABLE = 7;
  VISUAL_CATALOG_MISMATCH = 8;
  THEME_SERVICE_INTERNAL = 9;
}

// ============================================================================
// Token value messages
// ============================================================================

message ColorValue {
  // Linear-light reference; theme can supply RGB or LCH; renderers convert
  float r = 1;            // 0..1
  float g = 2;            // 0..1
  float b = 3;            // 0..1
  float a = 4;            // 0..1
}

message TypographyValue {
  string family = 1;
  uint32 size_logical_pixels = 2;
  uint32 weight = 3;
  bool italic = 4;
  float line_height_multiple = 5;
  float letter_spacing_em = 6;
}

message SpacingValue {
  uint32 base_pixels = 1;            // theme's base unit (e.g., 8)
  float multiplier = 2;              // per-slot multiplier
}

message MotionValue {
  uint32 duration_ms = 1;
  string easing_curve_id = 2;        // matches MOTION_EASING_*
  bool reduced_motion_zero = 3;
}

message IconValue {
  string aiosfs_pointer = 1;
  string vector_glyph_id = 2;
  string description = 3;
  bytes content_hash = 4;
}

message ColorTokenValue { ColorTokenName name = 1; ColorValue value = 2; }
message TypographyTokenValue { TypographyTokenName name = 1; TypographyValue value = 2; }
message SpacingTokenValue { SpacingTokenName name = 1; SpacingValue value = 2; }
message MotionTokenValue { MotionTokenName name = 1; MotionValue value = 2; }
message IconTokenValue { IconTokenName name = 1; IconValue value = 2; }

// ============================================================================
// Component recipe
// ============================================================================

message ComponentRecipe {
  ComponentName name = 1;
  string schema_node_template = 2;
  repeated TokenBinding token_bindings = 3;
  repeated DistinctionAxis required_axes = 4;
  bool constitutional = 5;
  string version = 6;
}

message TokenBinding {
  string node_path = 1;
  oneof token {
    ColorTokenName color = 2;
    TypographyTokenName typography = 3;
    SpacingTokenName spacing = 4;
    MotionTokenName motion = 5;
    IconTokenName icon = 6;
  }
}

// ============================================================================
// Theme
// ============================================================================

message Theme {
  string theme_id = 1;
  ThemeKind theme_kind = 2;
  string display_name = 3;
  string issuer = 4;
  google.protobuf.Timestamp issued_at = 5;
  bytes ed25519_signature = 6;

  repeated ColorTokenValue colors = 10;
  repeated TypographyTokenValue typography = 11;
  repeated SpacingTokenValue spacing = 12;
  repeated MotionTokenValue motion = 13;
  repeated IconTokenValue icons = 14;

  string a11y_profile_id = 20;
  bool supports_reduced_motion = 21;
  bool supports_high_contrast = 22;
  bool supports_colorblind = 23;
}

// ============================================================================
// RPC request/response
// ============================================================================

message LoadThemeRequest { Theme theme = 1; }
message LoadThemeResponse {
  oneof result {
    string registered_theme_id = 1;
    LoadThemeError error = 2;
  }
}

message LoadThemeError {
  LoadThemeErrorCode code = 1;
  string message = 2;
  string failing_binding = 3;        // populated when code = THEME_INVARIANT_VIOLATED
  string failing_token = 4;          // populated when code = THEME_INVARIANT_VIOLATED
  string failing_icon = 5;           // populated when code = CONSTITUTIONAL_ICON_ALTERED
}

message SwitchThemeRequest { string subject_canonical_id = 1; string theme_id = 2; }
message SwitchThemeResponse {
  oneof result {
    string activated_theme_id = 1;
    LoadThemeError error = 2;
  }
}

message ListThemesRequest { ThemeKind kind = 1; bool include_recovery = 2; }
message ListThemesResponse { repeated Theme themes = 1; }

message GetActiveThemeRequest { string subject_canonical_id = 1; }
message GetActiveThemeResponse { Theme theme = 1; bool is_recovery_active = 2; }

message ResolveTokensRequest {
  string theme_id = 1;
  repeated ColorTokenName colors = 2;
  repeated TypographyTokenName typography = 3;
  repeated SpacingTokenName spacing = 4;
  repeated MotionTokenName motion = 5;
  repeated IconTokenName icons = 6;
}
message ResolveTokensResponse {
  repeated ColorTokenValue colors = 1;
  repeated TypographyTokenValue typography = 2;
  repeated SpacingTokenValue spacing = 3;
  repeated MotionTokenValue motion = 4;
  repeated IconTokenValue icons = 5;
}

message ValidateThemeRequest { Theme theme = 1; }
message ValidateThemeResponse { bool valid = 1; LoadThemeError error = 2; }

message GetVisualLanguageInfoRequest {}
message GetVisualLanguageInfoResponse {
  string visual_catalog_version = 1;
  string schema_version = 2;          // "aios.visual.v1alpha1"
  uint32 active_theme_count = 3;
  uint32 max_theme_bundle_bytes = 4;
}
```

## See also

- [L0 §3 Constitutional Invariants — INV-019..INV-022](../L0_Governance_Evidence_Safety/04_invariants.md)
- [S5.1 — Identity Model](../L4_Policy_Identity_Vault/03_identity_model.md)
- [S7.1 — Surface + Composition Model](01_surface_composition.md)
- [S7.2 — Shared UI Schema](02_shared_ui_schema.md)
- [L7 Overview](00_overview.md)
- [Rev.2 Master Index](../00_MASTER_INDEX.md)
