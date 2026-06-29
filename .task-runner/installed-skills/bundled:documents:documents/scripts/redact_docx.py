#!/usr/bin/env python3
"""Redact/anonymize text in a DOCX while preserving layout as much as possible.

High-level approach
-------------------
We patch OOXML directly (document.xml, headers/footers, footnotes/endnotes,
optionally comments) at the paragraph level:

- Concatenate the paragraph's <w:t> nodes to a single string
- Apply regex matches against the full string
- Replace each match with a fixed-length mask (default) so line breaks and pagination are less likely to shift
- Write the modified string back into the original <w:t> node segmentation

This is *not* a cryptographic anonymization tool; it is a practical helper for "redact this doc for sharing".

Safety defaults
---------------
- Default is length-preserving masking (█ repeated) to minimize layout drift.
- If you supply a replacement string and disable length preservation, expect layout shifts.

"""

from __future__ import annotations

import argparse
import re
import tempfile
from dataclasses import dataclass
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
NS = {"w": W_NS}
XML_SPACE = "{http://www.w3.org/XML/1998/namespace}space"


def qn(local: str) -> str:
    return f"{{{W_NS}}}{local}"


@dataclass(frozen=True)
class RedactionRule:
    pattern: re.Pattern[str]
    label: str


def _iter_word_parts(unzipped: Path, include_comments: bool) -> list[Path]:
    word = unzipped / "word"
    parts: list[Path] = []
    for name in [
        "document.xml",
        "footnotes.xml",
        "endnotes.xml",
    ]:
        p = word / name
        if p.exists():
            parts.append(p)

    parts.extend(sorted(word.glob("header*.xml")))
    parts.extend(sorted(word.glob("footer*.xml")))

    if include_comments:
        p = word / "comments.xml"
        if p.exists():
            parts.append(p)

    return parts


def _get_text_nodes_in_paragraph(p: etree._Element) -> list[etree._Element]:
    return list(p.xpath(".//w:t", namespaces=NS))


def _materialize_spaces(t_el: etree._Element) -> None:
    # Preserve leading/trailing spaces in Word by setting xml:space="preserve".
    txt = t_el.text or ""
    if txt.startswith(" ") or txt.endswith(" "):
        t_el.set(XML_SPACE, "preserve")


def _apply_rules_to_paragraph(
    p: etree._Element,
    rules: list[RedactionRule],
    mask_char: str,
    replacement: str | None,
    preserve_length: bool,
) -> int:
    t_nodes = _get_text_nodes_in_paragraph(p)
    if not t_nodes:
        return 0

    original_segments = [n.text or "" for n in t_nodes]
    full = "".join(original_segments)
    if not full:
        return 0

    spans: list[tuple[int, int, str]] = []
    for rule in rules:
        for m in rule.pattern.finditer(full):
            if m.start() == m.end():
                continue
            spans.append((m.start(), m.end(), rule.label))

    if not spans:
        return 0

    # De-overlap: prefer earlier-longer by sorting, then greedily take non-overlapping.
    spans.sort(key=lambda t: (t[0], -(t[1] - t[0])))
    chosen: list[tuple[int, int, str]] = []
    last_end = -1
    for s, e, lbl in spans:
        if s < last_end:
            continue
        chosen.append((s, e, lbl))
        last_end = e

    out = list(full)
    for s, e, _lbl in reversed(chosen):
        length = e - s
        if preserve_length:
            if replacement is None:
                rep = mask_char * length
            else:
                # Repeat/truncate replacement to match length.
                rep = (replacement * ((length // max(1, len(replacement))) + 1))[:length]
            out[s:e] = list(rep)
        else:
            # Non-length-preserving rewrite: we still write back into the existing
            # node segmentation by truncating/blanking. Expect layout drift.
            rep = replacement if replacement is not None else mask_char * length
            out[s:e] = list(rep)

    new_full = "".join(out)

    if preserve_length and len(new_full) != len(full):
        raise RuntimeError("Internal error: expected length-preserving rewrite")

    # Write back into the existing node segmentation.
    idx = 0
    for node, seg in zip(t_nodes, original_segments, strict=True):
        take = len(seg)
        node.text = new_full[idx : idx + take]
        _materialize_spaces(node)
        idx += take

    return len(chosen)


def redact_docx(
    input_docx: Path,
    output_docx: Path,
    rules: list[RedactionRule],
    mask_char: str,
    replacement: str | None,
    preserve_length: bool,
    include_comments: bool,
) -> dict:
    with tempfile.TemporaryDirectory(prefix="docx_redact_") as td:
        tmp = Path(td)
        unzip_docx(input_docx, tmp)

        stats = {
            "file": str(input_docx),
            "parts_processed": 0,
            "paragraphs_touched": 0,
            "matches_redacted": 0,
        }

        for part in _iter_word_parts(tmp, include_comments=include_comments):
            parser = etree.XMLParser(remove_blank_text=False)
            tree = etree.parse(str(part), parser)
            root = tree.getroot()

            touched_in_part = 0
            redactions_in_part = 0

            for p in root.xpath(".//w:p", namespaces=NS):
                n = _apply_rules_to_paragraph(
                    p,
                    rules=rules,
                    mask_char=mask_char,
                    replacement=replacement,
                    preserve_length=preserve_length,
                )
                if n:
                    touched_in_part += 1
                    redactions_in_part += n

            if redactions_in_part:
                stats["paragraphs_touched"] += touched_in_part
                stats["matches_redacted"] += redactions_in_part

            stats["parts_processed"] += 1
            tree.write(str(part), xml_declaration=True, encoding="UTF-8", standalone="yes")

        zip_docx(tmp, output_docx)
        return stats


def _compile_rules(args: argparse.Namespace) -> list[RedactionRule]:
    rules: list[RedactionRule] = []

    if args.emails:
        rules.append(
            RedactionRule(
                pattern=re.compile(r"\b[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}\b", re.I),
                label="email",
            )
        )
    if args.phones:
        rules.append(
            RedactionRule(
                pattern=re.compile(
                    r"(?:(?:\+?\d{1,3}[\s-]?)?(?:\(\d{3}\)|\d{3})[\s-]?)\d{3}[\s-]?\d{4}",
                    re.I,
                ),
                label="phone",
            )
        )

    for i, pat in enumerate(args.pattern or []):
        rules.append(RedactionRule(pattern=re.compile(pat), label=f"custom_{i + 1}"))

    if not rules:
        raise SystemExit(
            "No redaction rules specified. Use --emails/--phones and/or --pattern REGEX."
        )
    return rules


def main() -> None:
    ap = argparse.ArgumentParser(description="Redact/anonymize text in a DOCX (OOXML patch).")
    ap.add_argument("input_docx", type=Path)
    ap.add_argument("output_docx", type=Path)

    rule = ap.add_argument_group("rules")
    rule.add_argument("--emails", action="store_true", help="Redact email addresses")
    rule.add_argument("--phones", action="store_true", help="Redact phone-like numbers")
    rule.add_argument(
        "--pattern",
        action="append",
        help="Custom regex to redact (can be repeated)",
    )

    out = ap.add_argument_group("output")
    out.add_argument(
        "--mask_char",
        default="█",
        help="Mask character for length-preserving redaction (default: █)",
    )
    out.add_argument(
        "--replacement",
        default=None,
        help=(
            "Optional replacement string. If --preserve_length (default), it will be repeated/truncated "
            "to match each match's length."
        ),
    )
    out.add_argument(
        "--no_preserve_length",
        action="store_true",
        help="Disable length preservation (may cause layout drift)",
    )
    out.add_argument(
        "--include_comments",
        action="store_true",
        help="Also redact word/comments.xml (if present)",
    )

    args = ap.parse_args()
    rules = _compile_rules(args)

    stats = redact_docx(
        input_docx=args.input_docx,
        output_docx=args.output_docx,
        rules=rules,
        mask_char=args.mask_char,
        replacement=args.replacement,
        preserve_length=not args.no_preserve_length,
        include_comments=args.include_comments,
    )

    import json

    print(json.dumps(stats, indent=2))


if __name__ == "__main__":
    main()
