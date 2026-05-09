# CLI Renderer (Rev.2)

| Field          | Value                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| -------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Status         | `REAL` (initial; written 2026-05-12; E1 evidence — file exists, structural contract complete)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| Phase tag      | S7.6                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| Layer          | L7 Interaction Renderers                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| Schema package | `aios.renderer.cli.v1alpha1`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| Consumes       | S7.1 (Surface + Composition; CLI column of §4.3 mapping; chrome boundary), S7.2 (Shared UI Schema; closed `NodeKind` 19 values), S7.3 (Visual Language; CLI mapping in §8.3 — bold/italic/underline/inverse ANSI proxies plus `DistinctionAxis` PATTERN+TYPOGRAPHY), S5.3 (Approval Mechanics; `CLI_TTY_PROMPT` channel), S9.1 (Recovery Boundary; recovery TTY is the primary recovery surface), S5.1 (Identity; recovery-mode flag, operator subjects), S0.1 (Action Envelope; submission), L0 INV-001/002/019/020/021/022/023 (constitutional visual + recovery obligations) |
| Produces       | typed CLI renderer service contract; deterministic compilation rules from S7.2 `NodeKind` to terminal primitives (UTF-8 box-drawing, ANSI styling, ASCII fallback); closed `CliRenderMode`, `CliCompilationResult`, `CliInputMode`, `AnsiSupportLevel` enums; recovery-TTY rendering rules; scripting-mode JSON contract; ANSI-injection sanitization rules; ten record types queued for S3.1                                                                                                                                                                                   |

## §1 Purpose

The CLI renderer is the **terminal-only** AIOS surface. It exists for three reasons that the KDE (S7.4) and Web (S7.5) renderers cannot satisfy:

1. **Recovery (INV-001 + INV-022).** When the host boots into recovery mode (S9.1 `RecoveryStage`), no graphical session is brought up. The operator interacts with AIOS exclusively through a controlling TTY. The CLI renderer is the **primary** and often **only** rendering surface during recovery.
2. **Headless administration.** AIOS hosts (TrueNAS-class storage, edge nodes, embedded gateways) frequently run without a display. Operator-grade administration over `ssh` requires a renderer that compiles a UI tree to a text grid.
3. **Scripting and automation.** Tooling that is not an AI agent — backup scripts, monitoring probes, CI pipelines — needs a typed, machine-parseable surface that respects the same approval and evidence discipline as the human-facing renderers. Free-form shell escapes are not an acceptable substitute (per S10.1 `AdapterIOMode` — free-form shell input is not a supported adapter mode).

This spec defines:

1. The closed `CliRenderMode` enum (4 values + UNSPECIFIED) and the mode-selection rule.
2. The closed `CliCompilationResult` enum (10 values + UNSPECIFIED).
3. The closed `CliInputMode` enum (5 values + UNSPECIFIED).
4. The closed `AnsiSupportLevel` enum (4 values + UNSPECIFIED).
5. The deterministic compilation table from every S7.2 `NodeKind` (19 values) to terminal output.
6. The recovery-TTY rendering rules (kind restrictions, theme lock, no autocomplete, operator authentication via challenge-response).
7. The scripting-mode contract — structured JSON output, no interactive prompts, exit-code discipline, evidence receipt-id propagation.
8. The constitutional distinction encoding in CLI: `DistinctionAxis` PATTERN + TYPOGRAPHY are the binding axes (color is best-effort; `ΔE` distinctness obviously does not apply when `AnsiSupportLevel = MONOCHROME`).
9. ANSI escape-sequence sanitization rules.
10. Ten FOREVER- or STANDARD-retained evidence record types queued for S3.1.

Out of scope: the recovery boot kernel pipeline (S9.3); the TTY device driver layer; specific terminal emulator quirks (xterm, kitty, alacritty); voice and mobile renderers (S7.7+, deferred); concrete byte-level ANSI sequences (those are a stage-3 implementation detail, not a constitutional commitment).

## §2 Core invariants

- **I1 — Renderer owns no authoritative state.** The CLI renderer is a target binding. Authoritative state (objects, evidence, policy decisions, sandbox profiles, surfaces, UI trees, visual tokens) lives in AIOS-FS, the policy kernel, the surface service, the UI schema service, and the visual language service. The renderer reads and emits text; it does not become a source of truth.
- **I2 — Recovery-TTY is the primary recovery surface.** When `subject.recovery_mode = true` (per S5.1 §7) and no graphical session is active, the CLI renderer is the only renderer permitted to bind to the controlling TTY. It enforces the recovery surface stack restriction from S7.1 §I6 — only `AIOS_SURFACE` is permitted; `APP_SURFACE` and `STREAM_SURFACE` creation requests are refused with `RECOVERY_SURFACE_KIND_REJECTED`. Binds INV-022 and INV-001.
- **I3 — Trust-bearing nodes constitutionally cannot be authored by AI subjects.** The trust-bearing kinds `SECURITY_INDICATOR`, `APPROVAL_PROMPT`, `EVIDENCE_LINK` are signed at the UI tree-signing service (S7.2 §I5) and are never compiled from AI-issued trees. The CLI renderer trusts the issuer's signature; if a tree containing those kinds is presented with an AI-issuer signature, it is rejected at S7.2 validation before reaching this renderer.
- **I4 — AIOS chrome boundary in CLI is the top fixed banner.** Per S7.1 §4.3 CLI column, the CHROME composition zone maps to a top fixed banner showing subject + current action + evidence id. The renderer reserves the topmost terminal row(s) for this banner across every render cycle in `NORMAL_INTERACTIVE` and `RECOVERY_TTY` modes. App content cannot scroll into or overwrite the chrome region. Binds INV-020 and INV-023.
- **I5 — `SECURITY_INDICATOR` rendered first, always.** When a UI tree contains a `SECURITY_INDICATOR` node, the CLI renderer emits its compiled text **before any other content** of the tree, regardless of structural ordering. This honors INV-020 — trust indicators are never displaced by app content. Tampering with the rendering order via tree manipulation is detected at S7.2 signature validation (the entire tree signature would invalidate).
- **I6 — AI-vs-human distinction is multi-axis (PATTERN + TYPOGRAPHY).** AI-origin nodes (per S7.2 `is_ai_origin = true`) are compiled with the `PATTERN` axis (a literal `[AI]` prefix glyph sequence, single-byte ASCII fallback `<AI>`) AND the `TYPOGRAPHY` axis (italic ANSI when supported; otherwise underlined). Color is a best-effort third axis but is **never the sole carrier**. When `AnsiSupportLevel = MONOCHROME`, the PATTERN+TYPOGRAPHY axes alone carry INV-021. This binds S7.3 §I4 (multi-axis distinction) at the CLI target.
- **I7 — No autocomplete in recovery.** Autocomplete, command history search, and any speculative completion machinery is disabled in `RECOVERY_TTY` mode. Autocomplete in normal mode may consult the L5 capability catalog (read-only); in recovery, L5 is unavailable per INV-001, and even if it were, autocomplete is suppressed to prevent the operator from accidentally executing a half-typed catastrophic command. Operator must type each token in full.
- **I8 — Auto-confirm rejection in `NORMAL_INTERACTIVE` mode.** When the renderer is attached to a TTY in `NORMAL_INTERACTIVE` mode and an `APPROVAL_PROMPT` is presented, the renderer reads the operator's response synchronously from the TTY. Piped input (`echo y | aios ...`) is **rejected**: the renderer detects that stdin is not a TTY (`isatty(0) == false`) and refuses to consume the piped byte as an approval response. The operator must respond on the controlling TTY. Binds INV-009 (approvals bind to one request) and INV-002 against piped-Y bypass attacks.
- **I9 — ANSI escape injection sanitization.** Any text content compiled into terminal output (text payloads, list items, table cells, agent messages, evidence labels) is sanitized: only an allowlisted subset of ANSI escape sequences emitted by the renderer itself is permitted. Embedded ESC characters (`0x1B`) in user- or AI-supplied content are stripped; CSI sequences from non-renderer sources are stripped; OSC sequences (terminal-title manipulation, hyperlink injection) are stripped unless explicitly allowlisted. Detected injection attempts emit `CLI_ANSI_INJECTION_BLOCKED` evidence (FOREVER retention) and the offending node is replaced with a `[content sanitized]` placeholder.
- **I10 — Scripting mode is non-interactive and structured.** When `CliRenderMode = SCRIPTING`, the renderer emits a stable JSON document on stdout per request, never opens a prompt, never blocks on TTY input, and exits with a non-zero status code on policy denial, sanitization rejection, or render failure. Evidence receipt-ids are propagated as fields in the JSON document, never as ANSI-styled text. Binds the typed-action discipline of S0.1 to non-human callers.

## §3 Closed enums

All enums in this section are closed. Adding a value is a versioned spec change. Renderers reject unknown values with `CLI_UNKNOWN_ENUM_VALUE` at decode time.

### §3.1 `CliRenderMode` (4 values)

```proto
enum CliRenderMode {
  CLI_RENDER_MODE_UNSPECIFIED = 0;

  NORMAL_INTERACTIVE = 1;     // human at a TTY; full feature set, ANSI styling, prompts permitted
  SCRIPTING = 2;              // tooling caller; structured JSON output; no prompts; exit codes
  RECOVERY_TTY = 3;           // recovery-boot console; AIOS_SURFACE only; no L5; locked theme
  DEGRADED_NO_COLOR = 4;      // ANSI not available or terminal hostile; ASCII-only structural fallback
}
```

#### §3.1.1 Mode selection rule (deterministic)

The mode is selected by the renderer at session start using a fixed precedence:

```text
IF subject.recovery_mode == true
  THEN mode = RECOVERY_TTY
ELSE IF caller invoked with --scripting flag OR stdout is a pipe AND --interactive not asserted
  THEN mode = SCRIPTING
ELSE IF AnsiSupportLevel == MONOCHROME OR terminal capability advertises no ANSI
  THEN mode = DEGRADED_NO_COLOR
ELSE
  mode = NORMAL_INTERACTIVE
```

The selected mode is recorded in the session record and emitted as `CLI_RENDER_STARTED` evidence (§10).

### §3.2 `CliCompilationResult` (10 values)

```proto
enum CliCompilationResult {
  CLI_COMPILATION_RESULT_UNSPECIFIED = 0;

  COMPILED_RICH = 1;                       // Full ANSI styling + UTF-8 box-drawing applied
  COMPILED_PLAIN = 2;                      // Structural compilation succeeded; no styling (mode = SCRIPTING or DEGRADED_NO_COLOR)
  COMPILED_RECOVERY = 3;                   // Recovery-TTY compilation; only AIOS_SURFACE, AIOS_RECOVERY theme, ASCII frames
  DEGRADED_PARTIAL = 4;                    // Tree compiled but one or more node kinds unsupported in CLI; placeholder shown
  FAILED_NODE_KIND_UNSUPPORTED = 5;        // A required (not placeholder-able) kind cannot render in CLI
  FAILED_TREE_SIGNATURE_INVALID = 6;       // S7.2 tree signature failed; renderer refuses partial render
  FAILED_RECOVERY_KIND_REJECTED = 7;       // Tree references APP_SURFACE/STREAM_SURFACE in RECOVERY_TTY
  FAILED_ANSI_INJECTION_BLOCKED = 8;       // Sanitizer found injection in content; tree rejected
  FAILED_TREE_TOO_LARGE = 9;               // Tree node count exceeds CLI bound (per S7.2 §I8 + CLI cap §6.4)
  FAILED_RENDERER_INTERNAL = 10;           // Renderer bug; emits FOREVER evidence
}
```

The result is reported as a field on `RenderTreeResponse` (§11) and is the source of the renderer's exit code in `SCRIPTING` mode.

### §3.3 `CliInputMode` (5 values)

```proto
enum CliInputMode {
  CLI_INPUT_MODE_UNSPECIFIED = 0;

  INTERACTIVE_TTY = 1;        // stdin is a TTY; the renderer may issue prompts and read responses
  SCRIPT_PIPED = 2;           // stdin is a pipe carrying structured input (e.g., JSON request body)
  NON_INTERACTIVE = 3;        // no stdin; renderer is invoked once, emits output, exits
  NO_TTY = 4;                 // no controlling TTY at all (e.g., systemd unit without TTY allocation)
  READ_ONLY_QUERY = 5;        // caller asserts read-only intent; renderer rejects any APPROVAL_PROMPT in tree
}
```

`INTERACTIVE_TTY` is required for `NORMAL_INTERACTIVE` mode and is the only input mode under which the renderer reads operator approval responses (§I8). `SCRIPT_PIPED` is the only input mode permitted under `SCRIPTING` mode. `RECOVERY_TTY` mode requires `INTERACTIVE_TTY`.

### §3.4 `AnsiSupportLevel` (4 values)

```proto
enum AnsiSupportLevel {
  ANSI_SUPPORT_LEVEL_UNSPECIFIED = 0;

  TRUECOLOR = 1;              // 24-bit color (`COLORTERM=truecolor`); tokens compile to nearest sRGB
  COLOR_256 = 2;              // 256-color (`TERM=xterm-256color`); tokens compile to nearest 256-color slot
  COLOR_16 = 3;               // 16-color (`TERM=linux`, basic xterm); tokens compile to nearest of 16 ANSI slots
  MONOCHROME = 4;             // No color (`TERM=dumb`, NO_COLOR env, or sanitizer-asserted); ASCII frames; PATTERN+TYPOGRAPHY axes only
}
```

Detection is via the `terminfo` capability strings and the `COLORTERM`/`NO_COLOR` environment variables. `NO_COLOR=1` (per the `NO_COLOR` informal standard) forces `MONOCHROME` regardless of detected capability. `MONOCHROME` triggers the box-drawing fallback to ASCII (`+`, `-`, `|`) per §5.

### §3.5 Internal: `CliEvidenceRecordKind` (10 values)

The CLI renderer queues ten record types for S3.1 (§10). They are also represented locally:

```proto
enum CliEvidenceRecordKind {
  CLI_EVIDENCE_RECORD_KIND_UNSPECIFIED = 0;

  CLI_RENDER_STARTED = 1;                  // STANDARD_24M
  CLI_RENDER_FAILED = 2;                   // EXTENDED_60M
  CLI_NODE_KIND_UNSUPPORTED = 3;           // STANDARD_24M
  CLI_RECOVERY_KIND_REJECTED = 4;          // FOREVER
  CLI_AUTO_CONFIRM_REJECTED = 5;           // FOREVER
  CLI_ANSI_INJECTION_BLOCKED = 6;          // FOREVER
  CLI_DEGRADED_NO_TTY = 7;                 // STANDARD_24M
  CLI_SCRIPTING_MODE_INVOKED = 8;          // STANDARD_24M
  CLI_OPERATOR_AUTHENTICATED = 9;          // STANDARD_24M
  CLI_TRUST_INDICATOR_REORDERED = 10;      // FOREVER (tamper-class)
}
```

## §4 Mode lifecycle

A CLI session passes through this FSM:

```text
INIT → MODE_SELECTED → CHROME_BANNER_PINNED → READY → (RENDER_LOOP)* → EXIT
```

- **INIT** — process started; no mode selected; no chrome rendered.
- **MODE_SELECTED** — `CliRenderMode` chosen per §3.1.1; `CLI_RENDER_STARTED` evidence emitted.
- **CHROME_BANNER_PINNED** — for `NORMAL_INTERACTIVE` and `RECOVERY_TTY`, the top banner is reserved; `tput rmcup`/`smcup` discipline applied to ensure the banner row is not overwritten by content scrolling.
- **READY** — renderer awaits `RenderTree` calls (§11).
- **RENDER_LOOP** — `RenderTree` calls compile a tree and emit text; per-render evidence emitted on success, failure, or sanitization.
- **EXIT** — exit code reflects the terminal state (0 = success; 1 = render failure; 2 = policy denial; 3 = ANSI injection blocked; 4 = signature invalid; 5 = recovery kind rejected; 64+ = renderer internal; per `sysexits.h`-aligned discipline in §9).

`RECOVERY_TTY` sessions additionally pin operator-authenticated identity (§7) before transitioning to `READY`.

## §5 `NodeKind` → terminal compilation

Every value of S7.2's closed `NodeKind` enum (19 declared kinds plus `NODE_KIND_UNSPECIFIED`) compiles to a deterministic terminal output. The renderer rejects unknown kinds with `FAILED_NODE_KIND_UNSUPPORTED` and emits `CLI_NODE_KIND_UNSUPPORTED` evidence. The mapping is a function: given a valid `Node` and a known `(CliRenderMode, AnsiSupportLevel)`, exactly one compilation is selected.

| `NodeKind`           | Terminal compilation                                                                                                                                                                                                                                                                                                                                                                                                          |
| -------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `CONTAINER`          | Visual grouping by indentation. `LayoutMode = STACK_VERTICAL` → newline-separated children; `STACK_HORIZONTAL` → space-separated when total width fits; otherwise vertical fallback. `GRID` and `FLOW_WRAP` collapse to `STACK_VERTICAL` in CLI                                                                                                                                                                               |
| `DIVIDER`            | Full-width horizontal rule using box-drawing `─` (UTF-8) or `-` (ASCII) per `AnsiSupportLevel`                                                                                                                                                                                                                                                                                                                                |
| `SPACER`             | Blank line(s) per spacing token; flexible spacers collapse to one blank line in CLI                                                                                                                                                                                                                                                                                                                                           |
| `TEXT`               | Plain UTF-8 content; emphasis → italic ANSI (CSI `3 m`) when `AnsiSupportLevel ≥ COLOR_16`; strong → bold (CSI `1 m`); both stripped under MONOCHROME                                                                                                                                                                                                                                                                         |
| `HEADING`            | Bold text + blank line below; `level = 1` additionally emits a full-width `═` underline (UTF-8) or `=` (ASCII)                                                                                                                                                                                                                                                                                                                |
| `INLINE_CODE`        | Backtick-wrapped text with reverse video (CSI `7 m`) under `AnsiSupportLevel ≥ COLOR_16`; backtick-wrapped plain under MONOCHROME                                                                                                                                                                                                                                                                                             |
| `CODE_BLOCK`         | 4-space-indented multi-line block; optional language hint emitted as `// language: <hint>` on the line above; no syntax highlighting (would expand the ANSI surface area and complicate sanitization)                                                                                                                                                                                                                         |
| `CARD`               | ASCII-art-bordered text block: header line (with bold title), body text, optional action row. UTF-8 borders use `┌─┐ │ └─┘`; MONOCHROME falls back to `+--+ \| +--+`                                                                                                                                                                                                                                                          |
| `LIST`               | Bullet list `• ` (UTF-8) / `* ` (ASCII) for unordered; numbered `1. 2. 3.` for ordered. Indentation per nesting level. View-bound lists materialize the first page (`page_size`) and append `… (N more; cursor=<id>)` if cursor is non-empty                                                                                                                                                                                  |
| `TABLE`              | Box-drawing characters: `┌─┬─┐ │ ├─┼─┤ │ └─┴─┘` (UTF-8). Column widths computed per longest cell, wrapped at terminal width. **ASCII fallback under `AnsiSupportLevel = MONOCHROME`**: `+-+-+ \| +-+-+ \| +-+-+`. View-bound tables materialize the first page and append cursor footer                                                                                                                                       |
| `FORM`               | Field-by-field prompt-and-read. Each field renders as `<label>: <prompt>` with field-kind-specific input handling — `FIELD_TEXT` reads to newline; `FIELD_TEXT_MULTILINE` reads until a blank line or `EOF`; `FIELD_NUMBER` validates parse; `FIELD_BOOLEAN` accepts `y`/`n`/`yes`/`no`; `FIELD_ENUM` displays a numbered menu and reads a number. On submit, the renderer constructs the S0.1 action envelope (per S7.2 §I9) |
| `ACTION_BUTTON`      | Rendered as a numbered menu entry. Multiple `ACTION_BUTTON`s in a CONTAINER produce `1) <label>  2) <label>  …`; the renderer reads a number (or the action's leading character if unique) and submits the corresponding S0.1 envelope                                                                                                                                                                                        |
| `VISUALIZATION`      | **CLI cannot render.** Replaced with `[Visualization "<title>" — cannot render in CLI; switch to KDE or Web renderer]` followed by an EVIDENCE_LINK-style receipt-id. `DEGRADED_PARTIAL` result code emitted                                                                                                                                                                                                                  |
| `STREAM`             | **CLI cannot render.** Replaced with `[Stream "<title>" — cannot render in CLI; switch to KDE or Web renderer]`. `DEGRADED_PARTIAL` result code                                                                                                                                                                                                                                                                               |
| `SURFACE_EMBED`      | **CLI cannot render.** Replaced with `[Surface "<surface_id>" embedded — cannot render in CLI; switch to KDE or Web renderer]`. `DEGRADED_PARTIAL` result                                                                                                                                                                                                                                                                     |
| `SECURITY_INDICATOR` | **MANDATORY top-banner row** above all other tree content. Format: `[SECURE] subject=<canonical_id> action=<action_id> evidence=<receipt_id>`. Bold + reverse video ANSI under `≥ COLOR_16`; plain text + leading `[SECURE]` glyph under MONOCHROME. Compiled FIRST regardless of structural ordering (§I5)                                                                                                                   |
| `APPROVAL_PROMPT`    | Distinct **red/yellow framing** under `≥ COLOR_16` (red border for destructive, yellow for non-destructive); ASCII-frame `! ! ! APPROVAL ! ! !` markers under MONOCHROME. Body shows action summary + evidence-bound binding-id. Footer: `[y]es / [n]o / [a]bort: ` prompt, read from controlling TTY only (§I8)                                                                                                              |
| `EVIDENCE_LINK`      | Opaque receipt-id rendered as `evidence: evr_<ulid>` with a copy hint `(copy: $(aios evidence get evr_<ulid>))`. Hyperlinking via OSC 8 is **disabled** by default (sanitizer policy); operator copies the id manually                                                                                                                                                                                                        |
| `AGENT_MESSAGE`      | **Distinct prefix `[AI]`** on every line of the message (PATTERN axis); italic ANSI (CSI `3 m`) under `≥ COLOR_16` (TYPOGRAPHY axis); under MONOCHROME, prefix `<AI>` + underline (CSI `4 m`) which most monochrome terminals still render. Children of an AGENT_MESSAGE inherit the prefix but cannot themselves contain trust-bearing kinds (S7.2 enforced)                                                                 |
| (XR_SURFACE-bearing) | The CLI does not encounter `XR_SURFACE` directly; only via `SURFACE_EMBED`, which is handled as above                                                                                                                                                                                                                                                                                                                         |

### §5.1 Family compilation summary

| Family        | Primary primitive                                            | Renderable in CLI?         | Trust-bearing?                     |
| ------------- | ------------------------------------------------------------ | -------------------------- | ---------------------------------- |
| Structural    | indentation, blank lines                                     | yes                        | no                                 |
| Text          | plain text + ANSI styling                                    | yes                        | no                                 |
| Composite     | bullet/numbered list, box-drawing table, ASCII-bordered card | yes                        | no                                 |
| Interaction   | numbered menu, prompt-and-read                               | yes (only INTERACTIVE_TTY) | no                                 |
| Live / GPU    | `[cannot render]` placeholder                                | partial                    | no                                 |
| Trust-bearing | top banner, framed prompt, opaque receipt-id                 | yes                        | yes                                |
| AI-origin     | `[AI]` prefix + italic/underline                             | yes                        | no (but constitutionally distinct) |

### §5.2 Mandatory top-banner

In `NORMAL_INTERACTIVE` and `RECOVERY_TTY` modes, the renderer pins the top one-or-two terminal rows for the chrome banner. The banner is recomposed on every `RenderTree` call from the `SECURITY_INDICATOR` content. Non-banner content scrolls below; the banner does not scroll. Implementations may use `tput`/`terminfo` `cup` (cursor-position) and `csr` (change scrolling region) to enforce this. Failure to pin the banner (terminal does not support scrolling region) downgrades the renderer to `DEGRADED_NO_COLOR` and emits `CLI_DEGRADED_NO_TTY` evidence.

### §5.3 Per-mode allowed kinds

| Mode                 | Allowed `NodeKind`s                                                                                                                                                        |
| -------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `NORMAL_INTERACTIVE` | All 19 kinds; `VISUALIZATION`/`STREAM`/`SURFACE_EMBED` produce `DEGRADED_PARTIAL` placeholder                                                                              |
| `SCRIPTING`          | All 19 kinds compile to JSON fields; `APPROVAL_PROMPT` is **rejected** with `CLI_AUTO_CONFIRM_REJECTED` if `CliInputMode != SCRIPT_PIPED` carrying a pre-bound approval id |
| `RECOVERY_TTY`       | Only kinds compatible with `AIOS_SURFACE` (S7.1 §I6). `SURFACE_EMBED` referencing `APP_SURFACE` or `STREAM_SURFACE` is **rejected** with `FAILED_RECOVERY_KIND_REJECTED`   |
| `DEGRADED_NO_COLOR`  | All structural and text kinds; ANSI styling stripped; box-drawing falls back to ASCII                                                                                      |

## §6 Recovery-TTY mode

### §6.1 Activation

`RECOVERY_TTY` mode activates exactly when the renderer's session subject has `subject.recovery_mode = true` (per S5.1 §7) AND no graphical session is bound for the same subject. The mode is sticky for the entire session — recovery sessions cannot transition out of `RECOVERY_TTY` without a full process exit and reboot (per S9.1 exit-by-reboot rule).

### §6.2 Surface stack restriction (binds INV-022 + S7.1 §I6)

The renderer enforces:

- Only `AIOS_SURFACE` may be the host surface.
- `SURFACE_EMBED` nodes referencing `APP_SURFACE` or `STREAM_SURFACE` are rejected at tree validation with `FAILED_RECOVERY_KIND_REJECTED`.
- `STREAM` and `VISUALIZATION` nodes (which would imply external content sources) are rejected with the same code (recovery shows only static system state).
- `CLI_RECOVERY_KIND_REJECTED` evidence emitted (FOREVER retention) on each rejection.

### §6.3 Theme lock

The active visual theme is forced to `theme_aios_recovery` (the AIOS-root-signed recovery theme; per S7.3 §I6). User-authored themes are not loaded. The recovery theme drives:

- The chrome banner rendering: distinct ASCII frame style (e.g., double-line `╔═╗` UTF-8 / `=*=` ASCII frame characters) — visually distinct from normal-mode single-line frames.
- The `TYPOGRAPHY_RECOVERY` token compilation: bold + reverse video on banner rows; bold-only on heading rows; plain elsewhere.
- The `COLOR_BOUNDARY_RECOVERY` token: where color is available, recovery surfaces use a hue distinct from any normal-mode hue (per S7.3 cross-theme distinctness rule); under MONOCHROME, the frame style alone carries INV-022 distinctness.

### §6.4 Operator authentication

Recovery sessions are restricted to subjects in `_system` scope (per S9.1 §3.1 `RecoveryReadOnlyScope`/`RecoveryMutableScope`). The CLI renderer authenticates the operator via challenge-response **before** transitioning to `READY`:

1. The renderer issues a challenge nonce (16 random bytes, hex-encoded, displayed on the TTY).
2. The operator signs the nonce with their hardware-bound recovery key (Yubikey or equivalent) using a separate physical token interaction.
3. The signed nonce is read from the TTY (paste).
4. The renderer verifies the signature against the operator's recovery public key (loaded from `/aios/system/recovery/operators/<operator_id>/recovery_key.pub` in normal mode; in recovery mode, the public key is loaded from a recovery-only mirror per S9.1 §6).
5. On success: `CLI_OPERATOR_AUTHENTICATED` evidence emitted (STANDARD_24M); session enters `READY`.
6. On failure: process exits with code 64 (`EX_NOPERM`-aligned); `CLI_OPERATOR_AUTHENTICATED` is **not** emitted (failure evidence is owned by S5.1 identity service, not this renderer).

### §6.5 No autocomplete (binds INV-001)

Autocomplete, command history search, and any speculative completion machinery is disabled. In normal mode, autocomplete may consult the L5 capability catalog (read-only). In `RECOVERY_TTY` mode, L5 is unavailable per INV-001; even if it were available, autocomplete is suppressed. The operator must type each token in full. Up-arrow history is permitted but only across the current recovery session (no persistent history file is read or written in recovery).

### §6.6 Recovery banner content

The chrome banner in recovery mode shows:

```text
RECOVERY MODE — operator=_system:operator-<id>  reason=<RecoveryEntryReason>  ttl=<seconds-remaining>
```

The `ttl` countdown enforces the 8-hour hard cap from S9.1; when `ttl < 600 s`, the banner additionally renders in reverse video (where supported) and prepends `! ! ! TTL EXPIRES SOON ! ! !` to alert the operator.

## §7 Constitutional distinctions in CLI (INV-021 binding)

The CLI surface has the most-constrained visual axes of any AIOS renderer. Color is best-effort (and absent under `MONOCHROME`); spacing, motion, and iconography (in the strict glyph sense) are constrained by the terminal cell grid.

This spec binds **two** of S7.3's `DistinctionAxis` values as the CLI constitutional carriers:

### §7.1 `AXIS_PATTERN`

PATTERN axis is encoded as a **literal text prefix glyph sequence** prepended to every line of an AI-origin node:

- Default (UTF-8 capable terminal): `[AI] ` (4 visible columns).
- ASCII fallback (`MONOCHROME` or terminals refusing UTF-8): `<AI> ` (5 columns).

The prefix is **never optional** for AI-origin content. `AGENT_MESSAGE` always has `is_ai_origin = true` (S7.2 §I6) and is always prefixed. Other nodes carrying `is_ai_origin = true` (e.g., when the tree's issuer is an AI agent) inherit the prefix on their first line.

### §7.2 `AXIS_TYPOGRAPHY`

TYPOGRAPHY axis is encoded as **italic ANSI** (CSI `3 m`/`23 m`) when `AnsiSupportLevel ≥ COLOR_16` and the terminal advertises italic capability via `terminfo`. When italic is not supported, the fallback is **underline** (CSI `4 m`/`24 m`), which is supported by virtually every ANSI-capable terminal.

### §7.3 Why color is not a constitutional CLI axis

`AnsiSupportLevel = MONOCHROME` is a real and supported deployment (recovery TTYs, `screen` sessions, `dumb` terminals, `NO_COLOR=1` accessibility profiles). A constitutional invariant that fails silently on monochrome surfaces is not constitutional. CLI distinctions therefore use PATTERN+TYPOGRAPHY, both of which survive MONOCHROME (italic falls back to underline; underline is a typography variant carried by glyph rendering, not by color).

`ΔE` color distinctness (S7.3 §I3 `ΔE_HARMFUL_DISTINCT_RULE`) **does not apply** to CLI in any mode: the CLI surface either renders color (and the ΔE constraint is computed in the theme bundle by the visual language service before CLI ever sees it) or renders no color (and ΔE is irrelevant).

### §7.4 Recovery distinction (INV-022 binding)

Recovery distinction is encoded across:

- **PATTERN axis**: distinct frame style (double-line `╔═╗` vs single-line `┌─┐`; ASCII `=*=` vs `+-+`).
- **TYPOGRAPHY axis**: TYPOGRAPHY_RECOVERY token compiles to bold + reverse video on the banner row.

Both axes survive MONOCHROME. The recovery banner is therefore visually distinct from any normal-mode banner even on a `dumb` terminal.

## §8 Scripting mode

### §8.1 Output contract

`SCRIPTING` mode emits a single JSON document on stdout per `RenderTree` call. The document shape is:

```json
{
  "schema": "aios.renderer.cli.v1alpha1.ScriptOutput",
  "render_id": "rnd_<ulid>",
  "tree_id": "uit_<ulid>",
  "result": "<CliCompilationResult>",
  "compiled_nodes": <int>,
  "dropped_nodes": <int>,
  "security_indicator": {
    "subject": "<canonical_id>",
    "action": "<action_id-or-empty>",
    "evidence": "evr_<ulid>"
  },
  "content": [ /* node-shaped JSON structures */ ],
  "evidence_receipts": [ "evr_<ulid>", "evr_<ulid>", ... ],
  "warnings": [ ... ],
  "errors": [ ... ]
}
```

Stdout is **only** the JSON document; all logging, diagnostics, and ANSI styling go to stderr (and even stderr is plain when `NO_COLOR=1`). Tooling can `aios ... --scripting | jq` reliably.

### §8.2 No interactive prompts

`APPROVAL_PROMPT` nodes in the tree are handled deterministically:

- IF the caller pre-bound an approval id (via a request flag `--approval-id evr_<ulid>` referencing a previously-granted ApprovalBinding from S5.3), the renderer compiles the prompt as a **structured fact**, references the binding, and the action proceeds as approved.
- IF no pre-bound approval id is supplied, the renderer **rejects** the tree with `CLI_AUTO_CONFIRM_REJECTED` evidence (FOREVER retention) and exits with code 2 (policy denial). This binds INV-009 — approvals are bound to one request and one approver; piped Y-yes is not a valid approval.

### §8.3 Exit codes

```text
0    success — render succeeded; no policy denials; no errors
1    render failure — sanitization, signature, kind-rejection, or internal
2    policy denial — APPROVAL_PROMPT in tree without pre-bound approval id; or upstream S2.3 deny
3    ANSI injection blocked — sanitizer found injection; tree rejected
4    tree signature invalid — S7.2 signature did not verify
5    recovery kind rejected — RECOVERY_TTY mode encountered APP_SURFACE/STREAM_SURFACE/STREAM/VISUALIZATION
64   renderer internal — bug; FOREVER evidence emitted
65   data error (sysexits.h EX_DATAERR) — malformed input on stdin
66   no input (sysexits.h EX_NOINPUT) — required tree_id not resolvable
77   permission denied (sysexits.h EX_NOPERM) — recovery operator authentication failed
```

The exit code is the **only** machine-readable failure signal that downstream tooling needs; the JSON document is descriptive but the exit code is dispositive.

### §8.4 Evidence receipt-id propagation

Every action submitted from a scripting-mode session emits an evidence record (per the upstream Capability Runtime). The record's `evr_<ulid>` id is captured in the JSON document's `evidence_receipts` array. Tooling can correlate scripting-mode runs with evidence-log queries via these receipt-ids.

## §9 ANSI escape injection sanitization

### §9.1 Threat

Adversarial content sources — AI-origin tree content, view-bound list/table cells, evidence labels, agent messages — may attempt to embed ANSI escape sequences in user-visible text. Outcomes range from cosmetic (terminal title manipulation) to severe (cursor relocation to overwrite the chrome banner, OSC 8 hyperlink injection pointing to phishing URLs, terminal mode switches that cause subsequent input to be treated differently).

### §9.2 Allowlist

The renderer maintains a closed allowlist of escape sequences it itself emits:

- SGR (select graphic rendition) reset (`CSI 0 m`) and termination (`CSI <n> m` for `n` in `{0, 1, 3, 4, 7, 22, 23, 24, 27}` and color sequences `CSI 38;…m`/`CSI 48;…m`).
- Cursor positioning sequences emitted by the renderer's banner-pinning logic only (`CSI <row> ; <col> H`, `CSI s`, `CSI u`, `CSI <n> A/B/C/D`).
- Scrolling-region sequences (`CSI <top> ; <bottom> r`) emitted only at session start.
- DEC private mode set/reset for cursor visibility (`CSI ? 25 h/l`) emitted only at session start/end.

All other ANSI sequences are **denied**. In particular: OSC sequences (`ESC ]`), DCS sequences (`ESC P`), APC sequences (`ESC _`), PM sequences (`ESC ^`), and any CSI sequence not in the allowlist above.

### §9.3 Sanitization

Before any text content is emitted, the renderer scans for `0x1B` (ESC) bytes. Any ESC byte in non-renderer-issued content is stripped along with the canonical sequence terminator (per the CSI/OSC/DCS framing rules). Detected injection emits `CLI_ANSI_INJECTION_BLOCKED` evidence (FOREVER retention) and the offending node is rendered as `[content sanitized]`.

A recurrent injection pattern (≥ 3 detections per session) escalates: the entire tree is rejected with `FAILED_ANSI_INJECTION_BLOCKED` and the session exits with code 3. This protects the operator from sustained adversarial pressure.

### §9.4 Title and hyperlink injection

OSC 0 (`ESC ] 0 ; <text> BEL`) — terminal title manipulation — is denied. AIOS does not set the terminal title; an attacker setting `Approved by your bank` as the title cannot mislead the operator because the renderer never trusts the title.

OSC 8 (`ESC ] 8 ; <params> ; <uri> ESC \\`) — hyperlink injection — is denied. Evidence links are rendered as plain receipt-ids with copy hints; the operator's terminal cannot be tricked into rendering a link to `evil.com` while displaying `bank.com`.

## §10 Evidence integration

Ten record types are added to S3.1 RecordType vocabulary as part of this contract's adoption:

| Record type                     | Retention class | Carries                                                                                |
| ------------------------------- | --------------- | -------------------------------------------------------------------------------------- |
| `CLI_RENDER_STARTED`            | STANDARD_24M    | session_id, mode (`CliRenderMode`), input_mode (`CliInputMode`), ansi_level, term_id   |
| `CLI_RENDER_FAILED`             | EXTENDED_60M    | render_id, tree_id, result_code (`CliCompilationResult`), offending_node_id            |
| `CLI_NODE_KIND_UNSUPPORTED`     | STANDARD_24M    | render_id, tree_id, node_id, node_kind                                                 |
| `CLI_RECOVERY_KIND_REJECTED`    | FOREVER         | render_id, tree_id, node_id, attempted_kind (e.g., APP_SURFACE), operator_canonical_id |
| `CLI_AUTO_CONFIRM_REJECTED`     | FOREVER         | render_id, tree_id, prompt_node_id, action_canonical_hash, attempted_via               |
| `CLI_ANSI_INJECTION_BLOCKED`    | FOREVER         | render_id, tree_id, offending_node_id, injected_byte_count, sample_redacted            |
| `CLI_DEGRADED_NO_TTY`           | STANDARD_24M    | session_id, reason (`no_isatty` / `no_scrolling_region` / `terminfo_missing`)          |
| `CLI_SCRIPTING_MODE_INVOKED`    | STANDARD_24M    | session_id, caller_subject_canonical_id, command_line_redacted_hash                    |
| `CLI_OPERATOR_AUTHENTICATED`    | STANDARD_24M    | session_id, operator_subject_canonical_id, recovery_session_id (if recovery)           |
| `CLI_TRUST_INDICATOR_REORDERED` | FOREVER         | render_id, tree_id, expected_order, observed_order — TAMPER-class                      |

Each record carries `namespace_scope` (per S3.1 §23 touch-up) so the cross-group privacy ceiling applies to audit queries.

`CLI_TRUST_INDICATOR_REORDERED` is a **tamper-class** event: if the renderer ever observes that a `SECURITY_INDICATOR` node has been compiled after non-trust-bearing content (which it should never do per §I5), the renderer fails the render with `FAILED_RENDERER_INTERNAL`, emits this FOREVER record, and the operator is alerted via the standard L9 alerting path. This is defense-in-depth against renderer bugs that could subtly reorder trust indicators.

## §11 RPC surface (selected)

The CLI renderer exposes `CliRendererService` over gRPC for orchestration (the same service that S7.4 KDE and S7.5 Web expose, with renderer-specific parameters):

```proto
service CliRendererService {
  rpc RenderTree(CliRenderTreeRequest) returns (CliRenderTreeResponse);
  rpc OpenRecoveryShell(CliOpenRecoveryShellRequest) returns (CliOpenRecoveryShellResponse);
  rpc GetRendererInfo(CliGetRendererInfoRequest) returns (CliGetRendererInfoResponse);
}

message CliRenderTreeRequest {
  string tree_id = 1;                     // S7.2 uit_<ulid>
  string session_id = 2;
  string target_surface_id = 3;           // S7.1 surf_<ulid>; AIOS_SURFACE only in CLI
  CliRenderMode mode_override = 4;        // optional; defaults to deterministic selection §3.1.1
  CliInputMode input_mode = 5;
  AnsiSupportLevel ansi_level = 6;
  string approval_binding_id = 7;         // optional pre-bound approval id (scripting mode)
}

message CliRenderTreeResponse {
  oneof outcome {
    CliRenderTreeAccepted accepted = 1;
    CliRenderTreeError error = 2;
  }
}

message CliRenderTreeAccepted {
  string render_id = 1;
  CliCompilationResult result = 2;
  uint32 nodes_compiled = 3;
  uint32 nodes_dropped = 4;
  repeated string evidence_receipts = 5;
}

message CliRenderTreeError {
  CliCompilationResult result = 1;        // one of FAILED_*
  string message = 2;
  string offending_node_id = 3;
}
```

## §12 Determinism contract

```text
GIVEN
  identical tree_id (with identical signed bytes)
  identical CliRenderMode
  identical CliInputMode
  identical AnsiSupportLevel
  identical operator subject normalization (S5.1)
  identical recovery_mode flag

THEN
  the rendered byte stream is identical (modulo timing-dependent prompts in INTERACTIVE_TTY)
  the CliCompilationResult is identical
  the emitted evidence record types are identical
  the exit code (in SCRIPTING mode) is identical
```

This is **decision determinism + byte determinism in non-interactive modes**. In `INTERACTIVE_TTY`, prompt response timing is human-driven and not part of the determinism guarantee; the FSM transitions are still deterministic given the same response sequence.

## §13 Performance contract

| Operation                                      | p50      | p95      | p99      | Hard timeout |
| ---------------------------------------------- | -------- | -------- | -------- | ------------ |
| `RenderTree` (≤ 100 nodes, NORMAL_INTERACTIVE) | < 5 ms   | < 20 ms  | < 50 ms  | 500 ms       |
| `RenderTree` (≤ 100 nodes, SCRIPTING)          | < 2 ms   | < 10 ms  | < 30 ms  | 500 ms       |
| `RenderTree` (≤ 1000 nodes, any mode)          | < 50 ms  | < 200 ms | < 500 ms | 5 s          |
| Sanitizer scan per KB content                  | < 100 µs | < 500 µs | < 2 ms   | 10 ms        |
| Operator auth challenge (RECOVERY_TTY)         | n/a      | n/a      | n/a      | 60 s (human) |
| Banner pin / scrolling-region setup            | < 1 ms   | < 5 ms   | < 20 ms  | 100 ms       |

Failure modes — all fail closed:

- `RendererInternal` → exit code 64; `CLI_RENDER_FAILED` evidence; tree not rendered.
- `SanitizerOverPressure` → exit code 3 after 3 injection detections in a session.
- `TerminalCapabilityMissing` → downgrade to `DEGRADED_NO_COLOR`; emit `CLI_DEGRADED_NO_TTY`.

## §14 Adversarial robustness

### §14.1 Piped-Y auto-confirm bypass

Threat: an attacker (or a careless script) pipes `yes |` into `aios` to bypass approval prompts. Mitigation: the renderer detects `isatty(0) == false` in `NORMAL_INTERACTIVE` mode; reading approval response from a non-TTY stdin is rejected with `CLI_AUTO_CONFIRM_REJECTED` (FOREVER) and the action is denied. Binds INV-009 + INV-002.

### §14.2 ANSI escape injection

Threat: AI-origin or view-bound content embeds ANSI escapes that reposition the cursor over the chrome banner or set a misleading terminal title. Mitigation: §9 sanitization. All non-allowlisted escapes are stripped; injection emits `CLI_ANSI_INJECTION_BLOCKED` (FOREVER); recurrent injections terminate the session.

### §14.3 Tampered SECURITY_INDICATOR via malicious prefix

Threat: an attacker inserts a fake `[SECURE] subject=...` line into a TEXT node body, hoping the operator will mistake it for the real security indicator. Mitigation: §I5 + §10 — the chrome banner is a separate scrolling region; non-banner content cannot reach the banner row. Even if visually similar text appears in the body, the real banner remains pinned at top with content rendered FROM the signed `SECURITY_INDICATOR` node only. Additionally, sanitizer policy strips any literal `[SECURE]` glyph sequence appearing in non-trust-bearing node content (replaced with `[secure]` lowercase or `[s]`).

### §14.4 Recovery escape via APP_SURFACE creation

Threat: a script with stale capability tokens attempts to create an `APP_SURFACE` during recovery. Mitigation: §6.2 — surface creation is rejected at the surface service (S7.1 §I6) and at the renderer (defense in depth) with `FAILED_RECOVERY_KIND_REJECTED` and `CLI_RECOVERY_KIND_REJECTED` (FOREVER) evidence.

### §14.5 Operator-impersonation in recovery

Threat: an attacker on the local TTY presents as the operator without holding the recovery key. Mitigation: §6.4 challenge-response with hardware-bound key. Failed authentication exits the session before any rendering; no information leaks except whether the operator id exists (and that information is owned by S5.1, not this renderer).

### §14.6 Trust-indicator order tampering

Threat: a renderer bug or an in-process attacker reorders compiled trust-bearing nodes such that `SECURITY_INDICATOR` appears below other content. Mitigation: §I5 + §10 — the renderer self-checks ordering on every render; violations emit `CLI_TRUST_INDICATOR_REORDERED` (FOREVER, TAMPER-class) and fail the render. The check is intentionally redundant with §I5 to defend against subtle bugs.

### §14.7 Terminal-emulator quirks

Threat: a terminal emulator that does not honor scrolling-region commands (e.g., `screen` in some configurations) lets app content scroll over the chrome banner. Mitigation: at session start, the renderer probes scrolling-region behavior with a synthetic test pattern; if the test fails, the mode is downgraded to `DEGRADED_NO_COLOR` with the chrome banner rendered as a **prefix line on every render call** (less-elegant but functionally INV-020-compliant). `CLI_DEGRADED_NO_TTY` evidence emitted.

### §14.8 Stdin DoS

Threat: a script feeds an infinite or extremely large form payload to consume renderer resources. Mitigation: per-form-field input bounded to 64 KiB; per-session total stdin bounded to 16 MiB; exceeding bounds emits `CLI_RENDER_FAILED` and exits with code 65.

## §15 Cross-spec dependencies

| Spec       | Direction | What this spec contributes / consumes                                                                                                                                                                                                                                                          |
| ---------- | --------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| S7.1       | consumer  | CLI column of §4.3 mapping; recovery surface stack restriction (§6.2); chrome boundary (§I4)                                                                                                                                                                                                   |
| S7.2       | consumer  | Closed `NodeKind` 19 values; tree signature verified before compilation; `is_ai_origin`/`is_trust_bearing` flags read for distinction encoding                                                                                                                                                 |
| S7.3       | consumer  | `DistinctionAxis` PATTERN + TYPOGRAPHY are CLI's binding axes; `TYPOGRAPHY_RECOVERY`/`TYPOGRAPHY_AI_ORIGIN` consumed as bold/italic/underline ANSI proxies; `theme_aios_recovery` locked in RECOVERY_TTY                                                                                       |
| S5.3       | consumer  | `CLI_TTY_PROMPT` channel — APPROVAL_PROMPT is delivered to this renderer when channel selection algorithm picks `CLI_TTY_PROMPT`; renderer reads operator response from controlling TTY                                                                                                        |
| S9.1       | consumer  | `subject.recovery_mode = true` triggers `RECOVERY_TTY` mode; recovery-only mutation scopes; 8-hour TTL in banner; exit-by-reboot                                                                                                                                                               |
| S5.1       | consumer  | Operator subjects (`_system:operator-<id>`); recovery session class                                                                                                                                                                                                                            |
| S0.1       | producer  | FORM and ACTION_BUTTON submissions construct S0.1 ActionEnvelopes; submitter is session subject (per S7.2 §I9)                                                                                                                                                                                 |
| S2.3       | consumer  | Policy denials (`POLICY_DENIED` from upstream Capability Runtime) propagate to scripting-mode exit code 2                                                                                                                                                                                      |
| S3.1       | producer  | Ten record types queued (§10): CLI_RENDER_STARTED, CLI_RENDER_FAILED, CLI_NODE_KIND_UNSUPPORTED, CLI_RECOVERY_KIND_REJECTED, CLI_AUTO_CONFIRM_REJECTED, CLI_ANSI_INJECTION_BLOCKED, CLI_DEGRADED_NO_TTY, CLI_SCRIPTING_MODE_INVOKED, CLI_OPERATOR_AUTHENTICATED, CLI_TRUST_INDICATOR_REORDERED |
| L0 INV-001 | enforcer  | Recovery is independent of L5 — no autocomplete consults L5 in recovery (§6.5)                                                                                                                                                                                                                 |
| L0 INV-002 | enforcer  | Auto-confirm rejection in NORMAL_INTERACTIVE — AI/scripts cannot bypass approval (§I8 + §14.1)                                                                                                                                                                                                 |
| L0 INV-019 | enforcer  | Visual identity preserved: chrome banner, security indicator, AI-origin distinction always rendered                                                                                                                                                                                            |
| L0 INV-020 | enforcer  | Trust indicators always visible: chrome banner pinned via scrolling region; SECURITY_INDICATOR rendered first (§I5)                                                                                                                                                                            |
| L0 INV-021 | enforcer  | AI-vs-human distinct via PATTERN + TYPOGRAPHY (§7); survives MONOCHROME                                                                                                                                                                                                                        |
| L0 INV-022 | enforcer  | Recovery aesthetic distinct: theme_aios_recovery, distinct frame style, TYPOGRAPHY_RECOVERY (§6.3)                                                                                                                                                                                             |
| L0 INV-023 | enforcer  | CHROME zone reserved: top banner exclusive; non-chrome content cannot scroll into it (§I4)                                                                                                                                                                                                     |

## §16 Worked examples

### §16.1 Operator approves an action via prompt (NORMAL_INTERACTIVE)

```text
Setup:
  alice (HUMAN_USER) is logged in at a TTY in NORMAL_INTERACTIVE mode.
  An AI agent in family group has proposed action `aios.fs.write` to /aios/groups/family/users/alice/desktop/notes.txt.
  Policy decision: APPROVAL_PENDING; channel selected by S5.3 = CLI_TTY_PROMPT.
  Tree contains: SECURITY_INDICATOR + AGENT_MESSAGE + APPROVAL_PROMPT.

Render:
  Top row (banner, pinned):
    [SECURE] subject=family:alice  action=act_01HM…  evidence=evr_01HM…

  Body:
    [AI] Proposing to write 12 bytes to ~/desktop/notes.txt. (italic/underlined)
    ┌─ APPROVAL REQUIRED ────────────────────────────────────┐
    │ Action: aios.fs.write                                  │
    │ Target: /aios/groups/family/users/alice/desktop/notes  │
    │ Bytes:  12                                             │
    │ Bound to: evr_01HM…                                    │
    └────────────────────────────────────────────────────────┘
    [y]es / [n]o / [a]bort:

Operator types `y` on the controlling TTY.
  - isatty(0) == true → response accepted.
  - APPROVAL_GRANTED submitted with binding_id = evr_01HM…
  - Capability Runtime executes; SUCCEEDED.
  - Final render: [AI] Done. (12 bytes written; evidence: evr_01HN…)

Evidence emitted:
  CLI_RENDER_STARTED (STANDARD_24M)
  APPROVAL_GRANTED (owned by S5.3)
  CLI_RENDER_STARTED for the post-execute confirmation render
```

### §16.2 Recovery TTY operation (RECOVERY_TTY)

```text
Setup:
  Host has booted into recovery mode (S9.1 RecoveryEntryReason = OPERATOR_REQUESTED).
  No graphical session. /aios is NOT mounted (S9.1 §3.1).
  Operator _system:operator-247 has physical access to TTY1.

Mode selection (§3.1.1):
  subject.recovery_mode = true → mode = RECOVERY_TTY.

Operator authentication (§6.4):
  Renderer prints challenge nonce: 8b3f4a9c2d1e7f60 (16 bytes hex)
  Operator pastes signature from hardware token: 30450221…
  Renderer verifies against /aios/system/recovery/operators/operator-247/recovery_key.pub
    (recovery-mode mirror; per S9.1 §6).
  Verification OK → CLI_OPERATOR_AUTHENTICATED (STANDARD_24M) emitted.
  Session enters READY.

Banner (pinned, distinct frame):
  ╔══════════════════════════════════════════════════════════════════╗
  ║ RECOVERY MODE  operator=_system:operator-247  reason=OPERATOR_REQ ║
  ║                ttl=07:59:43                                       ║
  ╚══════════════════════════════════════════════════════════════════╝

Operator runs: aios policy bundle revert --to bundle_01HK…
  Tree contains SECURITY_INDICATOR + APPROVAL_PROMPT (recovery operations require STRONG_SOLO override per S5.4).
  No autocomplete; operator typed full command.
  Rendered prompt; operator confirms with hardware-token re-signature.

Operation completes. Banner ttl decreases. Operator types `aios recovery exit` → reboot.

Evidence emitted:
  CLI_RENDER_STARTED, CLI_OPERATOR_AUTHENTICATED, plus upstream override + policy + recovery records.

Adversarial branch:
  An app installed in family group somehow attempts CreateSurface(APP_SURFACE) during recovery.
  → Surface service rejects with RecoveryModeKindForbidden.
  → If a tree containing SURFACE_EMBED of APP_SURFACE is presented to the renderer (defense-in-depth),
    renderer rejects with FAILED_RECOVERY_KIND_REJECTED, exits code 5,
    emits CLI_RECOVERY_KIND_REJECTED (FOREVER).
```

### §16.3 Scripting mode with JSON output (SCRIPTING)

```text
Setup:
  CI script invokes `aios fs list /aios/groups/homelab/apps/bg.iconys.neurocad/runtime --scripting`
  stdin is closed; stdout is a pipe to `jq`.
  CliInputMode = NON_INTERACTIVE; CliRenderMode = SCRIPTING.
  No interactive prompt is in the tree (read-only query).

Mode selection (§3.1.1):
  subject.recovery_mode = false; --scripting flag asserted → mode = SCRIPTING.

Evidence emitted on session start:
  CLI_SCRIPTING_MODE_INVOKED (STANDARD_24M) carrying caller_subject_canonical_id and command-line hash.

Render:
  Tree contains SECURITY_INDICATOR + LIST (view-bound to S2.1 query).

Output on stdout (single JSON document):
  {
    "schema": "aios.renderer.cli.v1alpha1.ScriptOutput",
    "render_id": "rnd_01HN…",
    "tree_id": "uit_01HN…",
    "result": "COMPILED_PLAIN",
    "compiled_nodes": 14,
    "dropped_nodes": 0,
    "security_indicator": {
      "subject": "homelab:operator-luckyngoriko",
      "action": "",
      "evidence": "evr_01HN…"
    },
    "content": [
      { "kind": "LIST", "items": ["surf_01HN…", "surf_01HN…", "surf_01HN…"], "cursor": "" }
    ],
    "evidence_receipts": ["evr_01HN…"],
    "warnings": [],
    "errors": []
  }

Exit code: 0.

Adversarial branch (auto-confirm attempt):
  CI script invokes `aios action submit aios.fs.write … --scripting`.
  Tree contains APPROVAL_PROMPT.
  No --approval-id flag; no pre-bound ApprovalBinding.
  Renderer rejects: result = FAILED_NODE_KIND_UNSUPPORTED (more precisely, refuses APPROVAL_PROMPT in SCRIPTING without binding).
  CLI_AUTO_CONFIRM_REJECTED (FOREVER) emitted; exit code 2 (policy denial).
  jq receives:
    {
      "schema": "aios.renderer.cli.v1alpha1.ScriptOutput",
      "result": "FAILED_NODE_KIND_UNSUPPORTED",
      "errors": [
        { "code": "CLI_AUTO_CONFIRM_REJECTED",
          "message": "APPROVAL_PROMPT in SCRIPTING mode requires --approval-id pre-binding",
          "node_id": "uin_01HN…" }
      ]
    }
```

## §17 Telemetry contract

All metrics MUST use bounded label cardinality. **render_id, tree_id, session_id, subject_canonical_id, group_id are NEVER labels.**

| Metric                                | Type      | Labels (closed)                                                      |
| ------------------------------------- | --------- | -------------------------------------------------------------------- |
| `cli_render_started_total`            | counter   | `mode`, `input_mode`, `ansi_level`                                   |
| `cli_render_duration_seconds`         | histogram | `mode`, `result_class` (compiled/degraded/failed)                    |
| `cli_render_failed_total`             | counter   | `result` (CliCompilationResult value name)                           |
| `cli_node_kind_unsupported_total`     | counter   | `node_kind`                                                          |
| `cli_recovery_kind_rejected_total`    | counter   | `attempted_kind`                                                     |
| `cli_auto_confirm_rejected_total`     | counter   | `attempted_via` (`piped_stdin` / `non_tty` / `scripting_no_binding`) |
| `cli_ansi_injection_blocked_total`    | counter   | none                                                                 |
| `cli_degraded_no_tty_total`           | counter   | `reason`                                                             |
| `cli_scripting_mode_invoked_total`    | counter   | none                                                                 |
| `cli_operator_authenticated_total`    | counter   | `result` (success/failure)                                           |
| `cli_trust_indicator_reordered_total` | counter   | none                                                                 |
| `cli_sanitizer_scan_duration_seconds` | histogram | `result` (clean/stripped)                                            |

Cardinality budget: ≤ 100 active label tuples per metric.

## §18 Acceptance criteria

- [ ] `CliRenderMode` is a closed enum with four values plus UNSPECIFIED; mode selection is deterministic per §3.1.1.
- [ ] `CliCompilationResult` is a closed enum with ten values plus UNSPECIFIED.
- [ ] `CliInputMode` is a closed enum with five values plus UNSPECIFIED.
- [ ] `AnsiSupportLevel` is a closed enum with four values plus UNSPECIFIED.
- [ ] Every S7.2 `NodeKind` (19 values) has a deterministic compilation in §5.
- [ ] `SECURITY_INDICATOR` is rendered before any other tree content; out-of-order detection emits `CLI_TRUST_INDICATOR_REORDERED` (FOREVER).
- [ ] Chrome banner is pinned via scrolling region in `NORMAL_INTERACTIVE` and `RECOVERY_TTY`; failure to pin downgrades to `DEGRADED_NO_COLOR` with `CLI_DEGRADED_NO_TTY` evidence.
- [ ] `RECOVERY_TTY` mode rejects `APP_SURFACE` / `STREAM_SURFACE` / `STREAM` / `VISUALIZATION` with `FAILED_RECOVERY_KIND_REJECTED` and FOREVER `CLI_RECOVERY_KIND_REJECTED` evidence.
- [ ] `RECOVERY_TTY` mode loads only `theme_aios_recovery` (no user themes).
- [ ] `RECOVERY_TTY` operator authentication via challenge-response succeeds before the session reaches `READY`; failure exits before any rendering.
- [ ] No autocomplete consults L5 in `RECOVERY_TTY`; up-arrow history is per-session only.
- [ ] AI-vs-human distinction encoded across PATTERN (`[AI]` prefix) AND TYPOGRAPHY (italic/underline) axes; survives `AnsiSupportLevel = MONOCHROME`.
- [ ] `NORMAL_INTERACTIVE` mode rejects piped stdin as approval response; `isatty(0) == false` triggers `CLI_AUTO_CONFIRM_REJECTED` (FOREVER).
- [ ] `SCRIPTING` mode emits a single JSON document on stdout per `RenderTree` call; exit code reflects terminal state per §8.3.
- [ ] `SCRIPTING` mode rejects `APPROVAL_PROMPT` without `--approval-id` pre-binding with exit code 2 and FOREVER `CLI_AUTO_CONFIRM_REJECTED`.
- [ ] ANSI escape sequences in non-renderer-issued content are sanitized; injection emits FOREVER `CLI_ANSI_INJECTION_BLOCKED`; recurrent injection (≥ 3) terminates the session with exit code 3.
- [ ] OSC 0 (terminal title) and OSC 8 (hyperlink) sequences in content are denied unconditionally.
- [ ] Ten record types (§10) are queued for S3.1 with the retention classes listed.
- [ ] Telemetry conforms to §17; render_id / tree_id / session_id / subject / group never appear as labels.
- [ ] All three worked examples (§16) produce the specified outcomes.
- [ ] L0 INV-001/002/019/020/021/022/023 are addressed in §15 with the specific binding mechanism named.

## §19 Open deferrals

- **OSC 8 hyperlink allowlist** — currently denied unconditionally. A future contract may allowlist hyperlinks pointing only at `aios:` evidence URIs after the URI scheme is registered. Deferred.
- **Mouse support** — terminal mouse events for clickable menus. Deferred; numbered menus are sufficient for MVP.
- **Right-to-left language rendering in CLI** — bidirectional text in tables and forms. Deferred to a future internationalization sub-spec.
- **Per-renderer accessibility tree** — screen-reader integration via `terminfo` capabilities and `BEL`/`VT100` discipline. Deferred.
- **Multi-pane CLI surfaces** — split panes (à la `tmux`) hosting multiple AIOS surfaces concurrently. Deferred.
- **Voice / mobile renderers** — owned by S7.7+ (deferred). The present spec is the bottom of the L7 renderer family.
- **Concrete byte-level ANSI sequences for theme tokens** — the binding from `COLOR_*` token names to specific 256-color slots and from `TYPOGRAPHY_*` to specific bold/italic/underline combinations is a stage-3 visual artifact (per S7.3 §3) and is not committed in this spec.
- **Persistent recovery session history file** — currently disabled. A future enhancement may write a forensic log of the recovery session's keystrokes (sanitized) for post-incident review. Deferred.
- **Bidirectional pipes for streaming output** — `aios watch` style commands that emit a continuous stream of UI tree updates over a long-lived stdout pipe. This requires a streaming variant of the JSON contract (NDJSON or length-prefixed framing). Deferred.
- **Renderer-side compression for slow links** — over a slow `ssh` link, the renderer might benefit from emitting a more compact representation (e.g., terse mode without ASCII art). Deferred; the operator can already request `DEGRADED_NO_COLOR` mode explicitly via `NO_COLOR=1`.

## §20 Appendix A — Full Proto IDL

```proto
syntax = "proto3";
package aios.renderer.cli.v1alpha1;

import "google/protobuf/timestamp.proto";

// ============================================================================
// Service
// ============================================================================

service CliRendererService {
  rpc RenderTree(CliRenderTreeRequest) returns (CliRenderTreeResponse);
  rpc OpenRecoveryShell(CliOpenRecoveryShellRequest) returns (CliOpenRecoveryShellResponse);
  rpc GetRendererInfo(CliGetRendererInfoRequest) returns (CliGetRendererInfoResponse);
}

// ============================================================================
// Closed enums
// ============================================================================

enum CliRenderMode {
  CLI_RENDER_MODE_UNSPECIFIED = 0;
  NORMAL_INTERACTIVE = 1;
  SCRIPTING = 2;
  RECOVERY_TTY = 3;
  DEGRADED_NO_COLOR = 4;
}

enum CliCompilationResult {
  CLI_COMPILATION_RESULT_UNSPECIFIED = 0;
  COMPILED_RICH = 1;
  COMPILED_PLAIN = 2;
  COMPILED_RECOVERY = 3;
  DEGRADED_PARTIAL = 4;
  FAILED_NODE_KIND_UNSUPPORTED = 5;
  FAILED_TREE_SIGNATURE_INVALID = 6;
  FAILED_RECOVERY_KIND_REJECTED = 7;
  FAILED_ANSI_INJECTION_BLOCKED = 8;
  FAILED_TREE_TOO_LARGE = 9;
  FAILED_RENDERER_INTERNAL = 10;
}

enum CliInputMode {
  CLI_INPUT_MODE_UNSPECIFIED = 0;
  INTERACTIVE_TTY = 1;
  SCRIPT_PIPED = 2;
  NON_INTERACTIVE = 3;
  NO_TTY = 4;
  READ_ONLY_QUERY = 5;
}

enum AnsiSupportLevel {
  ANSI_SUPPORT_LEVEL_UNSPECIFIED = 0;
  TRUECOLOR = 1;
  COLOR_256 = 2;
  COLOR_16 = 3;
  MONOCHROME = 4;
}

enum CliEvidenceRecordKind {
  CLI_EVIDENCE_RECORD_KIND_UNSPECIFIED = 0;
  CLI_RENDER_STARTED = 1;
  CLI_RENDER_FAILED = 2;
  CLI_NODE_KIND_UNSUPPORTED = 3;
  CLI_RECOVERY_KIND_REJECTED = 4;
  CLI_AUTO_CONFIRM_REJECTED = 5;
  CLI_ANSI_INJECTION_BLOCKED = 6;
  CLI_DEGRADED_NO_TTY = 7;
  CLI_SCRIPTING_MODE_INVOKED = 8;
  CLI_OPERATOR_AUTHENTICATED = 9;
  CLI_TRUST_INDICATOR_REORDERED = 10;
}

// ============================================================================
// RenderTree
// ============================================================================

message CliRenderTreeRequest {
  string tree_id = 1;
  string session_id = 2;
  string target_surface_id = 3;
  CliRenderMode mode_override = 4;
  CliInputMode input_mode = 5;
  AnsiSupportLevel ansi_level = 6;
  string approval_binding_id = 7;
  uint32 terminal_columns = 8;
  uint32 terminal_rows = 9;
  string terminfo_term = 10;       // value of $TERM at session start
  bool no_color_env = 11;          // $NO_COLOR present and non-empty
}

message CliRenderTreeResponse {
  oneof outcome {
    CliRenderTreeAccepted accepted = 1;
    CliRenderTreeError error = 2;
  }
}

message CliRenderTreeAccepted {
  string render_id = 1;
  CliCompilationResult result = 2;
  uint32 nodes_compiled = 3;
  uint32 nodes_dropped = 4;
  uint32 sanitized_node_count = 5;
  repeated string evidence_receipts = 6;
}

message CliRenderTreeError {
  CliCompilationResult result = 1;
  string message = 2;
  string offending_node_id = 3;
  uint32 exit_code = 4;
}

// ============================================================================
// OpenRecoveryShell
// ============================================================================

message CliOpenRecoveryShellRequest {
  string operator_subject_canonical_id = 1;
  string boot_id = 2;
  string recovery_reason = 3;
  string controlling_tty_path = 4;     // e.g., /dev/tty1
  bytes  challenge_signature = 5;       // operator's signed nonce
}

message CliOpenRecoveryShellResponse {
  oneof outcome {
    CliRecoveryShellOpened opened = 1;
    CliRecoveryShellError error = 2;
  }
}

message CliRecoveryShellOpened {
  string recovery_session_id = 1;
  string active_theme_id = 2;            // always theme_aios_recovery
  uint64 ttl_seconds_remaining = 3;
}

enum CliRecoveryShellErrorCode {
  CLI_RECOVERY_SHELL_ERROR_CODE_UNSPECIFIED = 0;
  CLI_RECOVERY_OPERATOR_NOT_RESOLVED = 1;
  CLI_RECOVERY_CHALLENGE_VERIFICATION_FAILED = 2;
  CLI_RECOVERY_THEME_VERIFICATION_FAILED = 3;
  CLI_RECOVERY_TTY_NOT_AVAILABLE = 4;
  CLI_RECOVERY_RENDERER_INTERNAL = 5;
}

message CliRecoveryShellError {
  CliRecoveryShellErrorCode code = 1;
  string message = 2;
}

// ============================================================================
// RendererInfo
// ============================================================================

message CliGetRendererInfoRequest {}

message CliGetRendererInfoResponse {
  CliRendererInfo info = 1;
}

message CliRendererCapabilities {
  bool supports_truecolor = 1;
  bool supports_256_color = 2;
  bool supports_16_color = 3;
  bool supports_italic = 4;
  bool supports_underline = 5;
  bool supports_reverse = 6;
  bool supports_scrolling_region = 7;
  bool supports_utf8_box_drawing = 8;
  bool supports_alt_screen = 9;
  bool supports_bracketed_paste = 10;
  uint32 detected_columns = 11;
  uint32 detected_rows = 12;
}

message CliRendererInfo {
  string renderer_id = 1;                 // always "cli"
  string renderer_build_hash = 2;
  string surface_schema_version = 3;      // "aios.surface.v1alpha1"
  string ui_schema_version = 4;           // "aios.ui.v1alpha1"
  string visual_schema_version = 5;       // "aios.visual.v1alpha1"
  string cli_renderer_schema_version = 6; // "aios.renderer.cli.v1alpha1"
  CliRenderMode active_mode = 7;
  CliInputMode active_input_mode = 8;
  AnsiSupportLevel ansi_level = 9;
  CliRendererCapabilities capabilities = 10;
}
```

## See also

- [S0.1 — Action Envelope and Lifecycle](../XX_Cross_Cutting/01_action_envelope_lifecycle.md)
- [S5.1 — Identity Model](../L4_Policy_Identity_Vault/03_identity_model.md)
- [S5.3 — Approval Mechanics](../L4_Policy_Identity_Vault/04_approval_mechanics.md)
- [S6.4 — Constitutional Invariants (incl. INV-001/002/019/020/021/022/023)](../L0_Governance_Evidence_Safety/04_invariants.md)
- [S7.1 — Surface + Composition Model](01_surface_composition.md)
- [S7.2 — Shared UI Schema](02_shared_ui_schema.md)
- [S7.3 — Visual Language](03_visual_language.md)
- [S7.4 — KDE Plasma Renderer](04_kde_renderer.md)
- [S7.5 — Web Renderer](05_web_renderer.md)
- [S9.1 — Recovery Boundary](../L1_Kernel_Bootstrap_Recovery/01_recovery_boundary.md)
- [L7 Overview](00_overview.md)
- [Rev.2 Master Index](../00_MASTER_INDEX.md)
