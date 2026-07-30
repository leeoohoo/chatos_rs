// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

#[cfg(test)]
use std::collections::{BTreeMap, HashSet};
#[cfg(test)]
use std::fs::File;

use anyhow::Result;
use serde_json::{json, Value};
#[cfg(test)]
use zip::ZipArchive;

use crate::relay::RelayRequest;
use crate::LocalState;

use super::{
    file_size, input_file, input_file_any, optional_bool, read_zip_text, require_extension,
    required_text, safe_workspace_path,
};

mod chart_axes_xml;
mod chart_axis_inspection;
mod chart_context_inspection;
mod chart_data_parse;
mod chart_event_inspection;
mod chart_input;
mod chart_inspection;
mod chart_model;
mod chart_package_inspection;
mod chart_parse;
mod chart_replacement;
mod chart_result_inspection;
mod chart_series_style_inspection;
mod chart_snapshot;
mod chart_structure_inspection;
mod chart_xml;
mod chart_xml_common;
mod drawing_text_edit;
mod image;
mod inspection_common;
mod limits;
mod model;
mod package_edit;
mod package_entries;
mod package_io;
mod package_metadata;
mod package_paths;
mod presentation_inspection;
mod relationship_inspection;
mod render_validation;
mod slide_append;
mod slide_order_operations;
mod slide_parse;
mod slide_selection;
mod slide_shapes;
mod slide_xml;
mod table_cell_operations;
mod table_column_operations;
mod table_edit;
mod table_row_operations;
mod table_scan;
mod table_selection;
mod table_structure;
mod templates;
mod text_edit;
mod text_operations;
mod text_validation;
mod xml_structure;

use chart_axis_inspection::*;
use chart_input::*;
use chart_inspection::inspect_standard_pptx_chart_xml;
use chart_parse::*;
#[cfg(test)]
use drawing_text_edit::*;
use image::*;
use inspection_common::*;
use limits::*;
use model::*;
#[cfg(test)]
use package_edit::*;
use package_entries::*;
use package_io::*;
use package_metadata::*;
use package_paths::*;
use relationship_inspection::*;
pub(super) use render_validation::validate_pptx_for_render;
use slide_parse::*;
use table_scan::*;
use table_structure::*;
use text_edit::*;
use text_validation::*;
use xml_structure::*;

pub(super) fn create_pptx(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let target = required_text(arguments, "target_path")?;
    require_extension(target, ".pptx")?;
    let slides = parse_slides(arguments, state, request)?;
    let entries = presentation_entries(slides.as_slice())?;
    let (path, relative) = safe_workspace_path(state, request, target)?;
    let bytes = write_new_pptx(
        path.as_path(),
        entries,
        optional_bool(arguments, "overwrite"),
    )?;
    Ok(json!({
        "created": true,
        "path": relative,
        "bytes": bytes,
        "slides": slides.len(),
        "layouts": slides.iter().map(|slide| slide.layout.as_str()).collect::<Vec<_>>(),
        "images": slides.iter().filter(|slide| slide.image.is_some()).count(),
        "charts": slides.iter().filter(|slide| slide.chart.is_some()).count(),
        "chart_types": slides.iter().filter_map(|slide| slide.chart.as_ref().map(|chart| chart.chart_type.as_str())).collect::<Vec<_>>(),
        "speaker_notes": slides.iter().filter(|slide| !slide.notes.is_empty()).count(),
        "widescreen": true,
    }))
}

pub(super) fn append_pptx_slides(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    slide_append::append_pptx_slides(arguments, state, request)
}

pub(super) fn replace_pptx_text(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    text_operations::replace_pptx_text(arguments, state, request)
}

pub(super) fn replace_pptx_text_across_runs(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    text_operations::replace_pptx_text_across_runs(arguments, state, request)
}

pub(super) fn inspect_pptx_table(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    table_cell_operations::inspect_pptx_table(arguments, state, request)
}

pub(super) fn copy_pptx_table_cell_format(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    table_cell_operations::copy_pptx_table_cell_format(arguments, state, request)
}

pub(super) fn replace_pptx_table_cell_text(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    table_cell_operations::replace_pptx_table_cell_text(arguments, state, request)
}

pub(super) fn delete_pptx_table_row(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    table_row_operations::delete_pptx_table_row(arguments, state, request)
}

pub(super) fn insert_pptx_table_row(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    table_row_operations::insert_pptx_table_row(arguments, state, request)
}

pub(super) fn move_pptx_table_row(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    table_row_operations::move_pptx_table_row(arguments, state, request)
}

pub(super) fn delete_pptx_table_column(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    table_column_operations::delete_pptx_table_column(arguments, state, request)
}

pub(super) fn insert_pptx_table_column(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    table_column_operations::insert_pptx_table_column(arguments, state, request)
}

pub(super) fn move_pptx_table_column(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    table_column_operations::move_pptx_table_column(arguments, state, request)
}

pub(super) fn replace_pptx_notes_text(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    text_operations::replace_pptx_notes_text(arguments, state, request)
}

pub(super) fn reorder_pptx_slides(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    slide_order_operations::reorder_pptx_slides(arguments, state, request)
}

pub(super) fn delete_pptx_slides(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    slide_order_operations::delete_pptx_slides(arguments, state, request)
}

pub(super) fn inspect_pptx(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    presentation_inspection::inspect_pptx(arguments, state, request)
}

pub(super) fn inspect_pptx_charts(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    chart_inspection::inspect_pptx_charts(arguments, state, request)
}

pub(super) fn replace_pptx_chart(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    chart_replacement::replace_pptx_chart(arguments, state, request)
}

fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn unescape_xml(value: &str) -> String {
    value
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::AtomicBool;

    use base64::engine::general_purpose::STANDARD;
    use base64::Engine as _;

    use super::*;
    use crate::WorkspaceState;

    fn presentation_test_context(root: &Path) -> (LocalState, RelayRequest) {
        let state = LocalState {
            workspaces: vec![WorkspaceState {
                id: "workspace-1".to_string(),
                absolute_root: root.to_path_buf(),
                alias: "test".to_string(),
                fingerprint: "fp".to_string(),
                project_config_trust: None,
            }],
            ..LocalState::default()
        };
        let request = RelayRequest {
            _message_type: "skill_execute_request".to_string(),
            request_id: "request-1".to_string(),
            owner_user_id: Some("owner-1".to_string()),
            device_id: Some("device-1".to_string()),
            workspace_id: "workspace-1".to_string(),
            method: Some("POST".to_string()),
            path: Some("/skills/execute".to_string()),
            headers: BTreeMap::new(),
            body: Value::Null,
        };
        (state, request)
    }

    fn render_validation_fixture_entries(slide_relationships: &str) -> Vec<(String, Vec<u8>)> {
        vec![
            (
                "[Content_Types].xml".to_string(),
                br#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"/><Override PartName="/ppt/slides/slide1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slide+xml"/><Override PartName="/ppt/slideLayouts/slideLayout1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml"/></Types>"#.to_vec(),
            ),
            (
                "_rels/.rels".to_string(),
                br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="ppt/presentation.xml"/></Relationships>"#.to_vec(),
            ),
            (
                "ppt/presentation.xml".to_string(),
                br#"<?xml version="1.0"?><p:presentation xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><p:sldIdLst><p:sldId id="256" r:id="rId1"/></p:sldIdLst><p:sldSz cx="12192000" cy="6858000"/></p:presentation>"#.to_vec(),
            ),
            (
                "ppt/_rels/presentation.xml.rels".to_string(),
                br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="slides/slide1.xml"/></Relationships>"#.to_vec(),
            ),
            (
                "ppt/slides/slide1.xml".to_string(),
                br#"<?xml version="1.0"?><p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:cSld><p:spTree/></p:cSld></p:sld>"#.to_vec(),
            ),
            (
                "ppt/slides/_rels/slide1.xml.rels".to_string(),
                slide_relationships.as_bytes().to_vec(),
            ),
            (
                "ppt/slideLayouts/slideLayout1.xml".to_string(),
                br#"<?xml version="1.0"?><p:sldLayout xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:cSld><p:spTree/></p:cSld></p:sldLayout>"#.to_vec(),
            ),
        ]
    }

    #[test]
    fn render_validation_allows_internal_content_and_external_hyperlinks() {
        let temp = tempfile::tempdir().expect("temp");
        let path = temp.path().join("safe.pptx");
        let relationships = r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout1.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="https://example.com" TargetMode="External"/></Relationships>"#;
        write_new_pptx(
            path.as_path(),
            render_validation_fixture_entries(relationships),
            false,
        )
        .expect("write safe PPTX");
        validate_pptx_for_render(path.as_path()).expect("safe PPTX render validation");
    }

    #[test]
    fn render_validation_rejects_vba_and_embedded_parts() {
        let relationships = r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout1.xml"/></Relationships>"#;
        for (name, entry) in [
            ("active.pptx", "ppt/vbaProject.bin"),
            ("embedded.pptx", "ppt/embeddings/object1.bin"),
        ] {
            let temp = tempfile::tempdir().expect("temp");
            let path = temp.path().join(name);
            let mut entries = render_validation_fixture_entries(relationships);
            entries.push((entry.to_string(), vec![0, 1, 2, 3]));
            write_new_pptx(path.as_path(), entries, false).expect("write unsafe PPTX");
            assert!(validate_pptx_for_render(path.as_path())
                .expect_err("active or embedded content must be rejected")
                .to_string()
                .contains("rejects active, embedded"));
        }
    }

    #[test]
    fn render_validation_rejects_external_non_hyperlink_relationships() {
        let temp = tempfile::tempdir().expect("temp");
        let path = temp.path().join("external-image.pptx");
        let relationships = r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout1.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="https://example.com/image.png" TargetMode="External"/></Relationships>"#;
        write_new_pptx(
            path.as_path(),
            render_validation_fixture_entries(relationships),
            false,
        )
        .expect("write external PPTX");
        assert!(validate_pptx_for_render(path.as_path())
            .expect_err("external image must be rejected")
            .to_string()
            .contains("external non-hyperlink"));
    }

    #[test]
    fn image_fit_is_bounded_and_relationship_targets_stay_inside_package() {
        let image = PresentationImage {
            source_path: "image.png".to_string(),
            bytes: Vec::new(),
            format: PresentationImageFormat::Png,
            width: 1920,
            height: 1080,
            alt_text: "image".to_string(),
            fit: ImageFit::Contain,
        };
        let (x, y, cx, cy, crop) = fitted_image_box(&image, 0, 0, 1_000, 1_000);
        assert_eq!((x, y, cx), (0, 218, 1_000));
        assert_eq!(cy, 563);
        assert!(crop.is_empty());
        assert_eq!(
            resolve_part_target("ppt/slides/slide1.xml", "../notesSlides/notesSlide1.xml")
                .expect("notes target"),
            "ppt/notesSlides/notesSlide1.xml"
        );
        assert!(resolve_part_target("ppt/slides/slide1.xml", "../../../escape.xml").is_err());
    }

    #[test]
    fn exact_text_replacement_decodes_entities_and_never_crosses_runs() {
        let xml = r#"<p:sld xmlns:p="p" xmlns:a="a"><a:t>A &amp; B</a:t><a:t>C</a:t></p:sld>"#;
        let (updated, count, limited) =
            replace_drawing_text_runs(xml, "A & B", "D & E", 10).expect("replace entity text");
        assert_eq!(count, 1);
        assert!(!limited);
        assert!(updated.contains("<a:t>D &amp; E</a:t>"));
        let (_, count, _) =
            replace_drawing_text_runs(xml, "B C", "joined", 10).expect("cross-run scan");
        assert_eq!(count, 0);
    }

    #[test]
    fn cross_run_replacement_rewrites_one_unique_same_format_paragraph() {
        let xml = r#"<p:sld xmlns:p="p" xmlns:a="a"><p:cSld><p:spTree><p:sp><p:txBody><a:p><a:pPr/><a:r><a:rPr lang="zh-CN" sz="2400"/><a:t xml:space="preserve">Prefix </a:t></a:r><a:r><a:rPr lang="zh-CN" sz="2400"/><a:t>Quarter</a:t></a:r><a:r><a:rPr lang="zh-CN" sz="2400"/><a:t>ly Rev</a:t></a:r><a:r><a:rPr lang="zh-CN" sz="2400"/><a:t xml:space="preserve">iew suffix</a:t></a:r><a:endParaRPr/></a:p></p:txBody></p:sp></p:spTree></p:cSld></p:sld>"#;
        let scan = scan_pptx_cross_run_text(xml, "Quarterly Review").expect("scan cross-run text");
        assert_eq!(scan.occurrences, 1);
        let matched = scan.matched.expect("eligible cross-run match");
        let formatting_before = xml.matches("<a:rPr lang=\"zh-CN\" sz=\"2400\"/>").count();
        let (updated, runs_touched, emptied_runs) =
            rewrite_pptx_cross_run_match(xml, &matched, "Annual Summary")
                .expect("rewrite cross-run match");
        assert_eq!(runs_touched, 3);
        assert_eq!(emptied_runs, 1);
        assert_eq!(
            pptx_visible_text(updated.as_str()).expect("updated visible text"),
            "Prefix Annual Summary suffix"
        );
        assert_eq!(
            updated
                .matches("<a:rPr lang=\"zh-CN\" sz=\"2400\"/>")
                .count(),
            formatting_before
        );
    }

    #[test]
    fn cross_run_replacement_rejects_ambiguous_and_different_format_matches() {
        let paragraph = r#"<a:p><a:r><a:rPr b="1"/><a:t>Quarter</a:t></a:r><a:r><a:rPr b="1"/><a:t>ly</a:t></a:r></a:p>"#;
        let ambiguous = format!("<p:sld>{paragraph}{paragraph}</p:sld>");
        let scan =
            scan_pptx_cross_run_text(ambiguous.as_str(), "Quarterly").expect("scan ambiguous text");
        assert_eq!(scan.occurrences, 2);
        assert!(scan.matched.is_none());

        let different = r#"<p:sld><a:p><a:r><a:rPr b="1"/><a:t>Quarter</a:t></a:r><a:r><a:rPr b="0"/><a:t>ly</a:t></a:r></a:p></p:sld>"#;
        let scan =
            scan_pptx_cross_run_text(different, "Quarterly").expect("scan different-format text");
        assert_eq!(scan.occurrences, 1);
        assert!(scan.matched.is_none());
        assert!(scan
            .unsupported_reason
            .is_some_and(|reason| reason.contains("different DrawingML run properties")));
    }

    #[test]
    fn cross_run_replacement_rejects_fields_breaks_and_hyperlinks() {
        for (label, paragraph) in [
            (
                "field",
                r#"<a:p><a:fld id="1" type="slidenum"><a:rPr/><a:t>Quarter</a:t></a:fld><a:r><a:rPr/><a:t>ly</a:t></a:r></a:p>"#,
            ),
            (
                "break",
                r#"<a:p><a:r><a:rPr/><a:t>Quarter</a:t></a:r><a:br/><a:r><a:rPr/><a:t>ly</a:t></a:r></a:p>"#,
            ),
            (
                "hyperlink",
                r#"<a:p><a:r><a:rPr><a:hlinkClick r:id="rId2"/></a:rPr><a:t>Quarter</a:t></a:r><a:r><a:rPr><a:hlinkClick r:id="rId2"/></a:rPr><a:t>ly</a:t></a:r></a:p>"#,
            ),
        ] {
            let xml = format!("<p:sld>{paragraph}</p:sld>");
            let scan = scan_pptx_cross_run_text(xml.as_str(), "Quarterly")
                .unwrap_or_else(|error| panic!("scan {label}: {error}"));
            assert_eq!(scan.occurrences, 1, "{label}");
            assert!(scan.matched.is_none(), "{label}");
            assert!(scan.unsupported_reason.is_some(), "{label}");
        }
    }

    #[test]
    #[ignore = "requires CHATOS_DOCUMENT_RUNTIME_DIR with a real packaged runtime"]
    fn packaged_runtime_smoke_renders_cross_run_replacement() {
        let workspace = tempfile::tempdir().expect("presentation smoke workspace");
        let (state, request) = presentation_test_context(workspace.path());
        create_pptx(
            &json!({
                "target_path":"base.pptx",
                "slides":[
                    {
                        "title":"Cross-run Replacement QA",
                        "body":"Quarterly Review for Visual QA",
                        "notes":"The body must read Annual Summary for Visual QA."
                    },
                    {
                        "title":"Safety Contract",
                        "body":"- Unique selection\n- Same run properties\n- Source remains unchanged"
                    }
                ]
            }),
            &state,
            &request,
        )
        .expect("create cross-run render smoke PPTX");
        let base = workspace.path().join("base.pptx");
        let source = workspace.path().join("source.pptx");
        let mut archive =
            ZipArchive::new(File::open(base.as_path()).expect("base PPTX")).expect("base PPTX ZIP");
        let slide_path = "ppt/slides/slide1.xml";
        let slide_xml = read_zip_text(&mut archive, slide_path).expect("base slide XML");
        drop(archive);
        let text_element = "<a:t xml:space=\"preserve\">Quarterly Review for Visual QA</a:t>";
        let text_start = slide_xml.find(text_element).expect("smoke body text");
        let run_start = slide_xml[..text_start]
            .rfind("<a:r>")
            .expect("smoke body run start");
        let run_close = slide_xml[text_start + text_element.len()..]
            .find("</a:r>")
            .map(|offset| text_start + text_element.len() + offset)
            .expect("smoke body run end");
        let run_end = run_close + "</a:r>".len();
        let properties = &slide_xml[run_start + "<a:r>".len()..text_start];
        let split_runs = ["Quarter", "ly Rev", "iew for Visual QA"]
            .into_iter()
            .map(|chunk| {
                format!("<a:r>{properties}<a:t xml:space=\"preserve\">{chunk}</a:t></a:r>")
            })
            .collect::<String>();
        let split_slide = format!(
            "{}{}{}",
            &slide_xml[..run_start],
            split_runs,
            &slide_xml[run_end..]
        );
        rewrite_pptx_package(
            base.as_path(),
            source.as_path(),
            &BTreeMap::from([(slide_path.to_string(), split_slide.into_bytes())]),
            Vec::new(),
            false,
        )
        .expect("write split-run PPTX fixture");
        let source_before = fs::read(source.as_path()).expect("split source bytes");
        let updated = replace_pptx_text_across_runs(
            &json!({
                "path":"source.pptx",
                "target_path":"replaced.pptx",
                "selection":"Quarterly Review",
                "replacement":"Annual Summary"
            }),
            &state,
            &request,
        )
        .expect("replace cross-run smoke text");
        assert_eq!(updated.get("runs_touched").and_then(Value::as_u64), Some(3));
        assert_eq!(
            fs::read(source.as_path()).expect("source after replacement"),
            source_before
        );
        let inspected = inspect_pptx(&json!({"path":"replaced.pptx"}), &state, &request)
            .expect("inspect cross-run smoke output");
        let preview = inspected
            .pointer("/slide_metadata/0/text_preview")
            .and_then(Value::as_str)
            .expect("smoke text preview");
        assert!(preview.contains("Annual Summary"));
        assert!(preview.contains("for Visual QA"));
        let replaced_path = workspace.path().join("replaced.pptx");
        let mut replaced_archive =
            ZipArchive::new(File::open(replaced_path.as_path()).expect("replaced PPTX"))
                .expect("replaced PPTX ZIP");
        let replaced_slide =
            read_zip_text(&mut replaced_archive, slide_path).expect("replaced slide XML");
        assert!(pptx_visible_text(replaced_slide.as_str())
            .expect("replaced visible text")
            .contains("Annual Summary for Visual QA"));
        drop(replaced_archive);
        let rendered = super::super::docx_render::render_presentation_pages(
            &json!({
                "path":"replaced.pptx",
                "first_slide":1,
                "last_slide":2,
                "dpi":120,
                "pdf_target_path":"replaced.pdf"
            }),
            &state,
            &request,
            Some(&AtomicBool::new(false)),
        )
        .expect("render cross-run smoke PPTX");
        assert_eq!(
            rendered
                .pointer("/_structured_result/pages_total")
                .and_then(Value::as_u64),
            Some(2)
        );
        if let Some(output) = std::env::var_os("CHATOS_PRESENTATION_CROSS_RUN_SMOKE_OUTPUT_DIR") {
            let output = PathBuf::from(output);
            fs::create_dir_all(output.as_path()).expect("create smoke output directory");
            fs::copy(
                workspace.path().join("replaced.pptx"),
                output.join("presentation-cross-run.pptx"),
            )
            .expect("write smoke PPTX");
            fs::copy(
                workspace.path().join("replaced.pdf"),
                output.join("presentation-cross-run.pdf"),
            )
            .expect("write smoke PDF");
            for (index, slide) in rendered
                .get("_model_input")
                .and_then(Value::as_array)
                .expect("smoke slide images")
                .iter()
                .enumerate()
            {
                let encoded = slide
                    .get("image_url")
                    .and_then(Value::as_str)
                    .and_then(|value| value.strip_prefix("data:image/png;base64,"))
                    .expect("smoke PNG data URL");
                fs::write(
                    output.join(format!("slide-{}.png", index + 1)),
                    STANDARD.decode(encoded).expect("decode smoke slide PNG"),
                )
                .expect("write smoke slide PNG");
            }
        }
    }

    #[test]
    fn slide_deletion_rejects_cross_slide_structures_and_removes_exact_entries() {
        let presentation = r#"<p:presentation xmlns:p="p" xmlns:r="r"><p:sldIdLst><p:sldId id="256" r:id="rId1"/></p:sldIdLst><p:custShowLst/></p:presentation>"#;
        assert!(reject_unsupported_slide_deletion_references(presentation)
            .expect_err("custom shows must block deletion")
            .to_string()
            .contains("custom shows"));

        let relationships = r#"<Relationships><Relationship Id="rId1" Type="slide" Target="slides/slide1.xml"/><Relationship Id="rId2" Type="theme" Target="theme/theme1.xml"/></Relationships>"#;
        let updated =
            remove_relationship_entries(relationships, &HashSet::from(["rId1".to_string()]))
                .expect("remove exact slide relationship");
        assert!(!updated.contains("rId1"));
        assert!(updated.contains("rId2"));

        let content_types = r#"<Types><Override PartName="/ppt/slides/slide1.xml" ContentType="slide"/><Override PartName="/ppt/theme/theme1.xml" ContentType="theme"/></Types>"#;
        let updated = remove_content_type_overrides(
            content_types,
            &HashSet::from(["/ppt/slides/slide1.xml".to_string()]),
        )
        .expect("remove exact content type");
        assert!(!updated.contains("slide1.xml"));
        assert!(updated.contains("theme1.xml"));
    }
}
