#!/usr/bin/env python3
"""
W11-A layer inversion classifier (DEC-049).

For each layer-inversion edge surfaced by analyze.py:
  1. Look up the consumer sub-spec's Consumes header in markdown source.
  2. Extract the natural-language snippet that describes the import of
     the specific producer (the substring around the producer's S-tag).
  3. Score the snippet against W11-A keyword sets:
       - vocabulary  → "imports-vocabulary-from" (allowed upward per DEC-049)
       - runtime     → "requires-for-correctness"  (forbidden upward)
       - uncertain   → no decisive keywords (needs human review)
  4. Emit:
       - tools/capella/output/layer_inversion_classification.csv
       - tools/capella/output/layer_inversion_classification.md
       - Console summary

Reports are markdown-table grade so a human reviewer can scan and override.
"""

from __future__ import annotations

import csv
import json
import re
import sys
from collections import Counter
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent.parent
SPEC_ROOT = REPO_ROOT / "002.AI-OS.NET--SPECREV.2"
GAP_JSON = REPO_ROOT / "tools" / "capella" / "output" / "gap_report.json"
MANIFESTS = REPO_ROOT / "tools" / "capella" / "manifests"
OUT_DIR = REPO_ROOT / "tools" / "capella" / "output"
CSV_OUT = OUT_DIR / "layer_inversion_classification.csv"
MD_OUT = OUT_DIR / "layer_inversion_classification.md"

# W11-A keyword sets ──────────────────────────────────────────────────
VOCAB_KEYWORDS = {
    "type-level",
    "type level",
    "vocabulary",
    "closed enum",
    "schema package",
    "schema only",
    "recordtype",
    "record type",
    "string format",
    "wire name",
    "wire format",
    "shape co-defined",
    "shape co defined",
    "imports vocabulary",
    "type definition",
    "field name",
    "field shape",
    "id format",
    "id triple",
    "id shape",
    "enum value",
    "type system shape",
    "io contract shape",
    "header shape",
    "envelope shape",
    # iter-6 expansions surfaced by sampling 39 uncertain edges:
    " shape",
    " enum",
    " field",
    "discipline",
    "evidence record",
    "manifest",
    "approval-request shape",
    "signed manifest",
    "circuit breaker",
    "trust chain",
    "trust level",
    "policy decision shape",
    "vault-brokered",
    "vault brokered",
    "verification grammar",
}
RUNTIME_KEYWORDS = {
    "must be operational",
    "must be running",
    "must be live",
    "live dependency",
    "runtime dependency",
    "service dependency",
    "service must",
    "requires runtime",
    "requires the live",
    "blocks on",
    "synchronous call",
    "rpc dependency",
    "calls into",
    "depends on the running",
    # iter-6 expansion: explicit W11-A negative marker found in S9.1
    "requires for correctness",
    "requires the over",  # "requires the override" — S9.1 recovery → S5.4
}


def load_inversions() -> list[dict]:
    data = json.loads(GAP_JSON.read_text(encoding="utf-8"))
    return data["gaps"]["layer_inversions"]


def load_sub_spec_paths() -> dict[str, list[Path]]:
    """phase_tag → list of markdown paths (some tags shared by 2 files, e.g.
    S1.3 covers both AIOS-FS Object Model + Conflict Resolution)."""
    out: dict[str, list[Path]] = {}
    with (MANIFESTS / "sub_specs.csv").open(encoding="utf-8") as fh:
        for row in csv.DictReader(fh):
            raw_path = Path(row["path"])
            if raw_path.is_absolute():
                md_path = raw_path
            elif (REPO_ROOT / raw_path).exists():
                md_path = REPO_ROOT / raw_path
            else:
                md_path = SPEC_ROOT / raw_path
            out.setdefault(row["phase_tag"], []).append(md_path)
    return out


def extract_consumes_row(md_path: Path) -> str:
    """Return the raw Consumes row text from a sub-spec markdown file."""
    for line in md_path.read_text(encoding="utf-8").splitlines():
        if line.startswith("| Consumes"):
            # Strip leading "| Consumes ... | " and trailing " |"
            parts = line.split("|")
            if len(parts) >= 3:
                return parts[2].strip()
    return ""


def snippet_around_tag(text: str, tag: str) -> str:
    """Extract a window of text around the first mention of the producer's S-tag.

    Window heuristic: from the previous comma/paren before the tag, to the
    next comma/paren after the matching paren group (if any) or 80 chars.
    """
    pattern = re.escape(tag) + r"(?![\w.])"  # match S2.3 but not S2.30
    m = re.search(pattern, text)
    if m is None:
        return ""
    start = m.start()
    # Walk left for prior ',' or '. ' or sentence start
    left_cut = max(text.rfind(",", 0, start), text.rfind(". ", 0, start), 0)
    # Walk right past matching paren if next char is '('
    rest = text[m.end():]
    if rest.lstrip().startswith("("):
        # Find balanced close
        depth = 0
        idx = 0
        in_paren = False
        for i, ch in enumerate(rest):
            if ch == "(":
                depth += 1
                in_paren = True
            elif ch == ")":
                depth -= 1
                if depth == 0 and in_paren:
                    idx = i + 1
                    break
        right_end = m.end() + idx
    else:
        # 250 chars or next ". " (sentence end), whichever first
        nxt = rest.find(". ")
        right_end = m.end() + (nxt if 0 < nxt < 250 else min(250, len(rest)))
    return text[left_cut:right_end].strip().lstrip(",").strip()


_BACKTICK_GROUP = re.compile(r"`[^`]*[A-Za-z][^`]*`")
_EXCEPTION_BOUNDED = (
    "degraded subset only",
    "degraded subset",
    "degraded form",
    "subset only",
    "statically at boot",
    "from signed material",
)


def classify_snippet(snippet: str) -> tuple[str, list[str]]:
    """Return (classification, matched_keywords).

    Precedence:
      1. RUNTIME keyword present + bounded-exception marker → exception
         (controlled waiver — recovery/boot degraded paths only).
      2. RUNTIME keyword alone → runtime (forbidden).
      3. VOCAB keyword → vocabulary (allowed).
      4. Backtick-wrapped group (identifier, enum value, expression) →
         vocabulary. Catches `PackageKind`, `VkDevice`, `PackageKind = ADAPTER`.
      5. Bare 'S<x>.<y> <Title>' reference (under 80 chars, no descriptor)
         is most commonly a vocabulary ref — treat as vocabulary with the
         'bare-spec-ref' tag so a reviewer can sample-check if desired.
      6. Otherwise → uncertain.
    """
    low = snippet.lower()
    runtime_hits = [k for k in RUNTIME_KEYWORDS if k in low]
    exception_hits = [k for k in _EXCEPTION_BOUNDED if k in low]
    if runtime_hits and exception_hits:
        return "exception", runtime_hits + exception_hits
    if runtime_hits:
        return "runtime", runtime_hits
    vocab_hits = [k for k in VOCAB_KEYWORDS if k in low]
    if vocab_hits:
        return "vocabulary", vocab_hits
    backtick_matches = _BACKTICK_GROUP.findall(snippet)
    if backtick_matches:
        return "vocabulary", [f"backtick-group:{backtick_matches[0][:60]}"]
    if snippet and len(snippet) < 120:
        # Short reference with a spec-title noun (case-insensitive) and no
        # negative markers. Matches "S2.4 Verification", "S7.1 surface
        # composition", "S3.1 (evidence refs)", etc.
        if re.search(r"\bS\d+\.\d+\b", snippet) and re.search(
            r"(?i)\b(schema|model|composition|log|manifest|engine|service|grammar|language|layout|mechanics|override|sandbox|surface|verification|evidence|policy|profile|record|enum)\b",
            snippet,
        ):
            return "vocabulary", ["bare-spec-ref-list"]
    return "uncertain", []


def main() -> int:
    inversions = load_inversions()
    paths = load_sub_spec_paths()

    results = []
    for inv in inversions:
        consumer_full = inv["consumer"]
        producer_full = inv["producer"]
        consumer_layer = inv["consumer_layer"]
        producer_layer = inv["producer_layer"]
        # Parse the phase tag prefix "Sx.y" from the capability name
        c_tag = consumer_full.split(" ")[0] if " " in consumer_full else consumer_full
        p_tag = producer_full.split(" ")[0] if " " in producer_full else producer_full
        c_paths = paths.get(c_tag, [])
        snippet = ""
        c_path_str = ""
        for cp in c_paths:
            if not cp.exists():
                continue
            consumes_text = extract_consumes_row(cp)
            snip = snippet_around_tag(consumes_text, p_tag)
            if snip:
                snippet = snip
                c_path_str = str(cp.relative_to(REPO_ROOT))
                break
        if not c_path_str and c_paths:
            c_path_str = str(c_paths[0].relative_to(REPO_ROOT))
        classification, hits = classify_snippet(snippet)
        results.append(
            {
                "consumer_tag": c_tag,
                "consumer_layer": consumer_layer,
                "producer_tag": p_tag,
                "producer_layer": producer_layer,
                "snippet": snippet[:200] + ("..." if len(snippet) > 200 else ""),
                "classification": classification,
                "matched_keywords": "; ".join(sorted(hits)),
                "consumer_path": c_path_str or "(unresolved)",
            }
        )

    # CSV
    with CSV_OUT.open("w", encoding="utf-8", newline="") as fh:
        writer = csv.DictWriter(fh, fieldnames=list(results[0].keys()))
        writer.writeheader()
        writer.writerows(results)
    print(f"  CSV: {CSV_OUT.relative_to(REPO_ROOT)}")

    # Markdown
    counter = Counter(r["classification"] for r in results)
    lines = [
        "# W11-A layer-inversion classification",
        "",
        f"Source: `tools/capella/output/gap_report.json` (run `analyze.py` to refresh).",
        f"Inversions classified: {len(results)}",
        "",
        "## Per-class totals",
        "",
        "| Classification | Count | W11-A verdict |",
        "| --- | ---: | --- |",
        f"| vocabulary  | {counter['vocabulary']} | ALLOWED upward (DEC-049) |",
        f"| exception   | {counter['exception']} | ALLOWED — runtime dep with bounded waiver (e.g. recovery degraded subset) |",
        f"| runtime     | {counter['runtime']} | FORBIDDEN upward (INV-007 violation) |",
        f"| uncertain   | {counter['uncertain']} | needs manual reviewer decision |",
        "",
        "## All edges",
        "",
        "| Consumer | C-layer | Producer | P-layer | Class | Snippet | Keywords |",
        "| --- | --- | --- | --- | --- | --- | --- |",
    ]
    for r in sorted(results, key=lambda x: (x["classification"], x["consumer_tag"], x["producer_tag"])):
        snip_md = r["snippet"].replace("|", "\\|").replace("\n", " ")
        kw_md = r["matched_keywords"].replace("|", "\\|")
        lines.append(
            f"| {r['consumer_tag']} | {r['consumer_layer']} | "
            f"{r['producer_tag']} | {r['producer_layer']} | "
            f"**{r['classification']}** | {snip_md} | {kw_md} |"
        )

    if counter["uncertain"] > 0:
        lines += [
            "",
            "## Uncertain edges — review prompt",
            "",
            "For each `uncertain` row above: read the consumer's Consumes header text in context. ",
            "If the dependency is purely on type definitions / schema vocabulary that is statically "
            "embedded at compile time (the consumer never invokes the producer at runtime), mark as ",
            "**vocabulary** (allowed). If the consumer's correctness or operational behaviour depends on ",
            "the producer being live/running, mark as **runtime** (forbidden — INV-007 violation).",
            "",
            "Common allowed patterns:",
            "- Importing a `closed enum` value name (`RecordType::APPROVAL_GRANTED`)",
            "- Embedding a wire string format (e.g. `ULID_T_RANDOM_64_HEX`)",
            "- Type-level structural shape that's resolved at build time",
            "",
            "Common forbidden patterns:",
            "- Calling an upstream service's RPC",
            "- Requiring upstream caches/state to be warmed before consumer can proceed",
            "- Synchronous health-checks against a higher-layer service",
            "",
        ]
    if counter["runtime"] > 0:
        lines += [
            "",
            "## RUNTIME (W11-A forbidden) — must be resolved",
            "",
            "These edges are INV-007 violations: a lower-layer spec requires a "
            "higher-layer spec to be operational. Each must be either:",
            "  1. Relocated — move shared vocabulary into XX_Cross_Cutting/",
            "  2. Restructured — invert the dependency so the higher layer wraps the lower",
            "  3. Documented as a known exception with a written waiver in DEC log",
            "",
        ]
    MD_OUT.write_text("\n".join(lines), encoding="utf-8")
    print(f"  MD:  {MD_OUT.relative_to(REPO_ROOT)}")

    # Console summary
    print()
    print("=== W11-A INVERSION CLASSIFICATION ===")
    print(f"  vocabulary (allowed):     {counter['vocabulary']:>3}")
    print(f"  exception  (waived):      {counter['exception']:>3}")
    print(f"  runtime    (forbidden):   {counter['runtime']:>3}")
    print(f"  uncertain  (review):      {counter['uncertain']:>3}")
    print()
    if counter["runtime"]:
        print("  ⚠  Runtime edges found — see MD report.")
    if counter["uncertain"]:
        print(f"  ℹ  {counter['uncertain']} edges need human review (snippets in MD report).")

    return 0


if __name__ == "__main__":
    sys.exit(main())
