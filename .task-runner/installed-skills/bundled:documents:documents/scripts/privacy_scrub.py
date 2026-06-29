#!/usr/bin/env python3
"""Remove common personal metadata from a DOCX.

What it does
------------
- Clears core properties in docProps/core.xml:
  - dc:creator
  - cp:lastModifiedBy
- Removes docProps/custom.xml (custom properties) if present
- Removes relationship entries to custom properties if present
- Removes [Content_Types].xml overrides for removed parts
- Strips rsid* attributes from story parts (document, headers, footers, footnotes, endnotes)

This does not redact in-document PII; use redact_docx.py for that.

Usage
-----
python scripts/privacy_scrub.py in.docx --out out.docx
"""

from __future__ import annotations

import argparse
import re
import zipfile

from lxml import etree

W_NS = "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
REL_NS = "http://schemas.openxmlformats.org/package/2006/relationships"
CT_NS = "http://schemas.openxmlformats.org/package/2006/content-types"

CP_NS = "http://schemas.openxmlformats.org/package/2006/metadata/core-properties"
DC_NS = "http://purl.org/dc/elements/1.1/"

NS = {"w": W_NS, "rel": REL_NS, "ct": CT_NS, "cp": CP_NS, "dc": DC_NS}


def _read_xml(z: zipfile.ZipFile, name: str) -> etree._Element:
    return etree.fromstring(z.read(name))


def _xml_bytes(root: etree._Element) -> bytes:
    return etree.tostring(root, xml_declaration=True, encoding="UTF-8", standalone="yes")


def _iter_story_parts(z: zipfile.ZipFile) -> list[str]:
    parts = []
    for name in z.namelist():
        if name == "word/document.xml":
            parts.append(name)
        elif re.match(r"word/header\d+\.xml$", name) or re.match(r"word/footer\d+\.xml$", name):
            parts.append(name)
        elif name in ("word/footnotes.xml", "word/endnotes.xml"):
            parts.append(name)
    return parts


def _strip_rsid_attrs(root: etree._Element) -> int:
    removed = 0
    for el in root.iter():
        # Remove attributes like w:rsidR, w:rsidRDefault, w:rsidP, w:rsidDel, etc.
        to_del = []
        for k in el.attrib.keys():
            if k.startswith(f"{{{W_NS}}}rsid"):
                to_del.append(k)
        for k in to_del:
            del el.attrib[k]
            removed += 1
    return removed


def _scrub_core_props(core_root: etree._Element) -> bool:
    changed = False
    # dc:creator
    for el in core_root.xpath(".//dc:creator", namespaces=NS):
        if (el.text or "").strip() != "":
            el.text = ""
            changed = True
    # cp:lastModifiedBy
    for el in core_root.xpath(".//cp:lastModifiedBy", namespaces=NS):
        if (el.text or "").strip() != "":
            el.text = ""
            changed = True
    return changed


def _remove_custom_rels(rels_root: etree._Element) -> bool:
    changed = False
    for rel in list(rels_root.findall(f"{{{REL_NS}}}Relationship")):
        tgt = rel.get("Target") or ""
        # docProps/custom.xml is usually targeted as "../docProps/custom.xml" from package rels
        if tgt.endswith("docProps/custom.xml"):
            rels_root.remove(rel)
            changed = True
    return changed


def _remove_custom_ct_override(ct_root: etree._Element) -> bool:
    changed = False
    for ov in list(ct_root.findall(f"{{{CT_NS}}}Override")):
        if (ov.get("PartName") or "") == "/docProps/custom.xml":
            ct_root.remove(ov)
            changed = True
    return changed


def scrub(in_docx: str, out_docx: str) -> dict[str, int]:
    stats = {
        "rsid_attrs_removed": 0,
        "core_props_scrubbed": 0,
        "custom_props_removed": 0,
        "rels_updated": 0,
        "content_types_updated": 0,
    }

    with zipfile.ZipFile(in_docx, "r") as zin:
        overrides: dict[str, bytes] = {}

        # Story parts: strip rsid attributes
        for part in _iter_story_parts(zin):
            root = _read_xml(zin, part)
            n = _strip_rsid_attrs(root)
            if n:
                stats["rsid_attrs_removed"] += n
                overrides[part] = _xml_bytes(root)

        # Core props
        if "docProps/core.xml" in zin.namelist():
            core_root = _read_xml(zin, "docProps/core.xml")
            if _scrub_core_props(core_root):
                overrides["docProps/core.xml"] = _xml_bytes(core_root)
                stats["core_props_scrubbed"] = 1

        # Package rels may reference custom props
        pkg_rels = "_rels/.rels"
        if pkg_rels in zin.namelist():
            rels_root = _read_xml(zin, pkg_rels)
            if _remove_custom_rels(rels_root):
                overrides[pkg_rels] = _xml_bytes(rels_root)
                stats["rels_updated"] = 1

        # Content types override
        ct_path = "[Content_Types].xml"
        if ct_path in zin.namelist():
            ct_root = _read_xml(zin, ct_path)
            if _remove_custom_ct_override(ct_root):
                overrides[ct_path] = _xml_bytes(ct_root)
                stats["content_types_updated"] = 1

        with zipfile.ZipFile(out_docx, "w", zipfile.ZIP_DEFLATED) as zout:
            for info in zin.infolist():
                name = info.filename
                if name == "docProps/custom.xml":
                    stats["custom_props_removed"] = 1
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

    stats = scrub(args.in_docx, args.out)
    print(f"[OK] wrote {args.out} | {stats}")


if __name__ == "__main__":
    main()
