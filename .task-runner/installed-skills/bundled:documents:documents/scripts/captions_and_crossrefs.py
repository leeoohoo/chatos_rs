#!/usr/bin/env python3
"""Insert simple captions (Figure/Table) and optional cross-references.

This is a pragmatic OOXML-level helper for:
- Adding Figure/Table captions using SEQ fields
- (Optional) adding bookmarks around the caption number for later REF fields
- (Optional) materializing SEQ/REF fields so headless renders show correct numbers

It targets common automation needs, not the full Word caption feature set.
"""

from __future__ import annotations

import argparse
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
NS = {
    "w": W_NS,
    "r": "http://schemas.openxmlformats.org/officeDocument/2006/relationships",
}


def qn(local: str) -> str:
    return f"{{{W_NS}}}{local}"


def w_attr(local: str) -> str:
    return f"{{{W_NS}}}{local}"


def _xml_space_preserve(t_el: etree._Element) -> None:
    if t_el.text and (t_el.text.startswith(" ") or t_el.text.endswith(" ")):
        t_el.set("{http://www.w3.org/XML/1998/namespace}space", "preserve")


def _load_tree(path: Path) -> etree._ElementTree:
    parser = etree.XMLParser(remove_blank_text=False)
    return etree.parse(str(path), parser)


def _save_tree(tree: etree._ElementTree, path: Path) -> None:
    tree.write(str(path), xml_declaration=True, encoding="UTF-8", standalone="yes")


def _iter_word_parts(unzipped: Path) -> list[Path]:
    parts = [unzipped / "word" / "document.xml"]
    for pat in ("header*.xml", "footer*.xml"):
        parts.extend(sorted((unzipped / "word").glob(pat)))
    return [p for p in parts if p.exists()]


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


def _caption_paragraph(label: str, caption_text: str, seq_name: str) -> etree._Element:
    """Create a <w:p> containing: '<label> ' + SEQ field + ': <caption_text>'."""

    p = etree.Element(qn("p"))

    pPr = etree.SubElement(p, qn("pPr"))
    pStyle = etree.SubElement(pPr, qn("pStyle"))
    pStyle.set(w_attr("val"), "Caption")

    def _r_text(txt: str) -> etree._Element:
        r = etree.Element(qn("r"))
        t = etree.SubElement(r, qn("t"))
        t.text = txt
        _xml_space_preserve(t)
        return r

    p.append(_r_text(f"{label} "))

    # Field: SEQ <seq_name> \* ARABIC
    r_begin = etree.Element(qn("r"))
    fld_begin = etree.SubElement(r_begin, qn("fldChar"))
    fld_begin.set(w_attr("fldCharType"), "begin")
    p.append(r_begin)

    r_instr = etree.Element(qn("r"))
    instr = etree.SubElement(r_instr, qn("instrText"))
    instr.set("{http://www.w3.org/XML/1998/namespace}space", "preserve")
    instr.text = f" SEQ {seq_name} \\* ARABIC "
    p.append(r_instr)

    r_sep = etree.Element(qn("r"))
    fld_sep = etree.SubElement(r_sep, qn("fldChar"))
    fld_sep.set(w_attr("fldCharType"), "separate")
    p.append(r_sep)

    r_res = etree.Element(qn("r"))
    t_res = etree.SubElement(r_res, qn("t"))
    t_res.text = "0"  # placeholder; can be materialized later
    p.append(r_res)

    r_end = etree.Element(qn("r"))
    fld_end = etree.SubElement(r_end, qn("fldChar"))
    fld_end.set(w_attr("fldCharType"), "end")
    p.append(r_end)

    if caption_text:
        p.append(_r_text(f": {caption_text}"))

    return p


def _has_caption_following(el: etree._Element, label: str) -> bool:
    nxt = el.getnext()
    if nxt is None or nxt.tag != qn("p"):
        return False
    pStyle = nxt.find("w:pPr/w:pStyle", namespaces=NS)
    if pStyle is None:
        return False
    if pStyle.get(w_attr("val")) != "Caption":
        return False
    txt = "".join(nxt.xpath(".//w:t/text()", namespaces=NS)).strip()
    return txt.startswith(label)


def _insert_bookmark_around_number(p: etree._Element, name: str, bm_id: int) -> None:
    """Wrap the first <w:t> in the field result with a bookmark."""

    # Find the first result <w:t> that occurs after a fldChar separate.
    runs = p.xpath(".//w:r", namespaces=NS)
    seen_sep = False
    target_run = None
    for r in runs:
        fld = r.find("w:fldChar", namespaces=NS)
        if fld is not None and fld.get(w_attr("fldCharType")) == "separate":
            seen_sep = True
            continue
        if not seen_sep:
            continue
        t = r.find("w:t", namespaces=NS)
        if t is not None:
            target_run = r
            break

    if target_run is None:
        return

    parent = target_run.getparent()
    idx = parent.index(target_run)

    b_start = etree.Element(qn("bookmarkStart"))
    b_start.set(w_attr("id"), str(bm_id))
    b_start.set(w_attr("name"), name)

    b_end = etree.Element(qn("bookmarkEnd"))
    b_end.set(w_attr("id"), str(bm_id))

    parent.insert(idx, b_start)
    parent.insert(idx + 2, b_end)


def _materialize_fields_in_tree(tree: etree._ElementTree) -> None:
    """Materialize SEQ and REF fields in-place.

    We keep this implementation minimal and deterministic:
    - SEQ counters are computed in document order per SEQ name.
    - REF results are set to the current bookmarked text.
    """

    from fields_materialize import materialize_fields_in_root  # local import

    materialize_fields_in_root(tree.getroot(), {"SEQ", "REF"})


def add_captions(
    docx_path: Path,
    out_docx: Path,
    add_tables: bool,
    add_figures: bool,
    caption_text: str,
    add_bookmarks: bool,
    materialize: bool,
) -> None:
    with tempfile.TemporaryDirectory(prefix="docx_unz_") as td:
        unz = Path(td) / "unz"
        unzip_docx(docx_path, unz)

        for part in _iter_word_parts(unz):
            tree = _load_tree(part)
            root = tree.getroot()

            bm_id = _max_bookmark_id(root) + 1
            tbl_n = 0
            fig_n = 0

            # Tables: insert caption paragraph after each <w:tbl>
            if add_tables:
                for tbl in root.findall(".//w:tbl", namespaces=NS):
                    if _has_caption_following(tbl, "Table"):
                        continue
                    tbl_n += 1
                    cap = _caption_paragraph("Table", caption_text, "Table")
                    tbl.addnext(cap)
                    if add_bookmarks:
                        _insert_bookmark_around_number(cap, f"tbl{tbl_n}", bm_id)
                        bm_id += 1

            # Figures: insert after paragraphs with drawings
            if add_figures:
                for p in root.findall(".//w:p", namespaces=NS):
                    if not (
                        p.find(".//w:drawing", namespaces=NS) is not None
                        or p.find(".//w:pict", namespaces=NS) is not None
                    ):
                        continue
                    if _has_caption_following(p, "Figure"):
                        continue
                    fig_n += 1
                    cap = _caption_paragraph("Figure", caption_text, "Figure")
                    p.addnext(cap)
                    if add_bookmarks:
                        _insert_bookmark_around_number(cap, f"fig{fig_n}", bm_id)
                        bm_id += 1

            if materialize:
                _materialize_fields_in_tree(tree)

            _save_tree(tree, part)

        zip_docx(unz, out_docx)


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("input_docx", type=Path)
    ap.add_argument("output_docx", type=Path)
    ap.add_argument("--tables", action="store_true", help="Add table captions.")
    ap.add_argument("--figures", action="store_true", help="Add figure captions.")
    ap.add_argument(
        "--caption_text",
        default="",
        help="Text appended after the caption number, e.g. 'Results by category'.",
    )
    ap.add_argument(
        "--bookmarks",
        action="store_true",
        help="Add bookmarks around caption numbers (tbl1, tbl2, fig1...).",
    )
    ap.add_argument(
        "--materialize",
        action="store_true",
        help="Materialize SEQ/REF fields so headless renders show correct numbers.",
    )
    args = ap.parse_args()

    if not args.tables and not args.figures:
        ap.error("Please specify at least one of --tables or --figures")

    add_captions(
        args.input_docx,
        args.output_docx,
        add_tables=args.tables,
        add_figures=args.figures,
        caption_text=args.caption_text,
        add_bookmarks=args.bookmarks,
        materialize=args.materialize,
    )

    print(f"[OK] wrote {args.output_docx}")


if __name__ == "__main__":
    main()
