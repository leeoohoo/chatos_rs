#!/usr/bin/env python3
"""Add one or more Word comments to paragraphs matched by substring.

The toolkit can extract/strip comments and can add a single comment via
`docx_ooxml_patch.py`. In real review workflows you often need to inject
multiple comments across different sections and then later patch/resolve them.

This helper:
 - Ensures `word/comments.xml`, `document.xml.rels`, and `[Content_Types].xml`
   are wired up.
 - Finds the *first* paragraph containing each `--add` substring and attaches a
   comment spanning the whole paragraph.

Notes / pitfalls
----------------
- If your DOCX contains **tracked changes**, the visible text can live in both
  `<w:t>` (insertions/current text) and `<w:delText>` (deletions). For matching
  we consider both, so you can comment paragraphs that were recently edited.

Usage
-----
python scripts/comments_add.py in.docx --out out.docx \
  --add "contains=Second section needs clarification" \
  --add "Risk table=Double-check numbers"
"""

from __future__ import annotations

import argparse
import datetime as _dt
import re
import zipfile

from lxml import etree

W_NS = "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
R_NS = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
PKG_REL_NS = "http://schemas.openxmlformats.org/package/2006/relationships"

NS = {"w": W_NS, "r": R_NS, "pr": PKG_REL_NS}


def w(tag: str) -> str:
    return f"{{{W_NS}}}{tag}"


def r(tag: str) -> str:
    return f"{{{R_NS}}}{tag}"


def _xml_bytes(root: etree._Element) -> bytes:
    return etree.tostring(root, xml_declaration=True, encoding="UTF-8", standalone="yes")


def _read_xml(zin: zipfile.ZipFile, name: str) -> etree._Element:
    return etree.fromstring(zin.read(name))


def _ensure_content_types(ct_root: etree._Element) -> None:
    # Ensure comments.xml override exists.
    xpath = "//*[local-name()='Override' and @PartName='/word/comments.xml']"
    if ct_root.xpath(xpath):
        return
    override = etree.Element("Override")
    override.set("PartName", "/word/comments.xml")
    override.set(
        "ContentType",
        "application/vnd.openxmlformats-officedocument.wordprocessingml.comments+xml",
    )
    ct_root.append(override)


def _ensure_document_rels(rels_root: etree._Element) -> None:
    # Ensure a relationship to comments.xml exists.
    rel_type = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments"
    for rel in rels_root.xpath("//pr:Relationship", namespaces=NS):
        if rel.get("Type") == rel_type and rel.get("Target") == "comments.xml":
            return
    # Pick a new Id.
    ids = []
    for rel in rels_root.xpath("//pr:Relationship", namespaces=NS):
        m = re.fullmatch(r"rId(\d+)", rel.get("Id", ""))
        if m:
            ids.append(int(m.group(1)))
    new_id = f"rId{(max(ids) + 1) if ids else 1}"
    rel = etree.SubElement(rels_root, f"{{{PKG_REL_NS}}}Relationship")
    rel.set("Id", new_id)
    rel.set("Type", rel_type)
    rel.set("Target", "comments.xml")


def _ensure_comments_root(existing: bytes | None) -> etree._Element:
    if existing is not None:
        return etree.fromstring(existing)
    return etree.Element(w("comments"), nsmap={"w": W_NS})


def _next_comment_id(comments_root: etree._Element) -> int:
    ids = []
    for c in comments_root.xpath("//w:comment", namespaces=NS):
        try:
            ids.append(int(c.get(w("id"))))
        except Exception:
            pass
    return (max(ids) + 1) if ids else 0


def _paragraph_text_for_match(p: etree._Element) -> str:
    # Include deletions so matching works even when tracking is enabled.
    parts = p.xpath(".//w:t/text() | .//w:delText/text()", namespaces=NS)
    return "".join(parts)


def _add_comment_to_paragraph(doc_root: etree._Element, p: etree._Element, comment_id: int) -> None:
    # Insert range start at beginning.
    crs = etree.Element(w("commentRangeStart"))
    crs.set(w("id"), str(comment_id))
    p.insert(0, crs)

    # Insert range end at end.
    cre = etree.Element(w("commentRangeEnd"))
    cre.set(w("id"), str(comment_id))
    p.append(cre)

    # Add commentReference run.
    r_el = etree.SubElement(p, w("r"))
    cr = etree.SubElement(r_el, w("commentReference"))
    cr.set(w("id"), str(comment_id))


def _append_comment(comments_root: etree._Element, comment_id: int, text: str, author: str) -> None:
    c = etree.SubElement(comments_root, w("comment"))
    c.set(w("id"), str(comment_id))
    c.set(w("author"), author)
    c.set(w("date"), _dt.datetime.utcnow().replace(microsecond=0).isoformat() + "Z")
    p = etree.SubElement(c, w("p"))
    r_el = etree.SubElement(p, w("r"))
    t = etree.SubElement(r_el, w("t"))
    t.text = text


def add_comments(
    in_docx: str,
    out_docx: str,
    adds: list[tuple[str, str]],
    author: str,
    ignore_case: bool = False,
    require_all: bool = False,
) -> None:
    with zipfile.ZipFile(in_docx, "r") as zin:
        overrides: dict[str, bytes] = {}

        doc_root = _read_xml(zin, "word/document.xml")

        comments_bytes = (
            zin.read("word/comments.xml") if "word/comments.xml" in zin.namelist() else None
        )
        comments_root = _ensure_comments_root(comments_bytes)

        # Ensure package plumbing.
        ct_root = _read_xml(zin, "[Content_Types].xml")
        _ensure_content_types(ct_root)
        overrides["[Content_Types].xml"] = _xml_bytes(ct_root)

        rels_name = "word/_rels/document.xml.rels"
        rels_root = _read_xml(zin, rels_name)
        _ensure_document_rels(rels_root)
        overrides[rels_name] = _xml_bytes(rels_root)

        next_id = _next_comment_id(comments_root)
        used = 0
        missing: list[str] = []

        for contains, ctext in adds:
            needle = contains.lower() if ignore_case else contains
            hit = None
            for p in doc_root.xpath(".//w:p", namespaces=NS):
                hay = _paragraph_text_for_match(p)
                hay_cmp = hay.lower() if ignore_case else hay
                if needle in hay_cmp:
                    hit = p
                    break
            if hit is None:
                missing.append(contains)
                print(f"[warn] no paragraph matched contains={contains!r}")
                continue

            cid = next_id
            next_id += 1
            _add_comment_to_paragraph(doc_root, hit, cid)
            _append_comment(comments_root, cid, ctext, author=author)
            used += 1

        overrides["word/document.xml"] = _xml_bytes(doc_root)
        overrides["word/comments.xml"] = _xml_bytes(comments_root)

        with zipfile.ZipFile(out_docx, "w", zipfile.ZIP_DEFLATED) as zout:
            for info in zin.infolist():
                name = info.filename
                if name in overrides:
                    zout.writestr(name, overrides[name])
                else:
                    zout.writestr(name, zin.read(name))
            # If comments.xml didn't exist, ensure it's added.
            if "word/comments.xml" not in zin.namelist():
                zout.writestr("word/comments.xml", overrides["word/comments.xml"])

    if require_all and missing:
        raise SystemExit(f"[comments_add] {len(missing)} patterns were not matched: {missing}")

    print(f"[OK] wrote {out_docx} (added_comments={used}, unmatched_patterns={len(missing)})")


def main() -> None:
    ap = argparse.ArgumentParser(
        description="Add multiple Word comments by paragraph substring match"
    )
    ap.add_argument("in_docx")
    ap.add_argument("--out", required=True)
    ap.add_argument("--author", default="ChatGPT")
    ap.add_argument(
        "--add",
        action="append",
        default=[],
        help="Add a comment: contains=comment text (repeatable)",
    )
    ap.add_argument(
        "--ignore_case",
        action="store_true",
        help="Case-insensitive substring matching",
    )
    ap.add_argument(
        "--require_all",
        action="store_true",
        help="Fail if any --add pattern does not match a paragraph",
    )
    args = ap.parse_args()

    adds: list[tuple[str, str]] = []
    for a in args.add:
        if "=" not in a:
            raise SystemExit("--add must be formatted as contains=comment text")
        k, v = a.split("=", 1)
        adds.append((k, v))

    if not adds:
        raise SystemExit("Provide at least one --add")

    add_comments(
        args.in_docx,
        args.out,
        adds,
        author=args.author,
        ignore_case=args.ignore_case,
        require_all=args.require_all,
    )


if __name__ == "__main__":
    main()
