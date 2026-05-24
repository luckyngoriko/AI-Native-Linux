# `tools/capella/` — AIOS Rev.2 → Eclipse Capella simulation

This directory holds the **bridge between the markdown specification source-of-truth and Eclipse Capella MBSE tooling**. The user maintains AIOS specifications in markdown under `002.AI-OS.NET--SPECREV.2/`; Capella offers cross-cutting traceability views, ARCADIA-method modeling, scenario simulation, and live gap detection that complement the spec-side audits (Tier 5 / Tier 6).

The Capella model is a **derivable view**, not a parallel source. Specs evolve → re-run `extract.py` → import refreshed CSVs into Capella → catch new gaps.

## Directory layout

```
tools/capella/
├── README.md                  ← this file
├── extract.py                 ← markdown → CSV manifest extractor
├── manifests/                 ← machine-readable extracts
│   ├── invariants.csv         ← 24 INVs (constitutional capabilities)
│   ├── sub_specs.csv          ← 53 sub-specs (system functions)
│   ├── layers.csv             ← 11 layers + XX cross-cutting
│   ├── record_types.csv       ← 427 RecordTypes (closed evidence vocabulary)
│   ├── trace_inv_to_subspec.csv ← 331 INV citations across sub-specs
│   └── trace_consumes.csv     ← 238 cross-spec dependency edges
└── docs/
    ├── modeling_plan.md       ← AIOS → ARCADIA mapping (operational → physical)
    └── workspace_setup.md     ← step-by-step Capella IDE bootstrap guide
```

## Quick start

```bash
# 1. Re-extract from current spec state (one-shot, idempotent)
python3 tools/capella/extract.py

# 2. Open Capella IDE (already installed at ~/.local/opt/capella-7.0.1/)
capella  # symlink at ~/.local/bin/capella

# 3. Follow docs/workspace_setup.md to bootstrap the AIOS Capella project
```

## Why we do this

The AIOS specification has matured to **53 contract-grade sub-specs across 11 layers**, with:

- 24 constitutional invariants (INV-001..INV-024)
- 427 closed-vocabulary `RecordType` enum values
- ~150 cross-spec interface dependencies
- ~330 INV → enforcement-site traceability citations

Markdown is the right source-of-truth (human-editable, git-diffable, machine-grep-able). But **cross-cutting integrity** at this scale needs structured views:

| Gap type                                                    | Detected by (markdown)                           | Detected by (Capella)                   |
| ----------------------------------------------------------- | ------------------------------------------------ | --------------------------------------- |
| Orphan INV (declared, never enforced)                       | Tier 6 CROSS-PROMISE audit (manual one-shot)     | Traceability matrix → empty rows (live) |
| Orphan enforcement site (uses term not in INV catalog)      | grep-based audit                                 | reverse traceability matrix             |
| Dangling Consumes (imports X.foo but X doesn't produce foo) | Tier 6 audit                                     | Logical Interface mismatch matrix       |
| Orphan RecordType (defined, never emitted)                  | Manual audit                                     | RecordType × Emitter matrix             |
| Unreachable FSM state (declared, no incoming transition)    | Manual review                                    | State machine view                      |
| Missing scenario branch                                     | Per-spec golden fixtures                         | Capability flow diagram coverage        |
| Layer dependency violation (INV-007)                        | S2.4 `LAYER_DOWNWARD_DEPENDENCY_HOLDS` (Wave 14) | Architecture view (visual flag)         |

Capella turns these from **one-shot audit findings** into a **live model** that flags new gaps as the spec evolves.

## Discipline

Per the `feedback_no_technical_debt.md` rule (no items left as debt across milestones):

- The Capella model and the markdown spec MUST be kept in sync — drift is debt
- `extract.py` is the only authoritative bridge — the model imports CSVs, never the other way around
- New entities go into markdown first; Capella refreshes via extract
- Capella validation findings → audit-style report → fixed in markdown → re-extract

## Tooling versions

| Tool                        | Version    | Location                                       |
| --------------------------- | ---------- | ---------------------------------------------- |
| Eclipse Capella             | 7.0.1      | `~/.local/opt/capella-7.0.1/`                  |
| Java (bundled with Capella) | OpenJDK    | `~/.local/opt/capella-7.0.1/jre`               |
| System Java                 | OpenJDK 25 | `/usr/bin/java`                                |
| Python (for extract.py)     | 3.11+      | system                                         |
| Workspace dir               | —          | `~/.local/share/capella-workspaces/aios-rev2/` |

## See also

- `docs/modeling_plan.md` — full AIOS → ARCADIA mapping
- `docs/workspace_setup.md` — Capella IDE step-by-step bootstrap
- `002.AI-OS.NET--SPECREV.2/00_MASTER_INDEX.md` — source-of-truth navigation
- `002.AI-OS.NET--SPECREV.2/L0_Governance_Evidence_Safety/04_invariants.md` — INV catalog (the primary input to Capella's Operational Capability layer)
