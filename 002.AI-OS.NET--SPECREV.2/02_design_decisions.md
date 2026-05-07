# Design Decisions — Rev.2

Decision log. Each entry follows ADR (Architecture Decision Record) discipline: context, decision, consequences, status. Entries are append-only; superseded decisions are marked as such, never deleted.

---

## DEC-001 — README "Self-Evolving Backend" reframed as "Adaptive Backend"

- **Context:** Rev.1 README contained a "Self-Evolving Backend" section that implied autonomous AI patching of backend code, in conflict with the spec discipline of typed actions, policy gating, and evidence-first execution. Rev.1 SPECIFICATION.md never sanctioned self-evolution.
- **Decision:** Reframe as "Adaptive Backend": AI may _propose_ backend patches, kernel adaptation adjustments, distro compatibility profiles, and new runtime adapters; production promotion always requires human approval. The Cognitive Core may not modify the Policy Kernel, the Evidence Log, the Vault Broker, or the recovery boot path through this pipeline.
- **Consequences:** README and SPECIFICATION are now consistent on the AI's bounded execution model. The pipeline (Observe → Propose → Sandbox simulation → Tests → Human review → Staged deployment → Monitor → Rollback) is fully explicit.
- **Status:** `REAL` (applied in initial commit `be318da`)
- **Phase tag:** S0.2

---

## DEC-002 — Repository layout: revision-per-folder, layer-per-subfolder

- **Context:** Rev.1 was a single `SPECIFICATION.md` file. As rev.2 grows, a single file becomes unwieldy. Comparable projects (NeuroCAD `000.000.Roadmap/NEUROCAD_UNIFIED_PLATFORM_SPEC_REV*`) use one folder per revision with topic-named files inside.
- **Decision:** For AIOS:
  - One folder per revision: `001.AI-OS.NET--SPECREV.1/`, `002.AI-OS.NET--SPECREV.2/`, …
  - Inside each rev.2+ folder, one folder per layer (L0–L10) plus a `XX_Cross_Cutting/` folder for contracts shared by multiple layers.
  - Inside each layer folder, files numbered `00_overview.md`, `01_<topic>.md`, `02_<topic>.md`, …
  - Rev.1 stays as a flat verbatim move (the original two files) since it pre-dates this convention.
- **Consequences:** Clean navigation, easy to grow (e.g., L4 will hold Policy Kernel, Vault Broker, and Identity Model as three separate sub-specs). Slightly more nesting than NeuroCAD's flat-files-per-revision approach, but justified by AIOS layers containing multiple distinct sub-systems each.
- **Status:** `REAL` (applied in this commit)
- **Phase tag:** infrastructure

---

## DEC-003 — TBD

(Future decisions land here.)
