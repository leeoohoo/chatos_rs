#!/usr/bin/env python3
"""Report Word fields present in a DOCX.

Why this exists
--------------
Many "looks wrong" render issues are simply *stale fields* (PAGE/NUMPAGES/TOC/REF, etc.).
LibreOffice headless vs Word GUI can update fields differently, and some PDF exports can show
placeholders if fields weren't refreshed.

This script scans OOXML parts for fields and prints:
- Per-part list of field instructions (best-effort)
- Counts by field type (first token of instruction)
- A quick "stale field" hint (e.g., TOC present → remind to update fields)

It is intentionally simple: it does not try to fully interpret field switches.
"""

from __future__ import annotations

import argparse
import re
import zipfile
from collections import Counter, defaultdict
from pathlib import Path

from lxml import etree

W_NS = "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
R_NS = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"

NS = {"w": W_NS, "r": R_NS}


def _field_type(instr: str) -> str:
    s = (instr or "").strip()
    if not s:
        return "(empty)"
    return s.split()[0].upper()


def _iter_word_xml_parts(z: zipfile.ZipFile):
    # Document + headers/footers are the common places fields live.
    for name in z.namelist():
        if not name.startswith("word/"):
            continue
        if not name.endswith(".xml"):
            continue
        base = name.rsplit("/", 1)[-1]
        if base in {"document.xml", "settings.xml"}:
            yield name
        elif base.startswith(("header", "footer")):
            yield name
        elif base in {"footnotes.xml", "endnotes.xml"}:
            yield name


def extract_field_instructions(xml_bytes: bytes) -> list[str]:
    """Extract field instructions from a WordprocessingML part.

    Handles:
    - Simple fields: <w:fldSimple w:instr="..."/>
    - Complex fields: <w:instrText> tokens between fldChar begin/end

    For complex fields, we concatenate instrText nodes in document order.
    """

    parser = etree.XMLParser(recover=True, remove_blank_text=False)
    root = etree.fromstring(xml_bytes, parser=parser)

    out: list[str] = []

    # 1) fldSimple
    for el in root.findall(".//w:fldSimple", namespaces=NS):
        instr = el.get(f"{{{W_NS}}}instr") or el.get("w:instr")  # defensive
        if instr:
            out.append(instr)

    # 2) Complex fields
    # Approach: walk runs in order, track state when we see fldChar begin → gather instrText
    in_field = False
    buf: list[str] = []
    for node in root.iter():
        # fldChar
        if node.tag == f"{{{W_NS}}}fldChar":
            t = node.get(f"{{{W_NS}}}fldCharType")
            if t == "begin":
                in_field = True
                buf = []
            elif t == "end" and in_field:
                instr = "".join(buf).strip()
                if instr:
                    out.append(instr)
                in_field = False
                buf = []
            continue

        # instrText (can be split across runs)
        if in_field and node.tag == f"{{{W_NS}}}instrText":
            buf.append(node.text or "")

    # As a fallback, if a doc has instrText but malformed fldChar structure,
    # we still surface them so users can see something.
    if not out:
        texts = [
            (t.text or "")
            for t in root.findall(".//w:instrText", namespaces=NS)
            if (t.text or "").strip()
        ]
        out.extend(texts)

    # Normalize whitespace to make output readable
    normed: list[str] = []
    for s in out:
        s2 = re.sub(r"\s+", " ", s).strip()
        if s2:
            normed.append(s2)
    return normed


def main() -> None:
    ap = argparse.ArgumentParser(description="Scan a DOCX for Word fields (PAGE/TOC/REF/...).")
    ap.add_argument("docx", type=Path)
    ap.add_argument("--max_examples", type=int, default=8, help="Examples to print per field type")
    args = ap.parse_args()

    docx = args.docx
    if not docx.exists():
        raise FileNotFoundError(docx)

    by_part: dict[str, list[str]] = {}
    type_counts: Counter[str] = Counter()
    examples: dict[str, list[str]] = defaultdict(list)

    with zipfile.ZipFile(docx, "r") as z:
        for part in _iter_word_xml_parts(z):
            try:
                instrs = extract_field_instructions(z.read(part))
            except Exception:
                continue
            if not instrs:
                continue
            by_part[part] = instrs
            for instr in instrs:
                ft = _field_type(instr)
                type_counts[ft] += 1
                if len(examples[ft]) < args.max_examples:
                    examples[ft].append(instr)

    if not by_part:
        print("No fields found in document.xml/headers/footers.")
        return

    print("FIELD COUNTS")
    for ft, n in type_counts.most_common():
        print(f"- {ft}: {n}")

    print("\nEXAMPLES")
    for ft, _n in type_counts.most_common():
        ex = examples.get(ft, [])
        if not ex:
            continue
        print(f"\n[{ft}]")
        for s in ex:
            print(f"  - {s}")

    print("\nPER-PART DETAILS")
    for part, instrs in sorted(by_part.items()):
        print(f"\n== {part} ==")
        for s in instrs:
            print(f"- {s}")

    # Quick reminders
    needs_update = any(ft in type_counts for ft in ("TOC", "REF", "PAGEREF", "NUMPAGES", "PAGE"))
    if needs_update:
        print("\nREMINDER")
        print(
            "- If the PDF/PNGs show placeholders or wrong page numbers/TOC, open in Word and run: Ctrl+A → F9 (Update Fields), then re-render."
        )


if __name__ == "__main__":
    main()
