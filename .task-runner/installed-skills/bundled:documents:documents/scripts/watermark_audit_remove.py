#!/usr/bin/env python3
"""Audit and (heuristically) remove watermark-like background elements.

In Word, watermarks are often implemented as VML shapes in headers:
  w:hdr -> w:p -> w:r -> w:pict -> v:shape -> v:textpath string="DRAFT"

python-docx doesn't expose these well; LO vs Word rendering can differ; therefore
this helper focuses on OOXML inspection/removal with mandatory render + diff.

Modes
-----
- report: list watermark-like occurrences (VML or DrawingML) across document+headers+footers
- remove: delete matched objects whose embedded watermark text contains a substring

Usage
-----
python scripts/watermark_audit_remove.py in.docx --mode report
python scripts/watermark_audit_remove.py in.docx --mode remove --contains DRAFT --out out.docx
"""

from __future__ import annotations

import argparse
import re
import zipfile

from lxml import etree

W_NS = "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
V_NS = "urn:schemas-microsoft-com:vml"
R_NS = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"

NS = {"w": W_NS, "v": V_NS, "r": R_NS}


def _xml_bytes(root: etree._Element) -> bytes:
    return etree.tostring(root, xml_declaration=True, encoding="UTF-8", standalone="yes")


def _read_xml(z: zipfile.ZipFile, name: str) -> etree._Element:
    return etree.fromstring(z.read(name))


def _find_doc_parts(z: zipfile.ZipFile) -> list[str]:
    parts = ["word/document.xml"]
    for name in z.namelist():
        if re.fullmatch(r"word/header\d+\.xml", name) or re.fullmatch(r"word/footer\d+\.xml", name):
            parts.append(name)
    return sorted(set(parts))


def _scan_part(root: etree._Element) -> list[dict]:
    hits = []

    # VML watermark-like textpath
    for tp in root.xpath(".//v:textpath", namespaces=NS):
        s = tp.get("string") or ""
        if s.strip():
            hits.append({"kind": "vml_textpath", "text": s.strip()})

    # Fallback: any element with 'watermark' in attributes
    for el in root.iter():
        for v in el.attrib.values():
            if isinstance(v, str) and "watermark" in v.lower():
                hits.append({"kind": "attr_watermark", "text": v})
                break

    return hits


def report(docx: str) -> int:
    with zipfile.ZipFile(docx, "r") as z:
        parts = _find_doc_parts(z)
        total = 0
        for part in parts:
            root = _read_xml(z, part)
            hits = _scan_part(root)
            if hits:
                print(f"[{part}]")
                for h in hits[:50]:
                    print(f"  - {h['kind']}: {h['text']}")
                total += len(hits)
        print(f"[summary] watermark_like_hits={total}")
        return total


def remove(docx: str, out: str, contains: str) -> None:
    contains_l = contains.lower()
    with zipfile.ZipFile(docx, "r") as zin:
        parts = _find_doc_parts(zin)
        overrides: dict[str, bytes] = {}

        for part in parts:
            root = _read_xml(zin, part)
            changed = False

            # Remove any w:pict that contains a v:textpath string matching substring
            for pict in root.xpath(".//w:pict", namespaces=NS):
                tps = pict.xpath(".//v:textpath", namespaces=NS)
                hit = False
                for tp in tps:
                    s = (tp.get("string") or "").lower()
                    if contains_l in s:
                        hit = True
                        break
                if hit:
                    parent = pict.getparent()
                    if parent is not None:
                        parent.remove(pict)
                        changed = True

            if changed:
                overrides[part] = _xml_bytes(root)

        with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as zout:
            for info in zin.infolist():
                name = info.filename
                if name in overrides:
                    zout.writestr(name, overrides[name])
                else:
                    zout.writestr(name, zin.read(name))

    print(f"[OK] wrote {out}")


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("in_docx")
    ap.add_argument("--mode", choices=["report", "remove"], required=True)
    ap.add_argument("--contains", help="Substring to match for removal")
    ap.add_argument("--out", help="Output docx for remove")
    args = ap.parse_args()

    if args.mode == "report":
        report(args.in_docx)
        return

    if not args.contains or not args.out:
        ap.error("--contains and --out are required when --mode remove")

    remove(args.in_docx, args.out, args.contains)


if __name__ == "__main__":
    main()
