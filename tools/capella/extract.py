#!/usr/bin/env python3
"""
AIOS Rev.2 spec → Capella import manifests (CSV).

Walks 002.AI-OS.NET--SPECREV.2/ and extracts:
  - 24 INVs (operational capabilities)
  - 53 sub-specs (system functions)
  - 11 layers (logical components)
  - INV → enforcement site traceability links
  - sub-spec Consumes/Produces interface dependencies

Output: tools/capella/manifests/*.csv

Run from repo root:
    python3 tools/capella/extract.py

The CSVs are designed to be imported into Capella IDE via:
  - Project > New Library / Import > CSV (for entities)
  - Capella's Requirements addon (for INVs as system requirements)
  - Manual diagram authoring (Capella's GUI; the CSVs seed the model)

This is a one-shot snapshot — re-run after spec changes; diff the CSVs
to find new/removed entities; reflect in the Capella model.

Source-of-truth remains the markdown specs under 002.AI-OS.NET--SPECREV.2/.
The Capella model is a derivable view, not a parallel source.
"""

from __future__ import annotations

import csv
import re
from dataclasses import dataclass, field
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent.parent
SPEC_ROOT = REPO_ROOT / "002.AI-OS.NET--SPECREV.2"
OUT_DIR = Path(__file__).resolve().parent / "manifests"
OUT_DIR.mkdir(exist_ok=True)


# ── INVs ──────────────────────────────────────────────────────────────


@dataclass
class Invariant:
    id: str  # "INV-001"
    title: str  # "Recovery is independent of L5"
    statement: str  # first paragraph after "**Statement:**"
    why: str  # first paragraph after "**Why:**"
    enforced_by: str  # raw text after "**Enforced by:**"
    verified_by: str  # raw text after "**Verified by:**"
    cannot_be_loosened_by: str


def extract_invariants() -> list[Invariant]:
    text = (SPEC_ROOT / "L0_Governance_Evidence_Safety/04_invariants.md").read_text()
    pattern = re.compile(
        r"### (INV-\d+) — (.+?)\n+(.*?)(?=\n### INV-\d+|\n## )", re.DOTALL
    )
    invs: list[Invariant] = []
    for match in pattern.finditer(text):
        inv_id, title, body = match.group(1), match.group(2).strip(), match.group(3)
        invs.append(
            Invariant(
                id=inv_id,
                title=title,
                statement=_extract_field(body, r"\*\*Statement:\*\*"),
                why=_extract_field(body, r"\*\*Why:\*\*"),
                enforced_by=_extract_field(body, r"\*\*Enforced by:\*\*"),
                verified_by=_extract_field(body, r"\*\*Verified by:\*\*"),
                cannot_be_loosened_by=_extract_field(
                    body, r"\*\*Cannot be loosened by:\*\*"
                ),
            )
        )
    return invs


def _extract_field(body: str, prefix_pattern: str) -> str:
    match = re.search(prefix_pattern + r"\s*(.+?)(?=\n\*\*|\n###|\Z)", body, re.DOTALL)
    if not match:
        return ""
    return match.group(1).strip().replace("\n", " ").replace('"', "'")[:500]


# ── Sub-specs ─────────────────────────────────────────────────────────


@dataclass
class SubSpec:
    phase_tag: str  # "S2.3"
    title: str  # "Policy Kernel"
    layer: str  # "L4"
    layer_name: str  # "L4 Policy, Identity, Vault"
    relative_path: str  # "L4_Policy_Identity_Vault/01_policy_kernel.md"
    status: str  # "CONTRACT"
    schema_package: str  # "aios.policy.v1alpha1" if present
    consumes_raw: str
    produces_raw: str
    consumes_specs: list[str] = field(default_factory=list)
    invariant_citations: list[str] = field(default_factory=list)


def extract_sub_specs() -> list[SubSpec]:
    specs: list[SubSpec] = []
    for layer_dir in sorted(SPEC_ROOT.iterdir()):
        if not layer_dir.is_dir() or not (
            layer_dir.name.startswith("L") or layer_dir.name == "XX_Cross_Cutting"
        ):
            continue
        layer_label = (
            "XX" if layer_dir.name == "XX_Cross_Cutting" else layer_dir.name.split("_")[0]
        )
        layer_name = layer_dir.name.replace("_", " ", 1).replace("_", " ")
        for md in sorted(layer_dir.glob("*.md")):
            if md.name == "00_overview.md":
                continue
            text = md.read_text()
            phase = _row_value(text, "Phase tag")
            title = _first_h1(text)
            if not phase:
                continue  # only files with phase tag are formal sub-specs
            specs.append(
                SubSpec(
                    phase_tag=phase,
                    title=title,
                    layer=layer_label,
                    layer_name=layer_name,
                    relative_path=str(md.relative_to(SPEC_ROOT)),
                    status=_row_value(text, "Status"),
                    schema_package=_row_value(text, "Schema package"),
                    consumes_raw=_row_value(text, "Consumes"),
                    produces_raw=_row_value(text, "Produces"),
                    consumes_specs=_extract_consumes_specs(_row_value(text, "Consumes")),
                    invariant_citations=sorted(set(re.findall(r"INV-\d+", text))),
                )
            )
    return specs


def _row_value(text: str, field_name: str) -> str:
    pattern = (
        r"^\|\s*" + re.escape(field_name) + r"\s*\|\s*(.+?)\s*\|\s*$"
    )
    match = re.search(pattern, text, re.MULTILINE)
    if not match:
        return ""
    value = match.group(1).strip()
    # Markdown table rows may contain inline pipes escaped; rough cleanup:
    return value.replace("`", "").replace('"', "'")[:600]


def _first_h1(text: str) -> str:
    match = re.search(r"^#\s+(.+?)\s*$", text, re.MULTILINE)
    return match.group(1).strip() if match else "(untitled)"


_CONSUMES_NEGATIVE_SENTINELS = (
    r"\*\*Downstream consumer \(not an import\):\*\*",
    r"\*\*Consumed by \(not an import\):\*\*",
)


def _extract_consumes_specs(consumes_text: str) -> list[str]:
    """Extract S-tag references from Consumes row text (S0.1, S2.3, etc.).

    Honours negative sentinels: any portion of the row text after a marker
    like '**Downstream consumer (not an import):**' is parenthetical context
    (documents who consumes from THIS spec), not an upward import. Those
    references are excluded from the dependency edge list to prevent
    phantom cycles in the analyzer (see DEC: S7.1↔S8.2 cycle root cause).
    """
    # Truncate at the first negative sentinel match
    import_portion = consumes_text
    for sentinel in _CONSUMES_NEGATIVE_SENTINELS:
        m = re.search(sentinel, import_portion)
        if m:
            import_portion = import_portion[: m.start()]
    return sorted(set(re.findall(r"S\d+\.\d+\w*", import_portion)))


# ── Layers ────────────────────────────────────────────────────────────


def extract_layers() -> list[tuple[str, str, str]]:
    """Returns (layer_id, layer_name, responsibility) from master index.

    Includes L0..L10 from the main table plus a synthetic XX row for the
    Cross-Cutting contracts directory (which the master index lists in a
    separate Cross-cutting contracts section, not as a layer).
    """
    text = (SPEC_ROOT / "00_MASTER_INDEX.md").read_text()
    rows: list[tuple[str, str, str]] = []
    for match in re.finditer(
        r"^\|\s*(L\d+|XX)\s*\|\s*\[([^\]]+)\][^|]*\|\s*([^|]+?)\s*\|",
        text,
        re.MULTILINE,
    ):
        rows.append((match.group(1), match.group(2), match.group(3).strip()))
    rows.append(
        (
            "XX",
            "XX_Cross_Cutting",
            "cross-layer contracts shared by L0..L10 (S0.1 action envelope, S0.3 MVP golden path, S0.4 constitutional meta-principles, ProxGuard reference donor)",
        )
    )
    return rows


# ── RecordTypes (from Wave 13 IDL reconciliation) ─────────────────────


def extract_record_types() -> list[tuple[int, str, str]]:
    """Returns (id, wire_name, retention_source) from S3.1 Appendix A.

    Wave 13 IDL has explicit `NAME = N;` lines inside enum RecordType.
    We scan the Appendix A block for those.
    """
    text = (
        SPEC_ROOT / "L9_Observability_Admin_Operations" / "01_evidence_log.md"
    ).read_text()
    # Find the canonical enum RecordType block under "Appendix A"
    appendix_match = re.search(
        r"## Appendix A:.*?enum RecordType\s*\{(.*?)^\}", text, re.DOTALL | re.MULTILINE
    )
    if not appendix_match:
        # Fall back to any enum RecordType definition
        any_match = re.search(
            r"enum RecordType\s*\{(.*?)^\}", text, re.DOTALL | re.MULTILINE
        )
        if not any_match:
            return []
        block = any_match.group(1)
    else:
        block = appendix_match.group(1)

    records: list[tuple[int, str, str]] = []
    for match in re.finditer(
        r"^\s*([A-Z][A-Z0-9_]*)\s*=\s*(\d+)\s*;\s*(?://\s*(.*))?$",
        block,
        re.MULTILINE,
    ):
        wire = match.group(1)
        if wire == "RECORD_TYPE_UNSPECIFIED":
            continue
        rec_id = int(match.group(2))
        retention_hint = (match.group(3) or "").strip()
        records.append((rec_id, wire, retention_hint))
    records.sort()
    return records


# ── RecordType emitter mapping (iter 3d) ──────────────────────────────


def extract_record_type_emitters(
    sub_specs: list[SubSpec], record_types: list[tuple[int, str, str]]
) -> list[tuple[str, str, int]]:
    """For each of the 427 RecordType wire names, find which sub-specs
    mention it. Returns rows (wire_name, sub_spec_phase_tag, mention_count).

    Heuristic: any sub-spec OTHER than S3.1 (which is the catalog itself)
    that mentions a RecordType wire name is considered an emitter candidate.
    S3.1's own mentions are excluded because S3.1 defines all 427; including
    its self-references would mask real orphan-record detection.

    The wire name pattern is matched as a whole word (so RECORD_FOO won't
    match RECORD_FOO_BAR). The match is case-sensitive (RecordTypes are
    canonical SCREAMING_SNAKE_CASE per S3.1 Appendix A).

    Limitations:
      - This is mention-based, not strict emission-context-based; a sub-spec
        that REFERENCES a record type for verification (not emission) will
        still appear in the emitters list. Manual classification is the
        next refinement step.
      - References inside fenced code blocks (proto IDL) are counted; in
        practice they're emission declarations + cross-citations, so this
        is acceptable noise.
    """
    s31_relative_path = "L9_Observability_Admin_Operations/01_evidence_log.md"
    wire_names = {wire for (_id, wire, _hint) in record_types}

    rows: list[tuple[str, str, int]] = []
    for spec in sub_specs:
        if spec.relative_path == s31_relative_path:
            continue  # S3.1 IS the catalog; its mentions aren't emissions
        path = SPEC_ROOT / spec.relative_path
        text = path.read_text()
        for wire in wire_names:
            count = sum(
                1
                for _ in re.finditer(rf"\b{re.escape(wire)}\b", text)
            )
            if count > 0:
                rows.append((wire, spec.phase_tag, count))
    rows.sort()
    return rows


# ── Traceability matrices ─────────────────────────────────────────────


def build_inv_to_subspec_matrix(
    invs: list[Invariant], specs: list[SubSpec]
) -> list[tuple[str, str, str]]:
    """Each row: (INV id, sub-spec phase tag, evidence — citation found)"""
    rows: list[tuple[str, str, str]] = []
    for spec in specs:
        for inv_id in spec.invariant_citations:
            rows.append((inv_id, spec.phase_tag, "cited"))
    return rows


def build_consumes_matrix(specs: list[SubSpec]) -> list[tuple[str, str]]:
    """Each row: (consumer phase tag, producer phase tag)."""
    return sorted(
        {(s.phase_tag, c) for s in specs for c in s.consumes_specs if c != s.phase_tag}
    )


# ── CSV writers ───────────────────────────────────────────────────────


def write_csv(path: Path, header: list[str], rows: list[tuple]) -> None:
    with path.open("w", newline="", encoding="utf-8") as fh:
        writer = csv.writer(fh, quoting=csv.QUOTE_MINIMAL)
        writer.writerow(header)
        writer.writerows(rows)
    print(f"  wrote {path.relative_to(REPO_ROOT)} ({len(rows)} rows)")


def main() -> None:
    print(f"AIOS spec extraction → Capella manifests")
    print(f"  source: {SPEC_ROOT.relative_to(REPO_ROOT)}")
    print(f"  target: {OUT_DIR.relative_to(REPO_ROOT)}")
    print()

    invs = extract_invariants()
    write_csv(
        OUT_DIR / "invariants.csv",
        ["id", "title", "statement", "why", "enforced_by", "verified_by"],
        [
            (i.id, i.title, i.statement, i.why, i.enforced_by, i.verified_by)
            for i in invs
        ],
    )

    specs = extract_sub_specs()
    write_csv(
        OUT_DIR / "sub_specs.csv",
        ["phase_tag", "title", "layer", "status", "schema_package", "path"],
        [
            (s.phase_tag, s.title, s.layer, s.status, s.schema_package, s.relative_path)
            for s in specs
        ],
    )

    layers = extract_layers()
    write_csv(
        OUT_DIR / "layers.csv", ["layer_id", "name", "responsibility"], layers
    )

    records = extract_record_types()
    write_csv(
        OUT_DIR / "record_types.csv",
        ["id", "wire_name", "retention_hint"],
        records,
    )

    inv_matrix = build_inv_to_subspec_matrix(invs, specs)
    write_csv(
        OUT_DIR / "trace_inv_to_subspec.csv",
        ["invariant_id", "sub_spec_phase_tag", "evidence"],
        inv_matrix,
    )

    consumes_matrix = build_consumes_matrix(specs)
    write_csv(
        OUT_DIR / "trace_consumes.csv",
        ["consumer", "producer"],
        consumes_matrix,
    )

    rt_emitters = extract_record_type_emitters(specs, records)
    write_csv(
        OUT_DIR / "record_type_emitters.csv",
        ["wire_name", "sub_spec_phase_tag", "mention_count"],
        rt_emitters,
    )

    # Compute orphan RecordTypes for the summary
    emitting_types = {wire for wire, _spec, _n in rt_emitters}
    all_types = {wire for (_id, wire, _hint) in records}
    orphan_types = sorted(all_types - emitting_types)

    print()
    print(f"Summary:")
    print(f"  Invariants:        {len(invs):>3}  (expected 24)")
    print(f"  Sub-specs:         {len(specs):>3}  (expected ~53)")
    print(f"  Layers:            {len(layers):>3}  (expected 11 + XX)")
    print(f"  RecordTypes:       {len(records):>3}  (expected 427)")
    print(f"  INV citations:     {len(inv_matrix):>3}")
    print(f"  Consumes links:    {len(consumes_matrix):>3}")
    print(f"  RT emitter pairs:  {len(rt_emitters):>3}")
    print(f"  RT WITH emitter:   {len(emitting_types):>3}  (cited at least once outside S3.1)")
    print(f"  RT orphan (no emitter): {len(orphan_types):>3}")


if __name__ == "__main__":
    main()
