// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;

use anyhow::{anyhow, Context, Result};
use lopdf::{decode_text_string, Document, Object, ObjectId};
use serde_json::{json, Value};

use super::attachment_common::InspectedPdfEmbeddedFileEntry;
use super::attachment_filespec::inspect_pdf_embedded_filespec;
use super::{
    normalized_pdf_unicode_text, resolved_pdf_dictionary, resolved_pdf_object, MAX_PDF_ANNOTATIONS,
    MAX_PDF_ANNOTATION_PREVIEW, MAX_PDF_ATTACHMENT_TOTAL_BYTES,
    MAX_PDF_EMBEDDED_FILE_NAME_CHARACTERS, MAX_PDF_EMBEDDED_FILE_TREE_DEPTH,
    MAX_PDF_EMBEDDED_FILE_TREE_NODES,
};

struct EmbeddedFileCollector {
    node_count: usize,
    visited_nodes: HashSet<ObjectId>,
    seen_keys: HashSet<Vec<u8>>,
    last_key: Option<Vec<u8>>,
    total_bytes: usize,
    entries: Vec<InspectedPdfEmbeddedFileEntry>,
}

impl EmbeddedFileCollector {
    fn new() -> Self {
        Self {
            node_count: 0,
            visited_nodes: HashSet::new(),
            seen_keys: HashSet::new(),
            last_key: None,
            total_bytes: 0,
            entries: Vec::new(),
        }
    }

    fn collect_node(
        &mut self,
        document: &Document,
        node_object: Object,
        depth: usize,
    ) -> Result<()> {
        if depth >= MAX_PDF_EMBEDDED_FILE_TREE_DEPTH {
            return Err(anyhow!(
                "PDF EmbeddedFiles Name Tree exceeds the {MAX_PDF_EMBEDDED_FILE_TREE_DEPTH} level depth limit"
            ));
        }
        self.node_count = self
            .node_count
            .checked_add(1)
            .filter(|value| *value <= MAX_PDF_EMBEDDED_FILE_TREE_NODES)
            .ok_or_else(|| {
                anyhow!(
                    "PDF EmbeddedFiles Name Tree exceeds the {MAX_PDF_EMBEDDED_FILE_TREE_NODES} node limit"
                )
            })?;
        let node = match node_object {
            Object::Reference(object_id) => {
                if !self.visited_nodes.insert(object_id) {
                    return Err(anyhow!(
                        "PDF EmbeddedFiles Name Tree contains a repeated or cyclic node reference"
                    ));
                }
                document
                    .get_object(object_id)
                    .and_then(Object::as_dict)
                    .context("read PDF EmbeddedFiles Name Tree node")?
                    .clone()
            }
            Object::Dictionary(dictionary) => dictionary,
            _ => {
                return Err(anyhow!(
                    "PDF EmbeddedFiles Name Tree node must be a dictionary"
                ));
            }
        };

        if let Ok(limits_object) = node.get(b"Limits") {
            let limits_object =
                resolved_pdf_object(document, limits_object.clone(), "PDF EmbeddedFiles Limits")?;
            let limits = limits_object
                .as_array()
                .context("PDF EmbeddedFiles Limits must be an array")?;
            if limits.len() != 2 {
                return Err(anyhow!(
                    "PDF EmbeddedFiles Limits must contain exactly two name strings"
                ));
            }
            let lower = pdf_embedded_file_name_key(&limits[0], "PDF EmbeddedFiles lower Limit")?;
            let upper = pdf_embedded_file_name_key(&limits[1], "PDF EmbeddedFiles upper Limit")?;
            if lower.0 > upper.0 {
                return Err(anyhow!(
                    "PDF EmbeddedFiles Limits must be ordered from lower to upper"
                ));
            }
        }

        let has_names = node.has(b"Names");
        let has_kids = node.has(b"Kids");
        if has_names == has_kids {
            return Err(anyhow!(
                "PDF EmbeddedFiles Name Tree node must contain exactly one of Names or Kids"
            ));
        }
        if has_names {
            return self.collect_names(document, &node);
        }

        let kids_object = resolved_pdf_object(
            document,
            node.get(b"Kids")
                .context("PDF EmbeddedFiles Kids is missing")?
                .clone(),
            "PDF EmbeddedFiles Kids",
        )?;
        let kids = kids_object
            .as_array()
            .context("PDF EmbeddedFiles Kids must be an array")?;
        if kids.is_empty() {
            return Err(anyhow!(
                "PDF EmbeddedFiles Kids must contain at least one child node"
            ));
        }
        for child in kids {
            if !matches!(child, Object::Reference(_)) {
                return Err(anyhow!(
                    "PDF EmbeddedFiles Kids entries must be indirect node references"
                ));
            }
            self.collect_node(document, child.clone(), depth + 1)?;
        }
        Ok(())
    }

    fn collect_names(&mut self, document: &Document, node: &lopdf::Dictionary) -> Result<()> {
        let names_object = resolved_pdf_object(
            document,
            node.get(b"Names")
                .context("PDF EmbeddedFiles Names is missing")?
                .clone(),
            "PDF EmbeddedFiles Names",
        )?;
        let names = names_object
            .as_array()
            .context("PDF EmbeddedFiles Names must be an array")?;
        if names.is_empty() || names.len() % 2 != 0 {
            return Err(anyhow!(
                "PDF EmbeddedFiles Names must contain one or more name/Filespec pairs"
            ));
        }
        for pair in names.chunks_exact(2) {
            if self.entries.len() >= MAX_PDF_ANNOTATIONS {
                return Err(anyhow!(
                    "PDF EmbeddedFiles Name Tree exceeds the {MAX_PDF_ANNOTATIONS} entry limit"
                ));
            }
            let entry_number = self.entries.len() + 1;
            let (key_bytes, name) = pdf_embedded_file_name_key(
                &pair[0],
                format!("PDF EmbeddedFiles entry {entry_number} name").as_str(),
            )?;
            if !self.seen_keys.insert(key_bytes.clone()) {
                return Err(anyhow!(
                    "PDF EmbeddedFiles Name Tree contains a duplicate name key"
                ));
            }
            if self
                .last_key
                .as_ref()
                .is_some_and(|previous| previous >= &key_bytes)
            {
                return Err(anyhow!(
                    "PDF EmbeddedFiles Name Tree keys must be strictly ascending"
                ));
            }
            self.last_key = Some(key_bytes);
            let filespec_id = pair[1].as_reference().with_context(|| {
                format!(
                    "PDF EmbeddedFiles entry {entry_number} must reference an indirect Filespec"
                )
            })?;
            let label = format!("PDF EmbeddedFiles entry {entry_number}");
            let attachment = inspect_pdf_embedded_filespec(document, filespec_id, label.as_str())?;
            self.total_bytes = self
                .total_bytes
                .checked_add(attachment.content.len())
                .filter(|value| *value <= MAX_PDF_ATTACHMENT_TOTAL_BYTES)
                .ok_or_else(|| {
                    anyhow!(
                        "PDF embedded files exceed the {} MiB aggregate inspection limit",
                        MAX_PDF_ATTACHMENT_TOTAL_BYTES / (1024 * 1024)
                    )
                })?;
            self.entries
                .push(InspectedPdfEmbeddedFileEntry { name, attachment });
        }
        Ok(())
    }
}

pub(super) fn inspect_pdf_embedded_files(document: &Document) -> Result<Value> {
    let (entries, total_bytes) = collect_pdf_embedded_files(document)?;
    let preview = entries
        .iter()
        .take(MAX_PDF_ANNOTATION_PREVIEW)
        .enumerate()
        .map(|(index, entry)| {
            let mut item = entry.attachment.metadata.clone();
            item["embedded_file_index"] = json!(index + 1);
            item["name"] = Value::String(entry.name.clone());
            item
        })
        .collect::<Vec<_>>();
    Ok(json!({
        "count": entries.len(),
        "bytes": total_bytes,
        "preview": preview,
        "preview_truncated": entries.len() > MAX_PDF_ANNOTATION_PREVIEW,
    }))
}

pub(super) fn collect_pdf_embedded_files(
    document: &Document,
) -> Result<(Vec<InspectedPdfEmbeddedFileEntry>, usize)> {
    let catalog = document.catalog().context("read PDF catalog")?;
    let Ok(names_object) = catalog.get(b"Names") else {
        return Ok((Vec::new(), 0));
    };
    if matches!(names_object, Object::Null) {
        return Ok((Vec::new(), 0));
    }
    let names = resolved_pdf_dictionary(document, names_object.clone(), "PDF catalog Names")?;
    let Ok(root_object) = names.get(b"EmbeddedFiles") else {
        return Ok((Vec::new(), 0));
    };
    if matches!(root_object, Object::Null) {
        return Ok((Vec::new(), 0));
    }

    let mut collector = EmbeddedFileCollector::new();
    collector.collect_node(document, root_object.clone(), 0)?;
    Ok((collector.entries, collector.total_bytes))
}

fn pdf_embedded_file_name_key(value: &Object, label: &str) -> Result<(Vec<u8>, String)> {
    let Object::String(bytes, _) = value else {
        return Err(anyhow!("{label} must be a PDF text string"));
    };
    let decoded = decode_text_string(value).with_context(|| format!("decode {label}"))?;
    let normalized = normalized_pdf_unicode_text(
        decoded.as_str(),
        label,
        MAX_PDF_EMBEDDED_FILE_NAME_CHARACTERS,
        false,
    )?;
    Ok((bytes.clone(), normalized))
}
