#!/usr/bin/env python3
"""Exact Word table geometry helpers for python-docx.

python-docx can set visible table widths, but consistent Word/LibreOffice/
Google Docs rendering requires the same width to be present in three OOXML
places:
- table width: w:tblPr/w:tblW
- table indent: w:tblPr/w:tblInd
- grid columns: w:tblGrid/w:gridCol
- every cell: w:tcPr/w:tcW

Use apply_table_geometry() after all rows are created.
"""

from __future__ import annotations

import argparse
import xml.etree.ElementTree as ET
from pathlib import Path
from typing import Iterable, Sequence
from zipfile import ZipFile

from docx.enum.table import WD_TABLE_ALIGNMENT
from docx.oxml import OxmlElement
from docx.oxml.ns import qn
from docx.shared import Twips

DEFAULT_CONTENT_WIDTH_DXA = 9360
DEFAULT_CELL_MARGINS_DXA = {"top": 80, "bottom": 80, "start": 120, "end": 120}
# Word positions the visible table border separately from the first cell's
# text inset. Match the default start cell margin so the outer border aligns
# with surrounding paragraph text instead of sitting one cell margin to the left.
DEFAULT_TABLE_INDENT_DXA = DEFAULT_CELL_MARGINS_DXA["start"]
W_NS = "http://schemas.openxmlformats.org/wordprocessingml/2006/main"


def length_to_dxa(length) -> int:
    """Return a python-docx Length in DXA/twips."""

    try:
        return int(round(length.twips))
    except Exception:
        return int(round(float(length)))


def section_content_width_dxa(section) -> int:
    """Compute usable section width from page width minus side margins."""

    return (
        length_to_dxa(section.page_width)
        - length_to_dxa(section.left_margin)
        - length_to_dxa(section.right_margin)
    )


def column_widths_from_weights(
    weights: Sequence[float], total_width_dxa: int = DEFAULT_CONTENT_WIDTH_DXA
) -> list[int]:
    """Allocate integer DXA widths whose sum is exactly total_width_dxa."""

    if not weights:
        raise ValueError("weights must not be empty")
    if any(weight <= 0 for weight in weights):
        raise ValueError("all weights must be positive")

    total_weight = float(sum(weights))
    widths = [int(round(total_width_dxa * (weight / total_weight))) for weight in weights]
    widths[-1] += total_width_dxa - sum(widths)
    if any(width <= 0 for width in widths):
        raise ValueError(f"invalid computed widths: {widths}")
    return widths


def exact_column_widths(
    widths_dxa: Iterable[int], total_width_dxa: int = DEFAULT_CONTENT_WIDTH_DXA
) -> list[int]:
    """Return integer widths adjusted so their sum exactly matches total_width_dxa."""

    widths = [int(width) for width in widths_dxa]
    if not widths:
        raise ValueError("widths_dxa must not be empty")
    if any(width <= 0 for width in widths):
        raise ValueError("all column widths must be positive")
    widths[-1] += total_width_dxa - sum(widths)
    if any(width <= 0 for width in widths):
        raise ValueError(f"invalid adjusted widths: {widths}")
    return widths


def _ensure_child(parent, tag: str):
    child = parent.find(qn(tag))
    if child is None:
        child = OxmlElement(tag)
        parent.append(child)
    return child


def _set_width(parent, tag: str, width_dxa: int) -> None:
    width = _ensure_child(parent, tag)
    width.set(qn("w:type"), "dxa")
    width.set(qn("w:w"), str(int(width_dxa)))


def _set_cell_margins(cell, margins_dxa: dict[str, int]) -> None:
    tc_pr = cell._tc.get_or_add_tcPr()
    tc_mar = _ensure_child(tc_pr, "w:tcMar")
    for side in ("top", "bottom", "start", "end"):
        margin = _ensure_child(tc_mar, f"w:{side}")
        margin.set(qn("w:w"), str(int(margins_dxa[side])))
        margin.set(qn("w:type"), "dxa")


def _replace_table_grid(table, column_widths_dxa: Sequence[int]) -> None:
    tbl = table._tbl
    grid = tbl.tblGrid
    for child in list(grid):
        grid.remove(child)
    for width in column_widths_dxa:
        grid_col = OxmlElement("w:gridCol")
        grid_col.set(qn("w:w"), str(int(width)))
        grid.append(grid_col)


def apply_table_geometry(
    table,
    column_widths_dxa: Sequence[int],
    *,
    table_width_dxa: int | None = None,
    indent_dxa: int | None = None,
    cell_margins_dxa: dict[str, int] | None = None,
) -> None:
    """Apply exact, full-table Word geometry to a python-docx table.

    Call this after creating all rows. column_widths_dxa is the source of truth:
    table width, grid columns, and every cell width are synchronized from it.
    By default, the visible table border is indented by the start cell margin
    so it aligns with surrounding paragraph text. Pass indent_dxa=0 only when
    that legacy table-edge alignment is intentional.
    """

    widths = [int(width) for width in column_widths_dxa]
    if not widths:
        raise ValueError("column_widths_dxa must not be empty")
    if any(width <= 0 for width in widths):
        raise ValueError("all column widths must be positive")

    width_total = int(table_width_dxa if table_width_dxa is not None else sum(widths))
    if sum(widths) != width_total:
        raise ValueError(
            f"column widths must sum to table_width_dxa: sum={sum(widths)} width={width_total}"
        )

    cell_margins = dict(DEFAULT_CELL_MARGINS_DXA)
    if cell_margins_dxa:
        cell_margins.update({k: int(v) for k, v in cell_margins_dxa.items()})
    resolved_indent_dxa = (
        int(cell_margins.get("start", DEFAULT_TABLE_INDENT_DXA))
        if indent_dxa is None
        else int(indent_dxa)
    )

    table.autofit = False
    table.alignment = WD_TABLE_ALIGNMENT.LEFT

    tbl = table._tbl
    tbl_pr = tbl.tblPr
    _set_width(tbl_pr, "w:tblW", width_total)

    table_indent = _ensure_child(tbl_pr, "w:tblInd")
    table_indent.set(qn("w:type"), "dxa")
    table_indent.set(qn("w:w"), str(resolved_indent_dxa))

    layout = _ensure_child(tbl_pr, "w:tblLayout")
    layout.set(qn("w:type"), "fixed")

    _replace_table_grid(table, widths)

    for col_idx, width in enumerate(widths):
        table.columns[col_idx].width = Twips(width)

    for row in table.rows:
        if len(row.cells) != len(widths):
            raise ValueError(
                "apply_table_geometry expects unmerged rows: "
                f"row has {len(row.cells)} cells, expected {len(widths)}"
            )
        row.height = None
        for col_idx, cell in enumerate(row.cells):
            width = widths[col_idx]
            cell.width = Twips(width)
            tc_pr = cell._tc.get_or_add_tcPr()
            _set_width(tc_pr, "w:tcW", width)
            _set_cell_margins(cell, cell_margins)


def audit_docx_tables(path: Path) -> int:
    """Print table width/indent/grid/cell-width checks. Returns issue count."""

    ns = {"w": W_NS}
    attr = lambda name: f"{{{W_NS}}}{name}"
    issues = 0

    with ZipFile(path) as zf:
        root = ET.fromstring(zf.read("word/document.xml"))

    for table_idx, tbl in enumerate(root.findall(".//w:tbl", ns), start=1):
        tbl_w = tbl.find("w:tblPr/w:tblW", ns)
        width = int(tbl_w.get(attr("w"), "0")) if tbl_w is not None else 0
        width_type = tbl_w.get(attr("type")) if tbl_w is not None else None
        tbl_ind = tbl.find("w:tblPr/w:tblInd", ns)
        indent = int(tbl_ind.get(attr("w"), "0")) if tbl_ind is not None else None
        indent_type = tbl_ind.get(attr("type")) if tbl_ind is not None else None
        grid = [int(col.get(attr("w"), "0")) for col in tbl.findall("w:tblGrid/w:gridCol", ns)]
        grid_sum = sum(grid)
        start_margins = []
        for tc in tbl.findall(".//w:tc", ns):
            start_margin = tc.find("w:tcPr/w:tcMar/w:start", ns)
            if start_margin is not None:
                start_margins.append(int(start_margin.get(attr("w"), "0")))
        expected_indent = start_margins[0] if start_margins else DEFAULT_TABLE_INDENT_DXA

        print(
            f"table {table_idx}: tblW={width_type}:{width} "
            f"tblInd={indent_type}:{indent} grid_sum={grid_sum} grid={grid}"
        )
        if width_type != "dxa" or width <= 0:
            print("  ISSUE: table width is missing or not DXA")
            issues += 1
        if indent_type != "dxa" or indent is None:
            print("  ISSUE: table indent is missing or not DXA")
            issues += 1
        elif indent != expected_indent:
            print(
                "  ISSUE: table indent should match start cell margin "
                f"({expected_indent} DXA) so the visible border aligns with body text"
            )
            issues += 1
        if len(set(start_margins)) > 1:
            print("  ISSUE: start cell margins are inconsistent within the table")
            issues += 1
        if grid_sum != width:
            print("  ISSUE: grid column sum does not equal table width")
            issues += 1

        for row_idx, tr in enumerate(tbl.findall("w:tr", ns), start=1):
            cell_widths = []
            for tc in tr.findall("w:tc", ns):
                tc_w = tc.find("w:tcPr/w:tcW", ns)
                cell_widths.append(int(tc_w.get(attr("w"), "0")) if tc_w is not None else 0)
            print(f"  row {row_idx}: tcW={cell_widths} sum={sum(cell_widths)}")
            if cell_widths != grid:
                print("  ISSUE: row cell widths do not match grid columns")
                issues += 1

    if issues == 0:
        print("OK: all tables have matching tblW, tblInd, tblGrid, and tcW")
    else:
        print(f"ISSUES: {issues}")
    return issues


def main() -> None:
    parser = argparse.ArgumentParser(description="Audit exact DOCX table geometry")
    parser.add_argument("docx", type=Path)
    args = parser.parse_args()
    raise SystemExit(1 if audit_docx_tables(args.docx) else 0)


if __name__ == "__main__":
    main()
