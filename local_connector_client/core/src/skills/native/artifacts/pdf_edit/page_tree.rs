// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, HashSet};

use anyhow::{anyhow, Context, Result};
use lopdf::{Dictionary, Document, Object, ObjectId};

const INHERITED_PAGE_KEYS: [&[u8]; 4] = [b"Resources", b"MediaBox", b"CropBox", b"Rotate"];
const UNSAFE_PAGE_ARRANGE_CATALOG_KEYS: [&[u8]; 9] = [
    b"AcroForm",
    b"Dests",
    b"Names",
    b"OpenAction",
    b"Outlines",
    b"PageLabels",
    b"StructTreeRoot",
    b"Threads",
    b"AA",
];

pub(super) fn validate_arrangeable_pdf(
    document: &Document,
    page_map: &BTreeMap<u32, ObjectId>,
) -> Result<()> {
    let catalog = document.catalog().context("read PDF catalog")?;
    for key in UNSAFE_PAGE_ARRANGE_CATALOG_KEYS {
        if catalog.has(key) {
            return Err(anyhow!(
                "PDF page arrangement does not support catalog feature /{}",
                String::from_utf8_lossy(key)
            ));
        }
    }
    let pages_root_id = catalog
        .get(b"Pages")
        .and_then(Object::as_reference)
        .context("read PDF catalog Pages reference")?;
    let pages_root = document
        .get_object(pages_root_id)
        .and_then(Object::as_dict)
        .context("read PDF pages root")?;
    if !pages_root
        .get(b"Type")
        .and_then(Object::as_name)
        .is_ok_and(|value| value == b"Pages")
    {
        return Err(anyhow!(
            "PDF catalog Pages reference is not a Pages dictionary"
        ));
    }
    let mut page_ids = HashSet::with_capacity(page_map.len());
    for (page_number, page_id) in page_map {
        if !page_ids.insert(*page_id) {
            return Err(anyhow!("PDF page tree contains duplicate page references"));
        }
        let page = document
            .get_object(*page_id)
            .and_then(Object::as_dict)
            .with_context(|| format!("read page {page_number} dictionary"))?;
        if !page
            .get(b"Type")
            .and_then(Object::as_name)
            .is_ok_and(|value| value == b"Page")
        {
            return Err(anyhow!("page {page_number} is not a Page dictionary"));
        }
        if page.has(b"Annots") {
            return Err(anyhow!(
                "PDF page arrangement does not support page annotations"
            ));
        }
    }
    Ok(())
}

pub(super) fn merge_documents(documents: Vec<Document>) -> Result<Document> {
    let mut max_id = 1_u32;
    let mut pages = Vec::<(ObjectId, Object)>::new();
    let mut objects = BTreeMap::<ObjectId, Object>::new();

    for mut document in documents {
        document.renumber_objects_with(max_id);
        max_id = document.max_id.saturating_add(1);
        for page_id in document.get_pages().into_values() {
            pages.push((page_id, materialized_page(&document, page_id)?));
        }
        objects.extend(document.objects);
    }

    let mut catalog: Option<(ObjectId, Dictionary)> = None;
    let mut pages_root: Option<(ObjectId, Dictionary)> = None;
    let mut merged = Document::with_version("1.7");

    for (object_id, object) in objects {
        match object.type_name().unwrap_or(b"") {
            b"Catalog" => {
                if catalog.is_none() {
                    catalog = Some((object_id, object.as_dict()?.clone()));
                }
            }
            b"Pages" => {
                if pages_root.is_none() {
                    pages_root = Some((object_id, object.as_dict()?.clone()));
                }
            }
            b"Page" | b"Outlines" | b"Outline" => {}
            _ => {
                merged.objects.insert(object_id, object);
            }
        }
    }

    let (pages_id, mut pages_dictionary) =
        pages_root.ok_or_else(|| anyhow!("PDF pages root not found"))?;
    let (catalog_id, mut catalog_dictionary) =
        catalog.ok_or_else(|| anyhow!("PDF catalog root not found"))?;

    let mut kids = Vec::with_capacity(pages.len());
    for (page_id, page) in pages {
        let mut page = page.as_dict()?.clone();
        page.set("Parent", pages_id);
        merged.objects.insert(page_id, Object::Dictionary(page));
        kids.push(Object::Reference(page_id));
    }
    pages_dictionary.set("Count", kids.len() as u32);
    pages_dictionary.set("Kids", kids);
    pages_dictionary.remove(b"Parent");
    merged
        .objects
        .insert(pages_id, Object::Dictionary(pages_dictionary));

    catalog_dictionary.set("Pages", pages_id);
    catalog_dictionary.remove(b"Outlines");
    catalog_dictionary.remove(b"PageMode");
    merged
        .objects
        .insert(catalog_id, Object::Dictionary(catalog_dictionary));
    merged.trailer.set("Root", catalog_id);
    merged.max_id = merged
        .objects
        .keys()
        .map(|object_id| object_id.0)
        .max()
        .unwrap_or(0);
    Ok(merged)
}

pub(super) fn materialized_page(document: &Document, page_id: ObjectId) -> Result<Object> {
    let mut page = document.get_object(page_id)?.as_dict()?.clone();
    for key in INHERITED_PAGE_KEYS {
        if !page.has(key) {
            if let Some(value) = inherited_page_attribute(document, page_id, key) {
                page.set(key, value);
            }
        }
    }
    Ok(Object::Dictionary(page))
}

pub(super) fn inherited_page_attribute(
    document: &Document,
    page_id: ObjectId,
    key: &[u8],
) -> Option<Object> {
    let mut current = Some(page_id);
    let mut visited = HashSet::new();
    while let Some(object_id) = current {
        if !visited.insert(object_id) {
            return None;
        }
        let dictionary = document.get_object(object_id).ok()?.as_dict().ok()?;
        if let Ok(value) = dictionary.get(key) {
            return Some(value.clone());
        }
        current = dictionary
            .get(b"Parent")
            .and_then(Object::as_reference)
            .ok();
    }
    None
}
