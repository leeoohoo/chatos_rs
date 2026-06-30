#!/usr/bin/env python3
"""Flatten REF/PAGEREF fields to literal text runs.

LibreOffice headless rendering can sometimes omit REF field results even when
Word displays them correctly. For screenshot-based regression testing, it can
be helpful to replace the field construct with its current cached visible text.

This script finds field sequences of the form:
  <w:fldChar w:fldCharType="begin"/>
  ... <w:instrText>REF ...
  <w:fldChar w:fldCharType="separate"/>
  ... <w:t>VISIBLE RESULT</w:t>
  <w:fldChar w:fldCharType="end"/>
and replaces the entire block with a single run containing "VISIBLE RESULT".

Scope: best-effort on paragraph-contained fields.

Usage
-----
python scripts/flatten_ref_fields.py in.docx --out out.docx
"""

from __future__ import annotations

import argparse
import re
import zipfile

from lxml import etree

W_NS = "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
NS = {"w": W_NS}


def w(tag: str) -> str:
    return f"{{{W_NS}}}{tag}"


def _xml_bytes(root: etree._Element) -> bytes:
    return etree.tostring(root, xml_declaration=True, encoding="UTF-8", standalone="yes")


def _is_begin(el: etree._Element) -> bool:
    return el.tag == w("fldChar") and el.get(w("fldCharType")) == "begin"


def _is_separate(el: etree._Element) -> bool:
    return el.tag == w("fldChar") and el.get(w("fldCharType")) == "separate"


def _is_end(el: etree._Element) -> bool:
    return el.tag == w("fldChar") and el.get(w("fldCharType")) == "end"


def _instr_text(p: etree._Element, start_idx: int, end_idx: int) -> str:
    txt = []
    for r in p[start_idx:end_idx]:
        for it in r.xpath(".//w:instrText/text()", namespaces=NS):
            txt.append(it)
    return "".join(txt)


def _visible_text(p: etree._Element, start_idx: int, end_idx: int) -> str:
    txt = []
    for r in p[start_idx:end_idx]:
        for t in r.xpath(".//w:t/text()", namespaces=NS):
            txt.append(t)
    return "".join(txt)


def flatten_part(root: etree._Element) -> int:
    changed = 0
    for p in root.xpath(".//w:p", namespaces=NS):
        kids = list(p)
        i = 0
        while i < len(kids):
            # Look for run containing fldChar begin.
            r = kids[i]
            fld = r.find(".//w:fldChar", namespaces=NS)
            if fld is None or not _is_begin(fld):
                i += 1
                continue

            # Find the end run containing fldChar end.
            j = i + 1
            separate_idx = None
            end_idx = None
            while j < len(kids):
                fldj = kids[j].find(".//w:fldChar", namespaces=NS)
                if fldj is not None and _is_separate(fldj):
                    separate_idx = j
                if fldj is not None and _is_end(fldj):
                    end_idx = j
                    break
                j += 1
            if end_idx is None or separate_idx is None:
                i += 1
                continue

            instr = _instr_text(p, i, separate_idx + 1)
            if not re.search(r"\b(PAGE)?REF\b", instr, flags=re.IGNORECASE):
                i += 1
                continue

            visible = _visible_text(p, separate_idx + 1, end_idx)
            if not visible.strip():
                i = end_idx + 1
                continue

            # Replace i..end_idx with a single run of visible text.
            new_r = etree.Element(w("r"))
            t = etree.SubElement(new_r, w("t"))
            t.text = visible

            # Insert before i, then remove old nodes.
            p.insert(i, new_r)
            for k in range(i + 1, end_idx + 2):
                try:
                    p.remove(kids[k])
                except Exception:
                    pass

            changed += 1
            kids = list(p)
            i += 1

    return changed


def main() -> None:
    ap = argparse.ArgumentParser(description="Flatten REF/PAGEREF fields to literal text")
    ap.add_argument("in_docx")
    ap.add_argument("--out", required=True)
    args = ap.parse_args()

    with zipfile.ZipFile(args.in_docx, "r") as zin:
        overrides: dict[str, bytes] = {}
        root = etree.fromstring(zin.read("word/document.xml"))
        n = flatten_part(root)
        if n:
            overrides["word/document.xml"] = _xml_bytes(root)
        with zipfile.ZipFile(args.out, "w", zipfile.ZIP_DEFLATED) as zout:
            for info in zin.infolist():
                name = info.filename
                if name in overrides:
                    zout.writestr(name, overrides[name])
                else:
                    zout.writestr(name, zin.read(name))

    print(f"[OK] wrote {args.out} (fields_flattened={n})")


if __name__ == "__main__":
    main()
