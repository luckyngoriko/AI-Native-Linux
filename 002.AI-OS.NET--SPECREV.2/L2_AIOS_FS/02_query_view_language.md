# AIOS-FS Query and View Language (Rev.2)

| Field     | Value                                                                                 |
| --------- | ------------------------------------------------------------------------------------- |
| Status    | `CONTRACT` (refined 2026-05-08; awaiting implementation evidence)                     |
| Phase tag | S2.1                                                                                  |
| Layer     | L2 AIOS-FS                                                                            |
| Consumes  | S1.3 object model (objects, versions, pointers, transactions), S1.2 PrivacyClass enum |
| Produces  | semantic views, query results, projections; gRPC `AIOSFSQuery`                        |
| Approved  | 2026-05-08 (deltas D1–D12 applied; replaces draft from `dfa3be5`)                     |

## 1. Purpose

AIOS-FS views let users and agents ask for objects semantically without treating paths as identity. The query language is a **constrained declarative DSL** with a closed operator vocabulary; it is not arbitrary code.

Example utterance and its DSL projection:

```text
open latest stable sdf renderer
```

```sql
from objects
where kind = "PROJECT"
  and labels contains "sdf"
  and labels contains "renderer"
  and pointer("STABLE") exists
order by updated_at desc
limit 1
project posix_path("/aios/views/projects/{name}")
```

Identical results across runs (modulo new writes) are guaranteed by the deterministic operator vocabulary in §3 and the privacy filter rules in §5.

## 2. Design constraints

- **Closed vocabulary.** Every keyword and operator is enumerated in §3. There is no `eval`, no plugin, no user-defined function.
- **No side effects.** Queries never write. Read-write operations go through `AIOSFSObjects` (object model gRPC).
- **Privacy-aware by construction.** Queries cannot leak objects above the caller's authorized privacy ceiling (§5).
- **Reproducible.** Same query text + same `as of` snapshot → same result.
- **Explainable.** Every query has a deterministic execution plan accessible via `ExplainQuery`.

## 3. Formal grammar

EBNF for the rev.2 DSL:

```ebnf
query           = source where? group_by? order_by? limit? offset? as_of? project? ;

source          = "from" ( "objects" | "versions" | "pointers"
                          | "transactions" | "conflicts" | "evidence" ) ;

where           = "where" predicate ( "and" predicate )* ;
predicate       = field op value
                | field "in" "[" value ( "," value )* "]"
                | field "contains" string_literal
                | field "exists"
                | "pointer" "(" pointer_kind_literal ")" "exists"
                | "privacy" "<=" privacy_class_literal ;

field           = identifier ( "." identifier )* ;
op              = "=" | "!=" | "<" | "<=" | ">" | ">=" ;
value           = string_literal | number_literal | boolean_literal
                | timestamp_literal | identifier_literal ;

group_by        = "group" "by" field ( "," field )* aggregations? ;
aggregations    = "select" aggregation ( "," aggregation )* ;
aggregation     = ( "count" "(" "*" ")"
                  | "count" "(" field ")"
                  | "max"   "(" field ")"
                  | "min"   "(" field ")"
                  | "first" "(" field ")"
                  | "last"  "(" field ")" ) "as" identifier ;

order_by        = "order" "by" order_term ( "," order_term )* ;
order_term      = field ( "asc" | "desc" )? ;

limit           = "limit" integer_literal ;
offset          = "offset" integer_literal ;

as_of           = "as" "of" ( version_id_literal | timestamp_literal ) ;

project         = "project" projection_spec ;
projection_spec = "object_list"
                | "graph"
                | "timeline"
                | "mount_overlay"
                | "posix_path" "(" string_literal ")" ;

pointer_kind_literal  = "CURRENT" | "STABLE" | "CANDIDATE" | "ROLLBACK" | "QUARANTINE" ;
privacy_class_literal = "PUBLIC" | "INTERNAL" | "SENSITIVE" | "SECRET_BEARING" | "CLASSIFIED" ;
```

Identifiers are `[A-Za-z_][A-Za-z0-9_]*`. String literals use double quotes with `\\"` escape. Timestamps are RFC 3339. Numbers are JSON numbers. Booleans are `true` / `false`.

`and` is the only logical conjunction; `or` is intentionally absent. To express disjunction, run two queries and union client-side, or use `field in [...]`. This bounds engine complexity.

## 4. Closed operator vocabulary

### 4.1. Sources

| Source         | Yields                                           |
| -------------- | ------------------------------------------------ |
| `objects`      | `Object` records (S1.3 `01_object_model.md` §5)  |
| `versions`     | `Version` records                                |
| `pointers`     | `Pointer` records                                |
| `transactions` | `Transaction` records                            |
| `conflicts`    | `Conflict` records (`03_conflict_resolution.md`) |
| `evidence`     | Evidence receipts (read-only projection from L9) |

### 4.2. Queryable fields

Per source, allowed fields:

**Objects:** `object_id`, `kind`, `metadata.name`, `metadata.labels`, `metadata.mime`, `created_at`, `created_by`, `policy_tags`, `privacy_class`, `lifecycle_state`, `current_pointer_id`, `index_hints`.

**Versions:** `version_id`, `object_id`, `parent_version_ids`, `content_hash`, `state`, `created_at`, `created_by_action_id`, `created_by_transaction_id`, `quarantined_at`, `quarantine_reason`.

**Pointers:** `pointer_id`, `object_id`, `kind`, `current_version_id`, `last_promoted_at`, `last_promoted_by_transaction_id`.

**Transactions:** `transaction_id`, `subject`, `action_id`, `started_at`, `completed_at`, `state`.

**Conflicts:** `conflict_id`, `object_id`, `pointer_id`, `status`, `detected_at`, `resolved_at`, `resolved_by_subject`.

**Evidence:** `receipt_id`, `actor`, `action_id`, `category`, `occurred_at` (read-only; full schema in L9 S3.1).

Fields outside this list are not queryable in rev.2. Adding fields is an additive schema change.

### 4.3. Forbidden query authority

Queries cannot read or filter on:

- Raw secret material.
- Unverified model summaries or LLM-generated metadata.
- Hidden prompt content.
- Unaudited external metadata.
- Subject authentication tokens.

Attempting to use a forbidden field returns `InvalidQuery` with the offending field name (no information leakage about which other subjects had what data).

### 4.4. Projections

| Projection        | Output shape                                                     |
| ----------------- | ---------------------------------------------------------------- |
| `object_list`     | structured list of objects (default)                             |
| `graph`           | nodes and edges (objects + version parent links + pointer refs)  |
| `timeline`        | chronologically ordered versions/events                          |
| `mount_overlay`   | compatibility projection for application sandboxes (mount-shape) |
| `posix_path(...)` | path-template projection for POSIX-compatible apps               |

Projection does not create storage identity. Path projections produced by `posix_path(...)` are views, not authoritative paths.

## 5. Privacy class filter

Every query is evaluated under the caller's **privacy ceiling** — the maximum class that the calling subject is authorized to see, derived from L4 policy.

### 5.1. Default behavior

Without an explicit `privacy <= ...` predicate, the query implicitly applies `privacy <= caller_ceiling`.

### 5.2. Explicit predicate

A query may include `privacy <= INTERNAL` to constrain results below the caller's ceiling (useful for review or sharing). It cannot raise above the ceiling.

### 5.3. Filter semantics

Objects whose `privacy_class` exceeds the effective ceiling are **silently filtered out**:

- They do not appear in results.
- Counts and aggregations exclude them.
- An evidence record is emitted noting "N objects filtered by privacy ceiling" — no object IDs leaked, just a count.

### 5.4. Privacy ceiling sources

| Source                         | Effect                                                  |
| ------------------------------ | ------------------------------------------------------- |
| Subject's policy-derived class | Default ceiling                                         |
| Active session class           | May lower the ceiling (e.g. recovery mode = `INTERNAL`) |
| Renderer-declared class        | Renderer may declare a lower ceiling for embed contexts |

The router never raises the ceiling. `CLASSIFIED` content reaches a query only if all three sources align.

## 6. Materialization model

### 6.1. Virtual vs materialized

| Mode             | When                                                        |
| ---------------- | ----------------------------------------------------------- |
| **Virtual**      | Query evaluated on demand, no stored projection. Default.   |
| **Materialized** | View result is computed and stored; reads return the cache. |

`CreateView` accepts a `materialized: bool` flag. Materialized views require a refresh strategy.

### 6.2. Refresh strategies

| Strategy    | Trigger                                                                             |
| ----------- | ----------------------------------------------------------------------------------- |
| `ON_DEMAND` | Refresh when read after `staleness_threshold` (default 5 minutes) since last build. |
| `ON_WRITE`  | Refresh when an evidence event indicates a write to a queried source.               |
| `SCHEDULED` | Refresh on a fixed cadence (operator-set; minimum 1 minute).                        |
| `MANUAL`    | Refresh only via `RebuildView` RPC.                                                 |

### 6.3. Invalidation

A materialized view is invalidated by:

- Catalog version flip in S1.1.
- Object model schema migration (rev bump).
- Evidence event affecting any source the view queries (when strategy is `ON_WRITE`).
- Operator-triggered flush.
- Removal of any object that contributed to the materialized result.

Invalidation marks the view as stale; the next read triggers refresh per strategy.

### 6.4. Cost notes

Virtual queries pay query cost on every read but never stale. Materialized views pay query cost on refresh and may serve stale results within the staleness window. Choose materialized only when the query is read-heavy (>10× more reads than writes to its sources) and result is small (<10 MB).

## 7. Time-travel queries

Queries may add `as of <version_id>` or `as of <timestamp>` to evaluate against a historical snapshot.

### 7.1. Semantics

```text
as of ver_01HX...      -- evaluate as if pointer.current_version_id was ver_01HX... for the relevant pointer
as of "2026-05-08T12:00:00Z" -- evaluate as if reading at that wall time, using SNAPSHOT consistency
```

### 7.2. Constraints

- The referenced version must exist (and not be `PURGED`).
- For timestamp-based queries, the AIOS-FS transaction log must retain enough history; default 90 days; older queries fail with `TimeTravelHorizonExceeded`.
- Time-travel queries are always evaluated under `SNAPSHOT` consistency (object model §11).
- Privacy filter still applies at the historical snapshot — i.e., if an object's privacy class was lower at `T0` than now, the query honors the historical class. Privacy class is monotonically non-decreasing (S1.3 §4.1), so historical classes are always ≤ current.

### 7.3. Materialization

Materialized views with `as of` are forbidden — the snapshot semantics conflict with refresh strategies. Time-travel must be virtual.

## 8. Pagination, budget, and timeout

### 8.1. Pagination

Two pagination modes:

| Mode           | Use case                                                          |
| -------------- | ----------------------------------------------------------------- |
| `OFFSET_LIMIT` | Stable across small result sets; simple but unstable under writes |
| `CURSOR`       | Stable across writes; recommended for large or live datasets      |

Cursor is an opaque base32 string encoding the engine's resume position. Cursors are valid for 30 minutes; expired cursors fail with `CursorExpired` and require re-issuing the query.

### 8.2. Query budget

| Budget              | Default              | Behavior on exhaustion   |
| ------------------- | -------------------- | ------------------------ |
| Wall-clock timeout  | 5 s                  | `QueryTimeout`           |
| Memory              | 256 MB               | `QueryMemoryExhausted`   |
| Result size         | 10 MB or 10 000 rows | `QueryResultTooLarge`    |
| Source rows scanned | 1 000 000            | `QueryScanLimitExceeded` |

Operators may raise budgets per-subject via L4 policy (e.g. for analytics subjects).

### 8.3. Result format

```proto
message QueryResult {
  string query_id = 1;                    // "qry_<ULID>"
  ProjectionType projection = 2;
  repeated google.protobuf.Struct rows = 3;       // one row per result, schema per projection
  string next_cursor = 4;                 // empty when no more results
  uint64 row_count = 5;                   // count returned in this page
  uint64 total_estimated = 6;             // best-effort total, optional
  uint64 privacy_filtered_count = 7;      // §5.3
  google.protobuf.Timestamp evaluated_at = 8;
  string as_of_version_id = 9;            // for time-travel queries
  google.protobuf.Timestamp as_of_timestamp = 10;
}
```

Result rows are deterministically ordered when `order by` is present; otherwise ordered by source identity (e.g. `object_id` for `from objects`).

## 9. NL→query bridge

Natural-language goals reach the query engine only via S1.1 (Capability Translator). The contract:

| Step                                                                                      | Owner                                  |
| ----------------------------------------------------------------------------------------- | -------------------------------------- |
| Detect that an utterance is a query request                                               | Intent Engine (L5)                     |
| Translate utterance to action `aiosfs.query.execute` with target containing canonical DSL | S1.1 Capability Translator             |
| Execute the DSL                                                                           | This spec (`AIOSFSQuery.ExecuteQuery`) |
| Surface results to renderer                                                               | Renderer (L7)                          |

Direct DSL submission is allowed for advanced callers and tests. **The query engine never accepts natural language directly** — only structured DSL or proto query messages.

### 9.1. Evidence linkage

Every query execution records:

- `query_id`
- caller subject
- query DSL text (canonicalized)
- DSL hash (`hex_lower(BLAKE3(canonicalized_dsl))[:32]`, S0.1 §8.5 truncation rule)
- privacy ceiling applied
- `as_of` snapshot if any
- `intent_id` and `translation_id` if originated from S1.1
- result row count and `privacy_filtered_count`

Evidence is required even for read-only queries that touch `SECRET_BEARING` or `CLASSIFIED` sources.

## 10. View definition

```yaml
view_id: view_latest_stable_sdf_renderer
name: latest stable sdf renderer
description: Resolves to the most recently updated stable SDF renderer project.
materialized: false
refresh_strategy: ON_DEMAND
staleness_threshold_seconds: 300
query: |
  from objects
  where kind = "PROJECT"
    and labels contains "sdf"
    and labels contains "renderer"
    and pointer("STABLE") exists
  order by updated_at desc
  limit 1
  project posix_path("/aios/views/projects/{name}")
```

```proto
message ViewDefinition {
  string view_id = 1;
  string name = 2;
  string description = 3;
  bool materialized = 4;
  RefreshStrategy refresh_strategy = 5;
  uint32 staleness_threshold_seconds = 6;
  string query_dsl_version = 7;          // "aios.fs.query.v1alpha1"
  string query_text = 8;                 // canonicalized DSL
  string created_by_subject = 9;
  google.protobuf.Timestamp created_at = 10;
}

enum RefreshStrategy {
  REFRESH_STRATEGY_UNSPECIFIED = 0;
  ON_DEMAND  = 1;
  ON_WRITE   = 2;
  SCHEDULED  = 3;
  MANUAL     = 4;
}
```

Views are themselves `Object`s with kind `VIEW` (an additive enum bump on `ObjectKind` from S1.3). Deleting a view object removes the projection but never removes the underlying objects.

## 11. gRPC service surface

```proto
service AIOSFSQuery {
  rpc ExecuteQuery(ExecuteQueryRequest) returns (ExecuteQueryResponse);
  rpc ExplainQuery(ExplainQueryRequest) returns (ExplainQueryResponse);
  rpc CreateView(CreateViewRequest) returns (ViewDefinition);
  rpc RebuildView(RebuildViewRequest) returns (RebuildViewResponse);
  rpc ListViews(ListViewsRequest) returns (stream ViewDefinition);
  rpc DeleteView(DeleteViewRequest) returns (google.protobuf.Empty);
  rpc GetQueryEngineInfo(google.protobuf.Empty) returns (QueryEngineInfo);
}
```

Full message types in **Appendix A**.

## 12. Forbidden behaviors (recap)

The query engine MUST NOT:

- Execute arbitrary code or expressions outside the closed grammar (§3).
- Return objects above the caller's privacy ceiling (§5).
- Mutate any state.
- Accept natural-language utterances directly (NL goes through S1.1).
- Reveal raw secret material.
- Skip evidence linkage for queries touching `SECRET_BEARING`/`CLASSIFIED` sources.
- Cache `as of` results in materialized views (§7.3).

## 13. Cross-spec dependencies

| Spec                           | Relationship                                                                                |
| ------------------------------ | ------------------------------------------------------------------------------------------- |
| **S0.1** Action Envelope       | Hash encoding rules apply to query DSL hashes.                                              |
| **S1.1** Capability Translator | NL→query is a translator concern; queries reach this engine pre-translated.                 |
| **S1.2** Latency Tiering       | T0 cache hit may short-circuit query execution; query budget interacts with router timeout. |
| **S1.3** Object Model          | Sources, fields, and consistency modes inherited from object model.                         |
| **S2.2** Implementation Space  | Index store choices (SQLite, Tantivy) determine query performance shape.                    |
| **S2.3** Policy Kernel         | Privacy ceiling derives from L4 policy.                                                     |
| **S3.1** Evidence Log          | Evidence projection sources (`from evidence`) are read-only views over the log.             |

## 14. Open deferrals

- Embedding-similarity queries (`order by embedding_distance(...)`) — deferred to L5 vector sub-spec.
- Cross-instance federated queries — deferred to future L2 distributed sub-spec.
- User-defined views with parameters — deferred (parametric views must wait until parameter binding security is designed).
- Streaming query results for live updates — deferred (stable point-in-time results are sufficient for rev.2).

## 15. Acceptance criteria

- Queries parse against the EBNF in §3 or are rejected with `InvalidQuery`.
- Object identity survives view changes.
- Read-only views never invoke writes.
- Privacy ceiling filtering is enforced for every query.
- Time-travel queries return the historical snapshot subject to log retention.
- Evidence linkage is emitted for every query touching `SECRET_BEARING` or `CLASSIFIED` data.
- Views are rebuildable from object metadata; deleting a view never deletes objects.
- `ExplainQuery` returns a deterministic plan.
- All golden fixtures from §16 pass against the implementation.
- Telemetry metrics from §17 are emitted with bounded label cardinality.

## 16. Golden fixtures

### 16.1. Simple object filter

```yaml
fixture_id: aiosfs.qry.fix.simple_filter.v1
input_dsl: |
  from objects
  where kind = "PROJECT"
    and labels contains "renderer"
  order by created_at desc
  limit 5
expected:
  status: OK
  projection: object_list
  result_row_count_at_most: 5
  privacy_filtered: result excludes objects above caller ceiling
```

### 16.2. Time-travel snapshot

```yaml
fixture_id: aiosfs.qry.fix.time_travel.v1
input_dsl: |
  from objects
  where kind = "POLICY"
  as of "2026-04-01T00:00:00Z"
expected:
  status: OK
  evaluated_against_snapshot: 2026-04-01
  consistency: SNAPSHOT
```

### 16.3. Privacy filter — silent exclusion

```yaml
fixture_id: aiosfs.qry.fix.privacy_silent.v1
input_dsl: |
  from objects
  where kind = "MEMORY"
input_caller_ceiling: INTERNAL
catalog_state:
  - obj_a privacy_class=PUBLIC
  - obj_b privacy_class=SENSITIVE
  - obj_c privacy_class=CLASSIFIED
expected:
  status: OK
  rows_returned: 1   # obj_a only
  privacy_filtered_count: 2
  no object_ids leaked for filtered objects
```

### 16.4. Forbidden field

```yaml
fixture_id: aiosfs.qry.fix.forbidden_field.v1
input_dsl: |
  from objects
  where secret_value = "some_token"
expected:
  status: INVALID_QUERY
  error_field: "secret_value"
  message: field not in queryable schema
```

### 16.5. Pagination cursor

```yaml
fixture_id: aiosfs.qry.fix.cursor_pagination.v1
input_dsl: |
  from objects
  where kind = "FILE"
  order by object_id asc
input_pagination_mode: CURSOR
input_page_size: 100
scenario:
  - first call returns 100 rows + next_cursor
  - while cursor active, new objects inserted
  - second call with cursor returns next 100 rows excluding new ones
expected_property: cursor pagination is stable under concurrent writes
```

### 16.6. Materialized view refresh

```yaml
fixture_id: aiosfs.qry.fix.materialized_refresh.v1
view:
  {
    materialized: true,
    refresh_strategy: ON_WRITE,
    staleness_threshold_seconds: 300,
  }
scenario:
  - CreateView -> view built
  - Write to a queried source
  - Read view
expected: view marked stale on write event
  next read triggers refresh
  refresh evidence emitted
```

### 16.7. Aggregation

```yaml
fixture_id: aiosfs.qry.fix.aggregation.v1
input_dsl: |
  from objects
  group by kind
  select count(*) as count, max(created_at) as latest
  order by count desc
expected:
  status: OK
  rows_per_kind: 1
  fields_present: [kind, count, latest]
```

### 16.8. Disjunction via `in`

```yaml
fixture_id: aiosfs.qry.fix.in_clause.v1
input_dsl: |
  from objects
  where kind in ["PROJECT", "WORKSPACE"]
expected:
  status: OK
  rows: subset where kind matches either
```

### 16.9. Query budget exhaustion

```yaml
fixture_id: aiosfs.qry.fix.budget_exhausted.v1
input_dsl: |
  from versions
  where state = "STAGED"
  order by created_at asc
catalog_state: 5 million versions matching
expected:
  status: QueryScanLimitExceeded
  partial_result: not returned (fail closed)
```

## 17. Telemetry contract

| Metric                                 | Type      | Labels                            |
| -------------------------------------- | --------- | --------------------------------- |
| `aiosfs_query_total`                   | counter   | `source`, `outcome`               |
| `aiosfs_query_latency_seconds`         | histogram | `source`, `materialized`, `as_of` |
| `aiosfs_query_rows_returned`           | histogram | `source`                          |
| `aiosfs_query_privacy_filtered_total`  | counter   | `source`                          |
| `aiosfs_query_budget_exceeded_total`   | counter   | `budget_kind`                     |
| `aiosfs_view_count`                    | gauge     | `materialized`                    |
| `aiosfs_view_refresh_total`            | counter   | `strategy`, `outcome`             |
| `aiosfs_view_refresh_duration_seconds` | histogram | `strategy`                        |

Cardinality bounds: `source` = 6, `outcome` ≤ 5, `materialized` = 2, `strategy` = 4, `budget_kind` ≤ 4. Subject is **never** a metric label.

## 18. Namespace integration (S4.1 cross-spec touch-up)

Applied 2026-05-09. Source: [S4.1 §12.3](05_namespace_layout.md).

### 18.1 New closed query fields

The query field vocabulary gains four namespace fields:

| Field                  | Type                                | Notes                                                  |
| ---------------------- | ----------------------------------- | ------------------------------------------------------ |
| `target.scope`         | `aios.namespace.v1alpha1.ScopeKind` | enum: `SYSTEM` / `GROUP` / `USER`                      |
| `target.group_id`      | string                              | regex per S4.1 §7.1                                    |
| `target.user_id`       | string                              | regex per S4.1 §7.1                                    |
| `target.reserved_name` | string                              | matches one of the closed enums per scope (S4.1 §3–§6) |

Available operators: `=`, `!=`, `in`, `exists` for `target.reserved_name` and `target.scope`; `=`, `!=` for the id fields. Queries containing other operators on these fields fail validation with `UnsupportedOperator`.

### 18.2 Formal inbox / outbox views

Two named views are added to the standard view catalog:

```sql
-- Group inbox (one per group)
VIEW inbox(scope=GROUP, id=<group_id>) AS
  SELECT * FROM action_envelopes
  WHERE target.group_id = <group_id>
    AND lifecycle.phase = PENDING
    AND <visible to caller under privacy ceiling>
  ORDER BY created_at DESC;

-- Personal inbox (one per (group, user) pair)
VIEW inbox(scope=USER, id=<user_id>) AS
  SELECT * FROM action_envelopes
  WHERE target.user_id = <user_id>
    OR <user_id> IN addressed_user_ids
    AND lifecycle.phase = PENDING
  ORDER BY created_at DESC;

-- Personal outbox
VIEW outbox(user_id=<user_id>) AS
  SELECT * FROM action_envelopes
  WHERE submitter.user_id = <user_id>
  ORDER BY created_at DESC;
```

All three are `materialization = VIRTUAL`, `refresh = ON_DEMAND`, with cardinality bound 10 000 items per query and mandatory cursor pagination beyond that. Mutation through these views is rejected with `VirtualPathNotWritable`.

### 18.3 Cross-group query filtering

Queries that would yield rows from groups other than the caller's `primary_group_id` (without the `_system` audit exception) silently exclude those rows and return `PARTIAL` with a `suppressed_count` field on the `QueryResponse`. This mirrors the privacy ceiling discipline (§9). A query that would yield zero rows after filtering still returns `OK`/`PARTIAL` correctly; it does not leak the existence of foreign-group data.

### 18.4 Telemetry additions

Two metrics added with bounded label cardinality:

| Metric                            | Type    | Labels (closed)                                        |
| --------------------------------- | ------- | ------------------------------------------------------ |
| `aiosfs_namespace_query_total`    | counter | `target_scope` (system/group/user), `outcome`          |
| `aiosfs_cross_group_filter_total` | counter | none — total rows silently excluded across all queries |

## 19. See also

- [S1.3 Object Model](01_object_model.md)
- [S1.3 Conflict Resolution](03_conflict_resolution.md)
- [S2.2 Implementation Space](04_implementation_space.md)
- [S1.1 Capability Translator](../L5_Cognitive_Core/02_capability_translator.md)
- [S1.2 Latency Tiering](../L5_Cognitive_Core/03_latency_tiering.md)
- [S4.1 Namespace Layout](05_namespace_layout.md)
- [Rev.2 Master Index](../00_MASTER_INDEX.md)

## Appendix A: Complete proto IDL

```proto
syntax = "proto3";
package aios.fs.query.v1alpha1;

import "google/protobuf/timestamp.proto";
import "google/protobuf/struct.proto";
import "google/protobuf/empty.proto";

// ─────────────────────────────────────────────────────────────────
// Enums
// ─────────────────────────────────────────────────────────────────

enum ProjectionType {
  PROJECTION_TYPE_UNSPECIFIED = 0;
  OBJECT_LIST   = 1;
  GRAPH         = 2;
  TIMELINE      = 3;
  MOUNT_OVERLAY = 4;
  POSIX_PATH    = 5;
}

enum RefreshStrategy {
  REFRESH_STRATEGY_UNSPECIFIED = 0;
  ON_DEMAND  = 1;
  ON_WRITE   = 2;
  SCHEDULED  = 3;
  MANUAL     = 4;
}

enum PaginationMode {
  PAGINATION_MODE_UNSPECIFIED = 0;
  OFFSET_LIMIT = 1;
  CURSOR       = 2;
}

// ─────────────────────────────────────────────────────────────────
// Query records
// ─────────────────────────────────────────────────────────────────

message ExecuteQueryRequest {
  string schema_version = 1;       // "aios.fs.query.v1alpha1"
  string query_id = 2;             // optional; engine assigns "qry_<ULID>" if empty
  string query_text = 3;           // DSL per §3
  PaginationMode pagination = 4;
  uint32 page_size = 5;
  string cursor = 6;               // opaque; for PaginationMode=CURSOR
  uint32 offset = 7;               // for PaginationMode=OFFSET_LIMIT
  uint32 wall_clock_timeout_ms = 8;  // optional; bounded by operator policy
  string subject = 9;              // L4 subject; ceiling derived from L4 policy
}

message ExecuteQueryResponse {
  string query_id = 1;
  ProjectionType projection = 2;
  repeated google.protobuf.Struct rows = 3;
  string next_cursor = 4;
  uint64 row_count = 5;
  uint64 total_estimated = 6;
  uint64 privacy_filtered_count = 7;
  google.protobuf.Timestamp evaluated_at = 8;
  string as_of_version_id = 9;
  google.protobuf.Timestamp as_of_timestamp = 10;
  string evidence_receipt_id = 11;
}

message ExplainQueryRequest {
  string query_text = 1;
  string subject = 2;
}

message ExplainQueryResponse {
  string normalized_dsl = 1;
  repeated string plan_steps = 2;        // human-readable engine plan
  uint64 estimated_rows = 3;
  bool requires_full_scan = 4;
  repeated string indexes_used = 5;
  string warning = 6;
}

// ─────────────────────────────────────────────────────────────────
// View management
// ─────────────────────────────────────────────────────────────────

message ViewDefinition {
  string view_id = 1;
  string name = 2;
  string description = 3;
  bool materialized = 4;
  RefreshStrategy refresh_strategy = 5;
  uint32 staleness_threshold_seconds = 6;
  string query_dsl_version = 7;
  string query_text = 8;
  string created_by_subject = 9;
  google.protobuf.Timestamp created_at = 10;
  google.protobuf.Timestamp last_built_at = 11;
  google.protobuf.Timestamp invalidated_at = 12;
}

message CreateViewRequest {
  ViewDefinition view = 1;
}

message RebuildViewRequest {
  string view_id = 1;
}

message RebuildViewResponse {
  string view_id = 1;
  google.protobuf.Timestamp rebuilt_at = 2;
  uint64 row_count = 3;
  string evidence_receipt_id = 4;
}

message ListViewsRequest {
  bool materialized_only = 1;
}

message DeleteViewRequest {
  string view_id = 1;
}

// ─────────────────────────────────────────────────────────────────
// Engine info
// ─────────────────────────────────────────────────────────────────

message QueryEngineInfo {
  string engine_id = 1;
  repeated string supported_schema_versions = 2;
  string default_schema_version = 3;
  uint64 transaction_log_retention_days = 4;
  google.protobuf.Timestamp started_at = 5;
}

// ─────────────────────────────────────────────────────────────────
// Service
// ─────────────────────────────────────────────────────────────────

service AIOSFSQuery {
  rpc ExecuteQuery(ExecuteQueryRequest) returns (ExecuteQueryResponse);
  rpc ExplainQuery(ExplainQueryRequest) returns (ExplainQueryResponse);
  rpc CreateView(CreateViewRequest) returns (ViewDefinition);
  rpc RebuildView(RebuildViewRequest) returns (RebuildViewResponse);
  rpc ListViews(ListViewsRequest) returns (stream ViewDefinition);
  rpc DeleteView(DeleteViewRequest) returns (google.protobuf.Empty);
  rpc GetQueryEngineInfo(google.protobuf.Empty) returns (QueryEngineInfo);
}
```
