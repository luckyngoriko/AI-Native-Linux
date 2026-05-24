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


def import_layers(model, layers: list[dict]) -> list[dict]:
    """Create one LogicalComponent per AIOS layer under la.root_component.

    Layers are the 11 AIOS architecture layers (L0..L10) + XX cross-cutting.
    In Capella they appear as sub-components of the Logical Platform root,
    giving an at-a-glance layer view in the Logical Architecture diagram.
    """
    la_root = model.la.root_component
    imported = []
    for layer in layers:
        layer_id = layer["layer_id"]
        comp_uuid = _stable_uuid("layer", layer_id)
        comp = la_root.components.create(
            name=f"{layer_id} — {layer['name']}",
            uuid=comp_uuid,
        )
        if hasattr(comp, "description"):
            comp.description = f"<p>{layer['responsibility']}</p>"
        imported.append({"layer_id": layer_id, "uuid": comp_uuid, "name": layer["name"]})
    return imported


def wire_inv_traces(model, inv_records: list[dict], spec_records: list[dict], trace_rows: list[dict]) -> int:
    """For each sub-spec that cites an INV, create a capability-realization link
    from the sub-spec capability (SA/LA/PA) to the INV operational capability (OA).

    Capella semantic: the sub-spec REALIZES the operational capability — i.e.,
    a constitutional INV is realized through the sub-spec's concrete contract.
    """
    inv_lookup = {r["id"]: r["uuid"] for r in inv_records}
    spec_lookup = {r["phase_tag"]: r["uuid"] for r in spec_records}

    # Build a uuid → capability cache for O(1) lookup
    cap_by_uuid = {}
    for layer in ("oa", "sa", "la", "pa"):
        for cap in getattr(model, layer).all_capabilities:
            cap_by_uuid[cap.uuid] = cap

    wired = 0
    skipped = []
    for row in trace_rows:
        inv_id = row["invariant_id"]
        phase_tag = row["sub_spec_phase_tag"]
        if inv_id not in inv_lookup or phase_tag not in spec_lookup:
            skipped.append((inv_id, phase_tag))
            continue
        inv_cap = cap_by_uuid.get(inv_lookup[inv_id])
        spec_cap = cap_by_uuid.get(spec_lookup[phase_tag])
        if inv_cap is None or spec_cap is None:
            skipped.append((inv_id, phase_tag))
            continue
        # Sub-spec realizes the INV operational capability via an explicit
        # CapabilityRealization object (capellambse correct API — not the
        # derived `realized_capabilities` accessor which is read-only).
        try:
            spec_cap.capability_realizations.create(target=inv_cap)
            wired += 1
        except Exception as e:
            skipped.append((inv_id, phase_tag, str(e)[:80]))
    if skipped:
        print(f"      Skipped {len(skipped)} trace rows (first 3: {skipped[:3]})")
    return wired


def wire_consumes_traces(model, spec_records: list[dict], consumes_rows: list[dict]) -> int:
    """For each sub-spec X that consumes from sub-spec Y, create an explicit
    project-owned MergeLink (source=X, target=Y) capturing the cross-spec
    vocabulary-import dependency graph (the 238 edges from
    manifests/trace_consumes.csv).

    Iteration 3 correction (vs iteration 2): the previous
    `consumer_cap.traces.append(producer_cap)` pattern relied on a derived
    collection that did NOT reliably persist generic cross-package traces
    (only 33/238 visible on reload — most were no-ops). The correct
    capellambse API is `model.project.traces.create(source=, target=)`
    which materialises a real MergeLink owned by the project root, fully
    persistent on save+reload.
    """
    spec_lookup = {r["phase_tag"]: r["uuid"] for r in spec_records}
    cap_by_uuid = {}
    for layer in ("oa", "sa", "la", "pa"):
        for cap in getattr(model, layer).all_capabilities:
            cap_by_uuid[cap.uuid] = cap

    traces_collection = model.project.traces
    wired = 0
    skipped = []
    for row in consumes_rows:
        consumer = row["consumer"]
        producer = row["producer"]
        if consumer not in spec_lookup or producer not in spec_lookup:
            skipped.append((consumer, producer))
            continue
        consumer_cap = cap_by_uuid.get(spec_lookup[consumer])
        producer_cap = cap_by_uuid.get(spec_lookup[producer])
        if consumer_cap is None or producer_cap is None:
            skipped.append((consumer, producer))
            continue
        try:
            traces_collection.create(source=consumer_cap, target=producer_cap)
            wired += 1
        except Exception as e:
            skipped.append((consumer, producer, str(e)[:80]))
    if skipped:
        print(f"      Skipped {len(skipped)} consumes rows (first 3: {skipped[:3]})")
    return wired


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

    print("[4b/5] Importing 12 layers as Logical Components...")
    layers = _read_csv(MANIFESTS_DIR / "layers.csv")
    layer_records = import_layers(model, layers)
    print(f"      Imported {len(layer_records)} layers under la.root_component")

    print("[4c/5] Wiring INV traceability (sub-spec realizes INV)...")
    trace_rows = _read_csv(MANIFESTS_DIR / "trace_inv_to_subspec.csv")
    inv_links = wire_inv_traces(model, inv_records, spec_records, trace_rows)
    print(f"      Wired {inv_links}/{len(trace_rows)} INV realization links")

    print("[4d/5] Wiring Consumes traces (consumer → producer)...")
    consumes_rows = _read_csv(MANIFESTS_DIR / "trace_consumes.csv")
    consumes_links = wire_consumes_traces(model, spec_records, consumes_rows)
    print(f"      Wired {consumes_links}/{len(consumes_rows)} Consumes traces")

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
        "imported_layers": len(layer_records),
        "wired_inv_realization_links": inv_links,
        "wired_consumes_traces": consumes_links,
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
