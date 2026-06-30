#!/usr/bin/env python3
"""Insert Word cross-references (REF fields) by replacing lightweight markers.

Why this exists
--------------
Word cross-references are implemented as `REF` fields that point at a bookmark.
`python-docx` does not expose a first-class API for these fields.

This helper lets you author docs with simple markers like:

  "See [[REF:tbl1]] for details."

…and then replace those markers with real `REF` fields in OOXML.

Design goals
------------
- Minimal, deterministic, container-friendly.
- Works on document.xml + headers/footers.
- Keeps implementation small and easy to reuse/import.

Limitations
-----------
- The marker must be fully contained in a single `<w:t>` node.
  (If Word splits it across runs, retype the marker as a single contiguous token.)
- This does *not* create bookmarks. Pair with `captions_and_crossrefs.py --bookmarks`.
- For stable headless QA, run `fields_materialize.py` afterwards.

Example
-------
1) Add captions + bookmarks:
   python scripts/captions_and_crossrefs.py in.docx out_caps.docx --tables --bookmarks
2) Replace markers with REF fields:
   python scripts/insert_ref_fields.py out_caps.docx out_refs.docx
3) Materialize:
   python scripts/fields_materialize.py out_refs.docx --out out_refs_mat.docx
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
    # Fallback minimal zip helpers (keeps this script standalone).
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


def _make_run_text(txt: str) -> etree._Element:
    r = etree.Element(qn("r"))
    t = etree.SubElement(r, qn("t"))
    t.text = txt
    _xml_space_preserve(t)
    return r


def _make_ref_field_runs(bookmark: str) -> list[etree._Element]:
    """Create runs implementing a REF field (begin/instr/separate/result/end)."""
    runs: list[etree._Element] = []

    r_begin = etree.Element(qn("r"))
    fld_begin = etree.SubElement(r_begin, qn("fldChar"))
    fld_begin.set(w_attr("fldCharType"), "begin")
    runs.append(r_begin)

    r_instr = etree.Element(qn("r"))
    instr = etree.SubElement(r_instr, qn("instrText"))
    instr.set("{http://www.w3.org/XML/1998/namespace}space", "preserve")
    instr.text = f" REF {bookmark} \\h "
    runs.append(r_instr)

    r_sep = etree.Element(qn("r"))
    fld_sep = etree.SubElement(r_sep, qn("fldChar"))
    fld_sep.set(w_attr("fldCharType"), "separate")
    runs.append(r_sep)

    r_res = etree.Element(qn("r"))
    t_res = etree.SubElement(r_res, qn("t"))
    t_res.text = "0"  # placeholder; can be materialized later
    runs.append(r_res)

    r_end = etree.Element(qn("r"))
    fld_end = etree.SubElement(r_end, qn("fldChar"))
    fld_end.set(w_attr("fldCharType"), "end")
    runs.append(r_end)

    return runs


def replace_markers_in_part(
    tree: etree._ElementTree,
    marker_re: re.Pattern[str],
    prefix: str,
) -> int:
    """Replace markers in <w:t> nodes. Marker must be fully inside one node."""
    root = tree.getroot()
    changed = 0

    for t in root.findall(".//w:t", namespaces=NS):
        if t.text is None:
            continue
        matches = list(marker_re.finditer(t.text))
        if not matches:
            continue

        # Find the run containing this <w:t>
        r = t.getparent()
        if r is None or r.tag != qn("r"):
            continue
        p = r.getparent()
        if p is None:
            continue

        idx = p.index(r)

        # Remove original run
        p.remove(r)

        # Replace all markers inside this one <w:t>.
        insert_runs: list[etree._Element] = []
        pos = 0
        for m in matches:
            before = t.text[pos : m.start()]
            if before:
                insert_runs.append(_make_run_text(before))
            if prefix:
                insert_runs.append(_make_run_text(prefix))
            insert_runs.extend(_make_ref_field_runs(m.group("bookmark")))
            pos = m.end()

        after = t.text[pos:]
        if after:
            insert_runs.append(_make_run_text(after))

        for j, rr in enumerate(insert_runs):
            p.insert(idx + j, rr)

        changed += len(matches)

    return changed


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("input_docx", type=Path)
    ap.add_argument("output_docx", type=Path)
    ap.add_argument(
        "--pattern",
        default=r"\[\[REF:(?P<bookmark>[A-Za-z0-9_:\-]+)\]\]",
        help=(
            "Regex for markers. Must contain a named group 'bookmark'. "
            "Default matches [[REF:tbl1]]."
        ),
    )
    ap.add_argument(
        "--prefix",
        default="",
        help="Optional text inserted immediately before the REF field.",
    )
    args = ap.parse_args()

    marker_re = re.compile(args.pattern)

    with tempfile.TemporaryDirectory(prefix="docx_unz_") as td:
        unz = Path(td) / "unz"
        unzip_docx(args.input_docx, unz)

        total = 0
        for part in _iter_word_parts(unz):
            tree = _load_tree(part)
            total += replace_markers_in_part(tree, marker_re, args.prefix)
            _save_tree(tree, part)

        zip_docx(unz, args.output_docx)

    print(f"[OK] wrote {args.output_docx} | markers_replaced={total}")


if __name__ == "__main__":
    main()
