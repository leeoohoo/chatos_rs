// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use lopdf::{dictionary, Object};
use serde_json::json;
use sha2::{Digest, Sha256};
use uuid::Uuid;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use super::runtime::{current_platform, RUNTIME_MANIFEST_NAME};
use super::*;
use crate::WorkspaceState;

#[test]
fn runtime_manifest_rejects_hash_drift_and_path_traversal() {
    let runtime = tempfile::tempdir().expect("runtime");
    write_executable(
        runtime.path().join("soffice").as_path(),
        "#!/bin/sh\nexit 0\n",
    );
    write_executable(
        runtime.path().join("pdftoppm").as_path(),
        "#!/bin/sh\nexit 0\n",
    );
    write_runtime_manifest(
        runtime.path(),
        "../soffice",
        &"0".repeat(64),
        "pdftoppm",
        &"0".repeat(64),
    );
    let traversal = load_document_runtime(Some(runtime.path())).expect_err("traversal");
    assert!(traversal.to_string().contains("normalized relative path"));

    write_runtime_manifest(
        runtime.path(),
        "soffice",
        &"0".repeat(64),
        "pdftoppm",
        &"0".repeat(64),
    );
    let drift = load_document_runtime(Some(runtime.path())).expect_err("hash drift");
    assert!(drift.to_string().contains("hash does not match"));
}

#[test]
fn fake_verified_runtime_renders_transient_page_and_optional_pdf() {
    let (workspace, state, request) = test_context();
    let runtime = tempfile::tempdir().expect("runtime");
    let fixture_pdf = runtime.path().join("fixture.pdf");
    write_blank_pdf(fixture_pdf.as_path());
    let fixture_png = runtime.path().join("fixture.png");
    fs::write(
        fixture_png.as_path(),
        STANDARD
            .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=")
            .expect("PNG"),
    )
    .expect("write PNG");
    let soffice = runtime.path().join("soffice");
    write_executable(
        soffice.as_path(),
        format!(
            "#!/bin/sh\nout=''\nprevious=''\nfor value in \"$@\"; do\n  if [ \"$previous\" = '--outdir' ]; then out=\"$value\"; fi\n  previous=\"$value\"\ndone\n/bin/cp '{}' \"$out/input.pdf\"\n",
            fixture_pdf.display()
        )
        .as_str(),
    );
    let pdftoppm = runtime.path().join("pdftoppm");
    write_executable(
        pdftoppm.as_path(),
        format!(
            "#!/bin/sh\nprefix=''\nfor value in \"$@\"; do prefix=\"$value\"; done\n/bin/cp '{}' \"${{prefix}}-1.png\"\n",
            fixture_png.display()
        )
        .as_str(),
    );
    write_runtime_manifest(
        runtime.path(),
        "soffice",
        sha256_file(soffice.as_path())
            .expect("soffice hash")
            .as_str(),
        "pdftoppm",
        sha256_file(pdftoppm.as_path())
            .expect("pdftoppm hash")
            .as_str(),
    );
    write_minimal_docx(workspace.join("input.docx").as_path());

    let result = render_docx_pages_with_runtime(
        &json!({
            "path":"input.docx",
            "pdf_target_path":"output/rendered.pdf",
        }),
        &state,
        &request,
        Some(&AtomicBool::new(false)),
        Some(runtime.path()),
    )
    .expect("render DOCX");
    assert_eq!(
        result
            .pointer("/_structured_result/pages_total")
            .and_then(Value::as_u64),
        Some(1)
    );
    assert_eq!(
        result
            .pointer("/_structured_result/visual_review_status")
            .and_then(Value::as_str),
        Some("pending_model_review")
    );
    assert_eq!(
        result
            .pointer("/_structured_result/layout_verified")
            .and_then(Value::as_bool),
        Some(false)
    );
    assert!(result
        .pointer("/_model_input/0/image_url")
        .and_then(Value::as_str)
        .is_some_and(|value| value.starts_with("data:image/png;base64,")));
    assert!(!result
        .pointer("/_structured_result")
        .expect("structured result")
        .to_string()
        .contains("base64"));
    assert!(workspace.join("output/rendered.pdf").is_file());
    let source_before = fs::read(workspace.join("input.docx")).expect("source after render");
    assert!(!source_before.is_empty());
    let _ = fs::remove_dir_all(workspace);
}

#[test]
fn fake_verified_runtime_renders_transient_pdf_page_without_modifying_source() {
    let (workspace, state, request) = test_context();
    let runtime = tempfile::tempdir().expect("runtime");
    let fixture_png = runtime.path().join("fixture.png");
    fs::write(
        fixture_png.as_path(),
        STANDARD
            .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=")
            .expect("PNG"),
    )
    .expect("write PNG");
    let soffice = runtime.path().join("soffice");
    write_executable(soffice.as_path(), "#!/bin/sh\nexit 99\n");
    let pdftoppm = runtime.path().join("pdftoppm");
    write_executable(
        pdftoppm.as_path(),
        format!(
            "#!/bin/sh\nprefix=''\nfor value in \"$@\"; do prefix=\"$value\"; done\n/bin/cp '{}' \"${{prefix}}-1.png\"\n",
            fixture_png.display()
        )
        .as_str(),
    );
    write_runtime_manifest(
        runtime.path(),
        "soffice",
        sha256_file(soffice.as_path())
            .expect("soffice hash")
            .as_str(),
        "pdftoppm",
        sha256_file(pdftoppm.as_path())
            .expect("pdftoppm hash")
            .as_str(),
    );
    let source = workspace.join("input.pdf");
    write_blank_pdf(source.as_path());
    let source_before = fs::read(source.as_path()).expect("source PDF");

    let result = render_pdf_pages_with_runtime(
        &json!({"path":"input.pdf"}),
        &state,
        &request,
        Some(&AtomicBool::new(false)),
        Some(runtime.path()),
    )
    .expect("render PDF");
    assert_eq!(
        result
            .pointer("/_structured_result/pages_total")
            .and_then(Value::as_u64),
        Some(1)
    );
    assert_eq!(
        result
            .pointer("/_structured_result/visual_review_status")
            .and_then(Value::as_str),
        Some("pending_model_review")
    );
    assert_eq!(
        result
            .pointer("/_structured_result/layout_verified")
            .and_then(Value::as_bool),
        Some(false)
    );
    assert!(result
        .pointer("/_model_input/0/image_url")
        .and_then(Value::as_str)
        .is_some_and(|value| value.starts_with("data:image/png;base64,")));
    assert!(!result
        .pointer("/_structured_result")
        .expect("structured result")
        .to_string()
        .contains("base64"));
    assert_eq!(
        fs::read(source.as_path()).expect("source after render"),
        source_before
    );

    let range_error = render_pdf_pages_with_runtime(
        &json!({"path":"input.pdf","first_page":1,"last_page":9}),
        &state,
        &request,
        Some(&AtomicBool::new(false)),
        Some(runtime.path()),
    )
    .expect_err("oversized PDF page range");
    assert!(range_error
        .to_string()
        .contains("pdf_render/page_batch_limit_exceeded"));
    let _ = fs::remove_dir_all(workspace);
}

#[test]
fn fake_verified_runtime_persists_pdf_page_range_in_one_new_directory() {
    let (workspace, state, request) = test_context();
    let runtime = tempfile::tempdir().expect("runtime");
    let fixture_png = runtime.path().join("fixture.png");
    fs::write(
        fixture_png.as_path(),
        STANDARD
            .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=")
            .expect("PNG"),
    )
    .expect("write PNG");
    let soffice = runtime.path().join("soffice");
    write_executable(soffice.as_path(), "#!/bin/sh\nexit 99\n");
    let pdftoppm = runtime.path().join("pdftoppm");
    write_executable(
        pdftoppm.as_path(),
        format!(
            "#!/bin/sh\nprefix=''\nfirst=''\nlast=''\nprevious=''\nfor value in \"$@\"; do\n  if [ \"$previous\" = '-f' ]; then first=\"$value\"; fi\n  if [ \"$previous\" = '-l' ]; then last=\"$value\"; fi\n  prefix=\"$value\"\n  previous=\"$value\"\ndone\npage=\"$first\"\nwhile [ \"$page\" -le \"$last\" ]; do\n  /bin/cp '{}' \"${{prefix}}-${{page}}.png\"\n  page=$((page + 1))\ndone\n",
            fixture_png.display()
        )
        .as_str(),
    );
    write_runtime_manifest(
        runtime.path(),
        "soffice",
        sha256_file(soffice.as_path())
            .expect("soffice hash")
            .as_str(),
        "pdftoppm",
        sha256_file(pdftoppm.as_path())
            .expect("pdftoppm hash")
            .as_str(),
    );
    let source = workspace.join("input.pdf");
    write_blank_pdf_pages(source.as_path(), 3);
    let source_before = fs::read(source.as_path()).expect("source PDF");

    let result = export_pdf_pages_to_png_with_runtime(
        &json!({
            "path":"input.pdf",
            "target_directory":"exports/pages",
            "first_page":2,
            "last_page":3,
            "dpi":200,
            "filename_prefix":"sheet"
        }),
        &state,
        &request,
        Some(&AtomicBool::new(false)),
        Some(runtime.path()),
    )
    .expect("export PDF page images");
    assert_eq!(
        result
            .pointer("/_structured_result/rendered_pages")
            .and_then(Value::as_u64),
        Some(2)
    );
    assert_eq!(
        result
            .pointer("/_structured_result/files/0/path")
            .and_then(Value::as_str),
        Some("exports/pages/sheet-2.png")
    );
    assert_eq!(
        result
            .pointer("/_structured_result/files/1/path")
            .and_then(Value::as_str),
        Some("exports/pages/sheet-3.png")
    );
    assert_eq!(
        result
            .pointer("/_structured_result/visual_review_status")
            .and_then(Value::as_str),
        Some("not_performed")
    );
    assert!(result.get("_model_input").is_none());
    assert!(workspace.join("exports/pages/sheet-2.png").is_file());
    assert!(workspace.join("exports/pages/sheet-3.png").is_file());
    assert_eq!(
        fs::read_dir(workspace.join("exports/pages"))
            .expect("export directory")
            .count(),
        2
    );
    assert_eq!(
        fs::read(source.as_path()).expect("source after export"),
        source_before
    );
    let _ = fs::remove_dir_all(workspace);
}

#[test]
fn pdf_page_export_rejects_unsafe_targets_ranges_prefixes_and_cancellation() {
    let (workspace, state, request) = test_context();
    let source = workspace.join("input.pdf");
    write_blank_pdf(source.as_path());
    fs::create_dir_all(workspace.join("existing")).expect("existing directory");
    fs::write(workspace.join("existing/keep.txt"), b"keep").expect("existing file");

    let existing = export_pdf_pages_to_png_with_runtime(
        &json!({"path":"input.pdf","target_directory":"existing"}),
        &state,
        &request,
        Some(&AtomicBool::new(false)),
        None,
    )
    .expect_err("existing directory must fail before runtime loading");
    assert!(existing.to_string().contains("pdf_render/output_exists"));
    assert_eq!(
        fs::read(workspace.join("existing/keep.txt")).expect("preserved existing file"),
        b"keep"
    );

    let range = export_pdf_pages_to_png_with_runtime(
        &json!({
            "path":"input.pdf",
            "target_directory":"range-output",
            "first_page":1,
            "last_page":51
        }),
        &state,
        &request,
        Some(&AtomicBool::new(false)),
        None,
    )
    .expect_err("oversized page export must fail before runtime loading");
    assert!(range
        .to_string()
        .contains("pdf_render/page_batch_limit_exceeded"));
    assert!(!workspace.join("range-output").exists());

    let prefix = export_pdf_pages_to_png_with_runtime(
        &json!({
            "path":"input.pdf",
            "target_directory":"prefix-output",
            "filename_prefix":"../unsafe"
        }),
        &state,
        &request,
        Some(&AtomicBool::new(false)),
        None,
    )
    .expect_err("unsafe filename prefix must fail before runtime loading");
    assert!(prefix.to_string().contains("pdf_render/invalid_arguments"));
    assert!(!workspace.join("prefix-output").exists());

    let cancelled = export_pdf_pages_to_png_with_runtime(
        &json!({"path":"input.pdf","target_directory":"cancelled-output"}),
        &state,
        &request,
        Some(&AtomicBool::new(true)),
        None,
    )
    .expect_err("pre-cancelled page export must fail");
    assert!(cancelled.to_string().contains("pdf_render/cancelled"));
    assert!(!workspace.join("cancelled-output").exists());

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;

        symlink(workspace.join("existing"), workspace.join("linked-output"))
            .expect("output symlink");
        let symlink_error = export_pdf_pages_to_png_with_runtime(
            &json!({"path":"input.pdf","target_directory":"linked-output"}),
            &state,
            &request,
            Some(&AtomicBool::new(false)),
            None,
        )
        .expect_err("symlink output must fail");
        assert!(symlink_error
            .to_string()
            .contains("pdf_render/output_invalid"));
    }
    let _ = fs::remove_dir_all(workspace);
}

#[test]
fn pdf_page_export_rolls_back_a_new_directory_when_commit_is_cancelled() {
    let workspace = tempfile::tempdir().expect("workspace");
    let bytes = STANDARD
        .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=")
        .expect("PNG");
    let page = RenderedPage {
        number: 1,
        width: 1,
        height: 1,
        sha256: hex::encode(Sha256::digest(bytes.as_slice())),
        bytes,
    };
    let target = workspace.path().join("cancelled-pages");
    let error = persist_new_rendered_page_directory(
        &[page],
        target.as_path(),
        "cancelled-pages",
        "page",
        Some(&AtomicBool::new(true)),
    )
    .expect_err("cancelled commit must fail");
    assert!(error.to_string().contains("documents_render/cancelled"));
    assert!(!target.exists());
}

#[test]
fn fake_verified_runtime_renders_transient_presentation_slide_without_modifying_source() {
    let (workspace, state, request) = test_context();
    let runtime = tempfile::tempdir().expect("runtime");
    let fixture_pdf = runtime.path().join("fixture.pdf");
    write_blank_pdf(fixture_pdf.as_path());
    let fixture_png = runtime.path().join("fixture.png");
    fs::write(
        fixture_png.as_path(),
        STANDARD
            .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=")
            .expect("PNG"),
    )
    .expect("write PNG");
    let soffice = runtime.path().join("soffice");
    write_executable(
        soffice.as_path(),
        format!(
            "#!/bin/sh\nout=''\nprevious=''\nfilter_ok=0\nfor value in \"$@\"; do\n  if [ \"$previous\" = '--outdir' ]; then out=\"$value\"; fi\n  if [ \"$value\" = 'pdf:impress_pdf_Export' ]; then filter_ok=1; fi\n  previous=\"$value\"\ndone\nif [ \"$filter_ok\" -ne 1 ]; then exit 45; fi\n/bin/cp '{}' \"$out/input.pdf\"\n",
            fixture_pdf.display()
        )
        .as_str(),
    );
    let pdftoppm = runtime.path().join("pdftoppm");
    write_executable(
        pdftoppm.as_path(),
        format!(
            "#!/bin/sh\nprefix=''\nfor value in \"$@\"; do prefix=\"$value\"; done\n/bin/cp '{}' \"${{prefix}}-1.png\"\n",
            fixture_png.display()
        )
        .as_str(),
    );
    write_runtime_manifest(
        runtime.path(),
        "soffice",
        sha256_file(soffice.as_path())
            .expect("soffice hash")
            .as_str(),
        "pdftoppm",
        sha256_file(pdftoppm.as_path())
            .expect("pdftoppm hash")
            .as_str(),
    );
    super::super::presentation::create_pptx(
        &json!({
            "target_path":"input.pptx",
            "slides":[{
                "title":"Presentation render smoke",
                "layout":"title_body",
                "body":"Transient slide rendering"
            }]
        }),
        &state,
        &request,
    )
    .expect("create PPTX");
    let source = workspace.join("input.pptx");
    let source_before = fs::read(source.as_path()).expect("source PPTX");

    let result = render_presentation_pages_with_runtime(
        &json!({
            "path":"input.pptx",
            "pdf_target_path":"output/rendered.pdf"
        }),
        &state,
        &request,
        Some(&AtomicBool::new(false)),
        Some(runtime.path()),
    )
    .expect("render PPTX");
    assert_eq!(
        result
            .pointer("/_structured_result/slides_total")
            .and_then(Value::as_u64),
        Some(1)
    );
    assert_eq!(
        result
            .pointer("/_structured_result/slide_number_scope")
            .and_then(Value::as_str),
        Some("true_visible_presentation_order")
    );
    assert_eq!(
        result
            .pointer("/_structured_result/active_content_validation")
            .and_then(Value::as_str),
        Some("passed")
    );
    assert_eq!(
        result
            .pointer("/_structured_result/visual_review_status")
            .and_then(Value::as_str),
        Some("pending_model_review")
    );
    assert_eq!(
        result
            .pointer("/_structured_result/layout_verified")
            .and_then(Value::as_bool),
        Some(false)
    );
    assert!(result
        .pointer("/_model_input/0/image_url")
        .and_then(Value::as_str)
        .is_some_and(|value| value.starts_with("data:image/png;base64,")));
    assert!(!result
        .pointer("/_structured_result")
        .expect("structured result")
        .to_string()
        .contains("base64"));
    assert!(workspace.join("output/rendered.pdf").is_file());
    assert_eq!(
        fs::read(source.as_path()).expect("source after render"),
        source_before
    );

    let range_error = render_presentation_pages_with_runtime(
        &json!({"path":"input.pptx","first_slide":1,"last_slide":9}),
        &state,
        &request,
        Some(&AtomicBool::new(false)),
        Some(runtime.path()),
    )
    .expect_err("oversized presentation slide range");
    assert!(range_error
        .to_string()
        .contains("presentations_render/slide_batch_limit_exceeded"));

    super::super::create_artifact_template(
        &json!({
            "source_path":"input.pptx",
            "target_directory":"templates/deck",
            "template_name":"Render preview deck"
        }),
        &state,
        &request,
    )
    .expect("create PPTX template");
    let stored_template_artifact = workspace.join("templates/deck/artifact.pptx");
    let stored_before = fs::read(stored_template_artifact.as_path()).expect("stored template");
    let preview = super::super::render_artifact_template_preview_with_runtime(
        &json!({"template_directory":"templates/deck","first_page":1}),
        &state,
        &request,
        Some(&AtomicBool::new(false)),
        Some(runtime.path()),
    )
    .expect("render stored PPTX template reference");
    assert_eq!(
        preview
            .pointer("/_structured_result/template")
            .and_then(Value::as_str),
        Some("templates/deck")
    );
    assert_eq!(
        preview
            .pointer("/_structured_result/artifact_kind")
            .and_then(Value::as_str),
        Some("pptx")
    );
    assert_eq!(
        preview
            .pointer("/_structured_result/preview_of")
            .and_then(Value::as_str),
        Some("stored_template_reference")
    );
    assert_eq!(
        preview
            .pointer("/_structured_result/template_hash_valid")
            .and_then(Value::as_bool),
        Some(true)
    );
    assert!(preview
        .pointer("/_model_input/0/image_url")
        .and_then(Value::as_str)
        .is_some_and(|value| value.starts_with("data:image/png;base64,")));
    assert_eq!(
        fs::read(stored_template_artifact.as_path()).expect("stored template after preview"),
        stored_before
    );

    fs::write(stored_template_artifact.as_path(), b"tampered").expect("tamper template");
    let hash_error = super::super::render_artifact_template_preview_with_runtime(
        &json!({"template_directory":"templates/deck"}),
        &state,
        &request,
        Some(&AtomicBool::new(false)),
        Some(runtime.path()),
    )
    .expect_err("tampered template must not render");
    assert!(hash_error
        .to_string()
        .contains("template_render/template_hash_mismatch"));
    let _ = fs::remove_dir_all(workspace);
}

#[test]
fn fake_verified_runtime_renders_transient_spreadsheet_page_without_modifying_source() {
    let (workspace, state, request) = test_context();
    let runtime = tempfile::tempdir().expect("runtime");
    let fixture_pdf = runtime.path().join("fixture.pdf");
    write_blank_pdf(fixture_pdf.as_path());
    let fixture_png = runtime.path().join("fixture.png");
    fs::write(
        fixture_png.as_path(),
        STANDARD
            .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=")
            .expect("PNG"),
    )
    .expect("write PNG");
    let soffice = runtime.path().join("soffice");
    write_executable(
        soffice.as_path(),
        format!(
            "#!/bin/sh\nout=''\nprevious=''\nfilter_ok=0\nfor value in \"$@\"; do\n  if [ \"$previous\" = '--outdir' ]; then out=\"$value\"; fi\n  if [ \"$value\" = 'pdf:calc_pdf_Export' ]; then filter_ok=1; fi\n  previous=\"$value\"\ndone\nif [ \"$filter_ok\" -ne 1 ]; then exit 44; fi\n/bin/cp '{}' \"$out/input.pdf\"\n",
            fixture_pdf.display()
        )
        .as_str(),
    );
    let pdftoppm = runtime.path().join("pdftoppm");
    write_executable(
        pdftoppm.as_path(),
        format!(
            "#!/bin/sh\nprefix=''\nfor value in \"$@\"; do prefix=\"$value\"; done\n/bin/cp '{}' \"${{prefix}}-1.png\"\n",
            fixture_png.display()
        )
        .as_str(),
    );
    write_runtime_manifest(
        runtime.path(),
        "soffice",
        sha256_file(soffice.as_path())
            .expect("soffice hash")
            .as_str(),
        "pdftoppm",
        sha256_file(pdftoppm.as_path())
            .expect("pdftoppm hash")
            .as_str(),
    );
    super::super::spreadsheet::create_xlsx(
        &json!({
            "target_path":"input.xlsx",
            "worksheets":[
                {"name":"Summary","rows":[["Metric","Value"],["Revenue",125000]],"freeze_rows":1,"column_widths":[{"column":"A","width":24},{"column":"B","width":16}]},
                {"name":"Details","rows":[["Quarter","Revenue"],["Q1",60000],["Q2",65000]],"freeze_rows":1}
            ]
        }),
        &state,
        &request,
    )
    .expect("create XLSX");
    let source = workspace.join("input.xlsx");
    let source_before = fs::read(source.as_path()).expect("source XLSX");

    let result = render_spreadsheet_pages_with_runtime(
        &json!({
            "path":"input.xlsx",
            "pdf_target_path":"output/rendered.pdf"
        }),
        &state,
        &request,
        Some(&AtomicBool::new(false)),
        Some(runtime.path()),
    )
    .expect("render XLSX");
    assert_eq!(
        result
            .pointer("/_structured_result/pages_total")
            .and_then(Value::as_u64),
        Some(1)
    );
    assert_eq!(
        result
            .pointer("/_structured_result/workbook/worksheets")
            .and_then(Value::as_u64),
        Some(2)
    );
    assert_eq!(
        result
            .pointer("/_structured_result/active_content_validation")
            .and_then(Value::as_str),
        Some("passed")
    );
    assert_eq!(
        result
            .pointer("/_structured_result/visual_review_status")
            .and_then(Value::as_str),
        Some("pending_model_review")
    );
    assert_eq!(
        result
            .pointer("/_structured_result/layout_verified")
            .and_then(Value::as_bool),
        Some(false)
    );
    assert!(result
        .pointer("/_model_input/0/image_url")
        .and_then(Value::as_str)
        .is_some_and(|value| value.starts_with("data:image/png;base64,")));
    assert!(!result
        .pointer("/_structured_result")
        .expect("structured result")
        .to_string()
        .contains("base64"));
    assert!(workspace.join("output/rendered.pdf").is_file());
    assert_eq!(
        fs::read(source.as_path()).expect("source after render"),
        source_before
    );

    let range_error = render_spreadsheet_pages_with_runtime(
        &json!({"path":"input.xlsx","first_page":1,"last_page":9}),
        &state,
        &request,
        Some(&AtomicBool::new(false)),
        Some(runtime.path()),
    )
    .expect_err("oversized spreadsheet page range");
    assert!(range_error
        .to_string()
        .contains("spreadsheets_render/page_batch_limit_exceeded"));
    let _ = fs::remove_dir_all(workspace);
}

#[test]
#[ignore = "requires CHATOS_DOCUMENT_RUNTIME_DIR with a real packaged runtime"]
fn packaged_runtime_smoke_renders_a_real_docx_page() {
    let (workspace, state, request) = test_context();
    super::super::create_docx(
        &json!({
            "target_path":"smoke.docx",
            "title":"Document render smoke test 文档渲染",
            "paragraphs":["Unicode: 中文。", "The page image must be transient and visually reviewable."]
        }),
        &state,
        &request,
    )
    .expect("create smoke DOCX");
    let result = render_docx_pages(
        &json!({"path":"smoke.docx","dpi":120}),
        &state,
        &request,
        Some(&AtomicBool::new(false)),
    )
    .expect("render smoke DOCX");
    assert_eq!(
        result
            .pointer("/_structured_result/pages_total")
            .and_then(Value::as_u64),
        Some(1)
    );
    if let Some(output) = std::env::var_os("CHATOS_DOCUMENT_RENDER_SMOKE_OUTPUT") {
        let output = PathBuf::from(output);
        fs::copy(workspace.join("smoke.docx"), output.with_extension("docx"))
            .expect("write smoke DOCX");
        let data_url = result
            .pointer("/_model_input/0/image_url")
            .and_then(Value::as_str)
            .expect("page image");
        let encoded = data_url
            .strip_prefix("data:image/png;base64,")
            .expect("PNG data URL");
        fs::write(output, STANDARD.decode(encoded).expect("decode page PNG"))
            .expect("write smoke page PNG");
    }
    let _ = fs::remove_dir_all(workspace);
}

#[test]
#[ignore = "requires CHATOS_DOCUMENT_RUNTIME_DIR with a real packaged runtime"]
fn packaged_runtime_smoke_renders_a_real_pdf_page() {
    let (workspace, state, request) = test_context();
    super::super::pdf_edit::create_text_pdf(
        &json!({
            "target_path":"smoke.pdf",
            "title":"PDF render smoke test",
            "paragraphs":["The page image must be transient and visually reviewable."]
        }),
        &state,
        &request,
    )
    .expect("create smoke PDF");
    let result = render_pdf_pages(
        &json!({"path":"smoke.pdf","dpi":120}),
        &state,
        &request,
        Some(&AtomicBool::new(false)),
    )
    .expect("render smoke PDF");
    assert_eq!(
        result
            .pointer("/_structured_result/pages_total")
            .and_then(Value::as_u64),
        Some(1)
    );
    if let Some(output) = std::env::var_os("CHATOS_PDF_RENDER_SMOKE_OUTPUT") {
        let output = PathBuf::from(output);
        fs::copy(workspace.join("smoke.pdf"), output.with_extension("pdf"))
            .expect("write smoke PDF");
        let data_url = result
            .pointer("/_model_input/0/image_url")
            .and_then(Value::as_str)
            .expect("page image");
        let encoded = data_url
            .strip_prefix("data:image/png;base64,")
            .expect("PNG data URL");
        fs::write(output, STANDARD.decode(encoded).expect("decode page PNG"))
            .expect("write smoke page PNG");
    }
    let _ = fs::remove_dir_all(workspace);
}

#[test]
#[ignore = "requires CHATOS_DOCUMENT_RUNTIME_DIR with a real packaged runtime"]
fn packaged_runtime_smoke_exports_real_pdf_pages_to_png() {
    let (workspace, state, request) = test_context();
    super::super::pdf_edit::create_text_pdf(
        &json!({
            "target_path":"smoke.pdf",
            "title":"PDF page export smoke test",
            "paragraphs":[
                "The packaged Poppler runtime must persist this page as a verified PNG.",
                "The exported image remains a workspace artifact and is not transient model input."
            ]
        }),
        &state,
        &request,
    )
    .expect("create page export smoke PDF");
    let source_before = fs::read(workspace.join("smoke.pdf")).expect("source PDF");
    let result = export_pdf_pages_to_png(
        &json!({
            "path":"smoke.pdf",
            "target_directory":"exports",
            "dpi":150,
            "filename_prefix":"page"
        }),
        &state,
        &request,
        Some(&AtomicBool::new(false)),
    )
    .expect("export smoke PDF page");
    assert_eq!(
        result
            .pointer("/_structured_result/rendered_pages")
            .and_then(Value::as_u64),
        Some(1)
    );
    assert_eq!(
        result
            .pointer("/_structured_result/files/0/path")
            .and_then(Value::as_str),
        Some("exports/page-1.png")
    );
    assert_eq!(
        result
            .pointer("/_structured_result/visual_review_status")
            .and_then(Value::as_str),
        Some("not_performed")
    );
    assert_eq!(
        result
            .pointer("/_structured_result/layout_verified")
            .and_then(Value::as_bool),
        Some(false)
    );
    assert!(result.get("_model_input").is_none());
    assert!(workspace.join("exports/page-1.png").is_file());
    assert_eq!(
        fs::read(workspace.join("smoke.pdf")).expect("source after export"),
        source_before
    );
    if let Some(output) = std::env::var_os("CHATOS_PDF_EXPORT_SMOKE_OUTPUT_DIR") {
        let output = PathBuf::from(output);
        fs::create_dir_all(output.as_path()).expect("create smoke output directory");
        fs::copy(workspace.join("smoke.pdf"), output.join("source.pdf"))
            .expect("write smoke source PDF");
        fs::copy(
            workspace.join("exports/page-1.png"),
            output.join("page-1.png"),
        )
        .expect("write smoke page PNG");
    }
    let _ = fs::remove_dir_all(workspace);
}

#[test]
#[ignore = "requires CHATOS_DOCUMENT_RUNTIME_DIR with a real packaged runtime"]
fn packaged_runtime_smoke_renders_a_real_spreadsheet_workbook() {
    let (workspace, state, request) = test_context();
    super::super::spreadsheet::create_xlsx(
        &json!({
            "target_path":"smoke.xlsx",
            "worksheets":[
                {
                    "name":"Summary",
                    "rows":[
                        ["Quarter","Revenue","Cost","Margin"],
                        ["Q1",125000,83000,{"formula":"=(B2-C2)/B2","cached_value":0.336,"number_format":"percent_2"}],
                        ["Q2",142000,91000,{"formula":"=(B3-C3)/B3","cached_value":0.3591549296,"number_format":"percent_2"}],
                        ["Total",{"formula":"=SUM(B2:B3)","cached_value":267000,"number_format":"integer"},{"formula":"=SUM(C2:C3)","cached_value":174000,"number_format":"integer"},{"formula":"=(B4-C4)/B4","cached_value":0.3483146067,"number_format":"percent_2"}]
                    ],
                    "freeze_rows":1,
                    "column_widths":[{"column":"A","width":16},{"column":"B","width":18},{"column":"C","width":18},{"column":"D","width":16}]
                },
                {
                    "name":"Details",
                    "rows":[
                        ["Region","Owner","Revenue"],
                        ["North","Li",72000],
                        ["South","Chen",68000],
                        ["West","Wang",127000]
                    ],
                    "freeze_rows":1,
                    "column_widths":[{"column":"A","width":18},{"column":"B","width":18},{"column":"C","width":18}]
                }
            ]
        }),
        &state,
        &request,
    )
    .expect("create smoke XLSX");
    let result = render_spreadsheet_pages(
        &json!({
            "path":"smoke.xlsx",
            "dpi":120,
            "pdf_target_path":"smoke.pdf"
        }),
        &state,
        &request,
        Some(&AtomicBool::new(false)),
    )
    .expect("render smoke XLSX");
    assert!(result
        .pointer("/_structured_result/pages_total")
        .and_then(Value::as_u64)
        .is_some_and(|pages| pages >= 1));
    if let Some(output) = std::env::var_os("CHATOS_SPREADSHEET_RENDER_SMOKE_OUTPUT_DIR") {
        let output = PathBuf::from(output);
        fs::create_dir_all(output.as_path()).expect("create smoke output directory");
        fs::copy(workspace.join("smoke.xlsx"), output.join("workbook.xlsx"))
            .expect("write smoke XLSX");
        fs::copy(workspace.join("smoke.pdf"), output.join("workbook.pdf"))
            .expect("write smoke PDF");
        let model_input = result
            .get("_model_input")
            .and_then(Value::as_array)
            .expect("page images");
        for (index, page) in model_input.iter().enumerate() {
            let data_url = page
                .get("image_url")
                .and_then(Value::as_str)
                .expect("page image");
            let encoded = data_url
                .strip_prefix("data:image/png;base64,")
                .expect("PNG data URL");
            fs::write(
                output.join(format!("page-{}.png", index + 1)),
                STANDARD.decode(encoded).expect("decode page PNG"),
            )
            .expect("write smoke page PNG");
        }
    }
    let _ = fs::remove_dir_all(workspace);
}

#[test]
#[ignore = "requires CHATOS_DOCUMENT_RUNTIME_DIR with a real packaged runtime"]
fn packaged_runtime_smoke_renders_a_real_presentation_deck() {
    let (workspace, state, request) = test_context();
    super::super::presentation::create_pptx(
        &json!({
            "target_path":"smoke.pptx",
            "slides":[
                {
                    "title":"Presentation Render QA",
                    "layout":"title_body",
                    "body":"- Packaged LibreOffice conversion\n- Poppler slide rasterization\n- Transient visual review",
                    "notes":"Verify the title and three bullet lines."
                },
                {
                    "title":"Two-column Layout",
                    "layout":"two_column",
                    "left_body":"Safety\nSource immutability\nActive content rejected",
                    "right_body":"Quality\nVisible slide order\nNo clipping or overlap"
                },
                {
                    "title":"Ready for Model Review",
                    "layout":"section",
                    "body":"Rendering success does not claim layout verification"
                }
            ]
        }),
        &state,
        &request,
    )
    .expect("create smoke PPTX");
    let result = render_presentation_pages(
        &json!({
            "path":"smoke.pptx",
            "dpi":120,
            "pdf_target_path":"smoke.pdf"
        }),
        &state,
        &request,
        Some(&AtomicBool::new(false)),
    )
    .expect("render smoke PPTX");
    assert_eq!(
        result
            .pointer("/_structured_result/slides_total")
            .and_then(Value::as_u64),
        Some(3)
    );
    assert_eq!(
        result
            .pointer("/_structured_result/pages_total")
            .and_then(Value::as_u64),
        Some(3)
    );
    super::super::create_artifact_template(
        &json!({
            "source_path":"smoke.pptx",
            "target_directory":"templates/presentation",
            "template_name":"Presentation Render QA"
        }),
        &state,
        &request,
    )
    .expect("create smoke presentation template");
    let template_preview = super::super::render_artifact_template_preview(
        &json!({
            "template_directory":"templates/presentation",
            "first_page":1,
            "last_page":3,
            "dpi":120
        }),
        &state,
        &request,
        Some(&AtomicBool::new(false)),
    )
    .expect("render smoke presentation template reference");
    assert_eq!(
        template_preview
            .pointer("/_structured_result/artifact_kind")
            .and_then(Value::as_str),
        Some("pptx")
    );
    assert_eq!(
        template_preview
            .pointer("/_structured_result/preview_of")
            .and_then(Value::as_str),
        Some("stored_template_reference")
    );
    assert_eq!(
        template_preview
            .pointer("/_structured_result/pages_total")
            .and_then(Value::as_u64),
        Some(3)
    );
    if let Some(output) = std::env::var_os("CHATOS_PRESENTATION_RENDER_SMOKE_OUTPUT_DIR") {
        let output = PathBuf::from(output);
        fs::create_dir_all(output.as_path()).expect("create smoke output directory");
        fs::copy(
            workspace.join("smoke.pptx"),
            output.join("presentation.pptx"),
        )
        .expect("write smoke PPTX");
        fs::copy(workspace.join("smoke.pdf"), output.join("presentation.pdf"))
            .expect("write smoke PDF");
        let model_input = result
            .get("_model_input")
            .and_then(Value::as_array)
            .expect("slide images");
        for (index, slide) in model_input.iter().enumerate() {
            let data_url = slide
                .get("image_url")
                .and_then(Value::as_str)
                .expect("slide image");
            let encoded = data_url
                .strip_prefix("data:image/png;base64,")
                .expect("PNG data URL");
            fs::write(
                output.join(format!("slide-{}.png", index + 1)),
                STANDARD.decode(encoded).expect("decode slide PNG"),
            )
            .expect("write smoke slide PNG");
        }
    }
    if let Some(output) = std::env::var_os("CHATOS_TEMPLATE_RENDER_SMOKE_OUTPUT_DIR") {
        let output = PathBuf::from(output);
        fs::create_dir_all(output.as_path()).expect("create template smoke output directory");
        fs::copy(
            workspace.join("templates/presentation/artifact.pptx"),
            output.join("template-reference.pptx"),
        )
        .expect("write template reference PPTX");
        fs::copy(
            workspace.join("templates/presentation/template.json"),
            output.join("template.json"),
        )
        .expect("write template manifest");
        let model_input = template_preview
            .get("_model_input")
            .and_then(Value::as_array)
            .expect("template preview images");
        for (index, slide) in model_input.iter().enumerate() {
            let data_url = slide
                .get("image_url")
                .and_then(Value::as_str)
                .expect("template preview image");
            let encoded = data_url
                .strip_prefix("data:image/png;base64,")
                .expect("PNG data URL");
            fs::write(
                output.join(format!("preview-{}.png", index + 1)),
                STANDARD.decode(encoded).expect("decode preview PNG"),
            )
            .expect("write template preview PNG");
        }
    }
    let _ = fs::remove_dir_all(workspace);
}

#[test]
fn bounded_command_terminates_timed_out_process_group() {
    let directory = tempfile::tempdir().expect("directory");
    let script = directory.path().join("slow");
    write_executable(script.as_path(), "#!/bin/sh\n/bin/sleep 10\n");
    let started = Instant::now();
    let error = run_bounded_command(
        script.as_path(),
        &[],
        directory.path(),
        &private_process_environment(directory.path(), directory.path()),
        Duration::from_millis(100),
        None,
        "test",
    )
    .expect_err("timeout");
    assert!(error.to_string().contains("documents_render/timeout"));
    assert!(started.elapsed() < Duration::from_secs(3));
}

#[test]
fn windows_process_tree_contract_is_present_in_source() {
    let source = include_str!("process.rs");
    assert!(source.contains("CREATE_NEW_PROCESS_GROUP"));
    assert!(source.contains("CREATE_NO_WINDOW"));
    assert!(source.contains("taskkill.exe"));
    assert!(source.contains("\"/T\", \"/F\""));
}

fn test_context() -> (PathBuf, LocalState, RelayRequest) {
    let root = std::env::temp_dir().join(format!("chatos-docx-render-test-{}", Uuid::new_v4()));
    fs::create_dir_all(root.as_path()).expect("workspace");
    let state = LocalState {
        workspaces: vec![WorkspaceState {
            id: "workspace-1".to_string(),
            absolute_root: root.clone(),
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
    (root, state, request)
}

fn write_minimal_docx(path: &Path) {
    let file = File::create(path).expect("DOCX");
    let mut writer = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    writer
        .start_file("[Content_Types].xml", options)
        .expect("types");
    writer
        .write_all(br#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#)
        .expect("types XML");
    writer
        .start_file("word/document.xml", options)
        .expect("document");
    writer
        .write_all(br#"<?xml version="1.0"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>Render me</w:t></w:r></w:p><w:sectPr/></w:body></w:document>"#)
        .expect("document XML");
    writer.finish().expect("finish DOCX");
}

fn write_blank_pdf(path: &Path) {
    write_blank_pdf_pages(path, 1);
}

fn write_blank_pdf_pages(path: &Path, page_count: usize) {
    let mut document = Document::with_version("1.5");
    let pages_id = document.new_object_id();
    let page_ids = (0..page_count)
        .map(|_| {
            document.add_object(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            })
        })
        .collect::<Vec<_>>();
    document.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => page_ids.into_iter().map(Object::Reference).collect::<Vec<_>>(),
            "Count" => page_count as i64,
        }),
    );
    let catalog_id = document.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    document.trailer.set("Root", catalog_id);
    document.save(path).expect("save PDF");
}

fn write_runtime_manifest(
    root: &Path,
    soffice_path: &str,
    soffice_sha256: &str,
    pdftoppm_path: &str,
    pdftoppm_sha256: &str,
) {
    fs::create_dir_all(root.join("fonts")).expect("font directory");
    let font = root.join("fonts/test.ttf");
    fs::write(font.as_path(), b"fake-font-for-runtime-contract").expect("font");
    fs::write(
        root.join(RUNTIME_MANIFEST_NAME),
        serde_json::to_vec_pretty(&json!({
            "schema_version":1,
            "runtime_revision":"test-runtime-1",
            "platform":current_platform(),
            "soffice":{
                "path":soffice_path,
                "sha256":soffice_sha256,
                "version":"Fake LibreOffice 1"
            },
            "pdftoppm":{
                "path":pdftoppm_path,
                "sha256":pdftoppm_sha256,
                "version":"Fake Poppler 1"
            },
            "poppler_library_dir":null,
            "font_directory":"fonts",
            "fonts":[{
                "path":"fonts/test.ttf",
                "sha256":sha256_file(font.as_path()).expect("font hash")
            }]
        }))
        .expect("manifest JSON"),
    )
    .expect("manifest");
}

fn write_executable(path: &Path, contents: &str) {
    fs::write(path, contents).expect("write executable");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).expect("permissions");
    }
}
