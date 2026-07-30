// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{anyhow, Result};

use super::image::picture_shape;
use super::limits::{MAX_SLIDE_TEXT_CHARS, SLIDE_HEIGHT, SLIDE_WIDTH};
use super::model::{SlideDefinition, SlideLayout};
use super::slide_shapes::{chart_shape, table_shape, text_shape};
use super::templates::group_shape;
use super::text_validation::validate_slide_text;

pub(super) fn slide_xml(
    slide: &SlideDefinition,
    image_relationship: Option<&str>,
    chart_relationship: Option<&str>,
) -> Result<String> {
    let content =
        match slide.layout {
            SlideLayout::TitleBody => format!(
                "{}{}",
                text_shape(
                    2,
                    "Title",
                    685_800,
                    365_760,
                    10_820_400,
                    1_005_840,
                    2_800,
                    slide.title.as_str(),
                    true,
                    "1F2937",
                    "left",
                    None
                ),
                text_shape(
                    3,
                    "Body",
                    914_400,
                    1_554_480,
                    10_363_200,
                    4_572_000,
                    1_800,
                    slide.body.as_str(),
                    false,
                    "1F2937",
                    "left",
                    None
                )
            ),
            SlideLayout::TitleOnly => text_shape(
                2,
                "Title",
                914_400,
                2_194_560,
                10_363_200,
                1_828_800,
                3_200,
                slide.title.as_str(),
                true,
                "1F2937",
                "center",
                None,
            ),
            SlideLayout::Section => format!(
                "{}{}",
                text_shape(
                    2,
                    "Section Title",
                    914_400,
                    1_828_800,
                    10_363_200,
                    1_371_600,
                    3_200,
                    slide.title.as_str(),
                    true,
                    "FFFFFF",
                    "center",
                    Some(("2563EB", 100_000))
                ),
                text_shape(
                    3,
                    "Section Subtitle",
                    1_371_600,
                    3_383_280,
                    9_448_800,
                    1_371_600,
                    2_000,
                    slide.body.as_str(),
                    false,
                    "1F2937",
                    "center",
                    None
                )
            ),
            SlideLayout::TwoColumn => format!(
                "{}{}{}",
                text_shape(
                    2,
                    "Title",
                    685_800,
                    365_760,
                    10_820_400,
                    1_005_840,
                    2_800,
                    slide.title.as_str(),
                    true,
                    "1F2937",
                    "left",
                    None
                ),
                text_shape(
                    3,
                    "Left Column",
                    685_800,
                    1_554_480,
                    5_212_080,
                    4_754_880,
                    1_650,
                    slide.left_body.as_str(),
                    false,
                    "1F2937",
                    "left",
                    None
                ),
                text_shape(
                    4,
                    "Right Column",
                    6_294_120,
                    1_554_480,
                    5_212_080,
                    4_754_880,
                    1_650,
                    slide.right_body.as_str(),
                    false,
                    "1F2937",
                    "left",
                    None
                )
            ),
            SlideLayout::ImageRight => {
                let image = slide.image.as_ref().ok_or_else(|| {
                    anyhow!("PPTX image_right slide is missing its validated image")
                })?;
                let image_relationship = image_relationship.ok_or_else(|| {
                    anyhow!("PPTX image_right slide is missing its image relationship")
                })?;
                format!(
                    "{}{}{}",
                    text_shape(
                        2,
                        "Title",
                        685_800,
                        365_760,
                        10_820_400,
                        1_005_840,
                        2_800,
                        slide.title.as_str(),
                        true,
                        "1F2937",
                        "left",
                        None
                    ),
                    text_shape(
                        3,
                        "Body",
                        685_800,
                        1_554_480,
                        5_029_200,
                        4_754_880,
                        1_650,
                        slide.body.as_str(),
                        false,
                        "1F2937",
                        "left",
                        None
                    ),
                    picture_shape(
                        4,
                        image_relationship,
                        image,
                        6_035_040,
                        1_462_080,
                        5_486_400,
                        4_937_760
                    )
                )
            }
            SlideLayout::ImageFull => {
                let image = slide.image.as_ref().ok_or_else(|| {
                    anyhow!("PPTX image_full slide is missing its validated image")
                })?;
                let image_relationship = image_relationship.ok_or_else(|| {
                    anyhow!("PPTX image_full slide is missing its image relationship")
                })?;
                format!(
                    "{}{}{}",
                    picture_shape(
                        2,
                        image_relationship,
                        image,
                        0,
                        0,
                        SLIDE_WIDTH,
                        SLIDE_HEIGHT
                    ),
                    text_shape(
                        3,
                        "Title Overlay",
                        548_640,
                        365_760,
                        11_094_720,
                        1_097_280,
                        3_000,
                        slide.title.as_str(),
                        true,
                        "FFFFFF",
                        "left",
                        Some(("111827", 70_000))
                    ),
                    text_shape(
                        4,
                        "Body Overlay",
                        548_640,
                        5_120_640,
                        11_094_720,
                        1_188_720,
                        1_600,
                        slide.body.as_str(),
                        false,
                        "FFFFFF",
                        "left",
                        Some(("111827", 70_000))
                    )
                )
            }
            SlideLayout::Table => format!(
                "{}{}",
                text_shape(
                    2,
                    "Title",
                    685_800,
                    365_760,
                    10_820_400,
                    1_005_840,
                    2_800,
                    slide.title.as_str(),
                    true,
                    "1F2937",
                    "left",
                    None
                ),
                table_shape(
                    3,
                    "Table 1",
                    685_800,
                    1_554_480,
                    10_820_400,
                    4_754_880,
                    slide.table.as_ref().ok_or_else(|| anyhow!(
                        "PPTX table slide is missing its validated table"
                    ))?
                )?
            ),
            SlideLayout::Chart => format!(
                "{}{}",
                text_shape(
                    2,
                    "Title",
                    685_800,
                    365_760,
                    10_820_400,
                    1_005_840,
                    2_800,
                    slide.title.as_str(),
                    true,
                    "1F2937",
                    "left",
                    None
                ),
                chart_shape(
                    3,
                    "Chart 1",
                    chart_relationship.ok_or_else(|| anyhow!(
                        "PPTX chart slide is missing its chart relationship"
                    ))?,
                    685_800,
                    1_462_080,
                    10_820_400,
                    4_846_320,
                )
            ),
        };
    Ok(format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><p:sld xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:cSld><p:spTree>{}{content}</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sld>"#,
        group_shape()
    ))
}

pub(super) fn notes_slide_xml(notes: &str, slide_number: usize) -> Result<String> {
    validate_slide_text(notes, "notes", MAX_SLIDE_TEXT_CHARS)?;
    let shape = text_shape(
        2,
        "Speaker Notes",
        685_800,
        1_371_600,
        5_486_400,
        6_858_000,
        1_400,
        notes,
        false,
        "1F2937",
        "left",
        None,
    )
    .replace(
        "<p:nvPr/>",
        "<p:nvPr><p:ph type=\"body\" idx=\"1\"/></p:nvPr>",
    );
    Ok(format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><p:notes xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:cSld name="Slide {slide_number} Notes"><p:spTree>{}{shape}</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:notes>"#,
        group_shape()
    ))
}
