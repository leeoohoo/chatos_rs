// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, HashMap, HashSet};

use anyhow::{anyhow, Result};
use serde_json::Value;

use super::{
    content_types_for_part, document_relationships, empty_xml_start_tag, find_next_xml_tag_start,
    resolve_document_relationship_target, single_attribute_value,
};

const MAX_DOCX_HEADER_FOOTER_PARTS: usize = 128;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum HeaderFooterKind {
    Header,
    Footer,
}

impl HeaderFooterKind {
    fn reference_tag(self) -> &'static str {
        match self {
            Self::Header => "<w:headerReference",
            Self::Footer => "<w:footerReference",
        }
    }

    fn relationship_type(self) -> &'static str {
        match self {
            Self::Header => {
                "http://schemas.openxmlformats.org/officeDocument/2006/relationships/header"
            }
            Self::Footer => {
                "http://schemas.openxmlformats.org/officeDocument/2006/relationships/footer"
            }
        }
    }

    fn content_type(self) -> &'static str {
        match self {
            Self::Header => {
                "application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml"
            }
            Self::Footer => {
                "application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml"
            }
        }
    }

    fn root_tag(self) -> &'static str {
        match self {
            Self::Header => "w:hdr",
            Self::Footer => "w:ftr",
        }
    }

    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Header => "header",
            Self::Footer => "footer",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ReferencedHeaderFooterPart {
    pub(super) path: String,
    pub(super) kind: HeaderFooterKind,
}

pub(super) fn referenced_header_footer_parts(
    document_xml: &str,
    relationships_xml: &str,
    names: &HashSet<String>,
    content_types_xml: &str,
) -> Result<Vec<ReferencedHeaderFooterPart>> {
    let mut references_by_id = HashMap::<String, HeaderFooterKind>::new();
    for kind in [HeaderFooterKind::Header, HeaderFooterKind::Footer] {
        let mut cursor = 0usize;
        while let Some(start) = find_next_xml_tag_start(document_xml, kind.reference_tag(), cursor)
        {
            let end = document_xml[start..]
                .find('>')
                .map(|offset| start + offset + 1)
                .ok_or_else(|| anyhow!("DOCX {} reference is unterminated", kind.as_str()))?;
            let entry = &document_xml[start..end];
            if !empty_xml_start_tag(entry) {
                return Err(anyhow!(
                    "DOCX {} reference must be an empty XML element",
                    kind.as_str()
                ));
            }
            let relationship_id = single_attribute_value(entry, "r:id", "header/footer reference")?;
            if let Some(existing) = references_by_id.insert(relationship_id.clone(), kind) {
                if existing != kind {
                    return Err(anyhow!(
                        "DOCX relationship {relationship_id} is referenced as both a header and footer"
                    ));
                }
            }
            if references_by_id.len() > MAX_DOCX_HEADER_FOOTER_PARTS {
                return Err(anyhow!(
                    "DOCX exceeds the {MAX_DOCX_HEADER_FOOTER_PARTS} referenced header/footer part limit"
                ));
            }
            cursor = end;
        }
    }

    let relationships = document_relationships(relationships_xml)?;
    let relationships_by_id = relationships
        .iter()
        .map(|relationship| (relationship.id.as_str(), relationship))
        .collect::<HashMap<_, _>>();
    let mut parts_by_path = BTreeMap::<String, HeaderFooterKind>::new();
    for (relationship_id, kind) in references_by_id {
        let relationship = relationships_by_id
            .get(relationship_id.as_str())
            .ok_or_else(|| {
                anyhow!(
                    "DOCX {} reference uses a missing relationship: {relationship_id}",
                    kind.as_str()
                )
            })?;
        if relationship.external || relationship.relationship_type != kind.relationship_type() {
            return Err(anyhow!(
                "DOCX {} reference uses an external or unexpected relationship type",
                kind.as_str()
            ));
        }
        let path = resolve_document_relationship_target(relationship.target.as_str())?;
        if !names.contains(path.as_str()) {
            return Err(anyhow!(
                "DOCX is missing referenced {} part: {path}",
                kind.as_str()
            ));
        }
        let content_types = content_types_for_part(content_types_xml, format!("/{path}").as_str())?;
        if content_types.len() != 1 || content_types[0] != kind.content_type() {
            return Err(anyhow!(
                "DOCX referenced {} part has a missing, duplicate, or unexpected content type: {path}",
                kind.as_str()
            ));
        }
        if let Some(existing) = parts_by_path.insert(path.clone(), kind) {
            if existing != kind {
                return Err(anyhow!(
                    "DOCX package part is referenced as both a header and footer: {path}"
                ));
            }
        }
    }
    Ok(parts_by_path
        .into_iter()
        .map(|(path, kind)| ReferencedHeaderFooterPart { path, kind })
        .collect())
}

pub(super) fn selected_header_footer_parts(
    arguments: &Value,
    referenced_parts: &[ReferencedHeaderFooterPart],
) -> Result<Vec<ReferencedHeaderFooterPart>> {
    let Some(value) = arguments.get("part_names") else {
        return Ok(referenced_parts.to_vec());
    };
    let values = value
        .as_array()
        .ok_or_else(|| anyhow!("part_names must be an array of referenced DOCX part names"))?;
    if values.is_empty() || values.len() > MAX_DOCX_HEADER_FOOTER_PARTS {
        return Err(anyhow!(
            "part_names must contain between 1 and {MAX_DOCX_HEADER_FOOTER_PARTS} items"
        ));
    }
    let referenced_by_path = referenced_parts
        .iter()
        .map(|part| (part.path.as_str(), part))
        .collect::<HashMap<_, _>>();
    let mut selected = Vec::with_capacity(values.len());
    let mut seen = HashSet::with_capacity(values.len());
    for value in values {
        let path = value
            .as_str()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow!("part_names must contain non-empty strings"))?;
        if !seen.insert(path) {
            return Err(anyhow!("part_names must not contain duplicates"));
        }
        let part = referenced_by_path.get(path).ok_or_else(|| {
            anyhow!(
                "part_names contains a part that is not a referenced DOCX header or footer: {path}"
            )
        })?;
        selected.push((*part).clone());
    }
    Ok(selected)
}

pub(super) fn validate_header_footer_part_xml(xml: &str, kind: HeaderFooterKind) -> Result<()> {
    if xml.contains("<!--") || xml.contains("<![CDATA[") || xml.contains("<!DOCTYPE") {
        return Err(anyhow!(
            "DOCX {} editing does not support comments, CDATA, or DTD markup",
            kind.as_str()
        ));
    }
    let opening = format!("<{}", kind.root_tag());
    let closing = format!("</{}>", kind.root_tag());
    let Some(start) = find_next_xml_tag_start(xml, opening.as_str(), 0) else {
        return Err(anyhow!(
            "DOCX {} part is missing its standard root element",
            kind.as_str()
        ));
    };
    if find_next_xml_tag_start(xml, opening.as_str(), start + opening.len()).is_some()
        || xml.matches(closing.as_str()).count() != 1
    {
        return Err(anyhow!(
            "DOCX {} part contains ambiguous root elements",
            kind.as_str()
        ));
    }
    Ok(())
}
