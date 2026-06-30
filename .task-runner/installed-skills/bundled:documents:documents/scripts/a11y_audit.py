#!/usr/bin/env python3
"""Accessibility (A11y) audit for DOCX with optional safe fixes.

What it checks (high ROI)
-------------------------
- Heading hierarchy: flags skipped heading levels (e.g., Heading 1 -> Heading 3)
- Images missing alt text: checks <wp:docPr descr="..."> on inline/anchor drawings
- Tables missing header flag: checks first row for <w:tblHeader/>
- Non-descriptive hyperlinks: "click here", "here", "link", or raw URLs as visible text

Optional fixes
--------------
- --fix_image_alt from_filename: fill missing alt text using the relationship target filename
- --fix_table_headers first_row: set first row as header row (w:tblHeader)

Notes
-----
This is not a full WCAG checker. It aims for consistent, mechanical checks/fixes
that are stable in headless pipelines.

Usage
-----
python scripts/a11y_audit.py input.docx
python scripts/a11y_audit.py input.docx --fix_image_alt from_filename --out fixed.docx
python scripts/a11y_audit.py input.docx --fix_table_headers first_row --out fixed.docx
"""

from __future__ import annotations

import argparse
import json
import re
import zipfile
from dataclasses import dataclass
from typing import Any

from lxml import etree

W_NS = "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
R_NS = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
REL_NS = "http://schemas.openxmlformats.org/package/2006/relationships"
WP_NS = "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing"
A_NS = "http://schemas.openxmlformats.org/drawingml/2006/main"
PIC_NS = "http://schemas.openxmlformats.org/drawingml/2006/picture"

NS = {"w": W_NS, "r": R_NS, "rel": REL_NS, "wp": WP_NS, "a": A_NS, "pic": PIC_NS}


@dataclass
class Finding:
    severity: str  # high|medium|low
    kind: str
    message: str
    context: dict[str, Any]


NONDESCRIPTIVE = {"click here", "here", "link", "this link"}
URL_RE = re.compile(r"https?://\S+", re.IGNORECASE)


def _read_xml(z: zipfile.ZipFile, name: str) -> etree._Element:
    return etree.fromstring(z.read(name))


def _xml_bytes(root: etree._Element) -> bytes:
    return etree.tostring(root, xml_declaration=True, encoding="UTF-8", standalone="yes")


def _load_document_rels(z: zipfile.ZipFile) -> dict[str, str]:
    """Map rId -> Target for word/document.xml relationships."""
    rels_path = "word/_rels/document.xml.rels"
    if rels_path not in z.namelist():
        return {}
    rels = _read_xml(z, rels_path)
    out: dict[str, str] = {}
    for rel in rels.findall(f"{{{REL_NS}}}Relationship"):
        rid = rel.get("Id")
        tgt = rel.get("Target")
        if rid and tgt:
            out[rid] = tgt
    return out


def _iter_story_parts(z: zipfile.ZipFile) -> list[str]:
    """Return doc parts where content lives (main + headers/footers).

    For A11y, headers/footers matter (images, links).
    """
    parts = ["word/document.xml"]
    for name in z.namelist():
        if re.match(r"word/header\d+\.xml$", name) or re.match(r"word/footer\d+\.xml$", name):
            parts.append(name)
    return parts


def _heading_level_from_style(style_val: str | None) -> int | None:
    if not style_val:
        return None
    m = re.match(r"Heading\s*(\d+)$", style_val)
    if m:
        try:
            return int(m.group(1))
        except ValueError:
            return None
    return None


def audit_headings(root: etree._Element, part: str) -> list[Finding]:
    findings: list[Finding] = []
    last: int | None = None
    for p in root.xpath(".//w:p", namespaces=NS):
        ppr = p.find("w:pPr", namespaces=NS)
        if ppr is None:
            continue
        pstyle = ppr.find("w:pStyle", namespaces=NS)
        lvl = _heading_level_from_style(
            pstyle.get(f"{{{W_NS}}}val") if pstyle is not None else None
        )
        if lvl is None:
            continue
        if last is not None and lvl > last + 1:
            text = "".join([t.text or "" for t in p.xpath(".//w:t", namespaces=NS)])
            findings.append(
                Finding(
                    severity="medium",
                    kind="heading_skip",
                    message=f"Heading level jumped from {last} to {lvl}",
                    context={"part": part, "text": text[:120]},
                )
            )
        last = lvl
    return findings


def audit_images_alt(root: etree._Element, part: str) -> list[Finding]:
    findings: list[Finding] = []
    # Look for wp:docPr under drawings
    for docpr in root.xpath(".//wp:docPr", namespaces=NS):
        descr = docpr.get("descr") or ""
        title = docpr.get("title") or ""
        if (descr.strip() == "") and (title.strip() == ""):
            findings.append(
                Finding(
                    severity="high",
                    kind="image_missing_alt",
                    message="Image missing alt text (descr/title empty)",
                    context={
                        "part": part,
                        "id": docpr.get("id"),
                        "name": docpr.get("name"),
                    },
                )
            )
    return findings


def audit_tables(root: etree._Element, part: str) -> list[Finding]:
    findings: list[Finding] = []
    for tbl in root.xpath(".//w:tbl", namespaces=NS):
        rows = tbl.xpath("./w:tr", namespaces=NS)
        if not rows:
            continue
        first = rows[0]
        trpr = first.find("w:trPr", namespaces=NS)
        has_header = False
        if trpr is not None and trpr.find("w:tblHeader", namespaces=NS) is not None:
            has_header = True
        if not has_header:
            findings.append(
                Finding(
                    severity="medium",
                    kind="table_no_header_row",
                    message="Table first row is not marked as header (w:tblHeader missing)",
                    context={"part": part},
                )
            )
    return findings


def _visible_text_for_hyperlink(h: etree._Element) -> str:
    return "".join([t.text or "" for t in h.xpath(".//w:t", namespaces=NS)]).strip()


def audit_hyperlinks(root: etree._Element, part: str) -> list[Finding]:
    findings: list[Finding] = []
    for h in root.xpath(".//w:hyperlink", namespaces=NS):
        txt = _visible_text_for_hyperlink(h)
        if not txt:
            continue
        low = txt.strip().lower()
        if low in NONDESCRIPTIVE:
            findings.append(
                Finding(
                    severity="medium",
                    kind="hyperlink_nondescriptive",
                    message=f"Non-descriptive hyperlink text: '{txt}'",
                    context={"part": part},
                )
            )
        if URL_RE.fullmatch(txt.strip()):
            findings.append(
                Finding(
                    severity="low",
                    kind="hyperlink_raw_url",
                    message="Hyperlink display text is a raw URL (often less accessible)",
                    context={"part": part, "text": txt[:120]},
                )
            )
    return findings


def _fix_image_alt_from_filename(root: etree._Element, part: str, rels_map: dict[str, str]) -> int:
    """Fill missing docPr descr with image filename when possible."""
    changed = 0
    # Map docPr to embed relationship if possible by walking up to a:blip
    # Pattern: wp:docPr is sibling to a:graphic; inside it, a:blip r:embed="rId.."
    for drawing in root.xpath(".//w:drawing", namespaces=NS):
        docpr = drawing.xpath(".//wp:docPr", namespaces=NS)
        if not docpr:
            continue
        docpr = docpr[0]
        descr = (docpr.get("descr") or "").strip()
        title = (docpr.get("title") or "").strip()
        if descr or title:
            continue
        blips = drawing.xpath(".//a:blip", namespaces=NS)
        rid = None
        if blips:
            rid = blips[0].get(f"{{{R_NS}}}embed")
        filename = None
        if rid and rid in rels_map:
            # Target like media/image1.png
            filename = rels_map[rid].split("/")[-1]
        if filename:
            docpr.set("descr", f"Image: {filename}")
            changed += 1
        else:
            # fallback
            docpr.set("descr", "Image")
            changed += 1
    return changed


def _fix_table_headers_first_row(root: etree._Element) -> int:
    changed = 0
    for tbl in root.xpath(".//w:tbl", namespaces=NS):
        rows = tbl.xpath("./w:tr", namespaces=NS)
        if not rows:
            continue
        first = rows[0]
        trpr = first.find("w:trPr", namespaces=NS)
        if trpr is None:
            trpr = etree.SubElement(first, f"{{{W_NS}}}trPr")
        if trpr.find("w:tblHeader", namespaces=NS) is None:
            etree.SubElement(trpr, f"{{{W_NS}}}tblHeader")
            changed += 1
    return changed


def audit_docx(path: str) -> dict[str, Any]:
    with zipfile.ZipFile(path, "r") as z:
        rels_map = _load_document_rels(z)
        parts = _iter_story_parts(z)
        findings: list[Finding] = []
        for part in parts:
            root = _read_xml(z, part)
            findings += audit_headings(root, part)
            findings += audit_images_alt(root, part)
            findings += audit_tables(root, part)
            findings += audit_hyperlinks(root, part)
    out = {
        "file": path,
        "counts": {
            "high": sum(1 for f in findings if f.severity == "high"),
            "medium": sum(1 for f in findings if f.severity == "medium"),
            "low": sum(1 for f in findings if f.severity == "low"),
        },
        "findings": [f.__dict__ for f in findings],
    }
    return out


def apply_fixes(
    in_docx: str,
    out_docx: str,
    fix_image_alt: str | None,
    fix_table_headers: str | None,
) -> dict[str, int]:
    stats = {"image_alt_filled": 0, "table_headers_set": 0}
    with zipfile.ZipFile(in_docx, "r") as zin:
        rels_map = _load_document_rels(zin)
        parts = _iter_story_parts(zin)
        overrides: dict[str, bytes] = {}

        for part in parts:
            root = _read_xml(zin, part)
            changed = False
            if fix_image_alt == "from_filename":
                n = _fix_image_alt_from_filename(root, part, rels_map)
                if n:
                    stats["image_alt_filled"] += n
                    changed = True
            if fix_table_headers == "first_row":
                n = _fix_table_headers_first_row(root)
                if n:
                    stats["table_headers_set"] += n
                    changed = True
            if changed:
                overrides[part] = _xml_bytes(root)

        with zipfile.ZipFile(out_docx, "w", zipfile.ZIP_DEFLATED) as zout:
            for info in zin.infolist():
                name = info.filename
                if name in overrides:
                    zout.writestr(name, overrides[name])
                else:
                    zout.writestr(name, zin.read(name))
    return stats


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("in_docx")
    ap.add_argument("--fix_image_alt", choices=["from_filename"], help="Apply safe image alt fix")
    ap.add_argument("--fix_table_headers", choices=["first_row"], help="Mark first row as header")
    ap.add_argument("--out", help="Write fixed DOCX")
    ap.add_argument(
        "--out_json",
        help=(
            "Optional path to write the audit report JSON. "
            "When provided, stdout only prints a short summary."
        ),
    )
    args = ap.parse_args()

    if (args.fix_image_alt or args.fix_table_headers) and not args.out:
        raise SystemExit("--out is required when applying fixes")

    if args.fix_image_alt or args.fix_table_headers:
        stats = apply_fixes(args.in_docx, args.out, args.fix_image_alt, args.fix_table_headers)
        print(f"[OK] wrote {args.out} | {stats}")

    report = audit_docx(
        args.out if (args.out and (args.fix_image_alt or args.fix_table_headers)) else args.in_docx
    )

    if args.out_json:
        with open(args.out_json, "w", encoding="utf-8") as f:
            f.write(json.dumps(report, indent=2, ensure_ascii=False))
            f.write("\n")
        print(
            "[a11y] wrote report -> %s | high=%s medium=%s low=%s"
            % (
                args.out_json,
                report["counts"]["high"],
                report["counts"]["medium"],
                report["counts"]["low"],
            )
        )
    else:
        print(json.dumps(report, indent=2, ensure_ascii=False))

    # Exit codes:
    #  - 0: no high-severity findings
    #  - 1: high-severity findings present
    #  - 2: reserved for argparse/usage errors
    if report["counts"]["high"] > 0:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
