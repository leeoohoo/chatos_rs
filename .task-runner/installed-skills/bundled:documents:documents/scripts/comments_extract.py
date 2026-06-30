#!/usr/bin/env python3
"""Extract DOCX comments into JSON (with anchored snippet).

Why
---
Agents often need to summarize or triage reviewer feedback. Word comments are
stored in OOXML (word/comments.xml) and anchored in story parts via:
  - w:commentRangeStart / w:commentRangeEnd
  - w:commentReference

This helper produces a best-effort mapping:
- comment id -> comment text/author/date/(resolved)
- anchored snippet around the comment range (when possible)

Limitations
-----------
- If the doc uses complex anchoring, the snippet may be approximate.
- Comments in shapes/textboxes are not handled.

Usage
-----
python scripts/comments_extract.py input.docx --out comments.json
"""

from __future__ import annotations

import argparse
import json
import re
import zipfile
from typing import Any

from lxml import etree

W_NS = "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
REL_NS = "http://schemas.openxmlformats.org/package/2006/relationships"

NS = {"w": W_NS, "rel": REL_NS}


def _w(tag: str) -> str:
    return f"{{{W_NS}}}{tag}"


def _read_xml(z: zipfile.ZipFile, name: str) -> etree._Element:
    return etree.fromstring(z.read(name))


def _iter_story_parts(z: zipfile.ZipFile) -> list[str]:
    parts = ["word/document.xml"]
    for name in z.namelist():
        if re.match(r"word/header\d+\.xml$", name) or re.match(r"word/footer\d+\.xml$", name):
            parts.append(name)
    return parts


def _paragraph_plain_text(p: etree._Element) -> str:
    """Text for a single <w:p>, preserving tabs/linebreaks best-effort."""
    out: list[str] = []
    for el in p.iter():
        if el.tag == _w("t"):
            out.append(el.text or "")
        elif el.tag == _w("tab"):
            out.append("\t")
        elif el.tag in {_w("br"), _w("cr")}:
            out.append("\n")
    return "".join(out).strip()


def _comment_text(c: etree._Element) -> str:
    """Return comment body with paragraph breaks preserved.

    Word comment bodies are composed of one or more <w:p>. Concatenating all
    <w:t> nodes (the old behavior) smears paragraphs together, which makes the
    JSON hard to read and breaks patch workflows.
    """
    paras = [
        _paragraph_plain_text(p)
        for p in c.xpath("./w:p", namespaces=NS)
        if _paragraph_plain_text(p)
    ]
    if paras:
        return "\n".join(paras).strip()

    # Fallback for unusual comment bodies
    return "".join([t.text or "" for t in c.xpath(".//w:t", namespaces=NS)]).strip()


def _load_comments(z: zipfile.ZipFile) -> dict[str, dict[str, Any]]:
    if "word/comments.xml" not in z.namelist():
        return {}
    root = _read_xml(z, "word/comments.xml")
    out: dict[str, dict[str, Any]] = {}
    for c in root.xpath(".//w:comment", namespaces=NS):
        cid = c.get(f"{{{W_NS}}}id")
        if cid is None:
            continue
        done = c.get(f"{{{W_NS}}}done")
        out[cid] = {
            "id": cid,
            "author": c.get(f"{{{W_NS}}}author"),
            "date": c.get(f"{{{W_NS}}}date"),
            "initials": c.get(f"{{{W_NS}}}initials"),
            "resolved": done == "1",
            "text": _comment_text(c),
        }
    return out


def _collect_ranges(root: etree._Element) -> dict[str, dict[str, Any]]:
    """Return commentId -> {part-level anchors} with best-effort snippet."""
    ranges: dict[str, dict[str, Any]] = {}
    # Identify paragraphs that contain commentRangeStart/End
    for p in root.xpath(".//w:p", namespaces=NS):
        starts = p.xpath(".//w:commentRangeStart", namespaces=NS)
        ends = p.xpath(".//w:commentRangeEnd", namespaces=NS)
        if not starts and not ends:
            continue
        ptxt = _paragraph_plain_text(p)
        for s in starts:
            cid = s.get(f"{{{W_NS}}}id")
            if cid:
                ranges.setdefault(cid, {}).setdefault("paragraphs", []).append(
                    {"where": "start", "text": ptxt[:200]}
                )
        for e in ends:
            cid = e.get(f"{{{W_NS}}}id")
            if cid:
                ranges.setdefault(cid, {}).setdefault("paragraphs", []).append(
                    {"where": "end", "text": ptxt[:200]}
                )
    return ranges


def extract(in_docx: str) -> dict[str, Any]:
    with zipfile.ZipFile(in_docx, "r") as z:
        comments = _load_comments(z)
        anchors: dict[str, list[dict[str, Any]]] = {k: [] for k in comments}
        for part in _iter_story_parts(z):
            root = _read_xml(z, part)
            ranges = _collect_ranges(root)
            for cid, info in ranges.items():
                anchors.setdefault(cid, []).append({"part": part, **info})

    out = {
        "file": in_docx,
        "comment_count": len(comments),
        "comments": [],
    }
    for cid, c in sorted(comments.items(), key=lambda kv: int(kv[0]) if kv[0].isdigit() else kv[0]):
        out["comments"].append({**c, "anchors": anchors.get(cid, [])})
    return out


def main() -> None:
    ap = argparse.ArgumentParser(description="Extract DOCX comments into JSON")
    ap.add_argument("in_docx")
    ap.add_argument("--out", required=True)
    args = ap.parse_args()

    report = extract(args.in_docx)
    with open(args.out, "w", encoding="utf-8") as f:
        json.dump(report, f, indent=2, ensure_ascii=False)
    print(f"[OK] wrote {args.out} ({report['comment_count']} comments)")


if __name__ == "__main__":
    main()
