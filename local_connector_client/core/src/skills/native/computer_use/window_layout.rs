// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::action::is_unsafe_typed_character;
use super::dispatch::preflight_window_layout_snapshot;
use super::display::{active_display_layout_guard, current_platform_name, ApprovedDisplayGuard};
use super::{
    reject_unknown_fields, safe_approval_label, MAX_ACTIVE_DISPLAYS, MAX_WINDOW_LAYOUT_SNAPSHOTS,
    MAX_WINDOW_LAYOUT_WINDOWS, MIN_WINDOW_DIMENSION, WINDOW_LAYOUT_SCHEMA_VERSION,
    WINDOW_LAYOUT_SNAPSHOT_TTL,
};

#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ApprovedWindowLayoutGuard {
    pub(super) platform: String,
    pub(super) application: String,
    pub(super) process_identity: String,
    pub(super) pid: u32,
    pub(super) window_id: String,
    pub(super) position: [f64; 2],
    pub(super) size: [f64; 2],
}

impl ApprovedWindowLayoutGuard {
    pub(super) fn validate(&self) -> Result<()> {
        if !matches!(self.platform.as_str(), "macos" | "windows")
            || self.application.is_empty()
            || self.application.chars().count() > 240
            || self.application.chars().any(is_unsafe_typed_character)
            || self.process_identity.is_empty()
            || self.process_identity.chars().count() > 500
            || self.process_identity.chars().any(is_unsafe_typed_character)
            || self.pid == 0
            || self.window_id.is_empty()
            || self.window_id.chars().count() > 64
            || self.window_id.chars().any(is_unsafe_typed_character)
            || self
                .position
                .iter()
                .chain(self.size.iter())
                .any(|value| !value.is_finite())
            || self.size[0] < MIN_WINDOW_DIMENSION as f64
            || self.size[1] < MIN_WINDOW_DIMENSION as f64
        {
            return Err(anyhow!("window layout identity or geometry is invalid"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct WindowLayoutCapturePayload {
    pub(super) platform: String,
    #[serde(default)]
    pub(super) display_layout: Vec<ApprovedDisplayGuard>,
    pub(super) windows: Vec<ApprovedWindowLayoutGuard>,
    pub(super) excluded_window_count: usize,
    pub(super) truncated: bool,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct WindowLayoutSnapshot {
    pub(super) schema_version: u32,
    pub(super) snapshot_id: String,
    pub(super) snapshot_sha256: String,
    pub(super) platform: String,
    pub(super) display_layout: Vec<ApprovedDisplayGuard>,
    pub(super) windows: Vec<ApprovedWindowLayoutGuard>,
    pub(super) excluded_window_count: usize,
    pub(super) truncated: bool,
}

impl WindowLayoutSnapshot {
    pub(super) fn validate(&self) -> Result<()> {
        if self.schema_version != WINDOW_LAYOUT_SCHEMA_VERSION
            || !uuid::Uuid::parse_str(self.snapshot_id.as_str())
                .is_ok_and(|snapshot_id| snapshot_id.hyphenated().to_string() == self.snapshot_id)
            || self.snapshot_sha256.len() != 64
            || !self
                .snapshot_sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            || self.platform != current_platform_name()
            || self.display_layout.is_empty()
            || self.display_layout.len() > MAX_ACTIVE_DISPLAYS
            || self.windows.is_empty()
            || self.windows.len() > MAX_WINDOW_LAYOUT_WINDOWS
            || self
                .windows
                .iter()
                .any(|window| window.platform != self.platform || window.validate().is_err())
            || window_layout_sha256(self)? != self.snapshot_sha256
        {
            return Err(anyhow!("window layout snapshot is invalid"));
        }
        for window in &self.windows {
            validate_window_layout_geometry(window, &self.display_layout)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub(super) struct StoredWindowLayoutSnapshot {
    pub(super) captured_at: Instant,
    pub(super) snapshot: WindowLayoutSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct WindowLayoutReference {
    pub(super) snapshot_id: String,
    pub(super) snapshot_sha256: String,
}

pub(super) fn parse_window_layout_reference(arguments: &Value) -> Result<WindowLayoutReference> {
    reject_unknown_fields(arguments, &["snapshot_id", "snapshot_sha256"])?;
    let snapshot_id = arguments
        .get("snapshot_id")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("snapshot_id is required"))?;
    let parsed_id = uuid::Uuid::parse_str(snapshot_id)
        .map_err(|_| anyhow!("snapshot_id must be a canonical UUID"))?;
    if parsed_id.hyphenated().to_string() != snapshot_id {
        return Err(anyhow!("snapshot_id must be a canonical lowercase UUID"));
    }
    let snapshot_sha256 = arguments
        .get("snapshot_sha256")
        .and_then(Value::as_str)
        .filter(|value| {
            value.len() == 64
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        })
        .ok_or_else(|| anyhow!("snapshot_sha256 must be 64 lowercase hexadecimal characters"))?;
    Ok(WindowLayoutReference {
        snapshot_id: snapshot_id.to_string(),
        snapshot_sha256: snapshot_sha256.to_string(),
    })
}

pub(super) fn window_layout_sha256(snapshot: &WindowLayoutSnapshot) -> Result<String> {
    let payload = serde_json::to_vec(&(
        snapshot.schema_version,
        snapshot.snapshot_id.as_str(),
        snapshot.platform.as_str(),
        &snapshot.display_layout,
        &snapshot.windows,
        snapshot.excluded_window_count,
        snapshot.truncated,
    ))
    .context("encode window layout snapshot hash payload")?;
    Ok(hex::encode(Sha256::digest(payload)))
}

fn validate_window_layout_geometry(
    window: &ApprovedWindowLayoutGuard,
    display_layout: &[ApprovedDisplayGuard],
) -> Result<()> {
    let left = window.position[0];
    let top = window.position[1];
    let right = left + window.size[0];
    let bottom = top + window.size[1];
    if !display_layout.iter().any(|display| {
        let overlap_width =
            right.min(display.origin_x + display.width) - left.max(display.origin_x);
        let overlap_height =
            bottom.min(display.origin_y + display.height) - top.max(display.origin_y);
        overlap_width >= MIN_WINDOW_DIMENSION as f64
            && overlap_height >= MIN_WINDOW_DIMENSION as f64
    }) {
        return Err(anyhow!(
            "snapshotted window geometry must leave at least {MIN_WINDOW_DIMENSION} x {MIN_WINDOW_DIMENSION} desktop units visible"
        ));
    }
    Ok(())
}

fn window_layout_snapshot_store() -> &'static Mutex<BTreeMap<String, StoredWindowLayoutSnapshot>> {
    static STORE: OnceLock<Mutex<BTreeMap<String, StoredWindowLayoutSnapshot>>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(BTreeMap::new()))
}

pub(super) fn prune_expired_window_layout_snapshots(
    snapshots: &mut BTreeMap<String, StoredWindowLayoutSnapshot>,
    now: Instant,
) {
    snapshots.retain(|_, stored| {
        now.checked_duration_since(stored.captured_at)
            .is_some_and(|age| age <= WINDOW_LAYOUT_SNAPSHOT_TTL)
    });
}

pub(super) fn evict_window_layout_snapshot_for_insert(
    snapshots: &mut BTreeMap<String, StoredWindowLayoutSnapshot>,
) {
    while snapshots.len() >= MAX_WINDOW_LAYOUT_SNAPSHOTS {
        let Some(oldest) = snapshots
            .iter()
            .min_by_key(|(_, stored)| stored.captured_at)
            .map(|(snapshot_id, _)| snapshot_id.clone())
        else {
            break;
        };
        snapshots.remove(oldest.as_str());
    }
}

pub(super) fn store_window_layout_snapshot(snapshot: WindowLayoutSnapshot) -> Result<()> {
    snapshot.validate()?;
    let now = Instant::now();
    let mut snapshots = window_layout_snapshot_store()
        .lock()
        .map_err(|_| anyhow!("window layout snapshot store is unavailable"))?;
    prune_expired_window_layout_snapshots(&mut snapshots, now);
    evict_window_layout_snapshot_for_insert(&mut snapshots);
    snapshots.insert(
        snapshot.snapshot_id.clone(),
        StoredWindowLayoutSnapshot {
            captured_at: now,
            snapshot,
        },
    );
    Ok(())
}

pub(super) fn stored_window_layout_snapshot(
    reference: &WindowLayoutReference,
) -> Result<WindowLayoutSnapshot> {
    let now = Instant::now();
    let mut snapshots = window_layout_snapshot_store()
        .lock()
        .map_err(|_| anyhow!("window layout snapshot store is unavailable"))?;
    prune_expired_window_layout_snapshots(&mut snapshots, now);
    let snapshot = snapshots
        .get(reference.snapshot_id.as_str())
        .map(|stored| stored.snapshot.clone())
        .ok_or_else(|| anyhow!("window layout snapshot is missing or expired; capture it again"))?;
    if snapshot.snapshot_sha256 != reference.snapshot_sha256 {
        return Err(anyhow!("window layout snapshot SHA-256 does not match"));
    }
    snapshot.validate()?;
    Ok(snapshot)
}

pub(super) fn window_layout_approval_argument(snapshot: &WindowLayoutSnapshot) -> Result<String> {
    snapshot.validate()?;
    Ok(format!(
        "--window-layout-json={}",
        serde_json::to_string(snapshot)?
    ))
}

pub(super) fn approved_window_layout_snapshot(
    approved_command_args: Option<&[String]>,
) -> Result<WindowLayoutSnapshot> {
    let arguments = approved_command_args
        .ok_or_else(|| anyhow!("approved window layout snapshot is missing"))?;
    let encoded = arguments
        .iter()
        .find_map(|argument| argument.strip_prefix("--window-layout-json="))
        .ok_or_else(|| anyhow!("approved window layout snapshot is missing"))?;
    let snapshot = serde_json::from_str::<WindowLayoutSnapshot>(encoded)
        .context("decode approved window layout snapshot")?;
    snapshot.validate()?;
    Ok(snapshot)
}

pub(super) fn consume_approved_window_layout_snapshot(
    arguments: &Value,
    approved_command_args: Option<&[String]>,
) -> Result<()> {
    let reference = parse_window_layout_reference(arguments)?;
    let approved = approved_window_layout_snapshot(approved_command_args)?;
    if approved.snapshot_id != reference.snapshot_id
        || approved.snapshot_sha256 != reference.snapshot_sha256
    {
        return Err(anyhow!(
            "approved window layout snapshot does not match the requested opaque reference"
        ));
    }
    let now = Instant::now();
    let mut snapshots = window_layout_snapshot_store()
        .lock()
        .map_err(|_| anyhow!("window layout snapshot store is unavailable"))?;
    prune_expired_window_layout_snapshots(&mut snapshots, now);
    let stored = snapshots
        .get(reference.snapshot_id.as_str())
        .ok_or_else(|| anyhow!("window layout snapshot is missing or expired; capture it again"))?;
    if stored.snapshot != approved {
        return Err(anyhow!(
            "approved window layout snapshot no longer matches volatile Local Connector state"
        ));
    }
    snapshots.remove(reference.snapshot_id.as_str());
    Ok(())
}

pub(super) fn window_layout_application_summary(windows: &[ApprovedWindowLayoutGuard]) -> String {
    let mut counts = BTreeMap::<&str, usize>::new();
    for window in windows {
        *counts.entry(window.application.as_str()).or_default() += 1;
    }
    counts
        .into_iter()
        .map(|(application, count)| format!("{} ({count})", safe_approval_label(application)))
        .collect::<Vec<_>>()
        .join(", ")
}

pub(super) fn finalize_window_layout_capture(result: Value) -> Result<Value> {
    let payload = serde_json::from_value::<WindowLayoutCapturePayload>(result)
        .context("decode native window layout capture")?;
    if payload.platform != current_platform_name()
        || payload.windows.is_empty()
        || payload.windows.len() > MAX_WINDOW_LAYOUT_WINDOWS
        || payload
            .windows
            .iter()
            .any(|window| window.platform != payload.platform || window.validate().is_err())
    {
        return Err(anyhow!("native window layout capture is invalid"));
    }
    let mut identities = BTreeMap::<(&str, u32, &str), ()>::new();
    for window in &payload.windows {
        if identities
            .insert(
                (
                    window.process_identity.as_str(),
                    window.pid,
                    window.window_id.as_str(),
                ),
                (),
            )
            .is_some()
        {
            return Err(anyhow!(
                "native window layout capture contains duplicate identities"
            ));
        }
    }
    let display_layout = payload.display_layout;
    for window in &payload.windows {
        validate_window_layout_geometry(window, &display_layout)?;
    }
    let mut snapshot = WindowLayoutSnapshot {
        schema_version: WINDOW_LAYOUT_SCHEMA_VERSION,
        snapshot_id: uuid::Uuid::new_v4().hyphenated().to_string(),
        snapshot_sha256: String::new(),
        platform: payload.platform,
        display_layout,
        windows: payload.windows,
        excluded_window_count: payload.excluded_window_count,
        truncated: payload.truncated,
    };
    snapshot.snapshot_sha256 = window_layout_sha256(&snapshot)?;
    snapshot.validate()?;
    store_window_layout_snapshot(snapshot.clone())?;
    Ok(json!({
        "text": format!(
            "Captured a short-lived opaque layout snapshot for {} ordinary {} windows. The snapshot remains only in volatile Local Connector memory.",
            snapshot.windows.len(), snapshot.platform
        ),
        "_structured_result": {
            "success": true,
            "mode": "read_only",
            "capture_scope": "ordinary_window_layout",
            "platform": snapshot.platform,
            "snapshot_id": snapshot.snapshot_id,
            "snapshot_sha256": snapshot.snapshot_sha256,
            "window_count": snapshot.windows.len(),
            "applications": window_layout_application_summary(&snapshot.windows),
            "excluded_window_count": snapshot.excluded_window_count,
            "truncated": snapshot.truncated,
            "maximum_windows": MAX_WINDOW_LAYOUT_WINDOWS,
            "expires_in_seconds": WINDOW_LAYOUT_SNAPSHOT_TTL.as_secs(),
            "persisted": false,
            "ordinary_visible_normal_windows_only": true,
            "model_supplied_window_identities_or_coordinates": false,
        }
    }))
}

pub(super) fn validate_window_layout_snapshot_for_approval(
    snapshot: &WindowLayoutSnapshot,
) -> Result<()> {
    snapshot.validate()?;
    if active_display_layout_guard()? != snapshot.display_layout {
        return Err(anyhow!(
            "active display identity or geometry changed after layout capture; capture a new snapshot"
        ));
    }
    preflight_window_layout_snapshot(snapshot)
}

pub(super) fn validate_approved_window_layout_snapshot(
    snapshot: &WindowLayoutSnapshot,
) -> Result<()> {
    snapshot.validate()?;
    if active_display_layout_guard()? != snapshot.display_layout {
        return Err(anyhow!(
            "active display identity or geometry changed after layout restore approval; capture and approve again"
        ));
    }
    Ok(())
}
