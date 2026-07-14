// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use base64::engine::general_purpose::STANDARD as BASE64_STD;
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::Read;

const MAX_EXTRACTED_DOCUMENT_XML_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Attachment {
    pub id: Option<String>,
    pub name: Option<String>,
    pub mime_type: Option<String>,
    pub size: Option<u64>,
    pub data_url: Option<String>,
    pub text: Option<String>,
    pub r#type: Option<String>,
    pub storage_provider: Option<String>,
    pub bucket: Option<String>,
    pub object_key: Option<String>,
    pub url: Option<String>,
    pub view_url: Option<String>,
}

fn get_str(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(|v| v.as_str()).map(|s| s.to_string())
}

fn get_u64(v: &Value, key: &str) -> Option<u64> {
    v.get(key).and_then(|v| v.as_u64())
}

pub fn parse_attachments(list: &[Value]) -> Vec<Attachment> {
    list.iter()
        .map(|v| Attachment {
            id: get_str(v, "id"),
            name: get_str(v, "name"),
            mime_type: get_str(v, "mimeType").or_else(|| get_str(v, "mime")),
            size: get_u64(v, "size"),
            data_url: get_str(v, "dataUrl"),
            text: get_str(v, "text"),
            r#type: get_str(v, "type"),
            storage_provider: get_str(v, "storageProvider")
                .or_else(|| get_str(v, "storage_provider")),
            bucket: get_str(v, "bucket"),
            object_key: get_str(v, "objectKey").or_else(|| get_str(v, "object_key")),
            url: get_str(v, "url"),
            view_url: get_str(v, "viewUrl").or_else(|| get_str(v, "view_url")),
        })
        .collect()
}

pub async fn build_content_parts_async(user_text: &str, attachments: &[Attachment]) -> Value {
    let mut parts: Vec<Value> = Vec::new();
    let text = user_text.to_string();
    if !text.trim().is_empty() {
        parts.push(json!({"type": "text", "text": text}));
    }

    let max_chars = 20000usize;

    for att in attachments {
        let name = att.name.clone().unwrap_or_else(|| "attachment".to_string());
        let mime = att
            .mime_type
            .clone()
            .unwrap_or_else(|| "application/octet-stream".to_string());
        let size = att.size.unwrap_or(0);
        let public_url = attachment_public_url(att).await;
        let meta_line = match public_url.as_deref() {
            Some(url) => format!(
                "Attachment: {} ({}, {} bytes)\nURL: {}",
                name, mime, size, url
            ),
            None => format!("Attachment: {} ({}, {} bytes)", name, mime, size),
        };

        if mime.starts_with("image/") {
            if let Some(data_url) = att
                .data_url
                .as_deref()
                .map(str::trim)
                .filter(|value| is_supported_image_locator(value))
            {
                parts.push(json!({"type": "image_url", "image_url": {"url": data_url}}));
                parts.push(json!({"type": "text", "text": meta_line}));
                continue;
            }
            if let Some(object_data) = load_object_attachment_bytes(att).await {
                match object_data {
                    Ok(bytes) => {
                        let data_url = image_data_url(mime.as_str(), bytes.as_ref());
                        parts.push(json!({"type": "image_url", "image_url": {"url": data_url}}));
                        parts.push(json!({"type": "text", "text": meta_line}));
                    }
                    Err(err) => {
                        parts.push(json!({"type": "text", "text": format!("{} [image content not included: {}]", meta_line, err)}));
                    }
                }
                continue;
            }
            if let Some(url) = public_url.filter(|url| is_supported_image_locator(url.as_str())) {
                parts.push(json!({"type": "image_url", "image_url": {"url": url}}));
                parts.push(json!({"type": "text", "text": meta_line}));
                continue;
            }
        }

        if let Some(text) = &att.text {
            if !text.is_empty() {
                let body = if text.chars().count() > max_chars {
                    format!("{}\n...[truncated]", truncate_chars(text, max_chars))
                } else {
                    text.clone()
                };
                let fenced = format!("{}\n\n【File Content】\n\n{}", meta_line, body);
                parts.push(json!({"type": "text", "text": fenced}));
                continue;
            }
        }

        if let Some(data_url) = &att.data_url {
            if mime == "text/plain" {
                if let Some(decoded) = decode_data_url(data_url) {
                    let body = String::from_utf8_lossy(&decoded).to_string();
                    let body = if body.chars().count() > max_chars {
                        format!(
                            "{}\n...[truncated]",
                            truncate_chars(body.as_str(), max_chars)
                        )
                    } else {
                        body
                    };
                    parts.push(
                        json!({"type": "text", "text": format!("{}\n\n{}", meta_line, body)}),
                    );
                    continue;
                }
            }
            if mime == "application/pdf" {
                if let Some(decoded) = decode_data_url(data_url) {
                    if let Some(text) = extract_pdf_text(&decoded) {
                        let body = if text.chars().count() > max_chars {
                            format!(
                                "{}\n...[truncated]",
                                truncate_chars(text.as_str(), max_chars)
                            )
                        } else {
                            text
                        };
                        parts.push(json!({"type": "text", "text": format!("{}\n\n【Extracted from PDF】\n\n{}", meta_line, body)}));
                        continue;
                    }
                }
            }
            if mime == "application/vnd.openxmlformats-officedocument.wordprocessingml.document" {
                if let Some(decoded) = decode_data_url(data_url) {
                    if let Some(text) = extract_docx_text(&decoded) {
                        let body = if text.chars().count() > max_chars {
                            format!(
                                "{}\n...[truncated]",
                                truncate_chars(text.as_str(), max_chars)
                            )
                        } else {
                            text
                        };
                        parts.push(json!({"type": "text", "text": format!("{}\n\n【Extracted from DOCX】\n\n{}", meta_line, body)}));
                        continue;
                    }
                }
            }
        }

        if let Some(object_data) = load_object_attachment_bytes(att).await {
            match object_data {
                Ok(bytes) => {
                    if is_text_like_mime(mime.as_str()) {
                        let body = String::from_utf8_lossy(bytes.as_ref()).to_string();
                        let body = if body.chars().count() > max_chars {
                            format!(
                                "{}\n...[truncated]",
                                truncate_chars(body.as_str(), max_chars)
                            )
                        } else {
                            body
                        };
                        parts.push(
                            json!({"type": "text", "text": format!("{}\n\n{}", meta_line, body)}),
                        );
                        continue;
                    }
                    if mime == "application/pdf" {
                        if let Some(text) = extract_pdf_text(bytes.as_ref()) {
                            let body = if text.chars().count() > max_chars {
                                format!(
                                    "{}\n...[truncated]",
                                    truncate_chars(text.as_str(), max_chars)
                                )
                            } else {
                                text
                            };
                            parts.push(json!({"type": "text", "text": format!("{}\n\n[Extracted from PDF]\n\n{}", meta_line, body)}));
                            continue;
                        }
                    }
                    if mime
                        == "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
                    {
                        if let Some(text) = extract_docx_text(bytes.as_ref()) {
                            let body = if text.chars().count() > max_chars {
                                format!(
                                    "{}\n...[truncated]",
                                    truncate_chars(text.as_str(), max_chars)
                                )
                            } else {
                                text
                            };
                            parts.push(json!({"type": "text", "text": format!("{}\n\n[Extracted from DOCX]\n\n{}", meta_line, body)}));
                            continue;
                        }
                    }
                }
                Err(err) => {
                    parts.push(json!({"type": "text", "text": format!("{} [content not included: {}]", meta_line, err)}));
                    continue;
                }
            }
        }

        parts
            .push(json!({"type": "text", "text": format!("{} [content not included]", meta_line)}));
    }

    if parts.is_empty() {
        Value::String(text)
    } else {
        Value::Array(parts)
    }
}

async fn attachment_public_url(att: &Attachment) -> Option<String> {
    if let Some(url) = att
        .view_url
        .as_deref()
        .or(att.url.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Some(url.to_string());
    }

    let object_key = att.object_key.as_deref()?.trim();
    if object_key.is_empty() {
        return None;
    }
    let storage = crate::services::object_storage::service().await.ok()?;
    storage
        .signed_object_url(crate::services::object_storage::SignedObject {
            object_ref: crate::services::object_storage::StoredObjectRef {
                bucket: att.bucket.clone(),
                object_key: object_key.to_string(),
                name: att.name.clone(),
                mime_type: att.mime_type.clone(),
            },
            content_type: att
                .mime_type
                .clone()
                .unwrap_or_else(|| "application/octet-stream".to_string()),
            file_name: att.name.clone().unwrap_or_else(|| "attachment".to_string()),
        })
        .ok()
}

async fn load_object_attachment_bytes(att: &Attachment) -> Option<Result<bytes::Bytes, String>> {
    let object_key = att.object_key.as_deref()?.trim();
    if object_key.is_empty() {
        return None;
    }
    let storage = match crate::services::object_storage::service().await {
        Ok(storage) => storage,
        Err(err) => return Some(Err(err)),
    };
    Some(
        storage
            .get_object_bytes(
                &crate::services::object_storage::StoredObjectRef {
                    bucket: att.bucket.clone(),
                    object_key: object_key.to_string(),
                    name: att.name.clone(),
                    mime_type: att.mime_type.clone(),
                },
                Some(storage.max_read_bytes()),
            )
            .await
            .map(|object| object.bytes),
    )
}

fn is_text_like_mime(mime: &str) -> bool {
    mime.starts_with("text/")
        || matches!(
            mime,
            "application/json"
                | "application/xml"
                | "application/javascript"
                | "application/x-javascript"
                | "application/x-ndjson"
                | "application/yaml"
                | "application/toml"
                | "text/markdown"
        )
        || mime.ends_with("+json")
        || mime.ends_with("+xml")
}

fn decode_data_url(data_url: &str) -> Option<Vec<u8>> {
    let parts: Vec<&str> = data_url.split(',').collect();
    let b64 = if parts.len() > 1 { parts[1] } else { data_url };
    BASE64_STD.decode(b64.as_bytes()).ok()
}

fn image_data_url(mime: &str, bytes: &[u8]) -> String {
    let mime = if mime.trim().starts_with("image/") {
        mime.trim()
    } else {
        "image/png"
    };
    format!("data:{mime};base64,{}", BASE64_STD.encode(bytes))
}

fn is_supported_image_locator(value: &str) -> bool {
    let value = value.trim().to_ascii_lowercase();
    value.starts_with("https://")
        || value.starts_with("http://")
        || value.starts_with("data:image/")
}

fn extract_pdf_text(data: &[u8]) -> Option<String> {
    // Best-effort extraction; fallback to None on failure
    #[allow(unused_mut)]
    let mut text = None;
    #[cfg(feature = "pdf")]
    {
        // placeholder for feature-gated implementations
    }
    // Try lopdf if available
    if text.is_none() {
        if let Ok(doc) = lopdf::Document::load_mem(data) {
            let pages: Vec<u32> = doc.get_pages().keys().cloned().collect();
            if let Ok(t) = doc.extract_text(&pages) {
                let t = t.trim().to_string();
                if !t.is_empty() {
                    text = Some(t);
                }
            }
        }
    }
    text
}

fn extract_docx_text(data: &[u8]) -> Option<String> {
    let reader = std::io::Cursor::new(data);
    let mut archive = zip::ZipArchive::new(reader).ok()?;
    let mut file = archive.by_name("word/document.xml").ok()?;
    if file.size() > MAX_EXTRACTED_DOCUMENT_XML_BYTES {
        return None;
    }
    let mut xml = String::new();
    file.by_ref()
        .take(MAX_EXTRACTED_DOCUMENT_XML_BYTES.saturating_add(1))
        .read_to_string(&mut xml)
        .ok()?;
    if xml.len() as u64 > MAX_EXTRACTED_DOCUMENT_XML_BYTES {
        return None;
    }

    let mut reader = quick_xml::Reader::from_str(&xml);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut out = String::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(quick_xml::events::Event::Text(e)) => {
                if let Some(text) = e.decode().ok().and_then(|value| {
                    quick_xml::escape::unescape(value.as_ref())
                        .ok()
                        .map(|value| value.into_owned())
                }) {
                    out.push_str(&text);
                }
            }
            Ok(quick_xml::events::Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    let trimmed = out.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

pub fn sanitize_attachments_for_db(attachments: &[Attachment]) -> Vec<Value> {
    let max_inline = 150 * 1024usize;
    attachments.iter().map(|att| {
        let mut obj = json!({
            "id": att.id,
            "type": att.r#type.clone().unwrap_or_else(|| {
                if att.mime_type.as_deref().unwrap_or("").starts_with("image/") { "image".to_string() } else { "file".to_string() }
            }),
            "name": att.name,
            "size": att.size,
            "mimeType": att.mime_type,
        });
        if let Some(map) = obj.as_object_mut() {
            if let Some(storage_provider) = &att.storage_provider {
                map.insert("storageProvider".to_string(), Value::String(storage_provider.clone()));
            }
            if let Some(bucket) = &att.bucket {
                map.insert("bucket".to_string(), Value::String(bucket.clone()));
            }
            if let Some(object_key) = &att.object_key {
                map.insert("objectKey".to_string(), Value::String(object_key.clone()));
            }
            if let Some(url) = att.view_url.as_ref().or(att.url.as_ref()) {
                map.insert("url".to_string(), Value::String(url.clone()));
                map.insert("viewUrl".to_string(), Value::String(url.clone()));
            }
        }
        if let Some(data_url) = &att.data_url {
            if data_url.len() <= max_inline {
                if let Some(map) = obj.as_object_mut() {
                    map.insert("preview".to_string(), Value::String(data_url.clone()));
                }
            }
        }
        obj
    }).collect()
}

pub fn is_vision_model(model_name: &str) -> bool {
    let m = model_name.to_lowercase();
    m.contains("gpt-4o")
        || m.contains("gpt-4.1")
        || m.contains("gpt-4.1-mini")
        || m.contains("o3")
        || m.contains("omni")
        || m.contains("doubao")
        || m.contains("volc")
        || m.contains("ark")
        || m.contains("seed")
}

pub fn adapt_parts_for_model(
    model_name: &str,
    parts: &Value,
    supports_images: Option<bool>,
) -> Value {
    if supports_images == Some(true) {
        return parts.clone();
    }
    if supports_images.is_none() && is_vision_model(model_name) {
        return parts.clone();
    }
    if let Value::Array(arr) = parts {
        let mut out: Vec<Value> = Vec::new();
        for p in arr {
            if p.get("type").and_then(|v| v.as_str()) == Some("image_url") {
                out.push(json!({"type": "text", "text": "[Image attachment omitted: model does not support images]"}));
            } else {
                out.push(p.clone());
            }
        }
        Value::Array(out)
    } else {
        parts.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_content_parts_async, decode_data_url, image_data_url, is_supported_image_locator,
        Attachment,
    };

    #[test]
    fn object_image_bytes_become_a_valid_data_url() {
        let bytes = b"not-a-real-image-but-valid-base64";
        let data_url = image_data_url("image/jpeg", bytes);
        assert!(data_url.starts_with("data:image/jpeg;base64,"));
        assert_eq!(
            decode_data_url(data_url.as_str()).as_deref(),
            Some(bytes.as_slice())
        );
    }

    #[test]
    fn relative_attachment_routes_are_not_model_image_locators() {
        assert!(!is_supported_image_locator(
            "/api/attachments/object?token=signed-token"
        ));
        assert!(is_supported_image_locator(
            "https://oss.example.com/bucket/image.png?signature=ok"
        ));
        assert!(is_supported_image_locator("data:image/png;base64,Zm9v"));
    }

    #[tokio::test]
    async fn relative_view_url_is_kept_as_metadata_not_sent_as_an_image() {
        let parts = build_content_parts_async(
            "describe this image",
            &[Attachment {
                name: Some("photo.jpg".to_string()),
                mime_type: Some("image/jpeg".to_string()),
                size: Some(42),
                view_url: Some("/api/attachments/object?token=signed-token".to_string()),
                ..Attachment::default()
            }],
        )
        .await;
        let parts = parts.as_array().expect("content parts");
        assert!(!parts.iter().any(|part| {
            part.get("type").and_then(|value| value.as_str()) == Some("image_url")
        }));
        assert!(parts.iter().any(|part| {
            part.get("text")
                .and_then(|value| value.as_str())
                .is_some_and(|text| text.contains("/api/attachments/object?token=signed-token"))
        }));
    }
}
