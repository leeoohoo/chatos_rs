#!/usr/bin/env python3
"""Audit section/page layout settings in a DOCX.

Why this exists
---------------
Sections are the #1 source of "why is page 3 landscape" / header/footer bugs.
Word documents can have multiple sections with independent:
- orientation (portrait/landscape)
- page size & margins
- header/footer linkage (link to previous)
- different first page / odd-even headers

This script prints a compact per-section report.
"""

from __future__ import annotations

import argparse
from pathlib import Path

from docx import Document

EMU_PER_INCH = 914400


def _inches(x) -> float:
    # python-docx uses Length objects with .inches, but be defensive.
    try:
        return float(x.inches)
    except Exception:
        try:
            return float(x) / EMU_PER_INCH
        except Exception:
            return float("nan")


def main() -> None:
    ap = argparse.ArgumentParser(
        description="Audit DOCX sections: orientation, margins, header/footer linkage"
    )
    ap.add_argument("docx", type=Path)
    args = ap.parse_args()

    if not args.docx.exists():
        raise FileNotFoundError(args.docx)

    doc = Document(str(args.docx))

    print(f"SECTIONS: {len(doc.sections)}")
    for idx, sec in enumerate(doc.sections, start=1):
        try:
            orient = str(sec.orientation).split(".")[-1]
        except Exception:
            orient = "(unknown)"

        pw = _inches(sec.page_width)
        ph = _inches(sec.page_height)
        lm = _inches(sec.left_margin)
        rm = _inches(sec.right_margin)
        tm = _inches(sec.top_margin)
        bm = _inches(sec.bottom_margin)

        # Header/footer linkage
        hl = getattr(sec.header, "is_linked_to_previous", None)
        fl = getattr(sec.footer, "is_linked_to_previous", None)

        dfp = getattr(sec, "different_first_page_header_footer", None)
        oep = getattr(sec, "odd_and_even_pages_header_footer", None)

        # Start type (new page / continuous) if available
        st = getattr(sec, "start_type", None)
        st_s = str(st).split(".")[-1] if st is not None else "(unknown)"

        print(f"\n[Section {idx}] start_type={st_s} orientation={orient}")
        print(f"  page_size(in): {pw:.2f} x {ph:.2f}")
        print(f"  margins(in): L={lm:.2f} R={rm:.2f} T={tm:.2f} B={bm:.2f}")
        print(f"  header_linked_to_previous={hl} footer_linked_to_previous={fl}")
        print(f"  different_first_page={dfp} odd_even_headers={oep}")

    print("\nREMINDER")
    print("- If you change orientation mid-document, Word typically creates a new section.")
    print("- If headers/footers look wrong, check 'Link to Previous' per section.")


if __name__ == "__main__":
    main()
