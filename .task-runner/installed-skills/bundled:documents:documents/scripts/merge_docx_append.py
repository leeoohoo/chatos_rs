#!/usr/bin/env python3
"""Append the body of one DOCX to another by splicing OOXML.

Why this exists
---------------
"Text-only" merges (copying paragraph text) lose most formatting and structure.
Splicing the OOXML body preserves much more of the original content.

Scope & safety
--------------
- Only merges `word/document.xml` body children.
- Keeps the base document's final `w:sectPr` (section properties).
- Refuses drawings/images by default because relationships & binary parts are
  not merged. Use `--allow_drawings` only if you know both documents are
  compatible and you accept best-effort results.

Usage
-----
python scripts/merge_docx_append.py base.docx append.docx --out merged.docx
python scripts/merge_docx_append.py base.docx append.docx --out merged.docx --allow_drawings
"""

from __future__ import annotations

import argparse
import zipfile

from lxml import etree

W_NS = "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
NS = {"w": W_NS}


def _read_xml(z: zipfile.ZipFile, name: str) -> etree._Element:
    return etree.fromstring(z.read(name))


def _xml_bytes(root: etree._Element) -> bytes:
    return etree.tostring(root, xml_declaration=True, encoding="UTF-8", standalone="yes")


def _has_drawings(body: etree._Element) -> bool:
    return bool(body.xpath(".//w:drawing | .//w:pict", namespaces=NS))


def merge(base_docx: str, append_docx: str, out_docx: str, allow_drawings: bool) -> dict[str, int]:
    with zipfile.ZipFile(base_docx, "r") as zb, zipfile.ZipFile(append_docx, "r") as za:
        base_root = _read_xml(zb, "word/document.xml")
        append_root = _read_xml(za, "word/document.xml")

        base_body = base_root.find(f"{{{W_NS}}}body")
        append_body = append_root.find(f"{{{W_NS}}}body")
        if base_body is None or append_body is None:
            raise RuntimeError("Missing w:body in one of the documents")

        if not allow_drawings and _has_drawings(append_body):
            raise SystemExit(
                "append.docx contains drawings/images. "
                "Re-run with --allow_drawings if this is acceptable, "
                "or remove drawings first to keep the merge safer."
            )

        # Capture base sectPr (usually the last child of body)
        base_children = list(base_body)
        base_sectpr = None
        if base_children and base_children[-1].tag == f"{{{W_NS}}}sectPr":
            base_sectpr = base_children[-1]
            base_body.remove(base_sectpr)

        # Append all children from append body except its trailing sectPr
        append_children = list(append_body)
        if append_children and append_children[-1].tag == f"{{{W_NS}}}sectPr":
            append_children = append_children[:-1]

        inserted = 0
        for child in append_children:
            base_body.append(child)
            inserted += 1

        # Restore base sectPr at end (if any)
        if base_sectpr is not None:
            base_body.append(base_sectpr)

        new_document_xml = _xml_bytes(base_root)

        with zipfile.ZipFile(out_docx, "w", zipfile.ZIP_DEFLATED) as zout:
            for info in zb.infolist():
                name = info.filename
                if name == "word/document.xml":
                    zout.writestr(name, new_document_xml)
                else:
                    zout.writestr(name, zb.read(name))

    return {"body_children_appended": inserted}


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("base_docx")
    ap.add_argument("append_docx")
    ap.add_argument("--out", required=True)
    ap.add_argument("--allow_drawings", action="store_true")
    args = ap.parse_args()

    stats = merge(args.base_docx, args.append_docx, args.out, args.allow_drawings)
    print(f"[OK] wrote {args.out} | {stats}")


if __name__ == "__main__":
    main()
