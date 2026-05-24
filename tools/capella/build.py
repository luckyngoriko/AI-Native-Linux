#!/usr/bin/env python3
"""
AIOS Rev.2 → Eclipse Capella model builder.

Builds a populated Capella project from the CSV manifests produced by
extract.py, using the empty Capella template at tools/capella/template/
as the substrate.

Pattern reference: NeuroCAD archstack/capella_model_builder.py
(autonomous_factory/SPEC_ANALIZER_REV1/STACK_REV2/) — proven production
tool that builds the NeuroCAD REV11.4.3 Capella model from CSVs.

AIOS layer → ARCADIA mapping:
    L0  Governance, Evidence, Safety        → Operational Analysis
    L1  Kernel, Bootstrap, Recovery         → Physical Architecture
    L2  AIOS-FS                              → Logical Architecture
    L3  AIOS-SGR Service Graph Runtime      → Logical Architecture
    L4  Policy, Identity, Vault             → Logical Architecture
    L5  Cognitive Core                      → Logical Architecture
    L6  Apps, Packages, Compatibility       → Physical Architecture
    L7  Interaction Renderers               → Physical Architecture
    L8  Network, Hardware, Devices          → Physical Architecture
    L9  Observability, Admin, Operations    → Logical Architecture
    L10 Distribution, Ecosystem, Marketplace → Physical Architecture
    XX  Cross-Cutting                       → System Analysis

This is the inverse of NeuroCAD's mapping (their L0 = oa, L1/L2 = la,
L3/L4 = sa, L5 = la, L6/L7/L8 = pa, L9+ = epbs) because AIOS's layer
semantics differ: AIOS L0 is governance/constitutional truth (operational
in ARCADIA sense), AIOS L4 is constitutional services (logical), AIOS XX
is the system contract surface (system analysis).

Run from repo root:
    tools/capella/.venv/bin/python tools/capella/build.py

Output: tools/capella/output/aios-rev2/ (Capella project, openable in IDE)
"""

from __future__ import annotations

import csv
import json
import shutil
import sys
import uuid
from collections import Counter
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent.parent
TEMPLATE_DIR = Path(__file__).resolve().parent / "template"
MANIFESTS_DIR = Path(__file__).resolve().parent / "manifests"
OUTPUT_DIR = Path(__file__).resolve().parent / "output" / "aios-rev2"
REPORT_PATH = Path(__file__).resolve().parent / "output" / "build_report.json"

MODEL_NAME = "aios-rev2"
DISPLAY_NAME = "AIOS Rev.2 — AI-Native Linux Distribution"
UUID_NAMESPACE = uuid.UUID("aaa05000-1c4f-4f00-aa05-aaa000000000")

# AIOS layer → ARCADIA layer mapping. See module docstring for rationale.
LAYER_TARGETS = {
    "L0": "oa",  # Constitutional governance — operational capability surface
    "L1": "pa",  # Kernel + recovery substrate
    "L2": "la",  # AIOS-FS
    "L3": "la",  # Service Graph Runtime
    "L4": "la",  # Policy/Identity/Vault — constitutional services
    "L5": "la",  # Cognitive Core
    "L6": "pa",  # Apps + packages runtime deployment
    "L7": "pa",  # Renderers surface
    "L8": "pa",  # Network + hardware
    "L9": "la",  # Observability cross-cutting logical
    "L10": "pa",  # Distribution
    "XX": "sa",  # Cross-cutting contracts — system analysis surface
}

TEMPLATE_FILES = {
    ".project": ".project",
    "empty.afm": "{name}.afm",
    "empty.aird": "{name}.aird",
    "empty.capella": "{name}.capella",
}


def _stable_uuid(kind: str, item_id: str) -> str:
    """Deterministic UUID per (kind, item_id). Identical inputs → identical
    UUIDs across re-builds, so re-running the script doesn't churn the model."""
    return str(uuid.uuid5(UUID_NAMESPACE, f"aios.rev2.{kind}:{item_id}"))


def _read_csv(path: Path) -> list[dict]:
    if not path.exists():
        raise FileNotFoundError(f"Missing manifest: {path}")
    with path.open(encoding="utf-8") as fh:
        return list(csv.DictReader(fh))


def _copy_template() -> None:
    """Copy empty Capella template → output dir under aios-rev2 name."""
    if OUTPUT_DIR.exists():
        shutil.rmtree(OUTPUT_DIR)
    OUTPUT_DIR.mkdir(parents=True)

    missing = [name for name in TEMPLATE_FILES if not (TEMPLATE_DIR / name).exists()]
    if missing:
        raise FileNotFoundError(
            f"Empty Capella template missing files: {', '.join(missing)} "
            f"(expected at {TEMPLATE_DIR})"
        )

    for source_name, target_pattern in TEMPLATE_FILES.items():
        target_name = target_pattern.format(name=MODEL_NAME)
        shutil.copy2(TEMPLATE_DIR / source_name, OUTPUT_DIR / target_name)

    # Rename internal project references inside .project (Eclipse descriptor)
    project_path = OUTPUT_DIR / ".project"
    project_path.write_text(
        project_path.read_text(encoding="utf-8").replace(
            "<name>empty</name>", f"<name>{MODEL_NAME}</name>"
        ),
        encoding="utf-8",
    )

    # Rename internal references inside .aird (Sirius repr file) — points at
    # the .capella by name
    aird_path = OUTPUT_DIR / f"{MODEL_NAME}.aird"
    aird_path.write_text(
        aird_path.read_text(encoding="utf-8").replace(
            "empty.capella", f"{MODEL_NAME}.capella"
        ).replace(
            "empty.afm", f"{MODEL_NAME}.afm"
        ),
        encoding="utf-8",
    )


def _rename_model_roots(model) -> None:
    """Set top-level names per AIOS branding."""
    model.project.name = MODEL_NAME
    if hasattr(model.project, "model_root"):
        model.project.model_root.name = DISPLAY_NAME

    # ARCADIA layer root components/functions
    try:
        model.sa.root_component.name = "AIOS System"
    except Exception:
        pass
    try:
        model.la.root_component.name = "AIOS Logical Platform"
    except Exception:
        pass
    try:
        model.pa.root_component.name = "AIOS Physical Platform"
    except Exception:
        pass
    try:
        model.sa.root_function.name = "AIOS System Capability Realization"
    except Exception:
        pass
    try:
        model.la.root_function.name = "AIOS Logical Capability Realization"
    except Exception:
        pass
    try:
        model.pa.root_function.name = "AIOS Physical Capability Realization"
    except Exception:
        pass


def _capability_pkg(model, arcadia_target: str):
    """Get the capability_pkg from a given ARCADIA layer (oa/sa/la/pa)."""
    layer = getattr(model, arcadia_target, None)
    if layer is None or not hasattr(layer, "capability_pkg"):
        return None
    return layer.capability_pkg


def import_invariants(model, invariants: list[dict]) -> list[dict]:
    """Create one Operational Capability per INV. INVs always go to OA per
    AIOS layer mapping (constitutional governance = operational surface)."""
    oa_pkg = _capability_pkg(model, "oa")
    if oa_pkg is None:
        raise RuntimeError("Cannot access OA capability_pkg in model")

    imported = []
    for inv in invariants:
        inv_id = inv["id"]
        cap_uuid = _stable_uuid("invariant", inv_id)
        cap = oa_pkg.capabilities.create(
            name=f"{inv_id} — {inv['title']}",
            uuid=cap_uuid,
        )
        if hasattr(cap, "description"):
            cap.description = (
                f"<p><strong>Statement.</strong> {inv['statement']}</p>"
                f"<p><strong>Why.</strong> {inv['why']}</p>"
                f"<p><strong>Enforced by.</strong> {inv['enforced_by']}</p>"
                f"<p><strong>Verified by.</strong> {inv['verified_by']}</p>"
            )
        imported.append(
            {
                "id": inv_id,
                "uuid": cap_uuid,
                "arcadia_target": "oa",
                "title": inv["title"],
            }
        )
    return imported


def import_sub_specs(model, sub_specs: list[dict]) -> tuple[list[dict], list[dict]]:
    """Create one Operational/System/Logical/Physical Capability per sub-spec,
    routed by AIOS layer → ARCADIA target mapping."""
    imported = []
    skipped = []
    for spec in sub_specs:
        layer = spec["layer"]
        arcadia_target = LAYER_TARGETS.get(layer, "epbs")
        pkg = _capability_pkg(model, arcadia_target)
        if pkg is None:
            skipped.append({"phase_tag": spec["phase_tag"], "reason": f"no capability_pkg on {arcadia_target}"})
            continue
        phase_tag = spec["phase_tag"]
        # UUID seed from PATH (always unique) — phase_tag can repeat
        # (e.g., S1.3 = both AIOS-FS Object Model and Conflict Resolution per DEC-009 S1.3a/b)
        cap_uuid = _stable_uuid("sub_spec", spec["path"])
        cap = pkg.capabilities.create(
            name=f"{phase_tag} — {spec['title']}",
            uuid=cap_uuid,
        )
        if hasattr(cap, "description"):
            cap.description = (
                f"<p><strong>Layer.</strong> {layer}</p>"
                f"<p><strong>Status.</strong> {spec['status']}</p>"
                f"<p><strong>Schema package.</strong> {spec['schema_package'] or '—'}</p>"
                f"<p><strong>Source.</strong> {spec['path']}</p>"
            )
        imported.append(
            {
                "phase_tag": phase_tag,
                "uuid": cap_uuid,
                "arcadia_target": arcadia_target,
                "layer": layer,
                "title": spec["title"],
            }
        )
    return imported, skipped


def main() -> int:
    print(f"AIOS Rev.2 → Eclipse Capella model build")
    print(f"  template:  {TEMPLATE_DIR.relative_to(REPO_ROOT)}")
    print(f"  manifests: {MANIFESTS_DIR.relative_to(REPO_ROOT)}")
    print(f"  output:    {OUTPUT_DIR.relative_to(REPO_ROOT)}")
    print()

    print("[1/5] Copying empty template → output dir...")
    _copy_template()

    print("[2/5] Loading model via capellambse...")
    import capellambse

    aird_path = OUTPUT_DIR / f"{MODEL_NAME}.aird"
    model = capellambse.MelodyModel(str(aird_path))
    print(f"      Loaded. Layers: oa={type(model.oa).__name__}, "
          f"sa={type(model.sa).__name__}, la={type(model.la).__name__}, "
          f"pa={type(model.pa).__name__}")

    print("[3/5] Renaming model roots → aios-rev2...")
    _rename_model_roots(model)

    print("[4/5] Importing invariants + sub-specs...")
    invs = _read_csv(MANIFESTS_DIR / "invariants.csv")
    inv_records = import_invariants(model, invs)
    print(f"      Imported {len(inv_records)} INVs as Operational Capabilities")

    sub_specs = _read_csv(MANIFESTS_DIR / "sub_specs.csv")
    spec_records, spec_skipped = import_sub_specs(model, sub_specs)
    print(f"      Imported {len(spec_records)} sub-specs as Capabilities (across OA/SA/LA/PA)")
    if spec_skipped:
        print(f"      Skipped {len(spec_skipped)}: {spec_skipped}")

    # Distribution per ARCADIA target
    arcadia_dist = Counter(r["arcadia_target"] for r in (inv_records + spec_records))
    print(f"      ARCADIA distribution: {dict(arcadia_dist)}")

    print("[5/5] Saving model...")
    model.save()
    saved_aird_size = aird_path.stat().st_size
    saved_capella_size = (OUTPUT_DIR / f"{MODEL_NAME}.capella").stat().st_size
    print(f"      .aird:    {saved_aird_size:>10,} bytes")
    print(f"      .capella: {saved_capella_size:>10,} bytes")

    # Build report
    report = {
        "status": "PASS",
        "model_name": MODEL_NAME,
        "output_dir": str(OUTPUT_DIR.relative_to(REPO_ROOT)),
        "imported_invariants": len(inv_records),
        "imported_sub_specs": len(spec_records),
        "skipped_sub_specs": spec_skipped,
        "arcadia_distribution": dict(arcadia_dist),
        "file_sizes": {
            "aird": saved_aird_size,
            "capella": saved_capella_size,
        },
        "invariants": inv_records,
        "sub_specs": spec_records,
    }
    REPORT_PATH.parent.mkdir(exist_ok=True)
    REPORT_PATH.write_text(json.dumps(report, indent=2), encoding="utf-8")
    print(f"      Build report: {REPORT_PATH.relative_to(REPO_ROOT)}")

    print()
    print(f"✓ Capella project built at {OUTPUT_DIR.relative_to(REPO_ROOT)}")
    print(f"  Open in IDE: capella -data ~/.local/share/capella-workspaces/aios-rev2")
    print(f"  Then File > Import > General > Existing Projects into Workspace,")
    print(f"  pointing at {OUTPUT_DIR}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
