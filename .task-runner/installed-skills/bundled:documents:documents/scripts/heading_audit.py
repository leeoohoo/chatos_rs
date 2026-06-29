#!/usr/bin/env python3
"""Audit heading hierarchy and numbering in a DOCX.

Checks (best-effort)
-------------------
- Which heading styles appear (Heading 1/2/3/...)
- Whether heading levels jump (e.g., Heading 3 immediately after Heading 1)
- Whether paragraphs use numbering without a heading style (common source of broken TOC)

This is not a full style-linter. It is meant as a fast QA signal before you render.
"""

from __future__ import annotations

import argparse
from collections import Counter
from pathlib import Path

from docx import Document


def _heading_level(style_name: str | None) -> int | None:
    if not style_name:
        return None
    s = style_name.strip().lower()
    if not s.startswith("heading"):
        return None
    parts = s.split()
    if len(parts) < 2:
        return None
    try:
        return int(parts[1])
    except Exception:
        return None


def _has_numbering(p) -> bool:
    # python-docx doesn't expose numbering cleanly; check OOXML
    pPr = p._p.pPr
    if pPr is None:
        return False
    return pPr.numPr is not None


def main() -> None:
    ap = argparse.ArgumentParser(description="Audit heading hierarchy + numbering usage in a DOCX")
    ap.add_argument("docx", type=Path)
    ap.add_argument("--max_findings", type=int, default=20)
    args = ap.parse_args()

    if not args.docx.exists():
        raise FileNotFoundError(args.docx)

    doc = Document(str(args.docx))
    h_counts: Counter[int] = Counter()
    jumps: list[str] = []
    numbered_non_heading: list[str] = []

    last_h: int | None = None

    for i, p in enumerate(doc.paragraphs, start=1):
        style = getattr(p.style, "name", None)
        lvl = _heading_level(style)
        if lvl is not None:
            h_counts[lvl] += 1
            if last_h is not None and lvl > last_h + 1:
                jumps.append(f"p#{i}: Heading {last_h} → Heading {lvl}: {p.text[:80]!r}")
            last_h = lvl
        if _has_numbering(p) and lvl is None:
            # numbered list without Heading style - often intended to be numbered headings
            txt = (p.text or "").strip()
            if txt:
                numbered_non_heading.append(f"p#{i}: style={style!r} text={txt[:80]!r}")

    print("HEADING STYLE COUNTS")
    if not h_counts:
        print("- (no Heading styles found)")
    else:
        for lvl in sorted(h_counts):
            print(f"- Heading {lvl}: {h_counts[lvl]}")

    if jumps:
        print("\nPOTENTIAL HEADING LEVEL JUMPS (review)")
        for s in jumps[: args.max_findings]:
            print(f"- {s}")
        if len(jumps) > args.max_findings:
            print(f"- ... ({len(jumps) - args.max_findings} more)")

    if numbered_non_heading:
        print("\nNUMBERING WITHOUT HEADING STYLE (common TOC issue)")
        for s in numbered_non_heading[: args.max_findings]:
            print(f"- {s}")
        if len(numbered_non_heading) > args.max_findings:
            print(f"- ... ({len(numbered_non_heading) - args.max_findings} more)")

    print("\nREMINDER")
    print(
        "- TOC relies on Heading styles (Heading 1/2/3...). Avoid manual numbering + direct formatting for headings."
    )


if __name__ == "__main__":
    main()
