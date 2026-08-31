// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;
use std::path::{Path, PathBuf};

use axum::extract::State;
use axum::Json;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine as _;
use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::LocalRuntime;

const MAX_DIRECTORY_ENTRIES: usize = 256;
const MAX_METADATA_BYTES: u64 = 16 * 1024;
const MAX_FRAME_BYTES: u64 = 2 * 1024 * 1024;
const ACTIVE_FRAME_MAX_AGE_SECONDS: i64 = 15;

#[derive(Debug, Deserialize)]
struct VisualHostMetadata {
    protocol_version: u32,
    adapter_session_id: String,
    plugin_id: String,
    component_key: String,
}

#[derive(Debug, Deserialize)]
struct VisualSessionMetadata {
    protocol_version: u32,
    session_id: String,
    status: String,
    title: String,
    #[serde(default)]
    target_app: Option<String>,
    #[serde(default)]
    mime_type: Option<String>,
    #[serde(default)]
    frame_file: Option<String>,
    #[serde(default)]
    frame_sequence: u64,
    captured_at: String,
    #[serde(default)]
    width: Option<u32>,
    #[serde(default)]
    height: Option<u32>,
}

struct VisualSessionCandidate {
    captured_at: DateTime<Utc>,
    body: Value,
}

pub(crate) async fn local_plugin_visual_session(
    State(runtime): State<LocalRuntime>,
) -> Json<Value> {
    let root = runtime
        .plugin_installer
        .plugin_root()
        .join("visual-sessions");
    let session = newest_visual_session(root.as_path())
        .map(|candidate| candidate.body)
        .unwrap_or(Value::Null);
    Json(json!({ "session": session }))
}

fn newest_visual_session(root: &Path) -> Option<VisualSessionCandidate> {
    let now = Utc::now();
    session_directories(root)
        .into_iter()
        .filter_map(|directory| read_visual_session(directory.as_path(), now))
        .max_by_key(|candidate| candidate.captured_at)
}

fn session_directories(root: &Path) -> Vec<PathBuf> {
    let mut current = vec![root.to_path_buf()];
    for _ in 0..3 {
        let mut next = Vec::new();
        for directory in current {
            let Ok(entries) = fs::read_dir(directory) else {
                continue;
            };
            for entry in entries.flatten().take(MAX_DIRECTORY_ENTRIES) {
                let Ok(file_type) = entry.file_type() else {
                    continue;
                };
                if file_type.is_dir() && !file_type.is_symlink() {
                    next.push(entry.path());
                }
            }
        }
        current = next;
    }
    current
}

fn read_visual_session(directory: &Path, now: DateTime<Utc>) -> Option<VisualSessionCandidate> {
    let host: VisualHostMetadata = read_bounded_json(directory.join("host.json").as_path())?;
    let session: VisualSessionMetadata =
        read_bounded_json(directory.join("session.json").as_path())?;
    if host.protocol_version != 1
        || session.protocol_version != 1
        || session.status != "running"
        || !safe_label(host.adapter_session_id.as_str(), 256)
        || !safe_label(host.plugin_id.as_str(), 256)
        || !safe_label(host.component_key.as_str(), 256)
        || !safe_label(session.session_id.as_str(), 256)
        || !safe_label(session.title.as_str(), 120)
        || session
            .target_app
            .as_deref()
            .is_some_and(|value| !safe_label(value, 120))
    {
        return None;
    }
    let captured_at = DateTime::parse_from_rfc3339(session.captured_at.as_str())
        .ok()?
        .with_timezone(&Utc);
    let age = now.signed_duration_since(captured_at);
    if age > Duration::seconds(ACTIVE_FRAME_MAX_AGE_SECONDS) || age < Duration::seconds(-5) {
        return None;
    }

    let frame_data_url = read_frame_data_url(directory, &session);
    Some(VisualSessionCandidate {
        captured_at,
        body: json!({
            "session_id": session.session_id,
            "adapter_session_id": host.adapter_session_id,
            "plugin_id": host.plugin_id,
            "component_key": host.component_key,
            "title": session.title,
            "target_app": session.target_app,
            "status": session.status,
            "mime_type": session.mime_type,
            "frame_sequence": session.frame_sequence,
            "captured_at": session.captured_at,
            "width": session.width,
            "height": session.height,
            "frame_data_url": frame_data_url,
        }),
    })
}

fn read_bounded_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Option<T> {
    let metadata = fs::symlink_metadata(path).ok()?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() > MAX_METADATA_BYTES
    {
        return None;
    }
    serde_json::from_slice(fs::read(path).ok()?.as_slice()).ok()
}

fn read_frame_data_url(directory: &Path, session: &VisualSessionMetadata) -> Option<String> {
    let mime_type = session.mime_type.as_deref()?;
    let expected_file = match mime_type {
        "image/jpeg" => "frame.jpg",
        "image/png" => "frame.png",
        _ => return None,
    };
    if session.frame_file.as_deref()? != expected_file {
        return None;
    }
    let path = directory.join(expected_file);
    let metadata = fs::symlink_metadata(path.as_path()).ok()?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > MAX_FRAME_BYTES
    {
        return None;
    }
    let encoded = BASE64_STANDARD.encode(fs::read(path).ok()?);
    Some(format!("data:{mime_type};base64,{encoded}"))
}

fn safe_label(value: &str, max_bytes: usize) -> bool {
    !value.trim().is_empty() && value.len() <= max_bytes && !value.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_stale_visual_sessions() {
        let temp = tempfile::tempdir().expect("temporary directory");
        let directory = temp.path().join("a/b/c");
        fs::create_dir_all(directory.as_path()).expect("create visual directory");
        fs::write(
            directory.join("host.json"),
            br#"{"protocol_version":1,"adapter_session_id":"adapter","plugin_id":"computer-use","component_key":"computer-use"}"#,
        )
        .expect("write host metadata");
        fs::write(
            directory.join("session.json"),
            br#"{"protocol_version":1,"session_id":"session","status":"running","title":"Computer Use","captured_at":"2026-01-01T00:00:00Z","frame_sequence":1}"#,
        )
        .expect("write session metadata");

        assert!(newest_visual_session(temp.path()).is_none());
    }

    #[test]
    fn returns_the_newest_active_frame_as_a_local_data_url() {
        let temp = tempfile::tempdir().expect("temporary directory");
        let directory = temp.path().join("a/b/c");
        fs::create_dir_all(directory.as_path()).expect("create visual directory");
        fs::write(
            directory.join("host.json"),
            br#"{"protocol_version":1,"adapter_session_id":"adapter","plugin_id":"computer-use","component_key":"computer-use"}"#,
        )
        .expect("write host metadata");
        let captured_at = Utc::now().to_rfc3339();
        fs::write(
            directory.join("session.json"),
            serde_json::to_vec(&json!({
                "protocol_version": 1,
                "session_id": "session",
                "status": "running",
                "title": "Open Computer Use",
                "target_app": "Notes",
                "mime_type": "image/jpeg",
                "frame_file": "frame.jpg",
                "frame_sequence": 7,
                "captured_at": captured_at,
                "width": 960,
                "height": 600,
            }))
            .expect("encode session metadata"),
        )
        .expect("write session metadata");
        fs::write(directory.join("frame.jpg"), b"jpeg").expect("write frame");

        let session = newest_visual_session(temp.path()).expect("active visual session");
        assert_eq!(session.body["plugin_id"], "computer-use");
        assert_eq!(session.body["frame_sequence"], 7);
        assert_eq!(
            session.body["frame_data_url"],
            "data:image/jpeg;base64,anBlZw=="
        );
    }
}
