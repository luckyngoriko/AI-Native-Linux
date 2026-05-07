# AIOS-FS Query and View Language (Rev.2)

| Field     | Value                                 |
| --------- | ------------------------------------- |
| Status    | `CONTRACT` draft                      |
| Phase tag | S2.1                                  |
| Layer     | L2 AIOS-FS                            |
| Consumes  | AIOS-FS object metadata and indexes   |
| Produces  | semantic views and projections        |

## 1. Purpose

AIOS-FS views let users and agents ask for objects semantically without treating paths as identity.

Example:

```text
open latest stable sdf renderer
```

This becomes a query over object metadata, version state, labels, provenance, and recency.

## 2. Query model

The query language is a constrained declarative DSL. It is not arbitrary code.

```text
from objects
where kind = "project"
  and labels contains "sdf"
  and labels contains "renderer"
  and pointer("stable").exists
order by updated_at desc
limit 1
project path("/aios/views/projects/{name}")
```

## 3. View definition

```yaml
view_id: view_latest_stable_sdf_renderer
name: latest stable sdf renderer
query:
  from: objects
  where:
    kind: project
    labels_all: [sdf, renderer]
    pointer: stable
  order_by:
    - updated_at: desc
  limit: 1
projection:
  type: posix_path
  root: /aios/views/projects
```

Views are rebuildable. Deleting a view index must not delete objects.

## 4. Query sources

Allowed query fields:

- object kind
- labels
- owner subject
- policy tags
- version state
- pointer names
- provenance action id
- timestamps
- content hash
- declared semantic facts
- embedding references

Forbidden query authority:

- raw secrets
- unverified model summaries
- hidden prompt content
- unaudited external metadata

## 5. Natural language to query

The Cognitive Core may translate natural language into query DSL, but the DSL must be shown or explainable for state-changing operations. Read-only views may use best-effort translation with evidence.

Ambiguous query terms return multiple candidates or clarification.

## 6. Projection types

| Projection       | Meaning                                |
| ---------------- | -------------------------------------- |
| `posix_path`     | path-like projection for applications  |
| `object_list`    | structured object list                 |
| `graph`          | nodes and edges for renderer/agents    |
| `timeline`       | ordered versions/events                |
| `mount_overlay`  | compatibility projection for apps      |

Projection does not create storage identity.

## 7. Acceptance criteria

- Views can be rebuilt from object metadata.
- Object identity survives path/view changes.
- Query language has no arbitrary execution.
- Natural language view resolution is evidence-linked.
- Read-write projections still create object versions.

