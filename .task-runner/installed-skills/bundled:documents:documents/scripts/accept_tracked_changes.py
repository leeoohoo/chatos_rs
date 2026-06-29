#!/usr/bin/env python3
"""Accept/reject tracked changes in a DOCX by patching OOXML.

This is a pragmatic helper for the common requirement:
  "Give me the final version with tracked changes accepted."

It rewrites revision wrapper elements in word/document.xml:
  - w:ins / w:del
  - w:moveTo / w:moveFrom

Modes
-----
- report: print counts (no output file)
- accept: keep insertions/moveTo, drop deletions/moveFrom
- reject: drop insertions/moveTo, keep deletions/moveFrom

Caveats
-------
- This is not a full fidelity Word revision engine. It aims for common cases.
- After running, always render and visually review.

Usage
-----
python scripts/accept_tracked_changes.py in.docx --mode report
python scripts/accept_tracked_changes.py in.docx --mode accept --out out.docx
python scripts/accept_tracked_changes.py in.docx --mode reject --out out.docx
"""

from __future__ import annotations

import argparse
import zipfile
from dataclasses import dataclass

from lxml import etree

W_NS = "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
NS = {"w": W_NS}


@dataclass
class Counts:
    ins: int = 0
    del_: int = 0
    moveto: int = 0
    movefrom: int = 0


def _read_xml(z: zipfile.ZipFile, name: str) -> etree._Element:
    return etree.fromstring(z.read(name))


def _xml_bytes(root: etree._Element) -> bytes:
    return etree.tostring(root, xml_declaration=True, encoding="UTF-8", standalone="yes")


def _unwrap(el: etree._Element) -> None:
    """Replace element with its children (preserving order)."""
    parent = el.getparent()
    if parent is None:
        return
    idx = parent.index(el)
    children = list(el)
    parent.remove(el)
    for i, c in enumerate(children):
        parent.insert(idx + i, c)


def count_revisions(doc_root: etree._Element) -> Counts:
    c = Counts(
        ins=len(doc_root.xpath(".//w:ins", namespaces=NS)),
        del_=len(doc_root.xpath(".//w:del", namespaces=NS)),
        moveto=len(doc_root.xpath(".//w:moveTo", namespaces=NS)),
        movefrom=len(doc_root.xpath(".//w:moveFrom", namespaces=NS)),
    )
    return c


def apply_mode(doc_root: etree._Element, mode: str) -> None:
    """Mutate doc_root in-place."""
    # We must process deepest-first to avoid invalidating iterators.
    for tag, action in [
        ("moveTo", "ins"),
        ("moveFrom", "del"),
        ("ins", "ins"),
        ("del", "del"),
    ]:
        els = doc_root.xpath(f".//w:{tag}", namespaces=NS)
        # reverse document order
        for el in reversed(els):
            if action == "ins":
                if mode == "accept":
                    _unwrap(el)
                elif mode == "reject":
                    # drop entirely
                    parent = el.getparent()
                    if parent is not None:
                        parent.remove(el)
            else:  # del
                if mode == "accept":
                    parent = el.getparent()
                    if parent is not None:
                        parent.remove(el)
                elif mode == "reject":
                    _unwrap(el)


def disable_track_revisions(settings_root: etree._Element) -> bool:
    changed = False
    for el in settings_root.xpath(".//w:trackRevisions", namespaces=NS):
        el.getparent().remove(el)
        changed = True
    return changed


def write_out(
    src_docx: str, out_docx: str, doc_xml_bytes: bytes, settings_xml_bytes: bytes | None
) -> None:
    with (
        zipfile.ZipFile(src_docx, "r") as zin,
        zipfile.ZipFile(out_docx, "w", zipfile.ZIP_DEFLATED) as zout,
    ):
        for info in zin.infolist():
            name = info.filename
            if name == "word/document.xml":
                zout.writestr(name, doc_xml_bytes)
            elif settings_xml_bytes is not None and name == "word/settings.xml":
                zout.writestr(name, settings_xml_bytes)
            else:
                zout.writestr(name, zin.read(name))


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("in_docx")
    ap.add_argument("--mode", choices=["report", "accept", "reject"], required=True)
    ap.add_argument("--out", help="Output DOCX (required for accept/reject)")
    ap.add_argument(
        "--keep_tracking_on",
        action="store_true",
        help="Do not remove trackRevisions from settings.xml",
    )
    args = ap.parse_args()

    with zipfile.ZipFile(args.in_docx, "r") as z:
        doc_root = _read_xml(z, "word/document.xml")
        counts_before = count_revisions(doc_root)
        settings_root = None
        if "word/settings.xml" in z.namelist():
            settings_root = _read_xml(z, "word/settings.xml")

    print(
        f"[report] ins={counts_before.ins} del={counts_before.del_} moveTo={counts_before.moveto} moveFrom={counts_before.movefrom}"
    )

    if args.mode == "report":
        return

    if not args.out:
        raise SystemExit("--out is required for accept/reject")

    apply_mode(doc_root, args.mode)

    settings_bytes = None
    if settings_root is not None and not args.keep_tracking_on:
        if disable_track_revisions(settings_root):
            settings_bytes = _xml_bytes(settings_root)

    # Re-count
    counts_after = count_revisions(doc_root)
    print(
        f"[after]  ins={counts_after.ins} del={counts_after.del_} moveTo={counts_after.moveto} moveFrom={counts_after.movefrom}"
    )

    write_out(args.in_docx, args.out, _xml_bytes(doc_root), settings_bytes)
    print(f"[OK] wrote {args.out}")


if __name__ == "__main__":
    main()
