#!/usr/bin/env python3
"""Apply a template/style pack (DOTX or DOCX) onto a target DOCX.

Goal: make it easy to "start from template" or retrofit a style pack without
manual re-styling.

What it does (minimal, high ROI)
--------------------------------
- Copies key parts from the template into the target:
  - word/styles.xml
  - word/theme/theme1.xml
  - word/fontTable.xml (if present)
  - word/numbering.xml (if present)

It also ensures [Content_Types].xml has the required Overrides for any newly
added parts.

Usage
-----
python scripts/apply_template_styles.py --template template.dotx --target report.docx --out styled.docx

Caveats
-------
- This can change pagination/layout. Always render and inspect PNGs.
- If the target uses custom styles with the same IDs, they will be overwritten.
"""

from __future__ import annotations

import argparse
import zipfile

from lxml import etree

CT_NS = "http://schemas.openxmlformats.org/package/2006/content-types"


def _read(z: zipfile.ZipFile, name: str) -> bytes:
    return z.read(name)


def _has(z: zipfile.ZipFile, name: str) -> bool:
    return name in z.namelist()


def _ensure_override(ct_root: etree._Element, part_name: str, content_type: str) -> bool:
    changed = False
    # Normalize PartName to start with '/'
    if not part_name.startswith("/"):
        part_name = "/" + part_name

    # If override exists, update contentType if needed
    for ov in ct_root.findall(f"{{{CT_NS}}}Override"):
        if ov.get("PartName") == part_name:
            if ov.get("ContentType") != content_type:
                ov.set("ContentType", content_type)
                changed = True
            return changed

    ov = etree.SubElement(ct_root, f"{{{CT_NS}}}Override")
    ov.set("PartName", part_name)
    ov.set("ContentType", content_type)
    return True


def apply(template_path: str, target_path: str, out_path: str) -> None:
    parts = [
        ("word/styles.xml", None),
        (
            "word/theme/theme1.xml",
            "application/vnd.openxmlformats-officedocument.theme+xml",
        ),
        (
            "word/fontTable.xml",
            "application/vnd.openxmlformats-officedocument.wordprocessingml.fontTable+xml",
        ),
        (
            "word/numbering.xml",
            "application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml",
        ),
    ]

    with (
        zipfile.ZipFile(template_path, "r") as zt,
        zipfile.ZipFile(target_path, "r") as zg,
    ):
        overrides = {}
        for name, _ct in parts:
            if _has(zt, name):
                overrides[name] = _read(zt, name)

        # Update content types in target if we add/override optional parts
        ct_bytes = _read(zg, "[Content_Types].xml")
        ct_root = etree.fromstring(ct_bytes)
        ct_changed = False

        for name, ctype in parts:
            if name in overrides and ctype:
                ct_changed |= _ensure_override(ct_root, name, ctype)

        if ct_changed:
            overrides["[Content_Types].xml"] = etree.tostring(
                ct_root, xml_declaration=True, encoding="UTF-8", standalone="yes"
            )

        with zipfile.ZipFile(out_path, "w", zipfile.ZIP_DEFLATED) as zout:
            for info in zg.infolist():
                name = info.filename
                if name in overrides:
                    zout.writestr(name, overrides[name])
                else:
                    zout.writestr(name, zg.read(name))
            # Add new parts not present in target
            for name, data in overrides.items():
                if name not in {i.filename for i in zg.infolist()}:
                    zout.writestr(name, data)


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--template", required=True)
    ap.add_argument("--target", required=True)
    ap.add_argument("--out", required=True)
    args = ap.parse_args()

    apply(args.template, args.target, args.out)
    print(f"[OK] wrote {args.out}")


if __name__ == "__main__":
    main()
