#!/usr/bin/env python3
"""Insert a true footnote or endnote into a DOCX by patching OOXML.

Why
---
python-docx does not currently offer a stable high-level API for true footnotes
and endnotes. In OOXML, notes live in separate parts (footnotes.xml/endnotes.xml)
with references embedded in the main body.

This helper performs a minimal, high-ROI insertion:
- ensures word/footnotes.xml or word/endnotes.xml exists (with separators)
- inserts w:footnoteReference / w:endnoteReference at a marker in document.xml
- adds a new note entry in the part
- ensures document relationships and [Content_Types].xml overrides exist

Usage
-----
# Insert a footnote at marker [[FN]] in the doc
python scripts/insert_note.py in.docx --kind footnote --text "hello" --marker "[[FN]]" --out out.docx

# Insert an endnote
python scripts/insert_note.py in.docx --kind endnote --text "source" --marker "[[EN]]" --out out.docx

Always render and inspect PNGs after insertion.
"""

from __future__ import annotations

import argparse
import re
import zipfile

from lxml import etree

W_NS = "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
R_NS = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
PKGREL_NS = "http://schemas.openxmlformats.org/package/2006/relationships"
CT_NS = "http://schemas.openxmlformats.org/package/2006/content-types"

NS = {"w": W_NS, "r": R_NS, "rel": PKGREL_NS, "ct": CT_NS}

REL_TYPE_FOOTNOTES = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/footnotes"
REL_TYPE_ENDNOTES = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/endnotes"

CT_FOOTNOTES = "application/vnd.openxmlformats-officedocument.wordprocessingml.footnotes+xml"
CT_ENDNOTES = "application/vnd.openxmlformats-officedocument.wordprocessingml.endnotes+xml"


def _xml_bytes(root: etree._Element) -> bytes:
    return etree.tostring(root, xml_declaration=True, encoding="UTF-8", standalone="yes")


def _read_xml(z: zipfile.ZipFile, name: str) -> etree._Element:
    return etree.fromstring(z.read(name))


def _next_rid(rels_root: etree._Element) -> str:
    max_n = 0
    for rel in rels_root.findall(f"{{{PKGREL_NS}}}Relationship"):
        rid = rel.get("Id") or ""
        m = re.match(r"rId(\d+)$", rid)
        if m:
            max_n = max(max_n, int(m.group(1)))
    return f"rId{max_n + 1}"


def _ensure_note_relationship(rels_root: etree._Element, kind: str) -> bool:
    target = "footnotes.xml" if kind == "footnote" else "endnotes.xml"
    rel_type = REL_TYPE_FOOTNOTES if kind == "footnote" else REL_TYPE_ENDNOTES

    for rel in rels_root.findall(f"{{{PKGREL_NS}}}Relationship"):
        if rel.get("Type") == rel_type:
            return False

    rel = etree.SubElement(rels_root, f"{{{PKGREL_NS}}}Relationship")
    rel.set("Id", _next_rid(rels_root))
    rel.set("Type", rel_type)
    rel.set("Target", target)
    return True


def _ensure_override(ct_root: etree._Element, part_name: str, content_type: str) -> bool:
    if not part_name.startswith("/"):
        part_name = "/" + part_name

    for ov in ct_root.findall(f"{{{CT_NS}}}Override"):
        if ov.get("PartName") == part_name:
            if ov.get("ContentType") != content_type:
                ov.set("ContentType", content_type)
                return True
            return False

    ov = etree.SubElement(ct_root, f"{{{CT_NS}}}Override")
    ov.set("PartName", part_name)
    ov.set("ContentType", content_type)
    return True


def _make_empty_notes_part(kind: str) -> etree._Element:
    root = etree.Element(f"{{{W_NS}}}{kind}s", nsmap={"w": W_NS, "r": R_NS})

    # separators (-1 and 0) are expected by Word/LO
    def _sep(note_id: str, sep_type: str) -> None:
        note = etree.SubElement(root, f"{{{W_NS}}}{kind}")
        note.set(f"{{{W_NS}}}id", note_id)
        p = etree.SubElement(note, f"{{{W_NS}}}p")
        r = etree.SubElement(p, f"{{{W_NS}}}r")
        sep = etree.SubElement(r, f"{{{W_NS}}}{sep_type}")
        _ = sep

    _sep("-1", "separator")
    _sep("0", "continuationSeparator")
    return root


def _next_note_id(notes_root: etree._Element, kind: str) -> int:
    ids = []
    for n in notes_root.findall(f"{{{W_NS}}}{kind}"):
        v = n.get(f"{{{W_NS}}}id")
        try:
            iv = int(v) if v is not None else None
        except ValueError:
            iv = None
        if iv is not None and iv >= 1:
            ids.append(iv)
    return (max(ids) + 1) if ids else 1


def _append_note(notes_root: etree._Element, kind: str, note_id: int, text: str) -> None:
    note = etree.SubElement(notes_root, f"{{{W_NS}}}{kind}")
    note.set(f"{{{W_NS}}}id", str(note_id))

    p = etree.SubElement(note, f"{{{W_NS}}}p")
    r1 = etree.SubElement(p, f"{{{W_NS}}}r")
    tag = "endnoteRef" if kind == "endnote" else "footnoteRef"
    etree.SubElement(r1, f"{{{W_NS}}}{tag}")
    # Word often inserts a space after the reference
    r2 = etree.SubElement(p, f"{{{W_NS}}}r")
    t = etree.SubElement(r2, f"{{{W_NS}}}t")
    t.text = " " + text


def _insert_reference(doc_root: etree._Element, kind: str, marker: str, note_id: int) -> bool:
    ref_tag = "footnoteReference" if kind == "footnote" else "endnoteReference"

    for t in doc_root.xpath(".//w:t", namespaces=NS):
        if t.text and marker in t.text:
            # Remove marker text from this run
            t.text = t.text.replace(marker, "")
            # Insert reference run immediately after this run's parent <w:r>
            r = t.getparent()
            while r is not None and r.tag != f"{{{W_NS}}}r":
                r = r.getparent()
            if r is None:
                continue
            parent = r.getparent()
            idx = parent.index(r)
            new_r = etree.Element(f"{{{W_NS}}}r")
            ref = etree.SubElement(new_r, f"{{{W_NS}}}{ref_tag}")
            ref.set(f"{{{W_NS}}}id", str(note_id))
            parent.insert(idx + 1, new_r)
            return True

    return False


def insert_note(in_docx: str, out_docx: str, kind: str, marker: str, text: str) -> None:
    if kind not in ("footnote", "endnote"):
        raise ValueError("kind must be footnote or endnote")

    part_name = "word/footnotes.xml" if kind == "footnote" else "word/endnotes.xml"
    ct = CT_FOOTNOTES if kind == "footnote" else CT_ENDNOTES

    with zipfile.ZipFile(in_docx, "r") as zin:
        doc_root = _read_xml(zin, "word/document.xml")

        # notes part
        if part_name in zin.namelist():
            notes_root = _read_xml(zin, part_name)
        else:
            notes_root = _make_empty_notes_part(kind)

        new_id = _next_note_id(notes_root, kind)
        _append_note(notes_root, kind, new_id, text)

        inserted = _insert_reference(doc_root, kind, marker, new_id)
        if not inserted:
            # Append at end of document body if marker missing
            body = doc_root.find("w:body", namespaces=NS)
            if body is None:
                raise RuntimeError("No w:body in document.xml")
            p = etree.SubElement(body, f"{{{W_NS}}}p")
            r = etree.SubElement(p, f"{{{W_NS}}}r")
            ref = etree.SubElement(
                r,
                f"{{{W_NS}}}{'footnoteReference' if kind == 'footnote' else 'endnoteReference'}",
            )
            ref.set(f"{{{W_NS}}}id", str(new_id))

        # relationships
        rels_root = _read_xml(zin, "word/_rels/document.xml.rels")
        rels_changed = _ensure_note_relationship(rels_root, kind)

        # content types
        ct_root = _read_xml(zin, "[Content_Types].xml")
        ct_changed = _ensure_override(ct_root, part_name, ct)

        overrides: dict[str, bytes] = {
            "word/document.xml": _xml_bytes(doc_root),
            part_name: _xml_bytes(notes_root),
        }
        if rels_changed:
            overrides["word/_rels/document.xml.rels"] = _xml_bytes(rels_root)
        if ct_changed:
            overrides["[Content_Types].xml"] = _xml_bytes(ct_root)

        with zipfile.ZipFile(out_docx, "w", zipfile.ZIP_DEFLATED) as zout:
            for info in zin.infolist():
                name = info.filename
                if name in overrides:
                    zout.writestr(name, overrides[name])
                else:
                    zout.writestr(name, zin.read(name))
            # Add new parts not present in original
            for name, data in overrides.items():
                if name not in {i.filename for i in zin.infolist()}:
                    zout.writestr(name, data)


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("in_docx")
    ap.add_argument("--kind", choices=["footnote", "endnote"], required=True)
    ap.add_argument("--text", required=True)
    ap.add_argument("--marker", default="[[NOTE]]")
    ap.add_argument("--out", required=True)
    args = ap.parse_args()

    insert_note(args.in_docx, args.out, args.kind, args.marker, args.text)
    print(f"[OK] inserted {args.kind} and wrote {args.out}")


if __name__ == "__main__":
    main()
