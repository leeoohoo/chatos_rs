// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{anyhow, Result};

use super::escape_xml;
use super::model::PresentationTable;

#[allow(clippy::too_many_arguments)]
pub(super) fn table_shape(
    id: usize,
    name: &str,
    x: i64,
    y: i64,
    cx: i64,
    cy: i64,
    table: &PresentationTable,
) -> Result<String> {
    let first_table_row = table
        .cells
        .first()
        .ok_or_else(|| anyhow!("PPTX table shape requires at least one row"))?;
    let rows = table.cells.len();
    let columns = first_table_row.len();
    let column_widths = distributed_table_sizes(cx, columns)?;
    let row_heights = distributed_table_sizes(cy, rows)?;
    let font_size = if columns >= 12 || rows >= 30 {
        1_000
    } else if columns >= 8 || rows >= 20 {
        1_200
    } else {
        1_400
    };
    let grid = column_widths
        .iter()
        .map(|width| format!("<a:gridCol w=\"{width}\"/>"))
        .collect::<String>();
    let rows_xml = table
        .cells
        .iter()
        .zip(row_heights)
        .enumerate()
        .map(|(row_index, (row, height))| {
            let cells = row
                .iter()
                .map(|cell| {
                    let bold = usize::from(table.header_row && row_index == 0);
                    format!(
                        r#"<a:tc><a:txBody><a:bodyPr/><a:lstStyle/><a:p><a:pPr algn="l"/><a:r><a:rPr lang="zh-CN" sz="{font_size}" b="{bold}"><a:solidFill><a:srgbClr val="1F2937"/></a:solidFill></a:rPr><a:t xml:space="preserve">{}</a:t></a:r><a:endParaRPr lang="zh-CN" sz="{font_size}"/></a:p></a:txBody><a:tcPr marL="45720" marR="45720" marT="22860" marB="22860" anchor="ctr"/></a:tc>"#,
                        escape_xml(cell)
                    )
                })
                .collect::<String>();
            format!("<a:tr h=\"{height}\">{cells}</a:tr>")
        })
        .collect::<String>();
    let first_row = usize::from(table.header_row);
    Ok(format!(
        r#"<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id="{id}" name="{}"/><p:cNvGraphicFramePr/><p:nvPr/></p:nvGraphicFramePr><p:xfrm><a:off x="{x}" y="{y}"/><a:ext cx="{cx}" cy="{cy}"/></p:xfrm><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/table"><a:tbl><a:tblPr firstRow="{first_row}" bandRow="1"><a:tableStyleId>{{5C22544A-7EE6-4342-B048-85BDC9FD1C3A}}</a:tableStyleId></a:tblPr><a:tblGrid>{grid}</a:tblGrid>{rows_xml}</a:tbl></a:graphicData></a:graphic></p:graphicFrame>"#,
        escape_xml(name)
    ))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn chart_shape(
    id: usize,
    name: &str,
    relationship_id: &str,
    x: i64,
    y: i64,
    cx: i64,
    cy: i64,
) -> String {
    format!(
        r#"<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id="{id}" name="{}"/><p:cNvGraphicFramePr/><p:nvPr/></p:nvGraphicFramePr><p:xfrm><a:off x="{x}" y="{y}"/><a:ext cx="{cx}" cy="{cy}"/></p:xfrm><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:chart xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" r:id="{}"/></a:graphicData></a:graphic></p:graphicFrame>"#,
        escape_xml(name),
        escape_xml(relationship_id)
    )
}

fn distributed_table_sizes(total: i64, parts: usize) -> Result<Vec<i64>> {
    let parts = i64::try_from(parts)
        .map_err(|_| anyhow!("PPTX table dimension exceeds the supported range"))?;
    if parts == 0 {
        return Err(anyhow!("PPTX table dimension cannot be empty"));
    }
    let base = total / parts;
    Ok((0..parts)
        .map(|index| {
            if index + 1 == parts {
                total - base * (parts - 1)
            } else {
                base
            }
        })
        .collect())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn text_shape(
    id: usize,
    name: &str,
    x: i64,
    y: i64,
    cx: i64,
    cy: i64,
    font_size: usize,
    text: &str,
    bold: bool,
    color: &str,
    alignment: &str,
    fill: Option<(&str, usize)>,
) -> String {
    let paragraphs = text_paragraphs(text, font_size, bold, color, alignment);
    let fill = fill.map_or_else(
        || "<a:noFill/>".to_string(),
        |(color, alpha)| format!("<a:solidFill><a:srgbClr val=\"{color}\"><a:alpha val=\"{alpha}\"/></a:srgbClr></a:solidFill>"),
    );
    format!(
        r#"<p:sp><p:nvSpPr><p:cNvPr id="{id}" name="{}"/><p:cNvSpPr txBox="1"/><p:nvPr/></p:nvSpPr><p:spPr><a:xfrm><a:off x="{x}" y="{y}"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom>{fill}<a:ln><a:noFill/></a:ln></p:spPr><p:txBody><a:bodyPr wrap="square" lIns="91440" rIns="91440" tIns="45720" bIns="45720"/><a:lstStyle/>{paragraphs}</p:txBody></p:sp>"#,
        escape_xml(name)
    )
}

fn text_paragraphs(
    text: &str,
    font_size: usize,
    bold: bool,
    color: &str,
    alignment: &str,
) -> String {
    if text.is_empty() {
        return format!(
            "<a:p><a:pPr algn=\"{}\"/><a:endParaRPr lang=\"zh-CN\"/></a:p>",
            alignment_code(alignment)
        );
    }
    text.lines()
        .map(|line| {
            let (bullet, line) = line
                .strip_prefix("- ")
                .or_else(|| line.strip_prefix("* "))
                .map_or((false, line), |line| (true, line));
            let paragraph = if bullet {
                format!("<a:pPr algn=\"{}\" marL=\"342900\" indent=\"-285750\"><a:buChar char=\"•\"/></a:pPr>", alignment_code(alignment))
            } else {
                format!("<a:pPr algn=\"{}\"/>", alignment_code(alignment))
            };
            format!(
                "<a:p>{paragraph}<a:r><a:rPr lang=\"zh-CN\" sz=\"{font_size}\" b=\"{}\"><a:solidFill><a:srgbClr val=\"{color}\"/></a:solidFill></a:rPr><a:t xml:space=\"preserve\">{}</a:t></a:r><a:endParaRPr lang=\"zh-CN\"/></a:p>",
                usize::from(bold),
                escape_xml(line)
            )
        })
        .collect()
}

fn alignment_code(value: &str) -> &str {
    match value {
        "center" => "ctr",
        "right" => "r",
        _ => "l",
    }
}
