// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::atomic::AtomicBool;

use anyhow::Result;
use serde_json::Value;

use crate::relay::RelayRequest;
use crate::LocalState;

use super::{
    create_artifact_template, create_csv, create_docx, create_tsv, docx_edit, docx_render,
    extract_pdf_text, inspect_artifact_template, inspect_docx, inspect_pdf, inspect_spreadsheet,
    instantiate_artifact_template, pdf_edit, presentation, render_artifact_template_preview,
    spreadsheet, update_csv_range, update_tsv_range,
};

pub(in crate::skills::native) fn execute_with_cancellation(
    skill_id: &str,
    operation: &str,
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
    action_cancelled: Option<&AtomicBool>,
) -> Option<Result<Value>> {
    let result = match (skill_id, operation) {
        ("internal_skill_pdf", "inspect_pdf") => inspect_pdf(arguments, state, request),
        ("internal_skill_pdf", "extract_pdf_text") => extract_pdf_text(arguments, state, request),
        ("internal_skill_pdf", "render_pdf_pages") => {
            docx_render::render_pdf_pages(arguments, state, request, action_cancelled)
        }
        ("internal_skill_pdf", "export_pdf_pages_to_png") => {
            docx_render::export_pdf_pages_to_png(arguments, state, request, action_cancelled)
        }
        ("internal_skill_pdf", "create_text_pdf") => {
            pdf_edit::create_text_pdf(arguments, state, request)
        }
        ("internal_skill_pdf", "create_pdf_from_images") => {
            pdf_edit::create_pdf_from_images(arguments, state, request)
        }
        ("internal_skill_pdf", "update_pdf_metadata") => {
            pdf_edit::update_pdf_metadata(arguments, state, request)
        }
        ("internal_skill_pdf", "fill_pdf_form_fields") => {
            pdf_edit::fill_pdf_form_fields(arguments, state, request)
        }
        ("internal_skill_pdf", "merge_pdfs") => pdf_edit::merge_pdfs(arguments, state, request),
        ("internal_skill_pdf", "extract_pdf_pages") => {
            pdf_edit::extract_pdf_pages(arguments, state, request)
        }
        ("internal_skill_pdf", "arrange_pdf_pages") => {
            pdf_edit::arrange_pdf_pages(arguments, state, request)
        }
        ("internal_skill_pdf", "rotate_pdf_pages") => {
            pdf_edit::rotate_pdf_pages(arguments, state, request)
        }
        ("internal_skill_pdf", "add_pdf_text_annotation") => {
            pdf_edit::add_pdf_text_annotation(arguments, state, request)
        }
        ("internal_skill_pdf", "add_pdf_markup_annotation") => {
            pdf_edit::add_pdf_markup_annotation(arguments, state, request)
        }
        ("internal_skill_pdf", "add_pdf_link_annotation") => {
            pdf_edit::add_pdf_link_annotation(arguments, state, request)
        }
        ("internal_skill_pdf", "add_pdf_annotation_reply") => {
            pdf_edit::add_pdf_annotation_reply(arguments, state, request)
        }
        ("internal_skill_pdf", "update_pdf_annotation_text") => {
            pdf_edit::update_pdf_annotation_text(arguments, state, request)
        }
        ("internal_skill_pdf", "delete_pdf_annotation") => {
            pdf_edit::delete_pdf_annotation(arguments, state, request)
        }
        ("internal_skill_pdf", "add_pdf_file_attachment_annotation") => {
            pdf_edit::add_pdf_file_attachment_annotation(arguments, state, request)
        }
        ("internal_skill_pdf", "extract_pdf_file_attachment") => {
            pdf_edit::extract_pdf_file_attachment(arguments, state, request)
        }
        ("internal_skill_pdf", "extract_pdf_embedded_file") => {
            pdf_edit::extract_pdf_embedded_file(arguments, state, request)
        }
        ("internal_skill_pdf", "stamp_pdf_text") => {
            pdf_edit::stamp_pdf_text(arguments, state, request)
        }
        ("internal_skill_pdf", "stamp_pdf_page_numbers") => {
            pdf_edit::stamp_pdf_page_numbers(arguments, state, request)
        }
        ("internal_skill_pdf", "stamp_pdf_image") => {
            pdf_edit::stamp_pdf_image(arguments, state, request)
        }
        ("internal_skill_documents", "inspect_docx") => inspect_docx(arguments, state, request),
        ("internal_skill_documents", "render_docx_pages") => {
            docx_render::render_docx_pages(arguments, state, request, action_cancelled)
        }
        ("internal_skill_documents", "update_docx_metadata") => {
            docx_edit::update_docx_metadata(arguments, state, request)
        }
        ("internal_skill_documents", "create_docx") => create_docx(arguments, state, request),
        ("internal_skill_documents", "create_structured_docx") => {
            docx_edit::create_structured_docx(arguments, state, request)
        }
        ("internal_skill_documents", "append_docx_content") => {
            docx_edit::append_docx_content(arguments, state, request)
        }
        ("internal_skill_documents", "insert_docx_content_at_paragraph") => {
            docx_edit::insert_docx_content_at_paragraph(arguments, state, request)
        }
        ("internal_skill_documents", "insert_docx_content_at_paragraph_index") => {
            docx_edit::insert_docx_content_at_paragraph_index(arguments, state, request)
        }
        ("internal_skill_documents", "delete_docx_paragraph") => {
            docx_edit::delete_docx_paragraph(arguments, state, request)
        }
        ("internal_skill_documents", "delete_docx_paragraph_at_index") => {
            docx_edit::delete_docx_paragraph_at_index(arguments, state, request)
        }
        ("internal_skill_documents", "move_docx_paragraph") => {
            docx_edit::move_docx_paragraph(arguments, state, request)
        }
        ("internal_skill_documents", "move_docx_paragraph_at_index") => {
            docx_edit::move_docx_paragraph_at_index(arguments, state, request)
        }
        ("internal_skill_documents", "replace_docx_paragraph_with_content") => {
            docx_edit::replace_docx_paragraph_with_content(arguments, state, request)
        }
        ("internal_skill_documents", "replace_docx_paragraph_at_index_with_content") => {
            docx_edit::replace_docx_paragraph_at_index_with_content(arguments, state, request)
        }
        ("internal_skill_documents", "replace_docx_text") => {
            docx_edit::replace_docx_text(arguments, state, request)
        }
        ("internal_skill_documents", "replace_docx_text_across_runs") => {
            docx_edit::replace_docx_text_across_runs(arguments, state, request)
        }
        ("internal_skill_documents", "replace_docx_header_footer_text") => {
            docx_edit::replace_docx_header_footer_text(arguments, state, request)
        }
        ("internal_skill_documents", "replace_docx_table_cell_text") => {
            docx_edit::replace_docx_table_cell_text(arguments, state, request)
        }
        ("internal_skill_documents", "delete_docx_table_row") => {
            docx_edit::delete_docx_table_row(arguments, state, request)
        }
        ("internal_skill_documents", "insert_docx_table_row") => {
            docx_edit::insert_docx_table_row(arguments, state, request)
        }
        ("internal_skill_documents", "move_docx_table_row") => {
            docx_edit::move_docx_table_row(arguments, state, request)
        }
        ("internal_skill_documents", "insert_docx_image") => {
            docx_edit::insert_docx_image(arguments, state, request)
        }
        ("internal_skill_documents", "add_docx_header_footer") => {
            docx_edit::add_docx_header_footer(arguments, state, request)
        }
        ("internal_skill_documents", "add_docx_comment") => {
            docx_edit::add_docx_comment(arguments, state, request)
        }
        ("internal_skill_documents", "replace_docx_text_tracked") => {
            docx_edit::replace_docx_text_tracked(arguments, state, request)
        }
        ("internal_skill_documents", "resolve_docx_tracked_changes") => {
            docx_edit::resolve_docx_tracked_changes(arguments, state, request)
        }
        ("internal_skill_spreadsheets", "inspect_spreadsheet") => {
            inspect_spreadsheet(arguments, state, request)
        }
        ("internal_skill_spreadsheets", "render_spreadsheet_pages") => {
            docx_render::render_spreadsheet_pages(arguments, state, request, action_cancelled)
        }
        ("internal_skill_spreadsheets", "create_xlsx") => {
            spreadsheet::create_xlsx(arguments, state, request)
        }
        ("internal_skill_spreadsheets", "update_xlsx_range") => {
            spreadsheet::update_xlsx_range(arguments, state, request)
        }
        ("internal_skill_spreadsheets", "create_csv") => create_csv(arguments, state, request),
        ("internal_skill_spreadsheets", "update_csv_range") => {
            update_csv_range(arguments, state, request)
        }
        ("internal_skill_spreadsheets", "create_tsv") => create_tsv(arguments, state, request),
        ("internal_skill_spreadsheets", "update_tsv_range") => {
            update_tsv_range(arguments, state, request)
        }
        ("internal_skill_presentations", "inspect_pptx") => {
            presentation::inspect_pptx(arguments, state, request)
        }
        ("internal_skill_presentations", "inspect_pptx_charts") => {
            presentation::inspect_pptx_charts(arguments, state, request)
        }
        ("internal_skill_presentations", "replace_pptx_chart") => {
            presentation::replace_pptx_chart(arguments, state, request)
        }
        ("internal_skill_presentations", "inspect_pptx_table") => {
            presentation::inspect_pptx_table(arguments, state, request)
        }
        ("internal_skill_presentations", "render_presentation_pages") => {
            docx_render::render_presentation_pages(arguments, state, request, action_cancelled)
        }
        ("internal_skill_presentations", "create_pptx") => {
            presentation::create_pptx(arguments, state, request)
        }
        ("internal_skill_presentations", "append_pptx_slides") => {
            presentation::append_pptx_slides(arguments, state, request)
        }
        ("internal_skill_presentations", "reorder_pptx_slides") => {
            presentation::reorder_pptx_slides(arguments, state, request)
        }
        ("internal_skill_presentations", "delete_pptx_slides") => {
            presentation::delete_pptx_slides(arguments, state, request)
        }
        ("internal_skill_presentations", "replace_pptx_text") => {
            presentation::replace_pptx_text(arguments, state, request)
        }
        ("internal_skill_presentations", "replace_pptx_text_across_runs") => {
            presentation::replace_pptx_text_across_runs(arguments, state, request)
        }
        ("internal_skill_presentations", "replace_pptx_table_cell_text") => {
            presentation::replace_pptx_table_cell_text(arguments, state, request)
        }
        ("internal_skill_presentations", "copy_pptx_table_cell_format") => {
            presentation::copy_pptx_table_cell_format(arguments, state, request)
        }
        ("internal_skill_presentations", "delete_pptx_table_row") => {
            presentation::delete_pptx_table_row(arguments, state, request)
        }
        ("internal_skill_presentations", "insert_pptx_table_row") => {
            presentation::insert_pptx_table_row(arguments, state, request)
        }
        ("internal_skill_presentations", "move_pptx_table_row") => {
            presentation::move_pptx_table_row(arguments, state, request)
        }
        ("internal_skill_presentations", "delete_pptx_table_column") => {
            presentation::delete_pptx_table_column(arguments, state, request)
        }
        ("internal_skill_presentations", "insert_pptx_table_column") => {
            presentation::insert_pptx_table_column(arguments, state, request)
        }
        ("internal_skill_presentations", "move_pptx_table_column") => {
            presentation::move_pptx_table_column(arguments, state, request)
        }
        ("internal_skill_presentations", "replace_pptx_notes_text") => {
            presentation::replace_pptx_notes_text(arguments, state, request)
        }
        ("internal_skill_template_creator", "inspect_artifact_template") => {
            inspect_artifact_template(arguments, state, request)
        }
        ("internal_skill_template_creator", "create_artifact_template") => {
            create_artifact_template(arguments, state, request)
        }
        ("internal_skill_template_creator", "instantiate_artifact_template") => {
            instantiate_artifact_template(arguments, state, request)
        }
        ("internal_skill_template_creator", "render_artifact_template_preview") => {
            render_artifact_template_preview(arguments, state, request, action_cancelled)
        }
        _ => return None,
    };
    Some(result)
}
