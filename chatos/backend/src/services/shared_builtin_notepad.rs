// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use async_trait::async_trait;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine as _;
use regex::Regex;
use serde_json::Value;
use url::Url;

use chatos_mcp::NotepadStore;

use crate::services::notepad::{
    CreateNoteParams, ListNotesParams, NotepadService, SearchNotesParams, UpdateNoteParams,
};

#[derive(Clone)]
pub struct ChatosNotepadStore {
    service: NotepadService,
}

impl ChatosNotepadStore {
    pub fn new(user_id: &str) -> Result<Self, String> {
        Ok(Self {
            service: NotepadService::new(user_id)?,
        })
    }
}

#[async_trait]
impl NotepadStore for ChatosNotepadStore {
    async fn init(&self) -> Result<Value, String> {
        self.service.init().await
    }

    async fn list_folders(&self) -> Result<Value, String> {
        self.service.list_folders().await
    }

    async fn create_folder(&self, folder: &str) -> Result<Value, String> {
        self.service.create_folder(folder).await
    }

    async fn rename_folder(&self, from: &str, to: &str) -> Result<Value, String> {
        self.service.rename_folder(from, to).await
    }

    async fn delete_folder(&self, folder: &str, recursive: bool) -> Result<Value, String> {
        self.service.delete_folder(folder, recursive).await
    }

    async fn list_notes(&self, params: Value) -> Result<Value, String> {
        self.service
            .list_notes(ListNotesParams {
                folder: string_field(&params, "folder"),
                recursive: bool_field(&params, "recursive", true),
                tags: string_array_field(&params, "tags"),
                match_any: bool_field(&params, "match_any", false),
                query: string_field(&params, "query"),
                limit: usize_field(&params, "limit", 200),
            })
            .await
    }

    async fn create_note(&self, params: Value) -> Result<Value, String> {
        self.service
            .create_note(CreateNoteParams {
                folder: string_field(&params, "folder"),
                title: string_field(&params, "title"),
                content: string_field(&params, "content"),
                tags: string_array_field(&params, "tags"),
            })
            .await
    }

    async fn read_note(&self, id: &str, image_offset: usize) -> Result<Value, String> {
        let value = self.service.get_note(id).await?;
        Ok(enrich_note_with_images(value, image_offset).await)
    }

    async fn update_note(&self, params: Value) -> Result<Value, String> {
        self.service
            .update_note(UpdateNoteParams {
                id: string_field(&params, "id"),
                title: optional_string_field(&params, "title"),
                content: optional_string_field(&params, "content"),
                folder: optional_string_field(&params, "folder"),
                tags: optional_string_array_field(&params, "tags"),
            })
            .await
    }

    async fn delete_note(&self, id: &str) -> Result<Value, String> {
        self.service.delete_note(id).await
    }

    async fn list_tags(&self) -> Result<Value, String> {
        self.service.list_tags().await
    }

    async fn search_notes(&self, params: Value) -> Result<Value, String> {
        self.service
            .search_notes(SearchNotesParams {
                query: string_field(&params, "query"),
                folder: string_field(&params, "folder"),
                recursive: bool_field(&params, "recursive", true),
                tags: string_array_field(&params, "tags"),
                match_any: bool_field(&params, "match_any", false),
                include_content: bool_field(&params, "include_content", true),
                limit: usize_field(&params, "limit", 50),
            })
            .await
    }
}

async fn enrich_note_with_images(mut value: Value, requested_offset: usize) -> Value {
    const MAX_IMAGES: usize = 2;
    const MAX_TOTAL_BYTES: usize = 2 * 1024 * 1024;

    let content = value
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let image_tokens = markdown_image_urls(content)
        .into_iter()
        .filter_map(|raw_url| {
            let url = Url::parse(raw_url.as_str()).ok()?;
            if !is_attachment_object_path(url.path()) {
                return None;
            }
            url.query_pairs()
                .find_map(|(key, value)| (key == "token").then(|| value.into_owned()))
        })
        .collect::<Vec<_>>();
    if image_tokens.is_empty() {
        insert_image_page(&mut value, requested_offset, 0, 0, 0, 0);
        return value;
    }
    let storage = match crate::services::object_storage::service().await {
        Ok(storage) => storage,
        Err(_) => {
            let total = image_tokens.len();
            insert_image_page(&mut value, requested_offset, total, 0, 0, total);
            if let Some(page) = value.get_mut("imagePage").and_then(Value::as_object_mut) {
                page.insert("unavailable".to_string(), Value::Bool(true));
            }
            return value;
        }
    };
    let signed_images = image_tokens
        .into_iter()
        .filter_map(|token| storage.decode_signed_object(token.as_str()).ok())
        .filter(|signed| {
            matches!(
                signed.content_type.to_ascii_lowercase().as_str(),
                "image/png" | "image/jpeg" | "image/webp"
            )
        })
        .collect::<Vec<_>>();
    let total = signed_images.len();
    let offset = requested_offset.min(total);
    let mut total_bytes = 0usize;
    let mut images = Vec::new();
    let mut next_offset = offset;
    let mut skipped = 0usize;
    for (index, signed) in signed_images.iter().enumerate().skip(offset) {
        if images.len() >= MAX_IMAGES {
            break;
        }
        let mime_type = signed.content_type.to_ascii_lowercase();
        let Ok(object) = storage
            .get_object_bytes(&signed.object_ref, Some(MAX_TOTAL_BYTES as u64))
            .await
        else {
            skipped += 1;
            next_offset = index + 1;
            continue;
        };
        total_bytes = match total_bytes.checked_add(object.bytes.len()) {
            Some(total) if total <= MAX_TOTAL_BYTES => total,
            _ => {
                next_offset = index;
                break;
            }
        };
        images.push(serde_json::json!({
            "mimeType": mime_type,
            "data": BASE64_STANDARD.encode(object.bytes.as_ref()),
        }));
        next_offset = index + 1;
    }
    let returned = images.len();
    if let Some(object) = value.as_object_mut() {
        if !images.is_empty() {
            object.insert("_mcp_images".to_string(), Value::Array(images));
        }
    }
    insert_image_page(
        &mut value,
        requested_offset,
        total,
        returned,
        skipped,
        next_offset,
    );
    value
}

fn insert_image_page(
    value: &mut Value,
    requested_offset: usize,
    total: usize,
    returned: usize,
    skipped: usize,
    next_offset: usize,
) {
    let offset = requested_offset.min(total);
    let has_more = next_offset < total;
    let next_offset = has_more.then_some(next_offset);
    if let Some(object) = value.as_object_mut() {
        object.insert(
            "imagePage".to_string(),
            serde_json::json!({
                "offset": offset,
                "limit": 2,
                "returned": returned,
                "skipped": skipped,
                "total": total,
                "hasMore": has_more,
                "nextOffset": next_offset,
            }),
        );
    }
}

fn markdown_image_urls(markdown: &str) -> Vec<String> {
    let Ok(pattern) = Regex::new(r#"!\[[^\]]*\]\(\s*<?([^\s)>]+)>?\s*\)"#) else {
        return Vec::new();
    };
    pattern
        .captures_iter(markdown)
        .filter_map(|capture| capture.get(1).map(|value| value.as_str().to_string()))
        .collect()
}

fn is_attachment_object_path(path: &str) -> bool {
    path == "/api/attachments/object" || path.ends_with("/attachments/object")
}

fn string_field(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("")
        .to_string()
}

fn optional_string_field(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn string_array_field(value: &Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn optional_string_array_field(value: &Value, key: &str) -> Option<Vec<String>> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|_| string_array_field(value, key))
}

fn bool_field(value: &Value, key: &str, default: bool) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(default)
}

fn usize_field(value: &Value, key: &str, default: usize) -> usize {
    value
        .get(key)
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::{insert_image_page, is_attachment_object_path, markdown_image_urls};
    use serde_json::json;

    #[test]
    fn extracts_plain_and_angle_wrapped_markdown_images() {
        let urls = markdown_image_urls(
            "before\n![one](<https://example.test/api/attachments/object?token=a>)\n![two](https://example.test/api/attachments/object?token=b)",
        );
        assert_eq!(urls.len(), 2);
        assert!(urls[0].ends_with("token=a"));
        assert!(urls[1].ends_with("token=b"));
    }

    #[test]
    fn accepts_public_and_gateway_attachment_paths() {
        assert!(is_attachment_object_path("/api/attachments/object"));
        assert!(is_attachment_object_path("/api/chatos/attachments/object"));
        assert!(!is_attachment_object_path("/api/attachments/other"));
    }

    #[test]
    fn image_page_points_to_the_next_two_image_batch() {
        let mut note = json!({"content": "note"});
        insert_image_page(&mut note, 2, 5, 2, 0, 4);

        assert_eq!(note["imagePage"]["offset"], 2);
        assert_eq!(note["imagePage"]["returned"], 2);
        assert_eq!(note["imagePage"]["skipped"], 0);
        assert_eq!(note["imagePage"]["total"], 5);
        assert_eq!(note["imagePage"]["hasMore"], true);
        assert_eq!(note["imagePage"]["nextOffset"], 4);
    }

    #[test]
    fn final_image_page_has_no_next_offset() {
        let mut note = json!({"content": "note"});
        insert_image_page(&mut note, 4, 5, 1, 0, 5);

        assert_eq!(note["imagePage"]["hasMore"], false);
        assert!(note["imagePage"]["nextOffset"].is_null());
    }
}
