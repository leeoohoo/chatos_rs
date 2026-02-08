use base64::engine::general_purpose::STANDARD as BASE64_STD;
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::Read;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Attachment {
    pub id: Option<String>,
    pub name: Option<String>,
    pub mime_type: Option<String>,
    pub size: Option<u64>,
    pub data_url: Option<String>,
    pub text: Option<String>,
    pub r#type: Option<String>,
}

fn get_str(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(|v| v.as_str()).map(|s| s.to_string())
}

fn get_u64(v: &Value, key: &str) -> Option<u64> {
    v.get(key).and_then(|v| v.as_u64())
}

pub fn parse_attachments(list: &[Value]) -> Vec<Attachment> {
    list.iter().map(|v| {
        Attachment {
            id: get_str(v, "id"),
            name: get_str(v, "name"),
            mime_type: get_str(v, "mimeType").or_else(|| get_str(v, "mime")),
            size: get_u64(v, "size"),
            data_url: get_str(v, "dataUrl"),
            text: get_str(v, "text"),
            r#type: get_str(v, "type"),
        }
    }).collect()
}

pub fn build_content_parts(user_text: &str, attachments: &[Attachment]) -> Value {
    let mut parts: Vec<Value> = Vec::new();
    let text = user_text.to_string();
    if !text.trim().is_empty() {
        parts.push(json!({"type": "text", "text": text}));
    }

    for att in attachments {
        let name = att.name.clone().unwrap_or_else(|| "attachment".to_string());
        let mime = att.mime_type.clone().unwrap_or_else(|| "application/octet-stream".to_string());
        let size = att.size.unwrap_or(0);
        let meta_line = format!("Attachment: {} ({}, {} bytes)", name, mime, size);

        if mime.starts_with("image/") && att.data_url.is_some() {
            parts.push(json!({"type": "image_url", "image_url": {"url": att.data_url.clone().unwrap_or_default()}}));
            parts.push(json!({"type": "text", "text": meta_line}));
            continue;
        }

        if let Some(text) = &att.text {
            if !text.is_empty() {
                let max = 8000usize;
                let body = if text.len() > max { format!("{}\n...[truncated]", &text[..max]) } else { text.clone() };
                let fenced = format!("{}\n\n【File Content】\n\n{}", meta_line, body);
                parts.push(json!({"type": "text", "text": fenced}));
                continue;
            }
        }

        if let Some(data_url) = &att.data_url {
            if mime == "text/plain" {
                if let Some(decoded) = decode_data_url(data_url) {
                    let body = String::from_utf8_lossy(&decoded).to_string();
                    let max = 8000usize;
                    let body = if body.len() > max { format!("{}\n...[truncated]", &body[..max]) } else { body };
                    parts.push(json!({"type": "text", "text": format!("{}\n\n{}", meta_line, body)}));
                    continue;
                }
            }
        }

        parts.push(json!({"type": "text", "text": format!("{} [content not included]", meta_line)}));
    }

    if parts.is_empty() {
        Value::String(text)
    } else {
        Value::Array(parts)
    }
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
        let mime = att.mime_type.clone().unwrap_or_else(|| "application/octet-stream".to_string());
        let size = att.size.unwrap_or(0);
        let meta_line = format!("Attachment: {} ({}, {} bytes)", name, mime, size);

        if mime.starts_with("image/") && att.data_url.is_some() {
            parts.push(json!({"type": "image_url", "image_url": {"url": att.data_url.clone().unwrap_or_default()}}));
            parts.push(json!({"type": "text", "text": meta_line}));
            continue;
        }

        if let Some(text) = &att.text {
            if !text.is_empty() {
                let body = if text.len() > max_chars { format!("{}\n...[truncated]", &text[..max_chars]) } else { text.clone() };
                let fenced = format!("{}\n\n【File Content】\n\n{}", meta_line, body);
                parts.push(json!({"type": "text", "text": fenced}));
                continue;
            }
        }

        if let Some(data_url) = &att.data_url {
            if mime == "text/plain" {
                if let Some(decoded) = decode_data_url(data_url) {
                    let body = String::from_utf8_lossy(&decoded).to_string();
                    let body = if body.len() > max_chars { format!("{}\n...[truncated]", &body[..max_chars]) } else { body };
                    parts.push(json!({"type": "text", "text": format!("{}\n\n{}", meta_line, body)}));
                    continue;
                }
            }
            if mime == "application/pdf" {
                if let Some(decoded) = decode_data_url(data_url) {
                    if let Some(text) = extract_pdf_text(&decoded) {
                        let body = if text.len() > max_chars { format!("{}\n...[truncated]", &text[..max_chars]) } else { text };
                        parts.push(json!({"type": "text", "text": format!("{}\n\n【Extracted from PDF】\n\n{}", meta_line, body)}));
                        continue;
                    }
                }
            }
            if mime == "application/vnd.openxmlformats-officedocument.wordprocessingml.document" {
                if let Some(decoded) = decode_data_url(data_url) {
                    if let Some(text) = extract_docx_text(&decoded) {
                        let body = if text.len() > max_chars { format!("{}\n...[truncated]", &text[..max_chars]) } else { text };
                        parts.push(json!({"type": "text", "text": format!("{}\n\n【Extracted from DOCX】\n\n{}", meta_line, body)}));
                        continue;
                    }
                }
            }
        }

        parts.push(json!({"type": "text", "text": format!("{} [content not included]", meta_line)}));
    }

    if parts.is_empty() {
        Value::String(text)
    } else {
        Value::Array(parts)
    }
}

fn decode_data_url(data_url: &str) -> Option<Vec<u8>> {
    let parts: Vec<&str> = data_url.split(',').collect();
    let b64 = if parts.len() > 1 { parts[1] } else { data_url };
    BASE64_STD.decode(b64.as_bytes()).ok()
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
    let mut xml = String::new();
    file.read_to_string(&mut xml).ok()?;

    let mut reader = quick_xml::Reader::from_str(&xml);
    reader.trim_text(true);
    let mut buf = Vec::new();
    let mut out = String::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(quick_xml::events::Event::Text(e)) => {
                if let Ok(t) = e.unescape() {
                    out.push_str(&t);
                }
            }
            Ok(quick_xml::events::Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    let trimmed = out.trim().to_string();
    if trimmed.is_empty() { None } else { Some(trimmed) }
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
    m.contains("gpt-4o") || m.contains("gpt-4.1") || m.contains("gpt-4.1-mini") || m.contains("o3") || m.contains("omni") || m.contains("doubao") || m.contains("volc") || m.contains("ark") || m.contains("seed")
}

pub fn adapt_parts_for_model(model_name: &str, parts: &Value, supports_images: Option<bool>) -> Value {
    if supports_images == Some(true) { return parts.clone(); }
    if supports_images.is_none() && is_vision_model(model_name) { return parts.clone(); }
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

