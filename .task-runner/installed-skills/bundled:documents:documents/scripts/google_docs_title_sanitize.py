#!/usr/bin/env python3
"""Remove Word Title-style rule/border residue from Google Docs-targeted DOCX files.

This is a deterministic guard for a specific failure mode: Word's built-in
Title style can carry a paragraph bottom border that imports/renders as a blue
rule under the document title.

The sanitizer removes paragraph borders from:
- the built-in Title paragraph style in styles.xml
- paragraphs using the Title style
- the leading title block in document.xml, before the first heading/table

Use --check to make the same condition a hard audit gate.
"""

from __future__ import annotations

import argparse
import shutil
import tempfile
import zipfile
from dataclasses import dataclass
from pathlib import Path

from lxml import etree

W_NS = "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
NS = {"w": W_NS}


@dataclass
class Result:
    style_borders_removed: int = 0
    paragraph_borders_removed: int = 0
    title_underlines_removed: int = 0

    @property
    def total_changes(self) -> int:
        return (
            self.style_borders_removed
            + self.paragraph_borders_removed
            + self.title_underlines_removed
        )


def _read_xml(zf: zipfile.ZipFile, name: str) -> etree._Element:
    return etree.fromstring(zf.read(name))


def _xml_bytes(root: etree._Element) -> bytes:
    return etree.tostring(root, xml_declaration=True, encoding="UTF-8", standalone="yes")


def _w_val(node: etree._Element | None) -> str | None:
    if node is None:
        return None
    return node.get(f"{{{W_NS}}}val")


def _paragraph_text(p: etree._Element) -> str:
    return "".join(t.text or "" for t in p.xpath(".//w:t", namespaces=NS)).strip()


def _paragraph_style_id(p: etree._Element) -> str | None:
    pstyle = p.find("./w:pPr/w:pStyle", namespaces=NS)
    return _w_val(pstyle)


def _style_name(style: etree._Element) -> str | None:
    name = style.find("./w:name", namespaces=NS)
    return _w_val(name)


def _is_title_style(style: etree._Element) -> bool:
    style_id = style.get(f"{{{W_NS}}}styleId")
    name = _style_name(style)
    return (style_id or "").lower() == "title" or (name or "").lower() == "title"


def _is_heading_style_id(style_id: str | None) -> bool:
    if not style_id:
        return False
    lower = style_id.lower()
    return lower.startswith(("heading", "h1", "h2"))


def _remove_p_borders_from_ppr(ppr: etree._Element | None) -> int:
    if ppr is None:
        return 0
    removed = 0
    for pbdr in list(ppr.findall("./w:pBdr", namespaces=NS)):
        ppr.remove(pbdr)
        removed += 1
    return removed


def _remove_underlines(root: etree._Element) -> int:
    removed = 0
    for underline in list(root.xpath(".//w:u", namespaces=NS)):
        parent = underline.getparent()
        if parent is not None:
            parent.remove(underline)
            removed += 1
    return removed


def sanitize_styles(root: etree._Element) -> Result:
    result = Result()
    for style in root.xpath(".//w:style[@w:type='paragraph']", namespaces=NS):
        if not _is_title_style(style):
            continue
        result.style_borders_removed += _remove_p_borders_from_ppr(
            style.find("./w:pPr", namespaces=NS)
        )
        result.title_underlines_removed += _remove_underlines(style)
    return result


def sanitize_document_part(root: etree._Element, leading_nonempty_paragraphs: int) -> Result:
    result = Result()
    leading_candidates_seen = 0
    still_in_leading_title_block = True

    for p in root.xpath(".//w:p", namespaces=NS):
        style_id = _paragraph_style_id(p)
        text = _paragraph_text(p)

        if still_in_leading_title_block:
            if _is_heading_style_id(style_id) or p.getparent().tag == f"{{{W_NS}}}tc":
                still_in_leading_title_block = False
            elif text:
                leading_candidates_seen += 1
                if leading_candidates_seen > leading_nonempty_paragraphs:
                    still_in_leading_title_block = False

        is_title_style_paragraph = (style_id or "").lower() == "title"
        is_leading_title_block_paragraph = (
            still_in_leading_title_block and leading_candidates_seen <= leading_nonempty_paragraphs
        )

        if is_title_style_paragraph or is_leading_title_block_paragraph:
            result.paragraph_borders_removed += _remove_p_borders_from_ppr(
                p.find("./w:pPr", namespaces=NS)
            )
            if is_title_style_paragraph:
                result.title_underlines_removed += _remove_underlines(p)

    return result


def _add_result(dst: Result, src: Result) -> None:
    dst.style_borders_removed += src.style_borders_removed
    dst.paragraph_borders_removed += src.paragraph_borders_removed
    dst.title_underlines_removed += src.title_underlines_removed


def sanitize_docx(
    input_docx: Path,
    output_docx: Path,
    *,
    leading_nonempty_paragraphs: int,
) -> Result:
    result = Result()
    replacements: dict[str, bytes] = {}

    with zipfile.ZipFile(input_docx, "r") as zin:
        if "word/styles.xml" in zin.namelist():
            styles_root = _read_xml(zin, "word/styles.xml")
            _add_result(result, sanitize_styles(styles_root))
            replacements["word/styles.xml"] = _xml_bytes(styles_root)

        part_names = ["word/document.xml"]
        part_names.extend(
            name
            for name in zin.namelist()
            if name.startswith("word/header") and name.endswith(".xml")
        )
        part_names.extend(
            name
            for name in zin.namelist()
            if name.startswith("word/footer") and name.endswith(".xml")
        )

        for name in part_names:
            if name not in zin.namelist():
                continue
            root = _read_xml(zin, name)
            _add_result(
                result,
                sanitize_document_part(
                    root,
                    leading_nonempty_paragraphs=leading_nonempty_paragraphs,
                ),
            )
            replacements[name] = _xml_bytes(root)

        output_docx.parent.mkdir(parents=True, exist_ok=True)
        with zipfile.ZipFile(output_docx, "w", compression=zipfile.ZIP_DEFLATED) as zout:
            for item in zin.infolist():
                data = replacements.get(item.filename)
                if data is None:
                    data = zin.read(item.filename)
                zout.writestr(item, data)

    return result


def check_docx(input_docx: Path, *, leading_nonempty_paragraphs: int) -> list[str]:
    with tempfile.TemporaryDirectory(prefix="title_sanitize_check_") as td:
        tmp = Path(td) / "sanitized.docx"
        result = sanitize_docx(
            input_docx,
            tmp,
            leading_nonempty_paragraphs=leading_nonempty_paragraphs,
        )
    issues: list[str] = []
    if result.style_borders_removed:
        issues.append("Title paragraph style contains paragraph border residue")
    if result.paragraph_borders_removed:
        issues.append("Title/leading title-block paragraph contains paragraph border residue")
    if result.title_underlines_removed:
        issues.append("Title style/paragraph contains underline residue")
    return issues


def main() -> None:
    ap = argparse.ArgumentParser(
        description="Sanitize or audit Google Docs title border/rule residue in DOCX files."
    )
    ap.add_argument("input_docx")
    ap.add_argument(
        "--out",
        default=None,
        help="Write sanitized DOCX here. Omit with --in-place or --check.",
    )
    ap.add_argument(
        "--in-place",
        action="store_true",
        help="Rewrite the input DOCX atomically after sanitizing.",
    )
    ap.add_argument(
        "--check",
        action="store_true",
        help="Fail if sanitization would remove any title border/rule residue.",
    )
    ap.add_argument(
        "--leading-nonempty-paragraphs",
        type=int,
        default=3,
        help="Treat this many leading non-empty body paragraphs as the title block.",
    )
    args = ap.parse_args()

    input_docx = Path(args.input_docx)
    if args.leading_nonempty_paragraphs < 1:
        ap.error("--leading-nonempty-paragraphs must be >= 1")

    if args.check:
        issues = check_docx(
            input_docx,
            leading_nonempty_paragraphs=args.leading_nonempty_paragraphs,
        )
        if issues:
            for issue in issues:
                print(f"[FAIL] {issue}")
            raise SystemExit(1)
        print("[OK] no Google Docs title border/rule residue detected")
        return

    if args.in_place and args.out:
        ap.error("Use either --in-place or --out, not both")
    if not args.in_place and not args.out:
        ap.error("Provide --out, --in-place, or --check")
    if args.out and Path(args.out).resolve() == input_docx.resolve():
        ap.error("--out must not overwrite the input DOCX; use --in-place instead")

    if args.in_place:
        with tempfile.NamedTemporaryFile(
            suffix=".docx", prefix="title_sanitized_", delete=False
        ) as tmp_file:
            tmp_path = Path(tmp_file.name)
        try:
            result = sanitize_docx(
                input_docx,
                tmp_path,
                leading_nonempty_paragraphs=args.leading_nonempty_paragraphs,
            )
            shutil.move(str(tmp_path), str(input_docx))
            out_path = input_docx
        finally:
            if tmp_path.exists():
                tmp_path.unlink()
    else:
        out_path = Path(args.out)
        result = sanitize_docx(
            input_docx,
            out_path,
            leading_nonempty_paragraphs=args.leading_nonempty_paragraphs,
        )

    print(
        f"[OK] wrote {out_path} | "
        f"style_borders_removed={result.style_borders_removed} "
        f"paragraph_borders_removed={result.paragraph_borders_removed} "
        f"title_underlines_removed={result.title_underlines_removed}"
    )


if __name__ == "__main__":
    main()
