#!/usr/bin/env python3
"""Audit images in a DOCX (inline vs floating, size, relationship targets).

Why this exists
---------------
Image placement is the #1 Word-vs-LibreOffice mismatch. A fast audit helps
answer "do we have floating/anchored figures" and "what size are the images".

This script:
- Scans document.xml + headers + footers for drawings
- Detects inline (wp:inline) vs floating (wp:anchor)
- Extracts the embed relationship id (r:embed) and resolves it to the image file
- Prints sizes (in inches) from the OOXML extents

Notes
-----
- python-docx mostly creates inline images; floating/anchored images usually come from Word UI.
- This script is best-effort; some drawings (charts, SmartArt) use different parts.
"""

from __future__ import annotations

import argparse
import zipfile
from collections import Counter
from pathlib import Path

from lxml import etree

W_NS = "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
R_NS = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
WP_NS = "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing"
A_NS = "http://schemas.openxmlformats.org/drawingml/2006/main"

REL_NS = "http://schemas.openxmlformats.org/package/2006/relationships"

NS = {
    "w": W_NS,
    "r": R_NS,
    "wp": WP_NS,
    "a": A_NS,
    "rel": REL_NS,
}

EMU_PER_INCH = 914400


def _iter_content_parts(z: zipfile.ZipFile):
    for name in z.namelist():
        if not name.startswith("word/"):
            continue
        if name.endswith("document.xml"):
            yield name
        base = name.rsplit("/", 1)[-1]
        if base.startswith("header") and base.endswith(".xml"):
            yield name
        if base.startswith("footer") and base.endswith(".xml"):
            yield name


def _rels_path_for_part(part_name: str) -> str:
    # e.g. word/document.xml -> word/_rels/document.xml.rels
    base = part_name.rsplit("/", 1)[-1]
    return f"word/_rels/{base}.rels"


def _load_rels_map(z: zipfile.ZipFile, part_name: str) -> dict[str, str]:
    rels_name = _rels_path_for_part(part_name)
    if rels_name not in z.namelist():
        return {}
    root = etree.fromstring(z.read(rels_name))
    out: dict[str, str] = {}
    for rel in root.findall("rel:Relationship", namespaces=NS):
        rid = rel.get("Id")
        tgt = rel.get("Target")
        if rid and tgt:
            out[rid] = tgt
    return out


def _inches_from_emu(v: str | None) -> float | None:
    if not v:
        return None
    try:
        return float(int(v)) / EMU_PER_INCH
    except Exception:
        return None


def main() -> None:
    ap = argparse.ArgumentParser(description="Audit images in a DOCX (inline vs floating, sizes)")
    ap.add_argument("docx", type=Path)
    ap.add_argument("--max_rows", type=int, default=50)
    args = ap.parse_args()

    if not args.docx.exists():
        raise FileNotFoundError(args.docx)

    rows = []
    kind_counts: Counter[str] = Counter()

    with zipfile.ZipFile(args.docx, "r") as z:
        for part in _iter_content_parts(z):
            rels = _load_rels_map(z, part)
            parser = etree.XMLParser(recover=True)
            root = etree.fromstring(z.read(part), parser=parser)

            # Find both inline and anchor drawings
            for kind, xpath in (
                ("inline", ".//wp:inline"),
                ("anchor", ".//wp:anchor"),
            ):
                for el in root.findall(xpath, namespaces=NS):
                    kind_counts[kind] += 1

                    # Size extents: wp:extent cx/cy
                    ext = el.find("wp:extent", namespaces=NS)
                    w_in = _inches_from_emu(ext.get("cx") if ext is not None else None)
                    h_in = _inches_from_emu(ext.get("cy") if ext is not None else None)

                    # Relationship id: a:blip r:embed
                    blip = el.find(".//a:blip", namespaces=NS)
                    rid = blip.get(f"{{{R_NS}}}embed") if blip is not None else None
                    tgt = rels.get(rid, "") if rid else ""

                    img_zip_path = ""
                    img_bytes = None
                    if tgt and tgt.startswith("media/"):
                        img_zip_path = "word/" + tgt
                        if img_zip_path in z.namelist():
                            try:
                                img_bytes = z.read(img_zip_path)
                            except Exception:
                                img_bytes = None

                    rows.append(
                        {
                            "part": part,
                            "kind": kind,
                            "rId": rid or "",
                            "target": tgt,
                            "zip_path": img_zip_path,
                            "size_in": f"{w_in:.2f} x {h_in:.2f}" if w_in and h_in else "(unknown)",
                            "bytes": str(len(img_bytes)) if img_bytes is not None else "",
                        }
                    )

    if not rows:
        print("No inline/anchored drawings found in document/header/footer parts.")
        return

    print("IMAGE KINDS")
    for k, n in kind_counts.most_common():
        print(f"- {k}: {n}")

    print("\nROWS (part | kind | size | target)")
    for r in rows[: args.max_rows]:
        print(f"- {r['part']} | {r['kind']} | {r['size_in']} | {r['target']}")
    if len(rows) > args.max_rows:
        print(f"- ... ({len(rows) - args.max_rows} more)")

    if kind_counts.get("anchor"):
        print("\nWARNING")
        print(
            "- Floating/anchored images detected (wp:anchor). These are the most common Word-vs-LO mismatch."
        )
        print("  Strongly recommend: render to PNG and check placement on every affected page.")


if __name__ == "__main__":
    main()
