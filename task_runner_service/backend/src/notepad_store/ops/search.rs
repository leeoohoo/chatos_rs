// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl TaskRunnerNotepadStore {
    pub(super) async fn list_tags_value(&self) -> Result<Value, String> {
        let notes = self.load_index().await?.notes;
        let mut counts = BTreeMap::<String, usize>::new();
        for note in notes {
            for tag in note.tags {
                *counts.entry(tag).or_default() += 1;
            }
        }
        Ok(json!({
            "ok": true,
            "tags": counts
                .into_iter()
                .map(|(tag, count)| json!({ "tag": tag, "count": count }))
                .collect::<Vec<_>>(),
        }))
    }

    pub(super) async fn search_notes_value(&self, params: Value) -> Result<Value, String> {
        let query = normalize_required(value_string(&params, "query").as_str(), "query")?;
        let folder = normalize_optional_folder(value_string(&params, "folder"))?;
        let recursive = params
            .get("recursive")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let tags = normalize_tags(value_string_array(&params, "tags"));
        let match_any = params
            .get("match_any")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let include_content = params
            .get("include_content")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let limit = params
            .get("limit")
            .and_then(Value::as_u64)
            .map(|value| value as usize)
            .unwrap_or(50)
            .clamp(1, 200);
        let mut notes = self.load_index().await?.notes;
        filter_notes(
            &mut notes,
            folder.as_deref(),
            recursive,
            &tags,
            match_any,
            "",
        );
        let needle = query.to_ascii_lowercase();
        let mut matches = Vec::new();
        for note in notes {
            let mut content_match = false;
            let mut preview = None;
            if include_content {
                let note_path = self.note_path(note.folder.as_str(), note.id.as_str());
                if let Ok(content) =
                    read_text_limited(note_path.as_path(), MAX_NOTE_CONTENT_BYTES).await
                {
                    let lowered = content.to_ascii_lowercase();
                    if lowered.contains(needle.as_str()) {
                        content_match = true;
                        preview = Some(content.chars().take(240).collect::<String>());
                    }
                }
            }
            let title_match = note.title.to_ascii_lowercase().contains(needle.as_str());
            let folder_match = note.folder.to_ascii_lowercase().contains(needle.as_str());
            if title_match || folder_match || content_match {
                matches.push(json!({
                    "note": self.note_output(&note),
                    "match": {
                        "title": title_match,
                        "folder": folder_match,
                        "content": content_match,
                    },
                    "preview": preview,
                }));
            }
            if matches.len() >= limit {
                break;
            }
        }
        Ok(json!({
            "ok": true,
            "query": query,
            "results": matches,
        }))
    }
}
