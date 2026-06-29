#!/usr/bin/env python3
"""Report footnotes/endnotes usage in a DOCX.

True footnotes/endnotes are NOT "text in footer". They live in separate OOXML
parts:
  - word/footnotes.xml
  - word/endnotes.xml

and are referenced from the body via:
  - w:footnoteReference w:id="..."
  - w:endnoteReference w:id="..."

This reporter scans for:
  - presence of footnotes.xml/endnotes.xml
  - counts of referenced IDs and defined IDs
  - first few snippets of note text

Usage
-----
python scripts/footnotes_report.py in.docx
"""

from __future__ import annotations

import zipfile
from collections import Counter

from lxml import etree

W_NS = "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
NS = {"w": W_NS}


def _read(z: zipfile.ZipFile, name: str) -> etree._Element:
    return etree.fromstring(z.read(name))


def _note_text(note_el: etree._Element) -> str:
    texts = [t.text for t in note_el.xpath(".//w:t", namespaces=NS) if t.text]
    s = "".join(texts).strip()
    return s


def _report_part(z: zipfile.ZipFile, part: str, kind: str) -> None:
    root = _read(z, part)
    # Footnotes/endnotes define separators with id -1,0; real notes are >=1
    notes = root.xpath(f".//w:{kind}[number(@w:id) >= 1]", namespaces=NS)
    ids = [int(n.get(f"{{{W_NS}}}id")) for n in notes]
    print(
        f"[{part}] defined_{kind}s={len(ids)} ids={sorted(ids)[:10]}{'...' if len(ids) > 10 else ''}"
    )
    for n in notes[:5]:
        nid = n.get(f"{{{W_NS}}}id")
        txt = _note_text(n)
        if txt:
            print(f"  - id={nid}: {txt[:120]}")


def main() -> None:
    import argparse

    ap = argparse.ArgumentParser()
    ap.add_argument("docx")
    args = ap.parse_args()

    with zipfile.ZipFile(args.docx, "r") as z:
        doc = _read(z, "word/document.xml")

        fn_refs = [
            int(x.get(f"{{{W_NS}}}id")) for x in doc.xpath(".//w:footnoteReference", namespaces=NS)
        ]
        en_refs = [
            int(x.get(f"{{{W_NS}}}id")) for x in doc.xpath(".//w:endnoteReference", namespaces=NS)
        ]

        print(f"[document.xml] footnoteReferences={len(fn_refs)} ids={sorted(set(fn_refs))}")
        print(f"[document.xml] endnoteReferences={len(en_refs)} ids={sorted(set(en_refs))}")

        if fn_refs:
            c = Counter(fn_refs)
            dupes = [k for k, v in c.items() if v > 1]
            if dupes:
                print(
                    f"[warn] duplicated footnote reference ids (multiple references to same note): {dupes}"
                )

        if "word/footnotes.xml" in z.namelist():
            _report_part(z, "word/footnotes.xml", "footnote")
        else:
            print("[word/footnotes.xml] MISSING")

        if "word/endnotes.xml" in z.namelist():
            _report_part(z, "word/endnotes.xml", "endnote")
        else:
            print("[word/endnotes.xml] MISSING")


if __name__ == "__main__":
    main()
