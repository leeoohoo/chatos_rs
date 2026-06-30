#!/usr/bin/env python3
"""Remove all comments from a DOCX (ranges + parts).

What it does
------------
- Removes comment range markers and references from all story parts:
  - w:commentRangeStart / w:commentRangeEnd / w:commentReference
- Removes comment parts if present:
  - word/comments.xml
  - word/commentsExtended.xml (newer Word)
- Removes relationship entries pointing to those parts
- Removes [Content_Types].xml overrides for comment parts

This is intended for "final" deliverables where the user wants a clean `.docx`.

Usage
-----
python scripts/comments_strip.py in.docx --out out.docx
"""

from __future__ import annotations

import argparse
import re
import zipfile

from lxml import etree

W_NS = "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
REL_NS = "http://schemas.openxmlformats.org/package/2006/relationships"
CT_NS = "http://schemas.openxmlformats.org/package/2006/content-types"

NS = {"w": W_NS, "rel": REL_NS, "ct": CT_NS}

COMMENT_REL_TYPES = {
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments",
    "http://schemas.microsoft.com/office/2011/relationships/commentsExtended",
}


def _read_xml(z: zipfile.ZipFile, name: str) -> etree._Element:
    return etree.fromstring(z.read(name))


def _xml_bytes(root: etree._Element) -> bytes:
    return etree.tostring(root, xml_declaration=True, encoding="UTF-8", standalone="yes")


def _iter_story_parts(z: zipfile.ZipFile) -> list[str]:
    parts = ["word/document.xml"]
    for name in z.namelist():
        if re.match(r"word/header\d+\.xml$", name) or re.match(r"word/footer\d+\.xml$", name):
            parts.append(name)
    return parts


def _strip_comment_markup(root: etree._Element) -> int:
    removed = 0
    for tag in ["commentRangeStart", "commentRangeEnd", "commentReference"]:
        els = root.xpath(f".//w:{tag}", namespaces=NS)
        for el in reversed(els):
            parent = el.getparent()
            if parent is not None:
                parent.remove(el)
                removed += 1
    return removed


def _remove_comment_relationships(rels_root: etree._Element) -> bool:
    changed = False
    for rel in list(rels_root.findall(f"{{{REL_NS}}}Relationship")):
        if rel.get("Type") in COMMENT_REL_TYPES:
            rels_root.remove(rel)
            changed = True
    return changed


def _remove_ct_overrides(ct_root: etree._Element) -> bool:
    changed = False
    for ov in list(ct_root.findall(f"{{{CT_NS}}}Override")):
        pn = ov.get("PartName") or ""
        if pn in ("/word/comments.xml", "/word/commentsExtended.xml"):
            ct_root.remove(ov)
            changed = True
    return changed


def strip_comments(in_docx: str, out_docx: str) -> dict[str, int]:
    stats = {
        "markup_removed": 0,
        "comments_part_removed": 0,
        "comments_extended_part_removed": 0,
        "rels_updated": 0,
        "content_types_updated": 0,
    }

    with zipfile.ZipFile(in_docx, "r") as zin:
        overrides: dict[str, bytes] = {}

        for part in _iter_story_parts(zin):
            root = _read_xml(zin, part)
            n = _strip_comment_markup(root)
            if n:
                stats["markup_removed"] += n
                overrides[part] = _xml_bytes(root)

        # relationships
        rels_path = "word/_rels/document.xml.rels"
        if rels_path in zin.namelist():
            rels_root = _read_xml(zin, rels_path)
            if _remove_comment_relationships(rels_root):
                overrides[rels_path] = _xml_bytes(rels_root)
                stats["rels_updated"] = 1

        # content types
        ct_path = "[Content_Types].xml"
        if ct_path in zin.namelist():
            ct_root = _read_xml(zin, ct_path)
            if _remove_ct_overrides(ct_root):
                overrides[ct_path] = _xml_bytes(ct_root)
                stats["content_types_updated"] = 1

        with zipfile.ZipFile(out_docx, "w", zipfile.ZIP_DEFLATED) as zout:
            for info in zin.infolist():
                name = info.filename
                if name in ("word/comments.xml", "word/commentsExtended.xml"):
                    if name.endswith("comments.xml"):
                        stats["comments_part_removed"] = 1
                    else:
                        stats["comments_extended_part_removed"] = 1
                    continue
                if name in overrides:
                    zout.writestr(name, overrides[name])
                else:
                    zout.writestr(name, zin.read(name))

    return stats


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("in_docx")
    ap.add_argument("--out", required=True)
    args = ap.parse_args()

    stats = strip_comments(args.in_docx, args.out)
    print(f"[OK] wrote {args.out} | {stats}")


if __name__ == "__main__":
    main()
