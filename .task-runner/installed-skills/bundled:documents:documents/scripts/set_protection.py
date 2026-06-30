#!/usr/bin/env python3
"""Set or clear Word document protection flags (restrict editing).

In OOXML, the setting lives in:
  word/settings.xml :: <w:documentProtection .../>

We support simple, non-password protection modes:
  - off
  - readOnly
  - comments
  - trackedChanges
  - forms

This is high ROI for templating and review workflows.

Usage
-----
python scripts/set_protection.py in.docx --mode readOnly --out out.docx
python scripts/set_protection.py in.docx --mode off --out out.docx
"""

from __future__ import annotations

import argparse
import zipfile

from lxml import etree

W_NS = "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
CT_NS = "http://schemas.openxmlformats.org/package/2006/content-types"

NS = {"w": W_NS, "ct": CT_NS}

SETTINGS_CT = "application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml"


def _read_xml(z: zipfile.ZipFile, name: str) -> etree._Element:
    return etree.fromstring(z.read(name))


def _xml_bytes(root: etree._Element) -> bytes:
    return etree.tostring(root, xml_declaration=True, encoding="UTF-8", standalone="yes")


def _ensure_settings_part(z: zipfile.ZipFile) -> etree._Element:
    if "word/settings.xml" in z.namelist():
        return _read_xml(z, "word/settings.xml")
    # minimal settings
    root = etree.Element(f"{{{W_NS}}}settings", nsmap={"w": W_NS})
    return root


def _ensure_settings_override(ct_root: etree._Element) -> bool:
    # Ensure [Content_Types].xml has Override for settings.xml
    for ov in ct_root.findall(f"{{{CT_NS}}}Override"):
        if ov.get("PartName") == "/word/settings.xml":
            if ov.get("ContentType") != SETTINGS_CT:
                ov.set("ContentType", SETTINGS_CT)
                return True
            return False
    ov = etree.SubElement(ct_root, f"{{{CT_NS}}}Override")
    ov.set("PartName", "/word/settings.xml")
    ov.set("ContentType", SETTINGS_CT)
    return True


def set_protection(settings_root: etree._Element, mode: str) -> bool:
    changed = False
    # remove any existing
    for el in list(settings_root.xpath(".//w:documentProtection", namespaces=NS)):
        el.getparent().remove(el)
        changed = True

    if mode == "off":
        return changed

    # Insert near top (Word doesn't care much)
    dp = etree.Element(f"{{{W_NS}}}documentProtection")
    # required-ish flags; enforce=1 is important
    dp.set(f"{{{W_NS}}}edit", mode)
    dp.set(f"{{{W_NS}}}enforcement", "1")
    # don't lock formatting by default
    dp.set(f"{{{W_NS}}}formatting", "0")
    # Place as first child for readability
    settings_root.insert(0, dp)
    return True


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("in_docx")
    ap.add_argument(
        "--mode",
        required=True,
        help="Protection mode: off | readOnly | comments | trackedChanges | forms (aliases accepted: read_only, read-only, readonly, tracked_changes, tracked-changes)",
    )
    ap.add_argument("--out", required=True)
    args = ap.parse_args()

    with zipfile.ZipFile(args.in_docx, "r") as zin:
        settings_root = _ensure_settings_part(zin)
        ct_root = _read_xml(zin, "[Content_Types].xml")

        # Accept ergonomic aliases (users naturally type read_only, tracked_changes, etc.).
        mode_in = str(args.mode).strip()
        norm = mode_in.replace("-", "_")
        norm = "_".join([p for p in norm.split() if p])
        key = norm.lower()
        mode_map = {
            "off": "off",
            "readonly": "readOnly",
            "read_only": "readOnly",
            "comments": "comments",
            "trackedchanges": "trackedChanges",
            "tracked_changes": "trackedChanges",
            "forms": "forms",
        }
        if key not in mode_map:
            raise SystemExit(
                f"[set_protection] invalid --mode={mode_in!r}. Use off, readOnly, comments, trackedChanges, forms (aliases: read_only, tracked_changes)."
            )
        canonical_mode = mode_map[key]

        changed_settings = set_protection(settings_root, canonical_mode)
        changed_ct = _ensure_settings_override(ct_root)

        overrides: dict[str, bytes] = {}
        if changed_settings:
            overrides["word/settings.xml"] = _xml_bytes(settings_root)
        if changed_ct:
            overrides["[Content_Types].xml"] = _xml_bytes(ct_root)

        with zipfile.ZipFile(args.out, "w", zipfile.ZIP_DEFLATED) as zout:
            existing = {i.filename for i in zin.infolist()}
            for info in zin.infolist():
                name = info.filename
                if name in overrides:
                    zout.writestr(name, overrides[name])
                else:
                    zout.writestr(name, zin.read(name))
            for name, data in overrides.items():
                if name not in existing:
                    zout.writestr(name, data)

    print(f"[OK] wrote {args.out} (mode={canonical_mode})")


if __name__ == "__main__":
    main()
