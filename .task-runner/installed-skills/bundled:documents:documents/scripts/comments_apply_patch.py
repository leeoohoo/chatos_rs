#!/usr/bin/env python3
"""Apply lifecycle edits to existing Word comments.

Supports common operations (by comment id):
 - append additional paragraph(s) to an existing comment
 - replace the entire comment text
 - mark a comment as resolved (`w:done="1"`)

Input patch format (JSON)
-------------------------
{
  "ops": [
    {"id": 2, "append": "Follow-up note"},
    {"id": 2, "append": "Second line\nThird line"},
    {"id": 3, "replace": "New full comment body"},
    {"id": 3, "resolved": true}
  ]
}

Notes
-----
- "append" and "replace" accept newlines; each line becomes a separate paragraph.
- "replace" clears existing paragraphs inside the comment.

Usage
-----
python scripts/comments_apply_patch.py in.docx patch.json --out out.docx
"""

from __future__ import annotations

import argparse
import datetime as _dt
import json
import zipfile

from lxml import etree

W_NS = "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
NS = {"w": W_NS}


def w(tag: str) -> str:
    return f"{{{W_NS}}}{tag}"


def _xml_bytes(root: etree._Element) -> bytes:
    return etree.tostring(root, xml_declaration=True, encoding="UTF-8", standalone="yes")


def _append_para(comment: etree._Element, text: str) -> None:
    p = etree.SubElement(comment, w("p"))
    r = etree.SubElement(p, w("r"))
    t = etree.SubElement(r, w("t"))
    t.text = text


def _split_lines(text: str) -> list[str]:
    # Keep empty lines out (Word comments don't love empty paragraphs).
    lines = [ln.strip("\r") for ln in str(text).split("\n")]
    return [ln for ln in (l.strip() for l in lines) if ln != ""]


def _set_comment_text(comment: etree._Element, text: str) -> None:
    # Remove existing paragraphs.
    for p in list(comment.findall("w:p", namespaces=NS)):
        comment.remove(p)

    lines = _split_lines(text)
    if not lines:
        # Preserve at least one paragraph node.
        _append_para(comment, "")
        return

    for ln in lines:
        _append_para(comment, ln)


def apply_patch(in_docx: str, patch_path: str, out_docx: str) -> None:
    patch = json.loads(open(patch_path, "r", encoding="utf-8").read())
    ops = patch.get("ops") or []

    with zipfile.ZipFile(in_docx, "r") as zin:
        if "word/comments.xml" not in zin.namelist():
            raise SystemExit("[comments_apply_patch] word/comments.xml not found")
        root = etree.fromstring(zin.read("word/comments.xml"))

        changed = 0
        touched_ids: set[str] = set()
        for op in ops:
            cid = op.get("id")
            if cid is None:
                continue
            cid_str = str(cid)
            comment = root.find(f".//w:comment[@w:id='{cid_str}']", namespaces=NS)
            if comment is None:
                print(f"[warn] comment id {cid_str} not found")
                continue

            did_any = False

            # Replace takes precedence over append.
            if op.get("replace") is not None:
                _set_comment_text(comment, str(op["replace"]))
                did_any = True

            if op.get("append") is not None:
                for ln in _split_lines(str(op["append"])):
                    _append_para(comment, ln)
                did_any = True

            if op.get("resolved") is True:
                comment.set(w("done"), "1")
                # Touch date to make the change visible.
                comment.set(
                    w("date"),
                    _dt.datetime.utcnow().replace(microsecond=0).isoformat() + "Z",
                )
                did_any = True
            elif op.get("resolved") is False:
                # Allow callers to explicitly clear the resolved state.
                if comment.get(w("done")) is not None:
                    comment.attrib.pop(w("done"), None)
                    comment.set(
                        w("date"),
                        _dt.datetime.utcnow().replace(microsecond=0).isoformat() + "Z",
                    )
                    did_any = True

            if did_any:
                changed += 1
                touched_ids.add(cid_str)

        overrides = {"word/comments.xml": _xml_bytes(root)}
        with zipfile.ZipFile(out_docx, "w", zipfile.ZIP_DEFLATED) as zout:
            for info in zin.infolist():
                name = info.filename
                if name in overrides:
                    zout.writestr(name, overrides[name])
                else:
                    zout.writestr(name, zin.read(name))

    print(f"[OK] wrote {out_docx} (comments_touched={len(touched_ids)} ops_applied={changed})")


def main() -> None:
    ap = argparse.ArgumentParser(description="Append/replace/resolve existing Word comments")
    ap.add_argument("in_docx")
    ap.add_argument("patch_json")
    ap.add_argument("--out", required=True)
    args = ap.parse_args()
    apply_patch(args.in_docx, args.patch_json, args.out)


if __name__ == "__main__":
    main()
