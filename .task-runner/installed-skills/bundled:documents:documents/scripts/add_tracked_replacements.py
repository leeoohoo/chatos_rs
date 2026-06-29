#!/usr/bin/env python3
"""Create tracked-change *replacements* in a DOCX by OOXML patching.

The v6 skill includes tools to enable tracking and to accept tracked changes,
but it doesn't provide a direct way to *generate* tracked insertions/deletions.
This helper adds simple replacements (old -> new) as `<w:del>` + `<w:ins>`.

Scope
-----
- Best-effort: only replaces occurrences within a single `w:t` text node.
- Enables `w:trackRevisions` in settings.xml.

Usage
-----
python scripts/add_tracked_replacements.py in.docx --out out.docx \
  --replace "foo=bar" --replace "old phrase=new phrase" \
  --author "Reviewer"
"""

from __future__ import annotations

import argparse
import datetime as _dt
import zipfile

from lxml import etree

W_NS = "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
NS = {"w": W_NS}


def w(tag: str) -> str:
    return f"{{{W_NS}}}{tag}"


def _xml_bytes(root: etree._Element) -> bytes:
    return etree.tostring(root, xml_declaration=True, encoding="UTF-8", standalone="yes")


def _enable_track(settings_root: etree._Element) -> bool:
    if settings_root.find("w:trackRevisions", namespaces=NS) is not None:
        return False
    settings_root.insert(0, etree.Element(w("trackRevisions")))
    return True


def _next_change_id(doc_root: etree._Element) -> int:
    ids = []
    for el in doc_root.xpath(".//*[@w:id]", namespaces=NS):
        try:
            ids.append(int(el.get(w("id"))))
        except Exception:
            pass
    return (max(ids) + 1) if ids else 1


def _make_del(text: str, cid: int, author: str, when: str) -> etree._Element:
    d = etree.Element(w("del"))
    d.set(w("id"), str(cid))
    d.set(w("author"), author)
    d.set(w("date"), when)
    r = etree.SubElement(d, w("r"))
    dt = etree.SubElement(r, w("delText"))
    dt.text = text
    return d


def _make_ins(text: str, cid: int, author: str, when: str) -> etree._Element:
    ins = etree.Element(w("ins"))
    ins.set(w("id"), str(cid))
    ins.set(w("author"), author)
    ins.set(w("date"), when)
    r = etree.SubElement(ins, w("r"))
    t = etree.SubElement(r, w("t"))
    t.text = text
    return ins


def _replace_in_text_node(
    t_node: etree._Element, old: str, new: str, cid_start: int, author: str, when: str
) -> tuple[int, bool]:
    txt = t_node.text or ""
    if old not in txt:
        return cid_start, False
    # Only handle a single occurrence per node to keep ids simple/deterministic.
    before, after = txt.split(old, 1)
    parent_r = t_node.getparent()  # w:r
    if parent_r is None:
        return cid_start, False
    run_parent = parent_r.getparent()
    if run_parent is None:
        return cid_start, False

    idx = run_parent.index(parent_r)

    # Replace the run with: [before run] <w:del>old</w:del> <w:ins>new</w:ins> [after run]
    rpr = parent_r.find("w:rPr", namespaces=NS)

    def make_run(s: str) -> etree._Element:
        r = etree.Element(w("r"))
        if rpr is not None:
            r.append(etree.fromstring(etree.tostring(rpr)))
        t = etree.SubElement(r, w("t"))
        t.text = s
        return r

    inserts = []
    if before:
        inserts.append(make_run(before))
    d = _make_del(old, cid_start, author, when)
    cid_start += 1
    ins = _make_ins(new, cid_start, author, when)
    cid_start += 1
    inserts.extend([d, ins])
    if after:
        inserts.append(make_run(after))

    for node in inserts[::-1]:
        run_parent.insert(idx, node)
    run_parent.remove(parent_r)
    return cid_start, True


def add_tracked_replacements(
    in_docx: str, out_docx: str, replaces: list[tuple[str, str]], author: str
) -> None:
    when = _dt.datetime.utcnow().replace(microsecond=0).isoformat() + "Z"
    with zipfile.ZipFile(in_docx, "r") as zin:
        overrides: dict[str, bytes] = {}

        doc_root = etree.fromstring(zin.read("word/document.xml"))
        settings_name = "word/settings.xml"
        settings_root = (
            etree.fromstring(zin.read(settings_name))
            if settings_name in zin.namelist()
            else etree.Element(w("settings"))
        )

        _enable_track(settings_root)
        cid = _next_change_id(doc_root)
        total = 0

        for old, new in replaces:
            for t in doc_root.xpath(".//w:t", namespaces=NS):
                cid, changed = _replace_in_text_node(t, old, new, cid, author=author, when=when)
                if changed:
                    total += 1
                    break

        overrides["word/document.xml"] = _xml_bytes(doc_root)
        overrides[settings_name] = _xml_bytes(settings_root)

        with zipfile.ZipFile(out_docx, "w", zipfile.ZIP_DEFLATED) as zout:
            for info in zin.infolist():
                name = info.filename
                if name in overrides:
                    zout.writestr(name, overrides[name])
                else:
                    zout.writestr(name, zin.read(name))

    print(f"[OK] wrote {out_docx} (replacements={total})")


def main() -> None:
    ap = argparse.ArgumentParser(description="Add tracked replacement edits (best-effort)")
    ap.add_argument("in_docx")
    ap.add_argument("--out", required=True)
    ap.add_argument("--author", default="ChatGPT")
    ap.add_argument(
        "--replace",
        action="append",
        default=[],
        help="Replacement formatted as OLD=NEW (repeatable)",
    )
    args = ap.parse_args()

    replaces: list[tuple[str, str]] = []
    for rpl in args.replace:
        if "=" not in rpl:
            raise SystemExit("--replace must be formatted as OLD=NEW")
        old, new = rpl.split("=", 1)
        replaces.append((old, new))
    if not replaces:
        raise SystemExit("Provide at least one --replace")

    add_tracked_replacements(args.in_docx, args.out, replaces, author=args.author)


if __name__ == "__main__":
    main()
