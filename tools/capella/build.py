#!/usr/bin/env python3
"""
AIOS Rev.3 → Eclipse Capella model builder.

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

Output: tools/capella/output/aios-rev3/ (Capella project, openable in IDE)
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
OUTPUT_DIR = Path(__file__).resolve().parent / "output" / "aios-rev3"
REPORT_PATH = Path(__file__).resolve().parent / "output" / "build_report.json"

MODEL_NAME = "aios-rev3"
DISPLAY_NAME = "AIOS Rev.3 — AI-Native Linux Distribution"
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


def import_record_types(model, record_types: list[dict]) -> list[dict]:
    """Create one Capella Class per RecordType under the LA data_pkg, grouped
    by Wave-of-introduction sub-package for IDE navigability.

    The 427 RecordType variants come from S3.1 Appendix A Wave 13 IDL
    reconciliation (DEC-051). Their canonical ID ranges per Wave:
      Wave 1 (Base):    1..22   (22 entries)  — original Appendix A
      §23 Namespace:   23..24   (2 entries)   — S4.1 namespace records
      Wave 5:          25..82   (58 entries)  — surface / theme / GPU
      Wave 6:          83..150  (68 entries)  — capability runtime / S8.1
      Wave 7:         151..196  (46 entries)  — kernel pipeline / repo
      Wave 8:         197..387  (191 entries) — full cross-spec catalog
      Wave 10:        388..427  (40 entries)  — vocabulary roll-up

    Wave 14+ additions (>=1000) are folded into a "Wave_14_Plus" package.
    """
    la_dp = model.la.data_pkg

    def wave_for(id_: int) -> str:
        if 1 <= id_ <= 22:
            return "Wave_1_Base"
        if 23 <= id_ <= 24:
            return "S23_Namespace"
        if 25 <= id_ <= 82:
            return "Wave_5_Surface_Theme_GPU"
        if 83 <= id_ <= 150:
            return "Wave_6_Capability_Runtime_Network"
        if 151 <= id_ <= 196:
            return "Wave_7_Kernel_Repository"
        if 197 <= id_ <= 387:
            return "Wave_8_Full_Catalog"
        if 388 <= id_ <= 427:
            return "Wave_10_Vocabulary_Rollup"
        if id_ >= 1000:
            return "Wave_14_Plus_Extensions"
        return "Wave_Other"

    # Create the umbrella RecordTypes data package + one sub-package per wave
    rt_pkg = la_dp.packages.create(
        name="RecordTypes",
        uuid=_stable_uuid("data_pkg", "RecordTypes"),
    )
    if hasattr(rt_pkg, "description"):
        rt_pkg.description = (
            "<p>S3.1 closed RecordType vocabulary materialized as Capella Classes "
            "for visual orphan-detection and emitter-traceability views. "
            "427 entries per Wave 13 IDL roll-up (DEC-051); grouped by Wave-of-"
            "introduction for IDE navigability. Each Class corresponds to one "
            "RecordType wire name; emitter traceability links from sub-spec "
            "capabilities arrive via project.traces.</p>"
        )

    wave_pkgs = {}

    imported = []
    for rt in record_types:
        rt_id = int(rt["id"])
        wave = wave_for(rt_id)
        if wave not in wave_pkgs:
            wave_pkgs[wave] = rt_pkg.packages.create(
                name=wave,
                uuid=_stable_uuid("data_pkg", f"RecordTypes/{wave}"),
            )
        cls_uuid = _stable_uuid("record_type", rt["wire_name"])
        cls = wave_pkgs[wave].classes.create(
            name=rt["wire_name"],
            uuid=cls_uuid,
        )
        if hasattr(cls, "description"):
            hint = rt.get("retention_hint") or ""
            cls.description = (
                f"<p><strong>RecordType ID.</strong> {rt_id}</p>"
                f"<p><strong>Wave.</strong> {wave.replace('_', ' ')}</p>"
                + (f"<p><strong>Retention/source hint.</strong> {hint[:200]}</p>" if hint else "")
            )
        imported.append(
            {"id": rt_id, "wire_name": rt["wire_name"], "wave": wave, "uuid": cls_uuid}
        )
    return imported


def wire_emitter_traces(
    model, rt_records: list[dict], spec_records: list[dict], emitter_rows: list[dict]
) -> int:
    """For each (RecordType, sub-spec) emitter mention, create a MergeLink
    trace from the sub-spec capability to the RecordType Class. Lets the
    Capella IDE traceability matrix view (rows=sub-specs, cols=RecordTypes)
    visually surface orphan RecordTypes as empty columns and over-emitting
    sub-specs as densely-filled rows.

    Same project.traces.create(source=, target=) pattern as Consumes (iter 3a).
    """
    rt_by_wire = {r["wire_name"]: r["uuid"] for r in rt_records}
    spec_by_phase = {r["phase_tag"]: r["uuid"] for r in spec_records}

    # uuid → model element cache
    elem_by_uuid = {}
    for cap in model.oa.all_capabilities:
        elem_by_uuid[cap.uuid] = cap
    for cap in model.sa.all_capabilities:
        elem_by_uuid[cap.uuid] = cap
    for cap in model.la.all_capabilities:
        elem_by_uuid[cap.uuid] = cap
    for cap in model.pa.all_capabilities:
        elem_by_uuid[cap.uuid] = cap
    for cls in model.la.all_classes:
        elem_by_uuid[cls.uuid] = cls

    traces_collection = model.project.traces
    wired = 0
    skipped = []
    for row in emitter_rows:
        wire = row["wire_name"]
        phase = row["sub_spec_phase_tag"]
        if wire not in rt_by_wire or phase not in spec_by_phase:
            skipped.append((wire, phase))
            continue
        rt_cls = elem_by_uuid.get(rt_by_wire[wire])
        spec_cap = elem_by_uuid.get(spec_by_phase[phase])
        if rt_cls is None or spec_cap is None:
            skipped.append((wire, phase))
            continue
        try:
            traces_collection.create(source=spec_cap, target=rt_cls)
            wired += 1
        except Exception as e:
            skipped.append((wire, phase, str(e)[:80]))
    if skipped:
        print(f"      Skipped {len(skipped)} emitter rows (first 3: {skipped[:3]})")
    return wired


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


def author_scenarios(model, inv_records: list[dict], spec_records: list[dict]) -> int:
    """Author 4 operational scenarios per the AIOS Capella modeling plan
    (tools/capella/docs/modeling_plan.md §"Operational Scenarios"):

      1. Golden path                — happy-path lifecycle (XX MVP)
      2. AI install attempted (denied) — INV-002 + INV-013 enforcement at runtime
      3. First-boot provisioning    — S9.2 with FIRST_BOOT mode discipline
      4. Tamper detected → recovery — S3.1 §11.4 + S9.1 RecoveryEntryReason

    Each scenario is created as a named Scenario object owned by its
    most relevant Capability (host). The body of each scenario lives in
    the description field as a flow narrative; the visual sequence
    diagram with participants + messages is authored in the Capella IDE
    using these scenarios as seeds. This is the standard MBSE pattern:
    Python pre-creates structure + content; humans author the diagram.
    """
    # Build name → capability lookup across all 4 ARCADIA layers
    cap_by_name: dict = {}
    for layer in ("oa", "sa", "la", "pa"):
        for cap in getattr(model, layer).all_capabilities:
            cap_by_name[cap.name] = cap

    def host(prefix: str):
        """Find first capability whose name starts with prefix."""
        return next((c for n, c in cap_by_name.items() if n.startswith(prefix)), None)

    scenarios = [
        {
            "host": host("S0.3"),
            "name": "Golden path — typed action lifecycle end-to-end",
            "description": (
                "<p><strong>Source:</strong> XX_Cross_Cutting/03_mvp_golden_path.md</p>"
                "<p><strong>Participants:</strong> HUMAN_USER operator, AIOS Capability Runtime (S10.1), "
                "Policy Kernel (S2.3), Sandbox Composer (S3.2), Verification Engine (S2.4), "
                "Evidence Log (S3.1), Renderer (S7.6).</p>"
                "<p><strong>Flow:</strong></p>"
                "<ol>"
                "<li>operator submits typed ActionEnvelope (act_&lt;ulid&gt;) per S0.1</li>"
                "<li>Capability Runtime validates envelope, normalizes subject (S2.3 §7)</li>"
                "<li>Policy Kernel evaluates → ALLOW (no hard-deny fires, no approval gate)</li>"
                "<li>Sandbox composed per S3.2 most-restrictive-wins</li>"
                "<li>Adapter dispatches typed action (ISOLATED_SANDBOX per S10.1)</li>"
                "<li>Verification probes run per S2.4 closed grammar</li>"
                "<li>Evidence Log appends 5-receipt chain: ACTION_RECEIVED → POLICY_DECISION → "
                "EXECUTION_STARTED → EXECUTION_COMPLETED → VERIFICATION_RESULT</li>"
                "<li>Renderer surfaces result + CHROME zone (INV-020) shows action_id + evidence link</li>"
                "</ol>"
                "<p><strong>Invariants exercised:</strong> INV-002 (AI proposes never executes — if "
                "subject is_ai, branches to AI scenario), INV-005 (append-only), INV-014 (no proof no "
                "completion), INV-020 (trust indicators visible).</p>"
            ),
        },
        {
            "host": cap_by_name.get("INV-002 — AI proposes, never executes"),
            "name": "AI install attempted — constitutional hard-deny",
            "description": (
                "<p><strong>Source:</strong> S2.3 §26.2.4 AIInstallInitiationBlocked (Wave 9)</p>"
                "<p><strong>Participants:</strong> AI_AGENT subject, Capability Runtime, Policy Kernel, "
                "Evidence Log.</p>"
                "<p><strong>Flow:</strong></p>"
                "<ol>"
                "<li>AI agent submits package.install action with subject.is_ai = true</li>"
                "<li>Capability Runtime forwards to Policy Kernel</li>"
                "<li>Policy Kernel §27 hard-deny chain: AIInstallInitiationBlocked fires "
                "(matches subject.is_ai=true AND request.action IN {package.install, app.install, "
                "package.uninstall.execute, app.uninstall.execute})</li>"
                "<li>Decision: DENY with reason_code = AIInstallInitiationBlocked</li>"
                "<li>Evidence Log emits APP_AI_DIRECT_INSTALL_ATTEMPTED_BLOCKED (FOREVER retention)</li>"
                "<li>Action lifecycle: PENDING → FAILED (no execution; no evidence of execution)</li>"
                "<li>Operator surface (if any) shows hard-deny notification</li>"
                "</ol>"
                "<p><strong>Invariants enforced:</strong> INV-002 (site 2 of 6 enforcement map per "
                "S0.4 §4), INV-013 (AI cannot system admin), INV-005 (FOREVER evidence of attempt).</p>"
                "<p><strong>SIM-D constitutional check:</strong> bypass attempt produces evidence — "
                "AI subjects literally cannot install software without leaving a permanent forensic "
                "trace; the constitutional core remains mechanically intact.</p>"
            ),
        },
        {
            "host": host("S9.2"),
            "name": "First-boot provisioning — FIRST_BOOT mode discipline",
            "description": (
                "<p><strong>Source:</strong> S9.2 First-Boot Flow (Rev.2)</p>"
                "<p><strong>Participants:</strong> LOCAL_OPERATOR _system:local:operator-1, "
                "firstboot-coordinator (SERVICE), installer, vault-init, identity-init, "
                "policy-compiler, AIOS-FS, Vault Broker (S5.2), Identity Service (S5.1).</p>"
                "<p><strong>Flow:</strong></p>"
                "<ol>"
                "<li>Recovery image first mount; RecoveryMode = FIRST_BOOT (S9.1 W9)</li>"
                "<li>firstboot-coordinator authenticated via hardware-key at console</li>"
                "<li>installer initialises /aios filesystem layout per S4.1 namespace catalog</li>"
                "<li>vault-init generates Ed25519 vault root key; uses one-shot "
                "BOOTSTRAP_KEY_SIGN (per-host exhaustion) per S5.2 §3</li>"
                "<li>identity-init registers bootstrap group (HUMAN_USER not yet present; hardware-key "
                "signature substitutes per S5.1 §5.2.1)</li>"
                "<li>policy-compiler loads constitutional policy bundle (signed)</li>"
                "<li>Each stage emits FOREVER FIRST_BOOT_OPERATION receipt (escape clause for "
                "INV-012 RecoveryRequiredForSystemMutation; per S2.3 §26.2.2 W9 update)</li>"
                "<li>At stage exit: firstboot marker written; is_first_boot flag self-extinguishes "
                "atomically across all subject sessions</li>"
                "<li>System transitions to RecoveryMode = NORMAL</li>"
                "</ol>"
                "<p><strong>Invariants threaded:</strong> INV-001 (no L5), INV-004 (recovery "
                "boundary), INV-012 (system mutation gating — first-boot exception), INV-018 (vault "
                "no leak), INV-005 (FOREVER evidence per stage).</p>"
            ),
        },
        {
            "host": cap_by_name.get("INV-005 — Evidence is append-only"),
            "name": "Tamper detected → recovery — constitutional anchor failure",
            "description": (
                "<p><strong>Source:</strong> S3.1 §11.4 tamper-recovery; S9.1 RecoveryEntryReason</p>"
                "<p><strong>Participants:</strong> Evidence Log Verifier (S3.1), Recovery Coordinator "
                "(S9.1), LOCAL_OPERATOR.</p>"
                "<p><strong>Flow:</strong></p>"
                "<ol>"
                "<li>Periodic chain audit by S3.1 VerifyChain RPC (or startup chain verification)</li>"
                "<li>Hash mismatch / signature invalid / segment seal broken detected</li>"
                "<li>S3.1 emits EVIDENCE_LOG_TAMPER_DETECTED (FOREVER) as final pre-shutdown record</li>"
                "<li>System refuses further evidence appends until operator intervention</li>"
                "<li>On next boot: GRUB selects recovery; RecoveryEntryReason = "
                "EVIDENCE_LOG_TAMPER_DETECTED</li>"
                "<li>Recovery shell shows tamper context; operator reviews FOREVER record chain</li>"
                "<li>Either: operator decides chain is unrecoverable (full reset-to-factory via "
                "S5.4 emergency override), or: forensic snapshot taken + chain quarantined + new "
                "fresh chain bootstrapped under recovery-mode discipline</li>"
                "</ol>"
                "<p><strong>Invariants threaded:</strong> INV-005 (the tamper IS the violation), "
                "INV-001 (recovery without L5 — verifier and recovery shell never invoke cognitive "
                "core), INV-014 (no completion claim survives tamper), INV-004 (recovery boundary "
                "engaged).</p>"
            ),
        },
    ]

    created = 0
    skipped = []
    for s in scenarios:
        host_cap = s["host"]
        if host_cap is None:
            skipped.append(s["name"])
            continue
        try:
            sc = host_cap.scenarios.create(name=s["name"])
            if hasattr(sc, "description"):
                sc.description = s["description"]
            created += 1
        except Exception as e:
            skipped.append(f"{s['name']}: {e}")
    if skipped:
        print(f"      Skipped {len(skipped)}: {skipped[:2]}")
    return created


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
    print(f"AIOS Rev.3 → Eclipse Capella model build")
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

    print("[3/5] Renaming model roots → aios-rev3...")
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

    print("[4e/5] Authoring 4 operational scenarios...")
    scenarios_created = author_scenarios(model, inv_records, spec_records)
    print(f"      Created {scenarios_created}/4 scenarios")

    print("[4f/5] Importing 427 RecordTypes as LA data Classes (Wave-grouped)...")
    rt_csv = _read_csv(MANIFESTS_DIR / "record_types.csv")
    rt_records = import_record_types(model, rt_csv)
    rt_wave_dist = Counter(r["wave"] for r in rt_records)
    print(f"      Imported {len(rt_records)} RecordType Classes; wave distribution: {dict(rt_wave_dist)}")

    print("[4g/5] Wiring RecordType emitter traces (sub-spec → RecordType)...")
    emitter_rows = _read_csv(MANIFESTS_DIR / "record_type_emitters.csv")
    emitter_links = wire_emitter_traces(model, rt_records, spec_records, emitter_rows)
    print(f"      Wired {emitter_links}/{len(emitter_rows)} emitter traces")

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
    print(f"  Open in IDE: capella -data ~/.local/share/capella-workspaces/aios-rev3")
    print(f"  Then File > Import > General > Existing Projects into Workspace,")
    print(f"  pointing at {OUTPUT_DIR}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
