// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use anyhow::{anyhow, Context, Result};
use lopdf::Object;
use serde_json::{json, Value};

use crate::relay::RelayRequest;
use crate::LocalState;

use super::super::required_text;
use super::annotation_common::is_pdf_markup_subtype;
use super::annotation_operation_common::{
    annotation_relation_type, load_guarded_annotation_pdf, save_guarded_annotation_output,
    select_annotation, GuardedAnnotationPdf, SelectedAnnotation,
};
use super::inspect_pdf_annotations;

pub(super) fn delete_pdf_annotation(
    arguments: &Value,
    state: &LocalState,
    request: &RelayRequest,
) -> Result<Value> {
    let GuardedAnnotationPdf {
        source,
        source_relative,
        expected_source_sha256,
        mut document,
        page_map,
        inspection: annotation_inspection,
    } = load_guarded_annotation_pdf(arguments, state, request)?;
    let initial_annotation_count = annotation_inspection
        .get("count")
        .and_then(Value::as_u64)
        .ok_or_else(|| anyhow!("PDF annotation inspection did not return a valid count"))?;

    let SelectedAnnotation {
        page_number,
        page_id,
        annotation_index,
        mut annotations,
        selected_id,
        label: selected_label,
        dictionary: selected,
        subtype,
    } = select_annotation(arguments, &document, &page_map)?;
    let deleted_indirect_object = selected_id.is_some();
    if subtype != "Text"
        && subtype != "Link"
        && subtype != "FileAttachment"
        && !is_pdf_markup_subtype(subtype.as_str())
    {
        return Err(anyhow!(
            "{selected_label} subtype /{subtype} is not eligible for safe annotation deletion"
        ));
    }
    let expected_subtype = required_text(arguments, "expected_subtype")?;
    if expected_subtype != subtype {
        return Err(anyhow!(
            "expected_subtype does not match {selected_label}; inspect the current PDF again"
        ));
    }
    let relation_type = annotation_relation_type(&selected, selected_label.as_str())?;
    if required_text(arguments, "expected_relation_type")? != relation_type {
        return Err(anyhow!(
            "expected_relation_type does not match {selected_label}; inspect the current PDF again"
        ));
    }
    if selected.has(b"StructParent") || selected.has(b"StructParents") {
        return Err(anyhow!(
            "{selected_label} participates in the tagged-PDF structure tree and cannot be deleted safely"
        ));
    }
    if selected.has(b"Popup") || selected.has(b"Parent") {
        return Err(anyhow!(
            "{selected_label} has a Popup or Parent relationship and cannot be deleted safely"
        ));
    }

    annotations.remove(annotation_index - 1);
    let remaining_annotations_on_page = annotations.len();
    let page = document
        .get_object_mut(page_id)
        .and_then(Object::as_dict_mut)
        .with_context(|| format!("read page {page_number} dictionary"))?;
    if annotations.is_empty() {
        page.remove(b"Annots");
    } else {
        page.set("Annots", annotations);
    }

    document.prune_objects();
    if selected_id.is_some_and(|object_id| document.objects.contains_key(&object_id)) {
        return Err(anyhow!(
            "{selected_label} is still referenced by a reply, group member, popup, or another reachable PDF object"
        ));
    }
    let remaining_page_map = document.get_pages();
    let remaining_inspection =
        inspect_pdf_annotations(&document, &remaining_page_map, Some(&json!(page_number)))?;
    let remaining_annotation_count = remaining_inspection
        .get("count")
        .and_then(Value::as_u64)
        .ok_or_else(|| anyhow!("updated PDF annotation inspection did not return a valid count"))?;
    if remaining_annotation_count.checked_add(1) != Some(initial_annotation_count) {
        return Err(anyhow!(
            "PDF annotation deletion changed an unexpected number of annotations"
        ));
    }

    let (target_relative, bytes) = save_guarded_annotation_output(
        arguments,
        state,
        request,
        source.as_path(),
        expected_source_sha256.as_str(),
        "PDF source changed while the annotation deletion was being prepared; no output was written",
        &mut document,
    )?;
    Ok(json!({
        "deleted": true,
        "operation": "delete_annotation",
        "source_path": source_relative,
        "source_sha256": expected_source_sha256,
        "path": target_relative,
        "page": page_number,
        "annotation_index": annotation_index,
        "subtype": subtype,
        "relation_type": relation_type,
        "deleted_indirect_object": deleted_indirect_object,
        "remaining_annotations_on_page": remaining_annotations_on_page,
        "remaining_annotations_total": remaining_annotation_count,
        "bytes": bytes,
    }))
}
