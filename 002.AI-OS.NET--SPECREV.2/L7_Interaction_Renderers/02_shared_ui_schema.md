# Shared UI Schema (Rev.2)

| Field          | Value                                                                                                                                                                                                                                                     |
| -------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Status         | `CONTRACT` (initial; written 2026-05-10)                                                                                                                                                                                                                  |
| Phase tag      | S7.2                                                                                                                                                                                                                                                      |
| Layer          | L7 Interaction Renderers                                                                                                                                                                                                                                  |
| Schema package | `aios.ui.v1alpha1`                                                                                                                                                                                                                                        |
| Consumes       | S0.1 (action target embedded in nodes), S1.3 (object refs), S2.1 (query views as data sources for List nodes), S3.1 (evidence refs), S5.1 (subject refs), S7.1 Surface + Composition (Surface as a node kind), L0 INV-019..INV-022 (renderer obligations) |
| Produces       | typed UI Node hierarchy, closed `NodeKind` vocabulary, render-target-agnostic schema consumed by L7.4 KDE renderer, L7.5 Web renderer, L7.6 CLI renderer                                                                                                  |

## 1. Purpose

AIOS renders the same authoritative system state to many surfaces — KDE Plasma, localhost Web, recovery CLI — and across all of them the structural grammar of an interaction is one. This spec fixes that grammar.

A **UI Schema tree** is a typed, target-agnostic node hierarchy. The same tree, signed by an issuer, is consumed by every renderer; each renderer compiles the tree into its target primitives:

- **KDE renderer**: Qt/QML widgets and an embedded `wgpu` `QQuickItem` for visualization nodes.
- **Web renderer**: DOM elements and a WebGPU `<canvas>` for visualization nodes.
- **CLI renderer**: terminal text grid with ANSI styling and box-drawing.

This spec defines:

1. The closed `NodeKind` vocabulary and the per-kind payload messages.
2. The signed `Node` envelope and the trust-bearing-node authorship rules.
3. The closed `LayoutMode` and `SizeMode` vocabularies (purely structural; no visual treatment).
4. Data binding from `LIST` / `TABLE` to S2.1 query views.
5. Form submission as an S0.1 action envelope.
6. Recovery-mode discipline (`recovery_only` flag and the rejection rule).
7. Schema versioning, performance contract, adversarial robustness.
8. The `UISchemaService` gRPC surface used by every renderer.

This spec is **purely structural**. Visual treatment — colors, typography, spacing, motion, the AI-vs-human visual distinction required by INV-021, the recovery aesthetic required by INV-022 — is the **separate L7.3 Visual Language** spec. This spec only declares the flags (`is_ai_origin`, `recovery_only`, `is_trust_bearing`) that the visual language consumes. A renderer compiled against this schema with no visual language attached produces a structurally correct but visually default tree.

The schema runs on top of the S7.1 Surface model: a UI tree is rendered into one or more Surfaces. `AIOS_SURFACE` instances host UI trees natively; `APP_SURFACE` and `STREAM_SURFACE` are referenced from this schema via the `SURFACE_EMBED` node kind.

## 2. Core invariants

- **I1 — Closed `NodeKind` vocabulary.** Adding a kind is a versioned spec change. Renderers reject trees containing unknown kinds.
- **I2 — Schema is target-agnostic.** No DOM-specific, Qt-specific, or terminal-specific types appear in the schema. A `TEXT` node carries text and accessibility metadata, not a `tag_name` or `qml_component`. A renderer that cannot compile a tree must reject it as a whole, not partially.
- **I3 — Stable node identity.** Every node has a `node_id` of the form `uin_<ulid>`. Two nodes in one tree may not share a `node_id`. Identity is stable across re-renders triggered by data-binding updates (the renderer reuses `uin_<ulid>` to preserve focus, scroll position, and accessibility tree continuity).
- **I4 — Subtree signature.** A UI tree (or a delegated subtree) is signed by the issuer's identity-service-issued tree-signing key. The renderer verifies the signature before compilation. Tampered subtrees are rejected with `TreeSignatureInvalid` and produce no partial render.
- **I5 — Trust-bearing nodes are constitutional.** The kinds `SECURITY_INDICATOR`, `APPROVAL_PROMPT`, and `EVIDENCE_LINK` cannot be authored by AI subjects. The issuer's tree-signing service refuses to sign a tree containing those kinds when the issuer's `SubjectKind` is `AI_AGENT`. This binds INV-020 (trust indicators always visible) at the authorship layer.
- **I6 — AI origin is unforgeable.** A node carrying `is_ai_origin = true` was authored by an AI subject; the flag is set by the tree-signing service from the issuer's identity, not by the issuer's input. `AGENT_MESSAGE` always carries `is_ai_origin = true`. The renderer applies INV-021 distinct visual treatment to every node with this flag, and the visual language (L7.3) defines what "distinct" means.
- **I7 — Recovery flag is enforced at validation, not at render.** A node with `recovery_only = true` is valid only when the active rendering session is in recovery mode (per S5.1 §7). Outside recovery, the renderer rejects the tree with `RecoveryNodeOutsideRecovery`. Conversely, a normal-mode tree containing `recovery_only` branches is not rejected; the branches are dropped silently and the metric `recovery_node_dropped_total` is incremented.
- **I8 — Bounded tree size.** A UI tree contains at most 10 000 nodes. Trees exceeding the limit are rejected with `SchemaTreeTooLarge`. Within a `LIST` or `TABLE`, the bound applies to materialized rows; views with cardinality above the limit must use cursors (per S2.1 §11.3 inbox semantics) and the renderer materializes only the visible page.
- **I9 — Action submission is canonical.** A `FORM` submitted by the user, or an `ACTION_BUTTON` clicked, produces an S0.1 action envelope. The `submitter` recorded in the envelope is the active session's subject — never the tree's issuer when they differ. This makes accountability traceable: the tree's issuer is the author of the UI; the submitter is the actor who chose to submit.

## 3. NodeKind closed enum

```proto
enum NodeKind {
  NODE_KIND_UNSPECIFIED = 0;

  // Structural / layout
  CONTAINER = 1;          // grouping with layout hints; no payload
  DIVIDER = 2;            // structural separator
  SPACER = 3;             // flexible or fixed gap

  // Text
  TEXT = 4;               // paragraph or label
  HEADING = 5;            // semantic heading (level 1..6); accessibility-load-bearing
  INLINE_CODE = 6;        // code-styled inline span
  CODE_BLOCK = 7;         // multi-line code block with optional language hint

  // Composite content
  CARD = 8;               // grouped content with title + body + actions
  LIST = 9;               // ordered/unordered list; data source inline or S2.1 view
  TABLE = 10;             // tabular data; filterable + sortable

  // Interaction
  FORM = 11;              // input collection; submits as S0.1 action envelope
  ACTION_BUTTON = 12;     // invokes a typed action when activated

  // Live / GPU
  VISUALIZATION = 13;     // chart/graph/topology rendered via wgpu inside an AIOS_SURFACE
  STREAM = 14;            // live data feed rendered into a STREAM_SURFACE
  SURFACE_EMBED = 15;     // embeds a Surface from S7.1 (typically APP_SURFACE)

  // Trust-bearing (constitutional; AI subjects cannot author)
  SECURITY_INDICATOR = 16; // AIOS chrome element showing subject + action + evidence link
  APPROVAL_PROMPT = 17;    // gates an action awaiting decision
  EVIDENCE_LINK = 18;      // clickable link to an S3.1 evidence receipt

  // AI-origin (always carries is_ai_origin = true)
  AGENT_MESSAGE = 19;      // rich output from an AI agent
}
```

Closed enum, 19 declared values plus `UNSPECIFIED`. Adding a kind is a versioned spec change. Renderers reject unknown kinds in the wire format with `UnknownNodeKind`.

The kinds partition into five families:

| Family        | Kinds                                                    | Notes                               |
| ------------- | -------------------------------------------------------- | ----------------------------------- |
| Structural    | `CONTAINER`, `DIVIDER`, `SPACER`                         | Layout-only; no semantic content    |
| Text          | `TEXT`, `HEADING`, `INLINE_CODE`, `CODE_BLOCK`           | Accessibility-load-bearing          |
| Composite     | `CARD`, `LIST`, `TABLE`                                  | May bind to S2.1 views              |
| Interaction   | `FORM`, `ACTION_BUTTON`                                  | Submits S0.1 action envelopes       |
| Live / GPU    | `VISUALIZATION`, `STREAM`, `SURFACE_EMBED`               | References an S7.1 Surface          |
| Trust-bearing | `SECURITY_INDICATOR`, `APPROVAL_PROMPT`, `EVIDENCE_LINK` | Constitutional authorship rule (§7) |
| AI-origin     | `AGENT_MESSAGE`                                          | Always `is_ai_origin = true`        |

## 4. Node hierarchy

### 4.1 Common envelope

Every node — regardless of kind — carries the same envelope fields:

```proto
message Node {
  // Identity
  string node_id = 1;                       // uin_<ulid>; stable across re-renders
  NodeKind kind = 2;
  string parent_id = 3;                     // empty for root

  // Accessibility (target-agnostic; renderer maps to ARIA / Qt accessible / CLI labels)
  string accessibility_label = 4;
  string accessibility_description = 5;
  string accessibility_role = 6;            // closed enum value (AccessibilityRole)

  // Layout
  LayoutHints layout_hints = 7;

  // Constitutional flags
  bool recovery_only = 8;                   // valid only in recovery sessions
  bool is_ai_origin = 9;                    // set by signing service from issuer.kind = AI_AGENT
  bool is_trust_bearing = 10;               // derived from kind; not author-settable

  // Authorship
  string issuer_subject_canonical_id = 11;  // L4 canonical id of the tree issuer
  string target_renderer_id = 12;           // optional renderer-specific override (kde|web|cli)

  // Per-kind payload
  oneof body {
    ContainerPayload container = 100;
    TextPayload text = 101;
    HeadingPayload heading = 102;
    InlineCodePayload inline_code = 103;
    CodeBlockPayload code_block = 104;
    DividerPayload divider = 105;
    SpacerPayload spacer = 106;
    CardPayload card = 107;
    ListPayload list = 108;
    TablePayload table = 109;
    FormPayload form = 110;
    ActionButtonPayload action_button = 111;
    VisualizationPayload visualization = 112;
    StreamPayload stream = 113;
    SurfaceEmbedPayload surface_embed = 114;
    SecurityIndicatorPayload security_indicator = 115;
    ApprovalPromptPayload approval_prompt = 116;
    EvidenceLinkPayload evidence_link = 117;
    AgentMessagePayload agent_message = 118;
  }

  repeated Node children = 200;             // structural children; used by CONTAINER, CARD, LIST, FORM, etc.
}
```

The `oneof body` discriminator must match `kind`; mismatch is rejected at validation with `KindPayloadMismatch`.

### 4.2 `AccessibilityRole` closed enum

Targets WAI-ARIA semantics with renderer-agnostic names. Closed:

```text
ROLE_NONE | ROLE_HEADING | ROLE_PARAGRAPH | ROLE_LIST | ROLE_LIST_ITEM |
ROLE_TABLE | ROLE_ROW | ROLE_CELL | ROLE_FORM | ROLE_TEXTBOX | ROLE_BUTTON |
ROLE_LINK | ROLE_NAVIGATION | ROLE_REGION | ROLE_DIALOG | ROLE_ALERT |
ROLE_STATUS | ROLE_GROUP
```

KDE maps to `QAccessible::Role`; Web maps to ARIA `role`; CLI uses the role to decide announce-on-focus text.

### 4.3 Per-kind payloads (selected)

```proto
message TextPayload { string content = 1; bool emphasis = 2; bool strong = 3; }

message HeadingPayload { string content = 1; uint32 level = 2; }   // level in [1..6]

message ListPayload {
  bool ordered = 1;
  oneof source {
    InlineListItems inline = 2;            // up to 256 items; static
    string view_ref = 3;                   // S2.1 query view id; live binding
  }
  uint32 page_size = 4;                    // for view_ref; default 50, max 500
}

message InlineListItems { repeated string items = 1; }

message TablePayload {
  repeated TableColumn columns = 1;
  oneof source {
    InlineRows inline = 2;
    string view_ref = 3;
  }
  uint32 page_size = 4;                    // default 100, max 1000
}

message FormPayload {
  string action_template_ref = 1;          // S0.1 action envelope template id
  repeated FormField fields = 2;
  string submit_label = 3;
}

message ActionButtonPayload {
  string action_template_ref = 1;          // S0.1 action envelope template id
  string label = 2;
  bool destructive = 3;                    // signals destructive intent for visual language
}

message VisualizationPayload {
  string surface_id = 1;                   // S7.1 AIOS_SURFACE owned by this tree
  string visualization_kind = 2;           // closed enum: chart_line, chart_bar, graph, topology, ...
  bytes data = 3;                          // visualization-kind-specific encoding
}

message StreamPayload {
  string surface_id = 1;                   // S7.1 STREAM_SURFACE
  string stream_kind = 2;                  // closed enum: action_log, evidence_feed, telemetry, ...
}

message SurfaceEmbedPayload {
  string surface_id = 1;                   // any S7.1 SurfaceKind (typically APP_SURFACE)
}

message SecurityIndicatorPayload {
  string subject_canonical_id = 1;
  string current_action_id = 2;            // S0.1 act_<ulid>
  string evidence_record_id = 3;           // S3.1 evr_<ulid> for current action
}

message ApprovalPromptPayload {
  string action_id = 1;                    // S0.1 act_<ulid>
  bytes request_hash = 2;                  // bound per S0.1 §4
  string question = 3;
  repeated ApprovalChoice choices = 4;     // typically {ACCEPT, REJECT}
}

message EvidenceLinkPayload {
  string evidence_record_id = 1;           // S3.1 evr_<ulid>
  string label = 2;
}

message AgentMessagePayload {
  string agent_canonical_id = 1;           // L4 AI_AGENT subject id
  repeated Node body_nodes = 2;            // rich content; constrained to non-trust-bearing kinds
  string reasoning_summary = 3;            // short summary for accessibility
}
```

The `body_nodes` of `AGENT_MESSAGE` is a sub-tree; it is validated with the recursive constraint that no descendant carries `is_trust_bearing = true`. AI subjects cannot smuggle trust-bearing children inside an `AGENT_MESSAGE` payload.

## 5. Layout model

The schema encodes layout abstractly; renderers translate to their target primitives.

```proto
enum LayoutMode {
  LAYOUT_MODE_UNSPECIFIED = 0;
  STACK_VERTICAL = 1;          // children stacked top-to-bottom
  STACK_HORIZONTAL = 2;        // children stacked left-to-right
  FLOW_WRAP = 3;               // children flow inline, wrapping at container edge
  GRID = 4;                    // fixed-column grid; column count in LayoutHints.grid_columns
  SCROLL_VERTICAL = 5;         // single-axis vertical scroll container
  SCROLL_HORIZONTAL = 6;       // single-axis horizontal scroll container
  FIXED = 7;                   // exactly one child, sized to container
}
```

Closed enum, 7 values. No floats. No absolute positioning. No CSS-style positioning vocabulary. The renderer translates `STACK_VERTICAL` to `QVBoxLayout` on KDE, to a flex column on Web, to one-child-per-line on CLI.

```proto
enum SizeMode {
  SIZE_MODE_UNSPECIFIED = 0;
  HUG_CONTENT = 1;             // size to content's intrinsic size
  FILL_PARENT = 2;             // expand to fill parent in this axis
  FIXED_LOGICAL_PIXELS = 3;    // size = LayoutHints.size_value (HiDPI-scaled by renderer)
  FRACTION_OF_PARENT = 4;      // size = LayoutHints.size_value / 1000 of parent
}
```

Closed enum, 4 values. Numeric `size_value` accompanies modes 3 and 4; ignored for modes 1 and 2.

```proto
message LayoutHints {
  LayoutMode mode = 1;
  SizeMode width_mode = 2;
  SizeMode height_mode = 3;
  uint32 width_value = 4;       // logical pixels (HiDPI-scaled) or per-mille
  uint32 height_value = 5;
  uint32 grid_columns = 6;      // used only when mode = GRID
  uint32 gap_logical_pixels = 7; // child spacing; renderer scales for HiDPI
  uint32 padding_logical_pixels = 8;
}
```

The CLI renderer ignores `gap_logical_pixels` and `padding_logical_pixels` (uses fixed character spacing); KDE and Web honor them. No colors, fonts, shadows, borders, animations are present at the schema layer; those belong to L7.3.

## 6. Data binding

### 6.1 List / Table → S2.1 query view

A `LIST` or `TABLE` may bind to an S2.1 query view by setting `source.view_ref = <view_id>`. The renderer:

1. Subscribes to the view on initial render.
2. Materializes the first `page_size` rows into list items / table rows.
3. Re-renders the affected subtree on view-change notifications, preserving `node_id` for surviving rows so focus and scroll position survive.
4. Honors S2.1 cursor semantics for paging beyond `page_size`.

Cardinality bound: per I8, materialized rows count toward the 10 000-node tree limit. A view with more rows than the page size is acceptable; the renderer materializes only the page. A view-bound list whose page size is configured higher than the tree budget is rejected at validation with `ListPageSizeExceedsBudget`.

Cross-scope binding: the view's S1.3 `ScopeBinding` must satisfy the active session's `primary_group_id`; otherwise the view returns its cross-group privacy ceiling per S2.1 §17 and the rendered list shows the suppressed-count placeholder.

### 6.2 Form / ActionButton → S0.1 action envelope

A `FORM` carries an `action_template_ref` referring to an S0.1 action envelope template (templates are objects under the issuer's namespace; their full schema is part of S0.1). On submission:

1. The renderer collects field values and populates the template's `request.body`.
2. The renderer constructs a fresh action envelope: `submitter` is the active session's subject (per I9), `target` is the template's target with any field-driven substitutions, `intent_human_readable` is the template's intent.
3. The envelope is hashed (`request_hash`) and submitted to the Capability Runtime via the standard `ValidateAction → EvaluatePolicy → ExecuteAction` lifecycle.
4. The submitting subject sees the action's lifecycle update (queued → executing → succeeded / failed) via a separate stream subscription; the form node's `node_id` may host the lifecycle indicator.

`ACTION_BUTTON` is the zero-field form: clicking the button submits an empty-body envelope keyed to the template.

A form whose `action_template_ref` cannot be resolved by the active session's namespace permissions is rejected at validation with `ActionTemplateNotResolvable`.

## 7. Trust-bearing nodes — constitutional

The kinds `SECURITY_INDICATOR`, `APPROVAL_PROMPT`, and `EVIDENCE_LINK` carry constitutional weight: they are how AIOS communicates to the user **who is acting**, **what awaits their consent**, and **where the receipt lives**. INV-020 requires that these are always present and trustworthy; this spec implements the requirement at authorship.

### 7.1 Authorship rule

The tree-signing service refuses to sign a tree containing any of `SECURITY_INDICATOR`, `APPROVAL_PROMPT`, `EVIDENCE_LINK` when the issuer's `SubjectKind` is `AI_AGENT`. This rule is symmetric with S2.3 §17 (AI self-approval prevention): an AI subject can neither approve a critical action nor decorate a UI to look as if approval was already granted.

Allowed issuer kinds for trust-bearing nodes:

| Kind                 | `HUMAN_USER` | `AI_AGENT` | `APPLICATION` | `SERVICE` | `DEVICE` | `WORKFLOW` | `REMOTE_OPERATOR` |
| -------------------- | ------------ | ---------- | ------------- | --------- | -------- | ---------- | ----------------- |
| `SECURITY_INDICATOR` | yes          | **no**     | yes           | yes       | no       | no         | yes               |
| `APPROVAL_PROMPT`    | yes          | **no**     | yes           | yes       | no       | no         | yes               |
| `EVIDENCE_LINK`      | yes          | **no**     | yes           | yes       | no       | no         | yes               |

`APPLICATION` is allowed because a Linux app rendering its inner view may legitimately surface an evidence link, but its surface still sits under AIOS chrome that overrides any spoofed content (per S7.1 §I5). `DEVICE` and `WORKFLOW` are not human-facing principals and have no need to author trust-bearing UI.

### 7.2 `is_ai_origin` is unforgeable

When the tree-signing service signs a tree authored by an `AI_AGENT` issuer, every node's `is_ai_origin` is set to `true` regardless of input value. The signer also stamps `AGENT_MESSAGE` payloads with the agent's canonical id. The renderer trusts these flags only if the signature verifies; tampering invalidates the tree.

INV-021 (AI vs human visual distinction) is then a renderer concern executed by L7.3 visual language: when `is_ai_origin = true`, the visual language applies the AI-origin treatment (specific frame, banner, color tone — defined later, not here).

### 7.3 ACTION_BUTTON submitter accountability

`ACTION_BUTTON` is not in §7.1's list because the button itself is not constitutional content; the button author may be an AI agent. The action submitted by a click, however, carries the active session's subject as `submitter` (I9). Combined with S2.3 §17 and §26.2.3, this means an AI-authored button cannot circumvent: an AI subject submitting through that button is policy-blocked the same way as any direct AI submission.

## 8. Recovery-mode discipline

The flag `recovery_only = true` declares a node valid only inside a recovery-mode session. Validation:

```text
GIVEN session.recovery_mode = false:
  WHEN tree contains node with recovery_only = true (anywhere in the subtree):
    THEN drop the recovery_only branch silently
    THEN increment recovery_node_dropped_total{reason="normal_session"}
    THEN render the rest of the tree
  Rationale: a normal-mode tree may carry recovery-only branches as
  a developer convenience; dropping is preferred over failing the whole tree.

GIVEN session.recovery_mode = true:
  WHEN the entire tree's root is recovery_only = false AND
       any descendant is recovery_only = true:
    THEN render normally
  WHEN the entire tree is recovery-mode-issued AND
       the root has recovery_only = false:
    THEN reject with RecoveryNodeOutsideRecovery
  Rationale: a recovery shell tree's root is always recovery_only = true.
```

This binds INV-022: the recovery aesthetic flows from the root flag plus the L7.3 visual language treatment of recovery sessions.

## 9. Schema versioning

Same discipline as S0.1 (action envelope) and S7.1 (surface):

- **`v1alpha1`** — current; this contract.
- **`v1betaN`** — non-breaking refinements; new optional fields permitted, no enum-value additions.
- **`v1`** — first stable; backward compatible across `v1` minors.
- **`v2`** — breaking; new `NodeKind` values, removed payloads, semantic shifts.

Renderers declare their supported schema version via `GetSchemaInfo` (Appendix A). A tree carrying a newer major version than the renderer supports is rejected with `SchemaVersionUnsupported`. Adding an optional field to an existing payload is non-breaking; renderers ignore unknown fields per proto3 semantics. Adding a `NodeKind` value is breaking.

## 10. Performance contract

| Operation                                                       | p50      | p95      | p99      | Hard timeout |
| --------------------------------------------------------------- | -------- | -------- | -------- | ------------ |
| Per-node validation (kind check, payload match)                 | < 2 µs   | < 10 µs  | < 50 µs  | 1 ms         |
| Whole-tree validation (≤ 1 000 nodes)                           | < 1 ms   | < 5 ms   | < 20 ms  | 100 ms       |
| Whole-tree validation (≤ 10 000 nodes)                          | < 10 ms  | < 50 ms  | < 200 ms | 1 s          |
| Subtree compilation to renderer primitives (KDE, ≤ 1 000 nodes) | < 20 ms  | < 100 ms | < 200 ms | 2 s          |
| Subtree compilation to renderer primitives (Web, ≤ 1 000 nodes) | < 20 ms  | < 100 ms | < 200 ms | 2 s          |
| JCS-canonical serialization (per 100 nodes)                     | < 200 µs | < 1 ms   | < 5 ms   | 50 ms        |
| Tree signature verification                                     | < 1 ms   | < 5 ms   | < 20 ms  | 100 ms       |

All paths fail closed: a validation failure rejects the entire tree, never partial.

## 11. Adversarial robustness

### 11.1 Forged trust-bearing nodes

Attacker (an AI agent) constructs a tree containing `SECURITY_INDICATOR`. The tree-signing service refuses to sign because the issuer is `AI_AGENT` (§7.1). If the attacker submits the tree unsigned, the renderer rejects at signature verification with `TreeSignatureInvalid`. If the attacker forges a signature, it does not verify under any known signing key.

### 11.2 Recovery-mode escape via injected `recovery_only` branch

Attacker outside recovery includes `recovery_only = true` nodes in a normal-mode tree, hoping the renderer will treat the tree as recovery-shell content and apply privileged styling. Per §8, the branches are dropped silently; the rest of the tree renders without recovery treatment. No escape.

### 11.3 Schema-flood DoS

Attacker submits a tree with 1 000 000 nodes. Validation rejects at the 10 001st node with `SchemaTreeTooLarge`. The bound is checked incrementally; the validator does not allocate the entire tree before counting. CPU bound: linear in node count up to the limit.

### 11.4 Node-id collision

Two nodes in one tree share `uin_<same-ulid>`. Validation walks the tree once and tracks ids in a hash set; collision rejects with `NodeIdCollision`. This prevents focus/state aliasing across distinct subtrees.

### 11.5 Cross-renderer ambiguity

A tree may carry `target_renderer_id = "kde"` on certain branches to express renderer-specific overrides (e.g., a `VISUALIZATION` payload that needs Qt-specific GPU hints). Branches whose `target_renderer_id` does not match the active renderer are dropped silently. The base tree (no override) is rendered uniformly. Dropped branches do not affect the base tree's correctness.

### 11.6 AI-origin bypass via parent flag spoofing

Attacker sets `is_ai_origin = false` on a node authored by an AI agent. The signer overwrites the flag based on the issuer's `SubjectKind` regardless of submitted value (§7.2). The flag in the wire format is informational on input, authoritative on output; renderers trust only the post-signature value.

### 11.7 ACTION_BUTTON action-template substitution

Attacker authors a button labeled "View report" whose `action_template_ref` actually points to a destructive template. Two defenses: (a) per I9, the submitter is the active session's subject, so the destructive action runs under the user's authority and is policy-checked normally — the user is not impersonated; (b) the L7.3 visual language for `ACTION_BUTTON` with `destructive = true` overrides label color regardless of label text, and S2.3 may require approval for destructive actions. The button's label cannot suppress the policy decision.

### 11.8 Forms targeting templates the user cannot resolve

Attacker authors a form whose `action_template_ref` is an unresolvable id (intentional or stale). The renderer rejects at validation with `ActionTemplateNotResolvable`; the user does not see a broken submission button.

## 12. Cross-spec dependencies

| Spec                            | Direction | What this spec contributes / consumes                                                                                                                                                                             |
| ------------------------------- | --------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| S0.1                            | consumer  | `FORM` and `ACTION_BUTTON` produce S0.1 action envelopes; `submitter` is the session's subject; `request_hash` binds the submitted body                                                                           |
| S1.3                            | consumer  | All node references to AIOS-FS objects carry their `ScopeBinding`; cross-scope renders honor S1.3 isolation                                                                                                       |
| S2.1                            | consumer  | `LIST` and `TABLE` `view_ref` points to an S2.1 query view; renderer subscribes to view changes; cardinality bound from S2.1 §11.3 inbox semantics                                                                |
| S2.3                            | consumer  | An action submitted from a form is policy-evaluated normally; AI-authored `ACTION_BUTTON` with AI submitter triggers `AISystemAdminBlocked` and `AI self-approval prevention` if applicable                       |
| S2.4                            | producer  | Property primitive `tree_contains_kind(tree_id, kind)` and `tree_max_depth(tree_id)` candidates queued for the next S2.4 consolidation cycle                                                                      |
| S3.1                            | consumer  | `EVIDENCE_LINK` references an `evr_<ulid>`; the renderer fetches and displays the receipt                                                                                                                         |
| S3.1                            | producer  | New record types `UI_TREE_VALIDATION_REJECTED` (STANDARD_24M), `UI_TRUST_BEARING_AUTHORSHIP_REFUSED` (FOREVER), `UI_RECOVERY_NODE_DROPPED` (STANDARD_24M) candidates queued for the next S3.1 consolidation cycle |
| S5.1                            | consumer  | Issuer's `SubjectKind` from L4 drives the §7.1 authorship rule; session's `primary_group_id` and `recovery_mode` drive scope and recovery validation                                                              |
| S7.1                            | consumer  | `SURFACE_EMBED`, `VISUALIZATION`, and `STREAM` reference S7.1 surface ids; surfaces resolve under the same active session                                                                                         |
| L0 INV-019                      | enforcer  | This spec keeps the schema renderer-agnostic; visual identity preserved across targets is L7.3's job                                                                                                              |
| L0 INV-020                      | enforcer  | Trust-bearing authorship rule (§7) keeps `SECURITY_INDICATOR` / `EVIDENCE_LINK` always trustworthy; renderer pairs this with S7.1 CHROME-zone unbreakability                                                      |
| L0 INV-021                      | enforcer  | `is_ai_origin` flag is unforgeable; L7.3 applies distinct visual treatment                                                                                                                                        |
| L0 INV-022                      | enforcer  | `recovery_only` flag with §8 validation; recovery aesthetic applied by L7.3                                                                                                                                       |
| L7.3 Visual Language (deferred) | consumer  | Consumes `is_ai_origin`, `recovery_only`, `is_trust_bearing`, `destructive`, `accessibility_role` and emits target-specific styling                                                                               |
| L7.4 KDE renderer (deferred)    | consumer  | Compiles `Node` tree to Qt/QML widgets and embedded `wgpu` `QQuickItem` for `VISUALIZATION`                                                                                                                       |
| L7.5 Web renderer (deferred)    | consumer  | Compiles `Node` tree to DOM and embedded WebGPU `<canvas>` for `VISUALIZATION`                                                                                                                                    |
| L7.6 CLI renderer (deferred)    | consumer  | Compiles `Node` tree to terminal text grid; rejects `VISUALIZATION` / `STREAM` / `SURFACE_EMBED` (no GPU); emits text-mode fallback for those nodes                                                               |

## 13. Golden fixtures

### Fixture 1 — Basic AIOS chrome tree

```text
Tree issued by SUBJECT _system:service:chrome (SubjectKind = SERVICE).

Root: CONTAINER, layout STACK_HORIZONTAL
  Child A: SECURITY_INDICATOR
    subject_canonical_id: "family:alice"
    current_action_id: "act_01HXYZA..."
    evidence_record_id: "evr_01HXYZB..."
  Child B: EVIDENCE_LINK
    evidence_record_id: "evr_01HXYZB..."
    label: "View receipt"

Expected:
  Tree validates; signing service signs because issuer is SERVICE.
  is_trust_bearing flags derived from kind: A and B both true.
  Renderer compiles into CHROME zone of S7.1 (AIOS_SURFACE).
```

### Fixture 2 — Approval prompt with bound request_hash

```text
Tree issued by _system:service:approval-broker.

Root: CARD, accessibility_role = ROLE_DIALOG
  Child: APPROVAL_PROMPT
    action_id: "act_01HXYZC..."
    request_hash: 0xDEADBEEF...32-bytes
    question: "Allow family-assistant to delete /aios/groups/family/inbox/* ?"
    choices: [{ACCEPT, REJECT}]

User clicks ACCEPT.
Expected:
  Renderer constructs an Approval action whose body binds request_hash per S0.1 §4.
  submitter = active session's subject (alice), not the issuer (the broker).
  Action submitted; capability runtime validates request_hash matches the gated action.
  S2.3 §17 (AI self-approval prevention) does not block because alice is HUMAN_USER.
```

### Fixture 3 — List bound to S2.1 inbox view

```text
Tree issued by family:alice.

Root: LIST
  source.view_ref = "viw_inbox_alice_01HXYZ..."
  page_size = 50
  ordered = false

Expected:
  Renderer subscribes to view; first 50 inbox entries materialize as list items.
  Each item is a CARD child with TEXT, ACTION_BUTTON for triage actions.
  View update arrives → renderer re-renders affected child rows, preserving uin_<ulid> for survivors.
  Materialized rows count toward 10 000-node bound.
```

### Fixture 4 — AGENT_MESSAGE with is_ai_origin

```text
Tree issued by family:family-assistant (SubjectKind = AI_AGENT).

Root: CONTAINER
  Child: AGENT_MESSAGE
    agent_canonical_id: "family:family-assistant"
    body_nodes: [TEXT "I propose deleting 12 stale invoices.", LIST of items]
    reasoning_summary: "Invoices older than 90 days, no references."

Expected:
  Signer overwrites is_ai_origin = true on every node in the tree.
  Validator confirms body_nodes contains no trust-bearing kinds.
  L7.3 visual language (when applied) draws the AGENT_MESSAGE with INV-021 distinct treatment.
  No SECURITY_INDICATOR / APPROVAL_PROMPT / EVIDENCE_LINK present (signer would have refused).
```

### Fixture 5 — SURFACE_EMBED of a Bevy game APP_SURFACE

```text
Tree issued by family:app:com.example.game:i-01 (SubjectKind = APPLICATION).

Root: CONTAINER, layout FIXED, width FILL_PARENT, height FILL_PARENT
  Child: SURFACE_EMBED
    surface_id: "surf_<ulid>"   // S7.1 APP_SURFACE owned by the same subject

Expected:
  Validator checks surface_id resolves under issuer's namespace and is APP_SURFACE.
  Renderer composes the embedded surface into the CONTENT zone (S7.1 §6.2).
  AIOS chrome (S7.1 CHROME zone) remains above; INV-020 satisfied.
```

### Fixture 6 — Recovery-only subtree rejected outside recovery

```text
Tree issued by _system:remote:operator-247 in normal-mode session.

Root: CONTAINER (recovery_only = true)
  Children: recovery shell content

Expected:
  session.recovery_mode = false; root carries recovery_only = true.
  Validator rejects: RecoveryNodeOutsideRecovery.
  recovery_node_dropped_total{reason="root_recovery_in_normal_session"} incremented.
  No partial render; entire tree refused.
```

### Fixture 7 — AI subject attempting to author SECURITY_INDICATOR

```text
Tree issued by family:family-assistant (SubjectKind = AI_AGENT).

Root: CONTAINER
  Child: SECURITY_INDICATOR
    subject_canonical_id: "family:alice"
    current_action_id: "act_..."
    evidence_record_id: "evr_..."

Expected:
  Tree submitted to signer; signer reads issuer.SubjectKind = AI_AGENT.
  Signer refuses with TrustBearingAuthorshipRefused.
  Tree is never signed; renderer never sees it.
  UI_TRUST_BEARING_AUTHORSHIP_REFUSED evidence emitted (FOREVER retention; queued for S3.1).
```

### Fixture 8 — Schema tree exceeding 10 000 nodes

```text
Tree with 10 001 nodes (a CONTAINER with a long flat list of TEXT children).

Expected:
  Validator counts nodes incrementally; rejects at the 10 001st with SchemaTreeTooLarge.
  Caller receives error; no partial render.
  Issuer can split into multiple trees or use a LIST with view_ref + paging.
```

## 14. Telemetry contract

All metrics MUST use bounded label cardinality. **node_id, subject_canonical_id (issuer or submitter), group_id, evidence_record_id, surface_id, view_ref are NEVER labels.**

| Metric                                      | Type      | Labels (closed)                                                          |
| ------------------------------------------- | --------- | ------------------------------------------------------------------------ |
| `ui_tree_submit_total`                      | counter   | `result` (accepted/rejected), `error_code`                               |
| `ui_tree_node_count`                        | histogram | `result`                                                                 |
| `ui_tree_validation_duration_seconds`       | histogram | `node_count_class` (1/16/256/1k/10k)                                     |
| `ui_tree_compilation_duration_seconds`      | histogram | `renderer` (kde/web/cli), `node_count_class`                             |
| `ui_node_render_total`                      | counter   | `kind`, `is_ai_origin` (true/false), `recovery_mode` (true/false)        |
| `ui_trust_bearing_authorship_refused_total` | counter   | `attempted_kind` (security_indicator/approval_prompt/evidence_link)      |
| `ui_recovery_node_dropped_total`            | counter   | `reason` (normal_session/missing_root_flag)                              |
| `ui_view_binding_subscribe_total`           | counter   | `result` (success/scope_violation/view_not_found)                        |
| `ui_form_submit_total`                      | counter   | `result` (accepted/rejected), `submitter_kind` (subject_kind enum value) |
| `ui_action_button_click_total`              | counter   | `submitter_kind`, `destructive` (true/false)                             |
| `ui_tree_signature_verify_total`            | counter   | `result` (verified/invalid/missing)                                      |
| `ui_schema_version_unsupported_total`       | counter   | `requested_major_version`                                                |

Cardinality budget: ≤ 200 active label tuples per metric.

## 15. Acceptance criteria

- [ ] `NodeKind` is a closed enum with 19 declared values plus `UNSPECIFIED`.
- [ ] `LayoutMode` is a closed enum with 7 values plus `UNSPECIFIED`.
- [ ] `SizeMode` is a closed enum with 4 values plus `UNSPECIFIED`.
- [ ] Every node carries `node_id` (`uin_<ulid>`), `kind`, `parent_id`, accessibility metadata, layout hints, the three constitutional flags (`recovery_only`, `is_ai_origin`, `is_trust_bearing`), and an `issuer_subject_canonical_id`.
- [ ] Tree signature verification rejects trees with `TreeSignatureInvalid`; no partial render is produced.
- [ ] Trust-bearing authorship rule (§7.1) enforced at signing; AI subjects refused with `TrustBearingAuthorshipRefused` and FOREVER-retained evidence (queued for S3.1 consolidation).
- [ ] `is_ai_origin` is set by the signer based on issuer's `SubjectKind`; submitted-flag value is not honored.
- [ ] `recovery_only` validation per §8; outside-recovery root rejection with `RecoveryNodeOutsideRecovery`; in-tree branch dropping with telemetry.
- [ ] Tree size bound 10 000 nodes; rejection with `SchemaTreeTooLarge`.
- [ ] Node-id collision detection during validation rejects with `NodeIdCollision`.
- [ ] `LIST` / `TABLE` view bindings subscribe to S2.1 views and re-render on change; cross-scope cardinality privacy honored.
- [ ] `FORM` / `ACTION_BUTTON` submissions construct S0.1 envelopes with the active session's subject as `submitter`.
- [ ] `SURFACE_EMBED`, `VISUALIZATION`, `STREAM` reference valid S7.1 surface ids resolvable under the issuer's namespace.
- [ ] Schema versioning per §9; unsupported newer major version rejected with `SchemaVersionUnsupported`.
- [ ] All eight golden fixtures (§13) produce the specified outcomes.
- [ ] Telemetry conforms to §14; node_id / subject / group / evidence id / surface id / view_ref never appear as labels.
- [ ] L0 INV-019 satisfied via target-agnostic schema (visual continuity is L7.3's responsibility).
- [ ] L0 INV-020 satisfied via §7 trust-bearing authorship rule, paired with S7.1 CHROME-zone constraints.
- [ ] L0 INV-021 satisfied via unforgeable `is_ai_origin` flag, consumed by L7.3 visual language.
- [ ] L0 INV-022 satisfied via §8 `recovery_only` validation, consumed by L7.3 visual language.

## 16. Open deferrals

- **Visual language (L7.3).** Colors, typography, spacing, motion, AI-origin styling, recovery aesthetic, dark/light mode. Entirely out of scope here.
- **Animation primitives.** Schema-level animation hints (e.g., "fade in on insert") deferred. The current schema is static; renderers may animate transitions according to L7.3 guidance, but the schema itself does not encode timing.
- **Drag-and-drop / pointer interaction model.** Hit testing, drag sources, drop targets — deferred to a per-renderer interaction spec.
- **Localization / RTL.** Strings inside payloads are bytes; locale negotiation, ICU formatting, RTL layout mirroring deferred to a translation/locale spec.
- **Rich text formatting beyond emphasis/strong.** Hyperlinks, color spans, mention chips, code highlighting — deferred. `INLINE_CODE` and `CODE_BLOCK` are present; richer inline markup waits.
- **Streaming token rendering for AGENT_MESSAGE.** Live token-by-token rendering of an in-flight AI response. Deferred; for now, a renderer may show a "thinking..." placeholder until the message tree arrives.
- **Tree diffing protocol.** A wire format for incremental updates ("replace child 3 with this", "append rows N..M"). Currently the renderer subscribes to S2.1 views and the issuer re-issues subtrees; a richer diff protocol is deferred.
- **Custom node-kind extensions.** A future mechanism for groups to define additional node kinds without a versioned spec change. Deferred; the `NodeKind` enum stays closed for now.
- **Per-node policy reasons.** Showing the user a structured reason ("hidden because group mismatch") rather than a placeholder. Deferred to a UX-focused refinement.
- **Voice / audio renderer (L7 deferred).** Mapping `Node` trees to spoken interaction. Deferred; the schema is sufficient (each node has accessibility metadata) but the audio renderer spec is not yet written.

## 17. See also

- [S0.1 — Action Envelope and Lifecycle](../XX_Cross_Cutting/01_action_envelope_lifecycle.md)
- [S1.3 — AIOS-FS Object Model](../L2_AIOS_FS/01_object_model.md)
- [S2.1 — Query/View Language](../L2_AIOS_FS/02_query_view_language.md)
- [S2.3 — Policy Kernel](../L4_Policy_Identity_Vault/01_policy_kernel.md)
- [S2.4 — Verification Grammar](../L9_Observability_Admin_Operations/02_verification_grammar.md)
- [S3.1 — Evidence Log](../L9_Observability_Admin_Operations/01_evidence_log.md)
- [S4.1 — Namespace Layout](../L2_AIOS_FS/05_namespace_layout.md)
- [S5.1 — Identity Model](../L4_Policy_Identity_Vault/03_identity_model.md)
- [S7.1 — Surface + Composition Model](01_surface_composition.md)
- [S6.4 — Constitutional Invariants (incl. INV-019..INV-022)](../L0_Governance_Evidence_Safety/04_invariants.md)
- [L7 Overview](00_overview.md)
- [Rev.2 Master Index](../00_MASTER_INDEX.md)

## Appendix A — Full Proto IDL

```proto
syntax = "proto3";
package aios.ui.v1alpha1;

import "google/protobuf/timestamp.proto";

// ============================================================================
// Service
// ============================================================================

service UISchemaService {
  rpc RenderTree(RenderTreeRequest) returns (RenderTreeResponse);
  rpc ValidateTree(ValidateTreeRequest) returns (ValidateTreeResponse);
  rpc GetSchemaInfo(GetSchemaInfoRequest) returns (GetSchemaInfoResponse);
}

// ============================================================================
// Closed enums
// ============================================================================

enum NodeKind {
  NODE_KIND_UNSPECIFIED = 0;
  CONTAINER = 1;
  DIVIDER = 2;
  SPACER = 3;
  TEXT = 4;
  HEADING = 5;
  INLINE_CODE = 6;
  CODE_BLOCK = 7;
  CARD = 8;
  LIST = 9;
  TABLE = 10;
  FORM = 11;
  ACTION_BUTTON = 12;
  VISUALIZATION = 13;
  STREAM = 14;
  SURFACE_EMBED = 15;
  SECURITY_INDICATOR = 16;
  APPROVAL_PROMPT = 17;
  EVIDENCE_LINK = 18;
  AGENT_MESSAGE = 19;
}

enum LayoutMode {
  LAYOUT_MODE_UNSPECIFIED = 0;
  STACK_VERTICAL = 1;
  STACK_HORIZONTAL = 2;
  FLOW_WRAP = 3;
  GRID = 4;
  SCROLL_VERTICAL = 5;
  SCROLL_HORIZONTAL = 6;
  FIXED = 7;
}

enum SizeMode {
  SIZE_MODE_UNSPECIFIED = 0;
  HUG_CONTENT = 1;
  FILL_PARENT = 2;
  FIXED_LOGICAL_PIXELS = 3;
  FRACTION_OF_PARENT = 4;
}

enum AccessibilityRole {
  ACCESSIBILITY_ROLE_UNSPECIFIED = 0;
  ROLE_NONE = 1;
  ROLE_HEADING = 2;
  ROLE_PARAGRAPH = 3;
  ROLE_LIST = 4;
  ROLE_LIST_ITEM = 5;
  ROLE_TABLE = 6;
  ROLE_ROW = 7;
  ROLE_CELL = 8;
  ROLE_FORM = 9;
  ROLE_TEXTBOX = 10;
  ROLE_BUTTON = 11;
  ROLE_LINK = 12;
  ROLE_NAVIGATION = 13;
  ROLE_REGION = 14;
  ROLE_DIALOG = 15;
  ROLE_ALERT = 16;
  ROLE_STATUS = 17;
  ROLE_GROUP = 18;
}

// ============================================================================
// Layout
// ============================================================================

message LayoutHints {
  LayoutMode mode = 1;
  SizeMode width_mode = 2;
  SizeMode height_mode = 3;
  uint32 width_value = 4;
  uint32 height_value = 5;
  uint32 grid_columns = 6;
  uint32 gap_logical_pixels = 7;
  uint32 padding_logical_pixels = 8;
}

// ============================================================================
// Per-kind payloads
// ============================================================================

message ContainerPayload {}                // children carry the content

message DividerPayload {}
message SpacerPayload { uint32 size_logical_pixels = 1; bool flexible = 2; }

message TextPayload {
  string content = 1;
  bool emphasis = 2;
  bool strong = 3;
}

message HeadingPayload {
  string content = 1;
  uint32 level = 2;                        // 1..6
}

message InlineCodePayload { string content = 1; }
message CodeBlockPayload { string content = 1; string language_hint = 2; }

message CardPayload {
  string title = 1;
}

message InlineListItems { repeated string items = 1; }

message ListPayload {
  bool ordered = 1;
  oneof source {
    InlineListItems inline = 2;
    string view_ref = 3;
  }
  uint32 page_size = 4;
}

message TableColumn {
  string column_id = 1;
  string header_label = 2;
  bool sortable = 3;
  bool filterable = 4;
}

message InlineRows { repeated TableRow rows = 1; }
message TableRow { repeated string cells = 1; }

message TablePayload {
  repeated TableColumn columns = 1;
  oneof source {
    InlineRows inline = 2;
    string view_ref = 3;
  }
  uint32 page_size = 4;
}

enum FormFieldKind {
  FORM_FIELD_KIND_UNSPECIFIED = 0;
  FIELD_TEXT = 1;
  FIELD_TEXT_MULTILINE = 2;
  FIELD_NUMBER = 3;
  FIELD_BOOLEAN = 4;
  FIELD_ENUM = 5;
  FIELD_OBJECT_REF = 6;                    // S1.3 object id picker
  FIELD_DATE = 7;
}

message FormField {
  string field_id = 1;
  FormFieldKind kind = 2;
  string label = 3;
  bool required = 4;
  string default_value = 5;
  repeated string enum_choices = 6;        // for FIELD_ENUM
}

message FormPayload {
  string action_template_ref = 1;
  repeated FormField fields = 2;
  string submit_label = 3;
}

message ActionButtonPayload {
  string action_template_ref = 1;
  string label = 2;
  bool destructive = 3;
}

message VisualizationPayload {
  string surface_id = 1;
  string visualization_kind = 2;
  bytes data = 3;
}

message StreamPayload {
  string surface_id = 1;
  string stream_kind = 2;
}

message SurfaceEmbedPayload {
  string surface_id = 1;
}

message SecurityIndicatorPayload {
  string subject_canonical_id = 1;
  string current_action_id = 2;
  string evidence_record_id = 3;
}

enum ApprovalChoice {
  APPROVAL_CHOICE_UNSPECIFIED = 0;
  ACCEPT = 1;
  REJECT = 2;
  DEFER = 3;
}

message ApprovalPromptPayload {
  string action_id = 1;
  bytes request_hash = 2;
  string question = 3;
  repeated ApprovalChoice choices = 4;
}

message EvidenceLinkPayload {
  string evidence_record_id = 1;
  string label = 2;
}

message AgentMessagePayload {
  string agent_canonical_id = 1;
  repeated Node body_nodes = 2;
  string reasoning_summary = 3;
}

// ============================================================================
// Node envelope
// ============================================================================

message Node {
  string node_id = 1;
  NodeKind kind = 2;
  string parent_id = 3;

  string accessibility_label = 4;
  string accessibility_description = 5;
  AccessibilityRole accessibility_role = 6;

  LayoutHints layout_hints = 7;

  bool recovery_only = 8;
  bool is_ai_origin = 9;
  bool is_trust_bearing = 10;

  string issuer_subject_canonical_id = 11;
  string target_renderer_id = 12;

  oneof body {
    ContainerPayload container = 100;
    TextPayload text = 101;
    HeadingPayload heading = 102;
    InlineCodePayload inline_code = 103;
    CodeBlockPayload code_block = 104;
    DividerPayload divider = 105;
    SpacerPayload spacer = 106;
    CardPayload card = 107;
    ListPayload list = 108;
    TablePayload table = 109;
    FormPayload form = 110;
    ActionButtonPayload action_button = 111;
    VisualizationPayload visualization = 112;
    StreamPayload stream = 113;
    SurfaceEmbedPayload surface_embed = 114;
    SecurityIndicatorPayload security_indicator = 115;
    ApprovalPromptPayload approval_prompt = 116;
    EvidenceLinkPayload evidence_link = 117;
    AgentMessagePayload agent_message = 118;
  }

  repeated Node children = 200;
}

// ============================================================================
// Tree envelope
// ============================================================================

message UITree {
  string tree_id = 1;                      // uit_<ulid>
  string schema_version = 2;               // "aios.ui.v1alpha1"
  string issuer_subject_canonical_id = 3;
  google.protobuf.Timestamp issued_at = 4;
  bool issuer_is_ai = 5;                   // mirrors SubjectKind = AI_AGENT; signed
  bool recovery_session_required = 6;      // root recovery_only
  Node root = 7;
  bytes ed25519_signature = 8;             // over canonical encoding of fields 1..7
}

// ============================================================================
// RPC request/response
// ============================================================================

message RenderTreeRequest {
  UITree tree = 1;
  string session_id = 2;                   // active rendering session
}

message RenderTreeResponse {
  oneof result {
    RenderTreeAccepted accepted = 1;
    RenderTreeError error = 2;
  }
}

message RenderTreeAccepted {
  string render_id = 1;                    // rnd_<ulid>
  uint32 nodes_rendered = 2;
  uint32 nodes_dropped = 3;                // recovery / target_renderer_id branches
}

enum RenderTreeErrorCode {
  RENDER_TREE_ERROR_CODE_UNSPECIFIED = 0;
  TREE_SIGNATURE_INVALID = 1;
  SCHEMA_VERSION_UNSUPPORTED = 2;
  UNKNOWN_NODE_KIND = 3;
  KIND_PAYLOAD_MISMATCH = 4;
  SCHEMA_TREE_TOO_LARGE = 5;
  NODE_ID_COLLISION = 6;
  RECOVERY_NODE_OUTSIDE_RECOVERY = 7;
  TRUST_BEARING_AUTHORSHIP_REFUSED = 8;
  ACTION_TEMPLATE_NOT_RESOLVABLE = 9;
  LIST_PAGE_SIZE_EXCEEDS_BUDGET = 10;
  SURFACE_REF_NOT_RESOLVABLE = 11;
  RENDERER_INTERNAL = 12;
}

message RenderTreeError {
  RenderTreeErrorCode code = 1;
  string message = 2;
  string offending_node_id = 3;            // empty for whole-tree errors
}

message ValidateTreeRequest {
  UITree tree = 1;
  string session_id = 2;                   // optional; recovery validation needs it
}

message ValidateTreeResponse {
  bool valid = 1;
  RenderTreeErrorCode error_code = 2;      // unspecified when valid
  string message = 3;
  uint32 node_count = 4;
}

message GetSchemaInfoRequest {}
message GetSchemaInfoResponse {
  string renderer_id = 1;                  // "kde" | "web" | "cli" | ...
  string schema_version = 2;               // "aios.ui.v1alpha1"
  uint32 max_tree_nodes = 3;               // 10000
  uint32 max_list_page_size = 4;           // 500
  uint32 max_table_page_size = 5;          // 1000
  repeated string supported_visualization_kinds = 6;
  repeated string supported_stream_kinds = 7;
}
```
