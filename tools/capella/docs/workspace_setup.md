# Capella IDE — AIOS workspace bootstrap

Step-by-step guide for opening Eclipse Capella 7.0.1 and creating the AIOS Rev.2 project. Run through this once; thereafter the workspace at `~/.local/share/capella-workspaces/aios-rev2/` is your live modeling environment.

## Prerequisites (verified)

- ✓ Capella 7.0.1 installed at `~/.local/opt/capella-7.0.1/`
- ✓ Symlink `~/.local/bin/capella` → Capella launcher
- ✓ Bundled JRE at `~/.local/opt/capella-7.0.1/jre`
- ✓ System Java 25 also available
- ✓ Workspace directory pre-created: `~/.local/share/capella-workspaces/` (will hold `aios-rev2/` once Capella creates it on first launch)
- ✓ Spec extraction CSVs ready: `tools/capella/manifests/*.csv` (run `python3 tools/capella/extract.py` to refresh)

## Step 1 — Launch Capella IDE

```bash
capella -data ~/.local/share/capella-workspaces/aios-rev2
```

The `-data` flag points Capella at a specific workspace directory. First launch creates the workspace skeleton (the `.metadata` subfolder) automatically. Capella may prompt for an empty `Welcome` close; do so to reach the main workbench.

## Step 2 — Create a new Capella Project

In Capella's menu:

`File > New > Capella Project`

- **Project name:** `aios-rev2`
- **Method:** ARCADIA (default)
- **Use default location:** ✓ (creates `<workspace>/aios-rev2/`)
- **Initial layers to populate:** check ALL of:
  - Operational Analysis
  - System Analysis
  - Logical Architecture
  - Physical Architecture
  - EPBS (End-Product Breakdown Structure) — optional, but useful for the 53-sub-spec layout

Click **Finish**. Capella generates the project skeleton.

Inside the new project, you should see:

```
aios-rev2/
├── aios-rev2.aird      ← Sirius diagram model (UI projection)
├── aios-rev2.melodymodeller  ← actual model semantic content
├── aios-rev2.capella   ← legacy Capella container (may or may not be present in 7.x)
└── .project            ← Eclipse project metadata
```

## Step 3 — Import the 24 INVs as Operational Capabilities

1. In the Project Explorer, navigate to `aios-rev2 > Operational Analysis > Operational Activities` (or `Operational Capabilities` folder)
2. Right-click → `Import > Capella > CSV file`
3. Select `tools/capella/manifests/invariants.csv`
4. Map CSV columns:
   - `id` → **Capability name**
   - `title` → **Description (first line)**
   - `statement` → **Description (body)**
   - `why` → **Rationale**
5. Confirm and import. You should see 24 Operational Capabilities created.

If Capella 7.0.1 does not have a built-in CSV import wizard for Operational Capabilities, use the **Requirements addon**:

`Help > Install New Software` → install Capella Requirements (vp-requirements)

After install, the Requirements view allows CSV import to a Requirements Module; you can then convert Requirements → Operational Capabilities via the model copier.

## Step 4 — Import the 53 sub-specs as System Functions

1. Navigate to `aios-rev2 > System Analysis > System Functions`
2. Create 12 sub-folders (one per layer): L0, L1, ..., L10, XX
3. For each row in `tools/capella/manifests/sub_specs.csv`:
   - Create a System Function under the matching layer folder
   - Set name = phase_tag + " " + title (e.g., "S2.3 Policy Kernel")
   - Set description = path (links to markdown source)
   - Set status = the spec's Status field (mostly CONTRACT)

CSV mass import for System Functions is awkward in stock Capella; use the **Python4Capella** addon if you have it (provides programmatic creation via EASE scripts). Otherwise, manually create the 53 functions following the spec order.

## Step 5 — Build the layer dependency graph

1. Open `aios-rev2 > Logical Architecture > Logical Components`
2. Create 12 Logical Components matching the layers (L0..L10 + XX)
3. For each row in `tools/capella/manifests/trace_consumes.csv`:
   - Create a Logical Interface from consumer (sub-spec component) to producer
   - Label the interface with the type imported (e.g., "Imports ActionEnvelope from S0.1")
4. Open the Logical Architecture Diagram view → arrange components by layer
5. **Visual INV-007 check:** look for any arrow pointing upward (lower layer to higher layer). If found, that's an INV-007 violation worth investigating.

## Step 6 — Populate the INV × sub-spec traceability matrix

Capella's Traceability Matrix view (`Window > Show View > Matrix`) lets you select rows and columns:

- Rows: filter to Operational Capabilities (the 24 INVs)
- Columns: filter to System Functions (the 53 sub-specs)
- Cell content: traceability link

Populate cells from `tools/capella/manifests/trace_inv_to_subspec.csv` — 331 traceability links. This is the **single most important live view** for gap detection.

If manual cell-by-cell entry is too slow, write a Python4Capella script that reads the CSV and creates the trace links programmatically. (Eventual `tools/capella/build.py` automation.)

## Step 7 — Author the 4 Operational Scenarios

Per `modeling_plan.md` §"Operational Scenarios":

1. **Golden path** — `XX_Cross_Cutting/03_mvp_golden_path.md`
2. **AI install denied** — S2.3 §26.2.4
3. **First-boot provisioning** — S9.2
4. **Tamper → recovery** — S3.1 §11.4 + S9.1

Each scenario uses the Sequence Diagram view in Operational Analysis:

- Drop the relevant Operational Entities (HUMAN_USER, AI_AGENT, SERVICE)
- Drop the Operational Activities in execution order
- Connect with interaction messages
- Annotate with the relevant INVs being enforced or tested

## Step 8 — Run validation

Capella's built-in validation framework:

`Project > Validate Model` (or right-click project → Validate)

Inspect the Problems view. Common findings to look for at this stage:

- Operational Capabilities without realising System Functions
- System Functions without allocations to Logical Components
- Logical Interfaces without realising System Functional Exchanges
- Dangling traceability links

Export the validation report → `tools/capella/output/validation_report.csv` (manual save, since Capella's export to CSV is also manual).

## Step 9 — Document gaps

For each finding from Step 8:

1. Open the corresponding markdown spec under `002.AI-OS.NET--SPECREV.2/`
2. Either fix the gap in the markdown source (preferred) or
3. Document it as a known carry-forward in the relevant DEC entry

After fixing markdown: re-run `python3 tools/capella/extract.py` → re-import refreshed CSVs into Capella → verify the gap is gone.

## Step 10 — Save the workspace state

`File > Save All` (Ctrl+Shift+S)

Capella saves to:

- `aios-rev2.aird` — diagram positions, view configurations
- `aios-rev2.melodymodeller` — semantic model content

**Important:** these files are NOT committed to the AIOS repo — they live only in the workspace directory. The markdown source remains the only versioned artifact. To recreate the workspace on another machine, re-run this guide from Step 1.

If you want to version the model, an optional follow-up task is to export the workspace as a Capella library (`Project > Export > Capella > Library`) and commit that to `tools/capella/output/aios-rev2.capellalibrary` — but this is a snapshot, not a working file, and re-import is one-way.

## Estimated time

- Step 1-2 (workspace + project): 5 minutes
- Step 3 (INV import): 15-30 minutes (depends on CSV wizard availability)
- Step 4 (sub-spec import): 1-2 hours manual; 15 minutes via Python4Capella
- Step 5 (layer graph): 30-45 minutes
- Step 6 (traceability matrix): 1-2 hours manual; 15 minutes scripted
- Step 7 (scenarios): 2-3 hours (one per scenario)
- Step 8 (validation): 30 minutes
- Step 9 (gap fix-up): variable, depends on findings
- **Total first-pass:** 6-10 hours interactive work in the IDE

## After first-pass complete

The workspace becomes a **living model**: each spec change triggers `extract.py` re-run, CSV refresh import, validation re-scan. The marginal effort per spec change is minimal (15-30 minutes) and the gap-detection value compounds.

## When to use Python4Capella for automation

The DSD-DBS `py-capellambse` library (pure-Python, no IDE required) is the long-term automation path. It can:

- Parse `aios-rev2.aird` headlessly
- Apply CSV imports as scripted batch operations
- Generate diagrams as SVG/PNG without GUI
- Integrate into CI (block PRs that introduce orphan INVs, etc.)

This is a separate Phase 2 work (per `tools/capella/README.md` planning), to be tackled once the manual workspace bootstrap proves the value.
