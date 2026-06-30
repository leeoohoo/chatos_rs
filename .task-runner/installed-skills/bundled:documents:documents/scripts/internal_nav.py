#!/usr/bin/env python3
"""Add internal navigation aids to a DOCX (bookmarks + internal hyperlinks).

This is an OOXML-level helper for deterministic navigation in headless flows.

What it does
------------
- Adds Top / Bottom / TOC bookmarks.
- Adds bookmarks on heading paragraphs (Heading1/2/3 by default).
- Builds a *static* Table of Contents section with internal hyperlinks.
- Adds a "Back to TOC" link on each heading paragraph.
- Optionally adds a quick-links bar including Top/Bottom + figN/tblN bookmarks.

Why static TOC?
--------------
Word's TOC is a field and typically requires Word to update. In automation
pipelines, a deterministic static TOC is easier to QA and reason about.

Usage
-----
python scripts/internal_nav.py in.docx --out out.docx
python scripts/internal_nav.py in.docx --out out.docx --levels 1 2
python scripts/internal_nav.py in.docx --out out.docx --no_quicklinks

Notes
-----
- Internal links use <w:hyperlink w:anchor="bookmarkName">.
- This script patches only word/document.xml.
"""

from __future__ import annotations

import argparse
import re
import tempfile
from pathlib import Path

from lxml import etree

try:
    from docx_ooxml_patch import unzip_docx, zip_docx
except Exception:
    import os
    import shutil
    import zipfile

    def unzip_docx(docx_path: Path, out_dir: Path) -> None:
        if out_dir.exists():
            shutil.rmtree(out_dir)
        out_dir.mkdir(parents=True, exist_ok=True)
        with zipfile.ZipFile(docx_path, "r") as z:
            z.extractall(out_dir)

    def zip_docx(in_dir: Path, out_docx_path: Path) -> None:
        if out_docx_path.exists():
            out_docx_path.unlink()
        with zipfile.ZipFile(out_docx_path, "w", compression=zipfile.ZIP_DEFLATED) as z:
            for root, _dirs, files in os.walk(in_dir):
                for f in files:
                    abs_path = Path(root) / f
                    rel_path = abs_path.relative_to(in_dir)
                    z.write(abs_path, rel_path.as_posix())


W_NS = "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
R_NS = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
NS = {"w": W_NS, "r": R_NS}
XML_NS = "http://www.w3.org/XML/1998/namespace"


def qn(local: str) -> str:
    return f"{{{W_NS}}}{local}"


def w_attr(local: str) -> str:
    return f"{{{W_NS}}}{local}"


def _xml_space_preserve(t_el: etree._Element) -> None:
    if t_el.text and (t_el.text.startswith(" ") or t_el.text.endswith(" ")):
        t_el.set(f"{{{XML_NS}}}space", "preserve")


def _load_tree(path: Path) -> etree._ElementTree:
    parser = etree.XMLParser(remove_blank_text=False)
    return etree.parse(str(path), parser)


def _save_tree(tree: etree._ElementTree, path: Path) -> None:
    tree.write(str(path), xml_declaration=True, encoding="UTF-8", standalone="yes")


def _max_bookmark_id(root: etree._Element) -> int:
    mx = 0
    for b in root.findall(".//w:bookmarkStart", namespaces=NS):
        v = b.get(w_attr("id"))
        if v and v.isdigit():
            mx = max(mx, int(v))
    for b in root.findall(".//w:bookmarkEnd", namespaces=NS):
        v = b.get(w_attr("id"))
        if v and v.isdigit():
            mx = max(mx, int(v))
    return mx


def _p_text(p: etree._Element) -> str:
    return "".join(p.xpath(".//w:t/text()", namespaces=NS)).strip()


def _p_style(p: etree._Element) -> str | None:
    st = p.find("w:pPr/w:pStyle", namespaces=NS)
    if st is None:
        return None
    return st.get(w_attr("val"))


def _outline_level(p: etree._Element) -> int | None:
    """Return Word outline level (0-based) if present, else None."""
    ol = p.find("w:pPr/w:outlineLvl", namespaces=NS)
    if ol is None:
        return None
    v = ol.get(w_attr("val"))
    if v is None or not v.isdigit():
        return None
    return int(v)


def _make_paragraph(text: str, style: str | None = None) -> etree._Element:
    p = etree.Element(qn("p"))
    if style is not None:
        pPr = etree.SubElement(p, qn("pPr"))
        pStyle = etree.SubElement(pPr, qn("pStyle"))
        pStyle.set(w_attr("val"), style)
    r = etree.SubElement(p, qn("r"))
    t = etree.SubElement(r, qn("t"))
    t.text = text
    _xml_space_preserve(t)
    return p


def _make_internal_hyperlink(anchor: str, text: str) -> etree._Element:
    hl = etree.Element(qn("hyperlink"))
    hl.set(w_attr("anchor"), anchor)

    r = etree.SubElement(hl, qn("r"))
    rPr = etree.SubElement(r, qn("rPr"))
    rStyle = etree.SubElement(rPr, qn("rStyle"))
    rStyle.set(w_attr("val"), "Hyperlink")

    t = etree.SubElement(r, qn("t"))
    t.text = text
    _xml_space_preserve(t)
    return hl


def _insert_bookmark_in_paragraph(p: etree._Element, name: str, bm_id: int) -> None:
    # Avoid duplicates: if bookmarkStart with same name exists in paragraph, skip
    for b in p.findall(".//w:bookmarkStart", namespaces=NS):
        if b.get(w_attr("name")) == name:
            return

    b_start = etree.Element(qn("bookmarkStart"))
    b_start.set(w_attr("id"), str(bm_id))
    b_start.set(w_attr("name"), name)
    b_end = etree.Element(qn("bookmarkEnd"))
    b_end.set(w_attr("id"), str(bm_id))

    # Put start at the beginning of paragraph content.
    p.insert(0, b_start)
    p.append(b_end)


def _bookmark_names(root: etree._Element) -> set[str]:
    out: set[str] = set()
    for b in root.findall(".//w:bookmarkStart", namespaces=NS):
        nm = b.get(w_attr("name"))
        if nm:
            out.add(nm)
    return out


def _find_fig_tbl_bookmarks(root: etree._Element) -> tuple[list[str], list[str]]:
    names = sorted(_bookmark_names(root))
    fig = [n for n in names if re.fullmatch(r"fig\d+", n)]
    tbl = [n for n in names if re.fullmatch(r"tbl\d+", n)]
    return fig, tbl


def patch_document(
    docx_in: Path,
    docx_out: Path,
    levels: list[int],
    toc_title: str,
    add_quicklinks: bool,
    add_back_to_toc: bool,
    add_top_bottom: bool,
) -> None:
    with tempfile.TemporaryDirectory(prefix="docx_nav_") as td:
        unz = Path(td) / "unz"
        unzip_docx(docx_in, unz)

        doc_xml = unz / "word" / "document.xml"
        tree = _load_tree(doc_xml)
        root = tree.getroot()
        body = root.find("w:body", namespaces=NS)
        if body is None:
            raise RuntimeError("word/document.xml missing w:body")

        paras = body.findall("w:p", namespaces=NS)
        if not paras:
            raise RuntimeError("No paragraphs found in document body")

        bm_id = _max_bookmark_id(root) + 1

        # Add Top / Bottom bookmarks (best-effort).
        if add_top_bottom:
            _insert_bookmark_in_paragraph(paras[0], "Top", bm_id)
            bm_id += 1
            _insert_bookmark_in_paragraph(paras[-1], "Bottom", bm_id)
            bm_id += 1

        # Identify headings and bookmark them.
        heading_infos: list[tuple[int, str, str, etree._Element]] = []
        sec_idx = 0

        for p in body.findall("w:p", namespaces=NS):
            # Prefer outline level if present (works even when the doc lacks Heading styles).
            ol = _outline_level(p)
            level = None
            if ol is not None:
                level = ol + 1
            else:
                st = _p_style(p) or ""
                m = re.fullmatch(r"Heading\s*(\d+)", st)
                if m is None:
                    # Common styleId is Heading1/Heading2
                    m = re.fullmatch(r"Heading(\d+)", st)
                if m is not None:
                    level = int(m.group(1))

            if level is None:
                continue
            if level not in levels:
                continue
            title = _p_text(p)
            if not title:
                continue
            sec_idx += 1
            bname = f"sec{sec_idx:03d}"
            _insert_bookmark_in_paragraph(p, bname, bm_id)
            bm_id += 1
            heading_infos.append((level, title, bname, p))

        # Insert TOC section at the start of the document.
        # Place it before the first paragraph.
        toc_heading = _make_paragraph(toc_title, style="Heading1")
        _insert_bookmark_in_paragraph(toc_heading, "TOC", bm_id)
        bm_id += 1

        insert_at = 0
        body.insert(insert_at, toc_heading)
        insert_at += 1

        # Build TOC entries
        for level, title, bname, _p in heading_infos:
            indent = "  " * (max(1, level) - 1)
            p_ent = etree.Element(qn("p"))
            if indent:
                r = etree.SubElement(p_ent, qn("r"))
                t = etree.SubElement(r, qn("t"))
                t.text = indent
                _xml_space_preserve(t)
            p_ent.append(_make_internal_hyperlink(bname, title))
            body.insert(insert_at, p_ent)
            insert_at += 1

        # Add a blank line after TOC
        body.insert(insert_at, _make_paragraph(""))
        insert_at += 1

        # Add quick links bar after TOC (implemented here to avoid placeholder run)
        if add_quicklinks:
            fig_bm, tbl_bm = _find_fig_tbl_bookmarks(root)
            p_links = etree.Element(qn("p"))
            p_links.append(_make_internal_hyperlink("Top", "Top"))

            # separator runs
            def _sep(txt: str) -> etree._Element:
                r = etree.Element(qn("r"))
                t = etree.SubElement(r, qn("t"))
                t.text = txt
                _xml_space_preserve(t)
                return r

            p_links.append(_sep(" | "))
            p_links.append(_make_internal_hyperlink("Bottom", "Bottom"))
            p_links.append(_sep(" | "))
            p_links.append(_make_internal_hyperlink("TOC", "TOC"))

            # Link to each table/figure bookmark (caption-number bookmarks)
            for b in tbl_bm:
                num = b.replace("tbl", "")
                p_links.append(_sep(" | "))
                p_links.append(_make_internal_hyperlink(b, f"Table {num}"))
            for b in fig_bm:
                num = b.replace("fig", "")
                p_links.append(_sep(" | "))
                p_links.append(_make_internal_hyperlink(b, f"Figure {num}"))

            body.insert(insert_at, p_links)
            insert_at += 1
            body.insert(insert_at, _make_paragraph(""))
            insert_at += 1

        # Add "Back to TOC" on headings
        if add_back_to_toc:
            for _level, _title, _bname, p in heading_infos:
                # Add a space run then hyperlink
                r = etree.SubElement(p, qn("r"))
                t = etree.SubElement(r, qn("t"))
                t.text = " "
                _xml_space_preserve(t)
                p.append(_make_internal_hyperlink("TOC", "Back to TOC"))

        _save_tree(tree, doc_xml)
        zip_docx(unz, docx_out)


def main() -> None:
    ap = argparse.ArgumentParser(
        description="Add internal navigation links/bookmarks and a static TOC"
    )
    ap.add_argument("input_docx")
    ap.add_argument("--out", required=True)
    ap.add_argument("--toc_title", default="Table of Contents")
    ap.add_argument(
        "--levels",
        nargs="+",
        type=int,
        default=[1, 2, 3],
        help="Heading levels to include",
    )
    ap.add_argument("--no_quicklinks", action="store_true", help="Disable quick links bar")
    ap.add_argument(
        "--no_back_to_toc",
        action="store_true",
        help="Do not add back-to-TOC links on headings",
    )
    ap.add_argument("--no_top_bottom", action="store_true", help="Do not add Top/Bottom bookmarks")
    args = ap.parse_args()

    patch_document(
        docx_in=Path(args.input_docx),
        docx_out=Path(args.out),
        levels=list(args.levels),
        toc_title=args.toc_title,
        add_quicklinks=not args.no_quicklinks,
        add_back_to_toc=not args.no_back_to_toc,
        add_top_bottom=not args.no_top_bottom,
    )
    print(f"[OK] wrote {args.out}")


if __name__ == "__main__":
    main()
