#!/usr/bin/env python3
"""Content controls (SDTs) helper.

`python-docx` does not support Word content controls (a.k.a. SDTs / forms).
This script provides a pragmatic subset:

- list: enumerate SDTs (tag/alias + current visible text)
- wrap_placeholders: wrap occurrences of `{{TAG}}` in a plain-text SDT tagged TAG
- fill: set the text of SDTs by tag

Notes
-----
- Focuses on plain-text SDTs (`<w:text/>` controls).
- Wrap works best when placeholders are "leaf" tokens, but the script will split
  runs when `{{TAG}}` appears inside longer text.
- By default, we patch `word/document.xml` and also `word/header*.xml`/`word/footer*.xml`.
"""

from __future__ import annotations

import argparse
import json
import re
import tempfile
from copy import deepcopy
from pathlib import Path

from lxml import etree

try:
    # Local helper in this repo.
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
XML_SPACE = "{http://www.w3.org/XML/1998/namespace}space"

PLACEHOLDER_RE = re.compile(r"\{\{([A-Za-z0-9_-]+)\}\}")


def qn(local: str) -> str:
    return f"{{{W_NS}}}{local}"


def _preserve_spaces(t_el: etree._Element, text: str) -> None:
    # Word collapses leading/trailing spaces unless xml:space="preserve".
    if text.startswith(" ") or text.endswith(" "):
        t_el.set(XML_SPACE, "preserve")


def parse_xml(path: Path) -> etree._ElementTree:
    parser = etree.XMLParser(remove_blank_text=False)
    return etree.parse(str(path), parser)


def text_of(el: etree._Element) -> str:
    return "".join(el.xpath(".//w:t/text()", namespaces=NS))


def sdt_tag(sdt: etree._Element) -> str:
    v = sdt.xpath("string(.//w:sdtPr/w:tag/@w:val)", namespaces=NS)
    return v or ""


def sdt_alias(sdt: etree._Element) -> str:
    v = sdt.xpath("string(.//w:sdtPr/w:alias/@w:val)", namespaces=NS)
    return v or ""


def iter_parts(unzipped: Path, include_headers_footers: bool) -> list[Path]:
    parts = [unzipped / "word" / "document.xml"]
    if include_headers_footers:
        parts += sorted((unzipped / "word").glob("header*.xml"))
        parts += sorted((unzipped / "word").glob("footer*.xml"))
    return [p for p in parts if p.exists()]


def make_run(text: str, rpr: etree._Element | None) -> etree._Element:
    r = etree.Element(qn("r"))
    if rpr is not None:
        r.append(deepcopy(rpr))
    t = etree.SubElement(r, qn("t"))
    t.text = text
    _preserve_spaces(t, text)
    return r


def make_sdt(tag: str, placeholder_text: str, rpr: etree._Element | None) -> etree._Element:
    sdt = etree.Element(qn("sdt"))

    sdtPr = etree.SubElement(sdt, qn("sdtPr"))
    tag_el = etree.SubElement(sdtPr, qn("tag"))
    tag_el.set(qn("val"), tag)
    alias_el = etree.SubElement(sdtPr, qn("alias"))
    alias_el.set(qn("val"), tag)
    etree.SubElement(sdtPr, qn("text"))  # plain-text content control

    sdtContent = etree.SubElement(sdt, qn("sdtContent"))
    r = etree.SubElement(sdtContent, qn("r"))
    if rpr is not None:
        r.append(deepcopy(rpr))
    t = etree.SubElement(r, qn("t"))
    t.text = placeholder_text
    _preserve_spaces(t, placeholder_text)
    return sdt


def wrap_placeholders_in_tree(root: etree._Element) -> int:
    """Wrap `{{TAG}}` placeholders found in runs into SDTs.

    Returns number of runs replaced (not number of placeholders).
    """

    changed = 0

    # We operate on runs; a placeholder may appear within a run across multiple w:t.
    runs = root.xpath(".//w:r", namespaces=NS)
    for r in runs:
        t_nodes = r.xpath("./w:t", namespaces=NS)
        if not t_nodes:
            continue
        full = "".join((t.text or "") for t in t_nodes)
        if not full or not PLACEHOLDER_RE.search(full):
            continue

        parent = r.getparent()
        if parent is None:
            continue

        rpr = r.find("w:rPr", namespaces=NS)
        idx = parent.index(r)

        pos = 0
        inserts: list[etree._Element] = []
        for m in PLACEHOLDER_RE.finditer(full):
            if m.start() > pos:
                inserts.append(make_run(full[pos : m.start()], rpr))
            tag = m.group(1)
            inserts.append(make_sdt(tag, m.group(0), rpr))
            pos = m.end()
        if pos < len(full):
            inserts.append(make_run(full[pos:], rpr))

        for node in inserts:
            parent.insert(idx, node)
            idx += 1
        parent.remove(r)
        changed += 1

    return changed


def fill_sdts_in_tree(root: etree._Element, values: dict[str, str]) -> int:
    """Fill SDTs by tag. Returns number of SDTs updated."""

    updated = 0
    sdts = root.xpath(".//w:sdt", namespaces=NS)
    for sdt in sdts:
        tag = sdt_tag(sdt)
        if not tag or tag not in values:
            continue
        value = values[tag]

        sdtContent = sdt.find("w:sdtContent", namespaces=NS)
        if sdtContent is None:
            continue

        # Preserve the first rPr we can find (best-effort).
        rpr = sdtContent.find(".//w:rPr", namespaces=NS)

        # Determine if this SDT is block-level (contains w:p) or inline.
        block = bool(sdtContent.findall("w:p", namespaces=NS))

        for child in list(sdtContent):
            sdtContent.remove(child)

        if block:
            p = etree.SubElement(sdtContent, qn("p"))
            r = etree.SubElement(p, qn("r"))
        else:
            r = etree.SubElement(sdtContent, qn("r"))

        if rpr is not None:
            r.append(deepcopy(rpr))

        t = etree.SubElement(r, qn("t"))
        t.text = value
        _preserve_spaces(t, value)

        updated += 1

    return updated


def cmd_list(docx: Path, include_headers_footers: bool, as_json: bool) -> int:
    with tempfile.TemporaryDirectory(prefix="docx_sdt_") as td:
        unz = Path(td) / "unz"
        unzip_docx(docx, unz)

        rows = []
        for part in iter_parts(unz, include_headers_footers=include_headers_footers):
            tree = parse_xml(part)
            root = tree.getroot()
            for sdt in root.xpath(".//w:sdt", namespaces=NS):
                content = sdt.find("w:sdtContent", namespaces=NS)
                rows.append(
                    {
                        "part": str(part.relative_to(unz)),
                        "tag": sdt_tag(sdt),
                        "alias": sdt_alias(sdt),
                        "text": text_of(content if content is not None else sdt),
                    }
                )
        if as_json:
            print(json.dumps(rows, indent=2, ensure_ascii=False))
        else:
            if not rows:
                print("[content_controls] no SDTs found")
            for r in rows:
                tag = r["tag"] or "(no-tag)"
                alias = r["alias"] or ""
                txt = r["text"].replace("\n", " ")
                print(f"- {r['part']}: tag={tag} alias={alias} text={txt}")

    return 0


def cmd_wrap(docx: Path, out_docx: Path, include_headers_footers: bool) -> int:
    with tempfile.TemporaryDirectory(prefix="docx_sdt_") as td:
        unz = Path(td) / "unz"
        unzip_docx(docx, unz)

        total = 0
        for part in iter_parts(unz, include_headers_footers=include_headers_footers):
            tree = parse_xml(part)
            root = tree.getroot()
            total += wrap_placeholders_in_tree(root)
            tree.write(str(part), xml_declaration=True, encoding="UTF-8", standalone="yes")

        zip_docx(unz, out_docx)
        print(f"[content_controls] wrapped placeholders in {total} run(s); wrote {out_docx}")

    return 0


def _parse_set_args(pairs: list[str]) -> dict[str, str]:
    out: dict[str, str] = {}
    for p in pairs:
        if "=" not in p:
            raise SystemExit(f"Invalid --set '{p}' (expected TAG=VALUE)")
        k, v = p.split("=", 1)
        k = k.strip()
        if not k:
            raise SystemExit(f"Invalid --set '{p}' (empty TAG)")
        out[k] = v
    return out


def cmd_fill(docx: Path, out_docx: Path, include_headers_footers: bool, pairs: list[str]) -> int:
    values = _parse_set_args(pairs)

    with tempfile.TemporaryDirectory(prefix="docx_sdt_") as td:
        unz = Path(td) / "unz"
        unzip_docx(docx, unz)

        total = 0
        for part in iter_parts(unz, include_headers_footers=include_headers_footers):
            tree = parse_xml(part)
            root = tree.getroot()
            total += fill_sdts_in_tree(root, values)
            tree.write(str(part), xml_declaration=True, encoding="UTF-8", standalone="yes")

        zip_docx(unz, out_docx)
        print(f"[content_controls] filled {total} SDT(s); wrote {out_docx}")

    return 0


def main() -> None:
    ap = argparse.ArgumentParser(description="Work with Word content controls (SDTs) in a DOCX")
    ap.add_argument("docx", type=Path, help="Input .docx")

    sub = ap.add_subparsers(dest="cmd", required=True)

    ap_list = sub.add_parser("list", help="List SDTs")
    ap_list.add_argument("--no_headers_footers", action="store_true")
    ap_list.add_argument("--json", action="store_true", help="JSON output")

    ap_wrap = sub.add_parser("wrap_placeholders", help="Wrap {{TAG}} placeholders into SDTs")
    ap_wrap.add_argument("--output", required=True, type=Path)
    ap_wrap.add_argument("--no_headers_footers", action="store_true")

    ap_fill = sub.add_parser("fill", help="Fill SDTs by tag")
    ap_fill.add_argument("--output", required=True, type=Path)
    ap_fill.add_argument(
        "--set",
        action="append",
        default=[],
        help="Set TAG=VALUE (repeatable)",
    )
    ap_fill.add_argument("--no_headers_footers", action="store_true")

    args = ap.parse_args()

    if args.cmd == "list":
        raise SystemExit(
            cmd_list(
                args.docx,
                include_headers_footers=not args.no_headers_footers,
                as_json=args.json,
            )
        )
    if args.cmd == "wrap_placeholders":
        raise SystemExit(
            cmd_wrap(
                args.docx,
                args.output,
                include_headers_footers=not args.no_headers_footers,
            )
        )
    if args.cmd == "fill":
        if not args.set:
            raise SystemExit("fill requires at least one --set TAG=VALUE")
        raise SystemExit(
            cmd_fill(
                args.docx,
                args.output,
                include_headers_footers=not args.no_headers_footers,
                pairs=args.set,
            )
        )


if __name__ == "__main__":
    main()
