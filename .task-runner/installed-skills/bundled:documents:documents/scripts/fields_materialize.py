#!/usr/bin/env python3
"""Materialize (freeze) common Word fields into plain text.

Why
---
Word fields (SEQ, REF, PAGE, etc.) are normally evaluated by Word. In automation
pipelines (and headless renderers), fields sometimes don't update, which makes
visual QA confusing.

This script "materializes" a subset of fields by writing their *display*
(result) text directly into the DOCX, while keeping the field code structure.
That way:
- PNG rendering shows stable numbers/text
- The DOCX still contains the original field instructions for Word to update

Supported
---------
- SEQ <Type>   : e.g., "SEQ Figure" or "SEQ Table" (sequential order in-document)
- REF <Bookmark>: writes the current text inside the bookmark

Not supported (by design)
--------------------------
- PAGE / NUMPAGES: requires pagination/layout
- TOC: should be rebuilt via Word or a dedicated TOC generator

Usage
-----
python scripts/fields_materialize.py in.docx --out out.docx
python scripts/fields_materialize.py in.docx --out out.docx --only SEQ
python scripts/fields_materialize.py in.docx --out out.docx --only SEQ REF
"""

from __future__ import annotations

import argparse
import glob
import os
import re
import tempfile
from pathlib import Path

from lxml import etree

try:
    from docx_ooxml_patch import unzip_docx, zip_docx
except Exception:
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
XML_NS = "http://www.w3.org/XML/1998/namespace"


def qn(local: str) -> str:
    return f"{{{W_NS}}}{local}"


def qna(local: str) -> str:
    # attribute in w: namespace
    return f"{{{W_NS}}}{local}"


def _set_text_preserve(t_el: etree._Element, value: str) -> None:
    t_el.text = value
    if value.startswith(" ") or value.endswith(" "):
        t_el.set(f"{{{XML_NS}}}space", "preserve")


def _list_parts(unzipped: Path) -> list[Path]:
    word = unzipped / "word"
    parts = [word / "document.xml"]
    parts += [Path(p) for p in sorted(glob.glob(str(word / "header*.xml")))]
    parts += [Path(p) for p in sorted(glob.glob(str(word / "footer*.xml")))]
    return [p for p in parts if p.exists()]


def build_bookmark_text(root: etree._Element) -> dict[str, str]:
    """Map bookmark name -> text inside the bookmark range (best-effort)."""

    # Stack of open bookmarks: list of [id, name, buf]
    open_stack: list[tuple[str, str, list[str]]] = []
    out: dict[str, str] = {}

    for el in root.iter():
        if el.tag == qn("bookmarkStart"):
            bid = el.get(qna("id")) or ""
            name = el.get(qna("name")) or ""
            # Skip Word internal bookmarks like _GoBack
            if name:
                open_stack.append((bid, name, []))
        elif el.tag == qn("bookmarkEnd"):
            bid = el.get(qna("id")) or ""
            # Pop the most recent matching id
            idx = None
            for i in range(len(open_stack) - 1, -1, -1):
                if open_stack[i][0] == bid:
                    idx = i
                    break
            if idx is not None:
                _bid, name, buf = open_stack.pop(idx)
                # If nested, remaining open bookmarks should also include text, but we keep it simple.
                out[name] = "".join(buf)
        elif el.tag == qn("t"):
            if not open_stack:
                continue
            txt = el.text or ""
            # Append to all open bookmarks (handles nesting; depth is usually tiny)
            for i in range(len(open_stack)):
                open_stack[i][2].append(txt)

    return out


def _materialize_seq_in_root(root: etree._Element) -> int:
    """Materialize SEQ fields in-place and return number of fields updated."""

    seq_counters: dict[str, int] = {}
    updated = 0

    for p in root.findall(".//w:p", namespaces=NS):
        runs = p.xpath(".//w:r", namespaces=NS)

        in_field = False
        seen_separate = False
        instr = ""
        result_text_nodes: list[etree._Element] = []

        for r in runs:
            fld = r.find("w:fldChar", namespaces=NS)
            if fld is not None:
                ftype = fld.get(qna("fldCharType"))
                if ftype == "begin":
                    in_field = True
                    seen_separate = False
                    instr = ""
                    result_text_nodes = []
                    continue
                if in_field and ftype == "separate":
                    seen_separate = True
                    continue
                if in_field and ftype == "end":
                    instr_norm = " ".join(instr.split())
                    m_seq = re.search(r"\bSEQ\s+([A-Za-z0-9_]+)", instr_norm)
                    if m_seq:
                        seq_name = m_seq.group(1)
                        seq_counters[seq_name] = seq_counters.get(seq_name, 0) + 1
                        value = str(seq_counters[seq_name])
                        if result_text_nodes:
                            _set_text_preserve(result_text_nodes[0], value)
                            for extra in result_text_nodes[1:]:
                                _set_text_preserve(extra, "")
                            updated += 1

                    in_field = False
                    seen_separate = False
                    instr = ""
                    result_text_nodes = []
                    continue

            if not in_field:
                continue

            instr_el = r.find("w:instrText", namespaces=NS)
            if instr_el is not None and not seen_separate:
                instr += instr_el.text or ""

            if seen_separate:
                for t in r.findall(".//w:t", namespaces=NS):
                    result_text_nodes.append(t)

    return updated


def _materialize_ref_in_root(root: etree._Element) -> int:
    """Materialize REF fields in-place (based on current bookmark text)."""

    bm_text = build_bookmark_text(root)
    updated = 0

    for p in root.findall(".//w:p", namespaces=NS):
        runs = p.xpath(".//w:r", namespaces=NS)

        in_field = False
        seen_separate = False
        instr = ""
        result_text_nodes: list[etree._Element] = []

        for r in runs:
            fld = r.find("w:fldChar", namespaces=NS)
            if fld is not None:
                ftype = fld.get(qna("fldCharType"))
                if ftype == "begin":
                    in_field = True
                    seen_separate = False
                    instr = ""
                    result_text_nodes = []
                    continue
                if in_field and ftype == "separate":
                    seen_separate = True
                    continue
                if in_field and ftype == "end":
                    instr_norm = " ".join(instr.split())
                    m_ref = re.search(r"\bREF\s+([A-Za-z0-9_:\-\.]+)", instr_norm)
                    if m_ref:
                        bname = m_ref.group(1)
                        value = bm_text.get(bname)
                        if value is not None and result_text_nodes:
                            _set_text_preserve(result_text_nodes[0], value)
                            for extra in result_text_nodes[1:]:
                                _set_text_preserve(extra, "")
                            updated += 1

                    in_field = False
                    seen_separate = False
                    instr = ""
                    result_text_nodes = []
                    continue

            if not in_field:
                continue

            instr_el = r.find("w:instrText", namespaces=NS)
            if instr_el is not None and not seen_separate:
                instr += instr_el.text or ""

            if seen_separate:
                for t in r.findall(".//w:t", namespaces=NS):
                    result_text_nodes.append(t)

    return updated


def materialize_fields_in_root(root: etree._Element, only: set[str]) -> int:
    """Materialize SEQ/REF fields in a single XML root.

    Important: REF results depend on the *current* text inside the bookmarked
    range. If a bookmark wraps a SEQ field result, we must materialize SEQ first
    so REF doesn't capture the placeholder "0".

    Returns number of fields updated.
    """

    total = 0
    if "SEQ" in only:
        total += _materialize_seq_in_root(root)
    if "REF" in only:
        total += _materialize_ref_in_root(root)
    return total


def main() -> None:
    ap = argparse.ArgumentParser(description="Materialize SEQ/REF field results inside a DOCX")
    ap.add_argument("input_docx")
    ap.add_argument("--out", required=True, help="Output DOCX path")
    ap.add_argument(
        "--only",
        nargs="+",
        default=["SEQ", "REF"],
        choices=["SEQ", "REF"],
        help="Which field types to materialize (default: SEQ REF)",
    )
    args = ap.parse_args()

    input_docx = Path(args.input_docx)
    out_docx = Path(args.out)
    only = set(args.only)

    with tempfile.TemporaryDirectory(prefix="docx_unzip_") as td:
        unzipped = Path(td) / "docx"
        unzip_docx(input_docx, unzipped)

        parser = etree.XMLParser(remove_blank_text=False)
        total = 0
        for part in _list_parts(unzipped):
            tree = etree.parse(str(part), parser)
            root = tree.getroot()
            n = materialize_fields_in_root(root, only=only)
            if n:
                tree.write(str(part), xml_declaration=True, encoding="UTF-8", standalone="yes")
            total += n

        zip_docx(unzipped, out_docx)

    print(f"[OK] materialized {total} field(s) -> {out_docx}")


if __name__ == "__main__":
    main()
