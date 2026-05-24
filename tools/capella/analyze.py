#!/usr/bin/env python3
"""
AIOS Capella model — gap analyzer.

Walks the built Capella model at tools/capella/output/aios-rev2/ and
surfaces architectural gaps that single-spec markdown audits miss.

Gap categories detected:
  1. Orphan INVs        — Operational Capability (INV) with zero realizing
                          sub-specs. Means a constitutional invariant has
                          no concrete enforcement-site citation in any
                          sub-spec contract.
  2. Orphan sub-specs   — Sub-spec capability with zero realized INVs.
                          May be acceptable (purely structural sub-spec)
                          or may indicate missing constitutional binding.
  3. Layer inversion    — Consumes edge from layer L_n to layer L_m where
                          m > n AND the edge is `requires-for-correctness`
                          (INV-007 violation). Imports-vocabulary-from is
                          allowed upward per the W11-A discipline.
  4. Consumes cycles    — Directed cycle in the consumer→producer graph.
                          Architectural violation regardless of layer order.
  5. Cross-layer hot spots — Sub-specs with very high incoming or outgoing
                          Consumes counts. Not necessarily defects, but
                          worth flagging for design review.

Output:
  - Console summary
  - tools/capella/output/gap_report.md (human-readable)
  - tools/capella/output/gap_report.json (machine-readable)

Run from repo root:
    tools/capella/.venv/bin/python tools/capella/analyze.py
"""

from __future__ import annotations

import json
import re
import sys
from collections import Counter, defaultdict, deque
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent.parent
OUTPUT_DIR = Path(__file__).resolve().parent / "output"
MODEL_AIRD = OUTPUT_DIR / "aios-rev2" / "aios-rev2.aird"
MANIFESTS_DIR = Path(__file__).resolve().parent / "manifests"
REPORT_MD = OUTPUT_DIR / "gap_report.md"
REPORT_JSON = OUTPUT_DIR / "gap_report.json"


# Per AIOS layer naming convention: L0..L10 numeric; XX cross-cutting
LAYER_ORDER = {f"L{n}": n for n in range(11)} | {"XX": -1}


def _layer_of_capability_name(name: str) -> str | None:
    """Extract AIOS layer from a sub-spec capability name like 'S2.3 — Policy Kernel'.
    Maps the phase tag's first numeric digit to layer (e.g., S2.x → L2)."""
    match = re.match(r"^S(\d+)\.", name)
    if match:
        n = int(match.group(1))
        # AIOS phase tag numbering: S0.* = XX cross-cutting,
        # S1-S15 each map to the layer of their owning sub-spec.
        # Simpler heuristic: by inspecting our extract output later.
        return None  # fallback: use the description field which embeds layer
    if name.startswith("INV-"):
        return None  # INV doesn't have a layer (it's operational)
    return None


def _extract_layer_from_description(description: str | None) -> str | None:
    """Description carries '<p><strong>Layer.</strong> L4</p>' for sub-specs."""
    if not description:
        return None
    match = re.search(r"Layer\.\s*</strong>\s*(L\d+|XX)", description)
    if match:
        return match.group(1)
    return None


def analyze(model_path: Path) -> dict:
    import capellambse

    print(f"Loading Capella model: {model_path.relative_to(REPO_ROOT)}")
    model = capellambse.MelodyModel(str(model_path))

    # Collect all capabilities across all 4 ARCADIA layers
    all_caps: list = []
    cap_by_uuid: dict = {}
    cap_by_name: dict = {}
    for layer_name in ("oa", "sa", "la", "pa"):
        for cap in getattr(model, layer_name).all_capabilities:
            all_caps.append((layer_name, cap))
            cap_by_uuid[cap.uuid] = cap
            cap_by_name[cap.name] = cap

    print(f"  Total capabilities: {len(all_caps)}")

    # Separate INVs (OA) from sub-specs (SA/LA/PA)
    inv_caps = [c for layer, c in all_caps if layer == "oa" and c.name.startswith("INV-")]
    spec_caps = [c for layer, c in all_caps if not c.name.startswith("INV-")]
    print(f"  INVs (OA):        {len(inv_caps)}")
    print(f"  Sub-spec caps:    {len(spec_caps)}")

    # ── Gap 1: Orphan INVs ──────────────────────────────────────────────
    # An INV is realized BY sub-specs. The realizing direction in Capella:
    # cap.realizing_capabilities returns capabilities that have a
    # CapabilityRealization pointing AT this cap.
    orphan_invs = []
    inv_realization_count = {}
    for inv in inv_caps:
        realizers = list(inv.realizing_capabilities)
        inv_realization_count[inv.name] = len(realizers)
        if len(realizers) == 0:
            orphan_invs.append({"name": inv.name, "uuid": inv.uuid})

    # ── Gap 2: Orphan sub-specs ─────────────────────────────────────────
    orphan_specs = []
    spec_realization_count = {}
    for spec in spec_caps:
        realized = list(spec.realized_capabilities)
        spec_realization_count[spec.name] = len(realized)
        if len(realized) == 0:
            orphan_specs.append({"name": spec.name, "uuid": spec.uuid})

    # ── Gap 3: Layer inversion (INV-007 candidates) ─────────────────────
    # Walk project-owned MergeLink traces (Iteration 3 — replaces the
    # iter-2 cap.traces approach which only saw 33/238 due to capellambse
    # API quirk; project.traces sees all 238 fully-persisted edges).
    # Skip CapabilityRealization entries (those are INV → sub-spec links
    # which appear in the same traces collection; we want Consumes here).
    layer_inversions = []
    consumes_edges: list[tuple[str, str]] = []  # (consumer, producer) names
    spec_uuids = {c.uuid for layer, c in all_caps if not c.name.startswith("INV-")}

    for trace in model.project.traces:
        src = getattr(trace, "source", None)
        tgt = getattr(trace, "target", None)
        if src is None or tgt is None:
            continue
        # Only count Consumes-style traces: both endpoints are sub-spec
        # capabilities (not INVs); skip CapabilityRealization links
        if type(trace).__name__ in ("CapabilityRealization", "AbstractCapabilityRealization"):
            continue
        if src.uuid not in spec_uuids or tgt.uuid not in spec_uuids:
            continue
        consumes_edges.append((src.name, tgt.name))
        consumer_layer = _extract_layer_from_description(getattr(src, "description", None))
        producer_layer = _extract_layer_from_description(getattr(tgt, "description", None))
        if consumer_layer is None or producer_layer is None:
            continue
        c_idx = LAYER_ORDER.get(consumer_layer, 999)
        p_idx = LAYER_ORDER.get(producer_layer, 999)
        if p_idx > c_idx and consumer_layer != "XX" and producer_layer != "XX":
            layer_inversions.append(
                {
                    "consumer": src.name,
                    "consumer_layer": consumer_layer,
                    "producer": tgt.name,
                    "producer_layer": producer_layer,
                    "note": "Consumer's layer index is numerically lower than producer's — INV-007 candidate. Verify whether the Consumes header marks this as `imports-vocabulary-from` (allowed) or `requires-for-correctness` (forbidden).",
                }
            )

    # ── Gap 4: Cycles in Consumes graph ─────────────────────────────────
    graph: dict[str, list[str]] = defaultdict(list)
    for consumer, producer in consumes_edges:
        graph[consumer].append(producer)

    def find_cycles(graph: dict[str, list[str]]) -> list[list[str]]:
        """Tarjan-lite cycle detection via iterative DFS with WHITE/GRAY/BLACK."""
        WHITE, GRAY, BLACK = 0, 1, 2
        color: dict[str, int] = defaultdict(lambda: WHITE)
        parent: dict[str, str | None] = {}
        cycles: list[list[str]] = []

        for start in list(graph.keys()):
            if color[start] != WHITE:
                continue
            stack: list[tuple[str, int]] = [(start, 0)]
            parent[start] = None
            while stack:
                node, idx = stack[-1]
                color[node] = GRAY
                neighbors = graph.get(node, [])
                if idx < len(neighbors):
                    stack[-1] = (node, idx + 1)
                    nbr = neighbors[idx]
                    if color[nbr] == WHITE:
                        parent[nbr] = node
                        stack.append((nbr, 0))
                    elif color[nbr] == GRAY:
                        # Cycle: nbr → ... → node → nbr
                        cycle = [nbr]
                        cur = node
                        while cur is not None and cur != nbr:
                            cycle.append(cur)
                            cur = parent.get(cur)
                        cycle.append(nbr)
                        cycle.reverse()
                        cycles.append(cycle)
                else:
                    color[node] = BLACK
                    stack.pop()
        return cycles

    consumes_cycles = find_cycles(graph)

    # ── Gap 6: Orphan RecordTypes ───────────────────────────────────────
    # Cross-reference manifests/record_types.csv (427 defined in S3.1
    # Appendix A) against manifests/record_type_emitters.csv (RecordTypes
    # cited by at least one sub-spec other than S3.1 itself). RecordTypes
    # in the first but not the second are orphan: defined in vocabulary
    # but never emission-contextualised elsewhere.
    orphan_record_types: list[dict] = []
    try:
        import csv

        with (MANIFESTS_DIR / "record_types.csv").open(encoding="utf-8") as fh:
            all_rts = [dict(row) for row in csv.DictReader(fh)]
        with (MANIFESTS_DIR / "record_type_emitters.csv").open(encoding="utf-8") as fh:
            emitter_rows = list(csv.DictReader(fh))
        emitting_wires = {row["wire_name"] for row in emitter_rows}
        for rt in all_rts:
            if rt["wire_name"] not in emitting_wires:
                orphan_record_types.append(
                    {
                        "id": int(rt["id"]),
                        "wire_name": rt["wire_name"],
                        "retention_hint": rt.get("retention_hint", ""),
                    }
                )
    except FileNotFoundError as e:
        print(f"  (skipped orphan-RecordType gap: {e})")

    # ── Gap 5: Hot spots ────────────────────────────────────────────────
    in_degree = Counter(producer for _, producer in consumes_edges)
    out_degree = Counter(consumer for consumer, _ in consumes_edges)
    top_consumers = out_degree.most_common(5)
    top_producers = in_degree.most_common(5)

    summary = {
        "totals": {
            "capabilities": len(all_caps),
            "invariants": len(inv_caps),
            "sub_specs": len(spec_caps),
            "consumes_edges": len(consumes_edges),
            "inv_realization_links": sum(inv_realization_count.values()),
        },
        "gaps": {
            "orphan_invariants": orphan_invs,
            "orphan_sub_specs": orphan_specs,
            "layer_inversions": layer_inversions,
            "consumes_cycles": consumes_cycles,
            "orphan_record_types": orphan_record_types,
        },
        "hot_spots": {
            "top_consumers": [{"name": n, "out_degree": d} for n, d in top_consumers],
            "top_producers": [{"name": n, "in_degree": d} for n, d in top_producers],
        },
        "distributions": {
            "inv_realizations_histogram": dict(Counter(inv_realization_count.values())),
            "spec_realizations_histogram": dict(Counter(spec_realization_count.values())),
        },
    }

    return summary


def render_markdown(summary: dict) -> str:
    g = summary["gaps"]
    lines = [
        "# AIOS Capella model — gap report",
        "",
        f"Source model: `tools/capella/output/aios-rev2/`",
        "",
        "## Summary",
        "",
        f"- Total capabilities: {summary['totals']['capabilities']}",
        f"  - Invariants (OA): {summary['totals']['invariants']}",
        f"  - Sub-specs (SA/LA/PA): {summary['totals']['sub_specs']}",
        f"- Consumes edges: {summary['totals']['consumes_edges']}",
        f"- INV realization links: {summary['totals']['inv_realization_links']}",
        "",
        "## Gaps detected",
        "",
        f"| Gap category | Count |",
        f"| --- | ---: |",
        f"| Orphan INVs (zero realizing sub-specs) | {len(g['orphan_invariants'])} |",
        f"| Orphan sub-specs (zero realized INVs) | {len(g['orphan_sub_specs'])} |",
        f"| Layer inversions (INV-007 candidates) | {len(g['layer_inversions'])} |",
        f"| Consumes-graph cycles | {len(g['consumes_cycles'])} |",
        f"| Orphan RecordTypes (defined in S3.1, cited nowhere else) | {len(g.get('orphan_record_types', []))} |",
        "",
    ]

    if g["orphan_invariants"]:
        lines += ["## Orphan invariants", ""]
        for o in g["orphan_invariants"]:
            lines.append(f"- **{o['name']}** — no sub-spec cites this INV")
        lines.append("")

    if g["orphan_sub_specs"]:
        lines += ["## Orphan sub-specs", ""]
        lines.append(
            "Sub-spec capabilities with zero INV realization links. "
            "Some are legitimately structural (no constitutional binding needed); "
            "others may be missing their INV citations."
        )
        lines.append("")
        for o in g["orphan_sub_specs"]:
            lines.append(f"- {o['name']}")
        lines.append("")

    if g["layer_inversions"]:
        lines += ["## Layer inversions (INV-007 candidates)", ""]
        lines.append(
            "Consumes edges where the producer's layer is numerically higher than the consumer's. "
            "Per the W11-A discipline (DEC-049), `imports-vocabulary-from` is allowed upward; "
            "only `requires-for-correctness` is forbidden. Verify each by reading the source sub-spec's `Consumes` header."
        )
        lines.append("")
        for inv in g["layer_inversions"]:
            lines.append(
                f"- **{inv['consumer']}** ({inv['consumer_layer']}) "
                f"→ **{inv['producer']}** ({inv['producer_layer']})"
            )
        lines.append("")

    if g["consumes_cycles"]:
        lines += ["## Consumes-graph cycles", ""]
        lines.append("Architectural cycles — sub-spec A imports from B which imports from A (directly or transitively).")
        lines.append("")
        for cycle in g["consumes_cycles"]:
            lines.append(f"- `{' → '.join(cycle)}`")
        lines.append("")

    if g.get("orphan_record_types"):
        lines += ["## Orphan RecordTypes", ""]
        lines.append(
            "These RecordType variants are defined in S3.1 Appendix A closed enum "
            "(Wave 13 IDL roll-up, 427 total) but no other sub-spec mentions them. "
            "Possible interpretations:"
        )
        lines.append("")
        lines.append("- **Truly orphan** — vocabulary defined for completeness but never wired into emission contexts. Candidate for RETIRED status per S6.1 taxonomy, OR for adding explicit emitter sub-specs.")
        lines.append("- **Implicit emission** — emitted by infrastructure layers (Capability Runtime, Sandbox Composer, AIOS-FS) without explicit mention in their sub-spec narrative. Worth audit + adding explicit cite-up.")
        lines.append("")
        for rt in g["orphan_record_types"]:
            lines.append(f"- **{rt['wire_name']}** (ID {rt['id']})")
            if rt.get("retention_hint"):
                lines.append(f"  - hint from S3.1: {rt['retention_hint'][:120]}")
        lines.append("")

    lines += [
        "## Hot spots",
        "",
        "Sub-specs with highest fan-out / fan-in on the Consumes graph.",
        "",
        "### Top consumers (most outgoing Consumes)",
        "",
    ]
    for hc in summary["hot_spots"]["top_consumers"]:
        lines.append(f"- {hc['name']} — consumes from {hc['out_degree']} sub-specs")
    lines += ["", "### Top producers (most incoming Consumes)", ""]
    for hp in summary["hot_spots"]["top_producers"]:
        lines.append(f"- {hp['name']} — consumed by {hp['in_degree']} sub-specs")
    lines += [
        "",
        "## Distributions",
        "",
        "### Per-INV realizing sub-spec count",
        "",
        f"`{summary['distributions']['inv_realizations_histogram']}`",
        "",
        "Interpretation: e.g. `{1: 3, 5: 8}` means 3 INVs have exactly 1 realizing sub-spec and 8 INVs have 5 realizing sub-specs. A bucket at 0 = orphan INVs (also enumerated above).",
        "",
        "### Per-sub-spec realized INV count",
        "",
        f"`{summary['distributions']['spec_realizations_histogram']}`",
        "",
    ]
    return "\n".join(lines)


def main() -> int:
    if not MODEL_AIRD.exists():
        print(f"Model not found at {MODEL_AIRD}. Run build.py first.", file=sys.stderr)
        return 1

    summary = analyze(MODEL_AIRD)

    # JSON report
    REPORT_JSON.write_text(json.dumps(summary, indent=2), encoding="utf-8")
    print(f"  JSON report: {REPORT_JSON.relative_to(REPO_ROOT)}")

    # Markdown report
    md = render_markdown(summary)
    REPORT_MD.write_text(md, encoding="utf-8")
    print(f"  MD report:   {REPORT_MD.relative_to(REPO_ROOT)}")

    print()
    print("=== GAP SUMMARY ===")
    g = summary["gaps"]
    print(f"  Orphan INVs:           {len(g['orphan_invariants']):>3}")
    print(f"  Orphan sub-specs:      {len(g['orphan_sub_specs']):>3}")
    print(f"  Layer inversions:      {len(g['layer_inversions']):>3}")
    print(f"  Consumes cycles:       {len(g['consumes_cycles']):>3}")
    print(f"  Orphan RecordTypes:    {len(g.get('orphan_record_types', [])):>3}")

    return 0


if __name__ == "__main__":
    sys.exit(main())
