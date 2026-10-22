use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::{
    fs::{self, File},
    io::Read,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};
use tauri::{http, AppHandle, LogicalSize, Manager, State, WebviewUrl, WebviewWindowBuilder};
use uuid::Uuid;

use crate::{
    general_settings::GeneralSettingsStore, global_utilities::GlobalUtilityPreferencesStore,
};

#[cfg(any(target_os = "windows", test))]
use chrono::Utc;
#[cfg(any(target_os = "windows", test))]
use sha2::{Digest, Sha256};
#[cfg(any(target_os = "windows", test))]
use std::{fs::OpenOptions, io::Write, path::Path};

const WINDOW_LABEL: &str = "clipboard-history";
const MAXIMUM_PAYLOAD_BYTES: u64 = 25 * 1_024 * 1_024;
#[cfg(target_os = "windows")]
const MAXIMUM_PREVIEW_CHARACTERS: usize = 360;
#[cfg(any(target_os = "windows", test))]
const MAXIMUM_INSPECTED_CHARACTERS: usize = 128 * 1_024;
const MAXIMUM_ENTRIES: usize = 500;
const MAXIMUM_SOURCE_PIXELS: u64 = 64_000_000;
#[cfg(any(target_os = "windows", test))]
const THIRTY_DAYS_MILLISECONDS: i64 = 30 * 86_400 * 1_000;

#[derive(Clone, Default)]
pub(crate) struct ClipboardHistoryManager {
    inner: Arc<Mutex<ClipboardHistoryInner>>,
    ignore_sequence: Arc<AtomicU32>,
}

#[derive(Default)]
struct ClipboardHistoryInner {
    store: Option<ClipboardHistoryStore>,
    monitor: Option<ClipboardMonitor>,
}

struct ClipboardMonitor {
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}

#[derive(Clone)]
struct ClipboardHistoryStore {
    root: PathBuf,
    connection: Arc<Mutex<Connection>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ClipboardKind {
    Text,
    Url,
    Files,
    Image,
}

impl ClipboardKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Url => "url",
            Self::Files => "files",
            Self::Image => "image",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "text" => Some(Self::Text),
            "url" => Some(Self::Url),
            "files" => Some(Self::Files),
            "image" => Some(Self::Image),
            _ => None,
        }
    }
}

#[cfg(any(target_os = "windows", test))]
struct CapturedClipboard {
    kind: ClipboardKind,
    bytes: Vec<u8>,
    preview: Option<String>,
    extension: &'static str,
}

#[derive(Clone, Debug)]
struct ClipboardEntry {
    id: Uuid,
    kind: ClipboardKind,
    created_at_unix_ms: i64,
    updated_at_unix_ms: i64,
    #[allow(dead_code)]
    content_hash: String,
    preview: Option<String>,
    payload_reference: String,
    byte_count: u64,
    is_pinned: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClipboardEntryProjection {
    id: String,
    kind: &'static str,
    created_at_unix_ms: i64,
    updated_at_unix_ms: i64,
    preview: Option<String>,
    byte_count: u64,
    is_pinned: bool,
    preview_url: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClipboardHistoryProjection {
    language: String,
    entries: Vec<ClipboardEntryProjection>,
}

impl ClipboardHistoryManager {
    pub(crate) fn open(&self, root: PathBuf) -> Result<(), String> {
        let store = ClipboardHistoryStore::open(root)?;
        let mut inner = self.inner.lock().map_err(|_| clipboard_error())?;
        inner.store = Some(store);
        Ok(())
    }

    pub(crate) fn set_monitoring(&self, enabled: bool) -> Result<(), String> {
        let mut inner = self.inner.lock().map_err(|_| clipboard_error())?;
        if !enabled {
            if let Some(mut monitor) = inner.monitor.take() {
                monitor.stop.store(true, Ordering::Release);
                let _ = monitor.thread.take();
            }
            return Ok(());
        }
        if inner.monitor.is_some() {
            return Ok(());
        }
        let store = inner.store.clone().ok_or_else(clipboard_error)?;
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = stop.clone();
        let ignore_sequence = self.ignore_sequence.clone();
        let worker = thread::Builder::new()
            .name("chatos-clipboard-monitor".into())
            .spawn(move || monitor_clipboard(store, thread_stop, ignore_sequence))
            .map_err(|_| clipboard_error())?;
        inner.monitor = Some(ClipboardMonitor {
            stop,
            thread: Some(worker),
        });
        Ok(())
    }

    fn store(&self) -> Result<ClipboardHistoryStore, String> {
        self.inner
            .lock()
            .map_err(|_| clipboard_error())?
            .store
            .clone()
            .ok_or_else(clipboard_error)
    }

    fn list(&self, language: String) -> Result<ClipboardHistoryProjection, String> {
        let entries = self.store()?.entries(MAXIMUM_ENTRIES)?;
        Ok(ClipboardHistoryProjection {
            language,
            entries: entries.into_iter().map(project_entry).collect(),
        })
    }

    fn restore(&self, id: Uuid) -> Result<(), String> {
        let store = self.store()?;
        let entry = store.entry(id)?.ok_or_else(clipboard_error)?;
        let bytes = store.payload(&entry)?;
        restore_payload(entry.kind, &bytes)?;
        #[cfg(target_os = "windows")]
        self.ignore_sequence
            .store(clipboard_sequence(), Ordering::Release);
        Ok(())
    }

    fn protocol_response(&self, request: http::Request<Vec<u8>>) -> http::Response<Vec<u8>> {
        let response = (|| {
            if request.method() != http::Method::GET {
                return None;
            }
            let id = request
                .uri()
                .path()
                .trim_matches('/')
                .parse::<Uuid>()
                .ok()?;
            let store = self.store().ok()?;
            let entry = store.entry(id).ok()??;
            if entry.kind != ClipboardKind::Image {
                return None;
            }
            let bytes = store.payload(&entry).ok()?;
            let format = image::guess_format(&bytes).ok()?;
            if !matches!(
                format,
                image::ImageFormat::Png | image::ImageFormat::Jpeg | image::ImageFormat::WebP
            ) {
                return None;
            }
            let dimensions = image::ImageReader::new(std::io::Cursor::new(&bytes))
                .with_guessed_format()
                .ok()?
                .into_dimensions()
                .ok()?;
            if u64::from(dimensions.0) * u64::from(dimensions.1) > MAXIMUM_SOURCE_PIXELS {
                return None;
            }
            let content_type = match format {
                image::ImageFormat::Png => "image/png",
                image::ImageFormat::Jpeg => "image/jpeg",
                image::ImageFormat::WebP => "image/webp",
                _ => return None,
            };
            Some((bytes, content_type))
        })();
        match response {
            Some((bytes, content_type)) => http::Response::builder()
                .status(http::StatusCode::OK)
                .header(http::header::CONTENT_TYPE, content_type)
                .header("X-Content-Type-Options", "nosniff")
                .header("Cache-Control", "no-store")
                .body(bytes)
                .expect("valid clipboard response"),
            None => http::Response::builder()
                .status(http::StatusCode::NOT_FOUND)
                .header(http::header::CONTENT_TYPE, "text/plain; charset=utf-8")
                .header("Cache-Control", "no-store")
                .body(b"Clipboard preview unavailable".to_vec())
                .expect("valid clipboard error response"),
        }
    }
}

impl Drop for ClipboardMonitor {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
    }
}

impl ClipboardHistoryStore {
    fn open(root: PathBuf) -> Result<Self, String> {
        let payloads = root.join("Payloads");
        fs::create_dir_all(&payloads).map_err(|_| clipboard_error())?;
        let connection =
            Connection::open(root.join("clipboard.sqlite3")).map_err(|_| clipboard_error())?;
        connection
            .execute_batch(
                "PRAGMA journal_mode=WAL;
                 PRAGMA synchronous=NORMAL;
                 CREATE TABLE IF NOT EXISTS clipboard_entries (
                   id TEXT PRIMARY KEY,
                   kind TEXT NOT NULL,
                   created_at_unix_ms INTEGER NOT NULL,
                   updated_at_unix_ms INTEGER NOT NULL,
                   content_hash TEXT NOT NULL UNIQUE,
                   preview TEXT,
                   payload_reference TEXT NOT NULL,
                   byte_count INTEGER NOT NULL,
                   is_pinned INTEGER NOT NULL DEFAULT 0
                 );
                 CREATE INDEX IF NOT EXISTS clipboard_entries_updated
                   ON clipboard_entries(is_pinned DESC, updated_at_unix_ms DESC);",
            )
            .map_err(|_| clipboard_error())?;
        Ok(Self {
            root,
            connection: Arc::new(Mutex::new(connection)),
        })
    }

    #[cfg(any(target_os = "windows", test))]
    fn add(&self, captured: CapturedClipboard) -> Result<ClipboardEntry, String> {
        if captured.bytes.is_empty() || captured.bytes.len() as u64 > MAXIMUM_PAYLOAD_BYTES {
            return Err(clipboard_error());
        }
        let mut hasher = Sha256::new();
        hasher.update(captured.kind.as_str().as_bytes());
        hasher.update([0]);
        hasher.update(&captured.bytes);
        let hash = format!("{:x}", hasher.finalize());
        let now = Utc::now().timestamp_millis();
        if let Some(mut existing) = self.entry_by_hash(&hash)? {
            self.connection
                .lock()
                .map_err(|_| clipboard_error())?
                .execute(
                    "UPDATE clipboard_entries SET updated_at_unix_ms = ?1 WHERE content_hash = ?2",
                    params![now, hash],
                )
                .map_err(|_| clipboard_error())?;
            existing.updated_at_unix_ms = now;
            return Ok(existing);
        }

        let id = Uuid::new_v4();
        let reference = format!("Payloads/{id}.{}", captured.extension);
        let path = self.root.join(&reference);
        write_private_atomic(&path, &captured.bytes)?;
        let result = self
            .connection
            .lock()
            .map_err(|_| clipboard_error())?
            .execute(
                "INSERT INTO clipboard_entries
                 (id,kind,created_at_unix_ms,updated_at_unix_ms,content_hash,preview,payload_reference,byte_count,is_pinned)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,0)",
                params![
                    id.to_string(),
                    captured.kind.as_str(),
                    now,
                    now,
                    hash,
                    captured.preview,
                    reference,
                    captured.bytes.len() as i64,
                ],
            );
        if result.is_err() {
            let _ = fs::remove_file(&path);
            return Err(clipboard_error());
        }
        self.cleanup()?;
        self.entry(id)?.ok_or_else(clipboard_error)
    }

    fn entries(&self, limit: usize) -> Result<Vec<ClipboardEntry>, String> {
        let connection = self.connection.lock().map_err(|_| clipboard_error())?;
        let mut statement = connection
            .prepare(
                "SELECT id,kind,created_at_unix_ms,updated_at_unix_ms,content_hash,preview,payload_reference,byte_count,is_pinned
                 FROM clipboard_entries ORDER BY is_pinned DESC, updated_at_unix_ms DESC LIMIT ?1",
            )
            .map_err(|_| clipboard_error())?;
        let rows = statement
            .query_map([limit.min(2_000) as i64], decode_entry)
            .map_err(|_| clipboard_error())?;
        let mut entries = Vec::new();
        for entry in rows.flatten() {
            entries.push(entry);
        }
        Ok(entries)
    }

    fn entry(&self, id: Uuid) -> Result<Option<ClipboardEntry>, String> {
        self.single_entry("id", &id.to_string())
    }

    #[cfg(any(target_os = "windows", test))]
    fn entry_by_hash(&self, hash: &str) -> Result<Option<ClipboardEntry>, String> {
        self.single_entry("content_hash", hash)
    }

    fn single_entry(&self, column: &str, value: &str) -> Result<Option<ClipboardEntry>, String> {
        let sql = format!(
            "SELECT id,kind,created_at_unix_ms,updated_at_unix_ms,content_hash,preview,payload_reference,byte_count,is_pinned
             FROM clipboard_entries WHERE {column} = ?1 LIMIT 1"
        );
        self.connection
            .lock()
            .map_err(|_| clipboard_error())?
            .query_row(&sql, [value], decode_entry)
            .optional()
            .map_err(|_| clipboard_error())
    }

    fn set_pinned(&self, id: Uuid, pinned: bool) -> Result<(), String> {
        let changed = self
            .connection
            .lock()
            .map_err(|_| clipboard_error())?
            .execute(
                "UPDATE clipboard_entries SET is_pinned = ?1 WHERE id = ?2",
                params![pinned, id.to_string()],
            )
            .map_err(|_| clipboard_error())?;
        if changed == 1 {
            Ok(())
        } else {
            Err(clipboard_error())
        }
    }

    fn delete(&self, id: Uuid) -> Result<(), String> {
        let Some(entry) = self.entry(id)? else {
            return Ok(());
        };
        self.connection
            .lock()
            .map_err(|_| clipboard_error())?
            .execute(
                "DELETE FROM clipboard_entries WHERE id = ?1",
                [id.to_string()],
            )
            .map_err(|_| clipboard_error())?;
        let _ = fs::remove_file(self.root.join(entry.payload_reference));
        Ok(())
    }

    fn clear(&self) -> Result<(), String> {
        let entries = self.entries(2_000)?;
        self.connection
            .lock()
            .map_err(|_| clipboard_error())?
            .execute("DELETE FROM clipboard_entries", [])
            .map_err(|_| clipboard_error())?;
        for entry in entries {
            let _ = fs::remove_file(self.root.join(entry.payload_reference));
        }
        Ok(())
    }

    fn payload(&self, entry: &ClipboardEntry) -> Result<Vec<u8>, String> {
        if entry.byte_count == 0 || entry.byte_count > MAXIMUM_PAYLOAD_BYTES {
            return Err(clipboard_error());
        }
        let path = self.root.join(&entry.payload_reference);
        if path.parent() != Some(self.root.join("Payloads").as_path()) {
            return Err(clipboard_error());
        }
        let file = File::open(path).map_err(|_| clipboard_error())?;
        let metadata = file.metadata().map_err(|_| clipboard_error())?;
        if !metadata.is_file() || metadata.len() != entry.byte_count {
            return Err(clipboard_error());
        }
        let mut bytes = Vec::with_capacity(entry.byte_count as usize);
        file.take(MAXIMUM_PAYLOAD_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| clipboard_error())?;
        if bytes.len() as u64 != entry.byte_count {
            return Err(clipboard_error());
        }
        Ok(bytes)
    }

    #[cfg(any(target_os = "windows", test))]
    fn cleanup(&self) -> Result<(), String> {
        let cutoff = Utc::now().timestamp_millis() - THIRTY_DAYS_MILLISECONDS;
        let mut unpinned = 0;
        for entry in self.entries(2_000)? {
            if entry.is_pinned {
                continue;
            }
            unpinned += 1;
            if unpinned > MAXIMUM_ENTRIES || entry.updated_at_unix_ms < cutoff {
                self.delete(entry.id)?;
            }
        }
        Ok(())
    }
}

#[tauri::command]
pub(crate) fn list_clipboard_history(
    manager: State<'_, ClipboardHistoryManager>,
    utilities: State<'_, GlobalUtilityPreferencesStore>,
    general: State<'_, GeneralSettingsStore>,
) -> Result<ClipboardHistoryProjection, String> {
    manager.list(active_language(&utilities, &general)?)
}

#[tauri::command]
pub(crate) fn set_clipboard_history_entry_pinned(
    id: String,
    pinned: bool,
    manager: State<'_, ClipboardHistoryManager>,
    utilities: State<'_, GlobalUtilityPreferencesStore>,
    general: State<'_, GeneralSettingsStore>,
) -> Result<ClipboardHistoryProjection, String> {
    let language = active_language(&utilities, &general)?;
    manager.store()?.set_pinned(parse_id(&id)?, pinned)?;
    manager.list(language)
}

#[tauri::command]
pub(crate) fn delete_clipboard_history_entry(
    id: String,
    manager: State<'_, ClipboardHistoryManager>,
    utilities: State<'_, GlobalUtilityPreferencesStore>,
    general: State<'_, GeneralSettingsStore>,
) -> Result<ClipboardHistoryProjection, String> {
    let language = active_language(&utilities, &general)?;
    manager.store()?.delete(parse_id(&id)?)?;
    manager.list(language)
}

#[tauri::command]
pub(crate) fn clear_clipboard_history(
    manager: State<'_, ClipboardHistoryManager>,
    utilities: State<'_, GlobalUtilityPreferencesStore>,
    general: State<'_, GeneralSettingsStore>,
) -> Result<ClipboardHistoryProjection, String> {
    let language = active_language(&utilities, &general)?;
    manager.store()?.clear()?;
    manager.list(language)
}

#[tauri::command]
pub(crate) fn restore_clipboard_history_entry(
    id: String,
    manager: State<'_, ClipboardHistoryManager>,
    utilities: State<'_, GlobalUtilityPreferencesStore>,
) -> Result<(), String> {
    utilities.active_owner()?;
    manager.restore(parse_id(&id)?)
}

#[tauri::command]
pub(crate) fn close_clipboard_history(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(WINDOW_LABEL) {
        window.hide().map_err(|_| clipboard_error())?;
    }
    Ok(())
}

pub(crate) fn show_clipboard_history(app: &AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(WINDOW_LABEL) {
        window.show().map_err(|_| clipboard_error())?;
        window.set_focus().map_err(|_| clipboard_error())?;
        return Ok(());
    }
    let window = WebviewWindowBuilder::new(
        app,
        WINDOW_LABEL,
        WebviewUrl::App("index.html?surface=clipboard-history".into()),
    )
    .title("ChatOS Clipboard History")
    .inner_size(700.0, 520.0)
    .min_inner_size(560.0, 340.0)
    .decorations(false)
    .transparent(true)
    .always_on_top(true)
    .skip_taskbar(true)
    .resizable(true)
    .center()
    .build()
    .map_err(|_| clipboard_error())?;
    window
        .set_min_size(Some(LogicalSize::new(560.0, 340.0)))
        .map_err(|_| clipboard_error())?;
    Ok(())
}

pub(crate) fn clipboard_history_protocol(
    manager: ClipboardHistoryManager,
    request: http::Request<Vec<u8>>,
) -> http::Response<Vec<u8>> {
    manager.protocol_response(request)
}

fn project_entry(entry: ClipboardEntry) -> ClipboardEntryProjection {
    ClipboardEntryProjection {
        id: entry.id.to_string(),
        kind: entry.kind.as_str(),
        created_at_unix_ms: entry.created_at_unix_ms,
        updated_at_unix_ms: entry.updated_at_unix_ms,
        preview: entry.preview,
        byte_count: entry.byte_count,
        is_pinned: entry.is_pinned,
        preview_url: (entry.kind == ClipboardKind::Image).then(|| {
            if cfg!(target_os = "windows") {
                format!("http://chatos-clipboard.localhost/{}", entry.id)
            } else {
                format!("chatos-clipboard://localhost/{}", entry.id)
            }
        }),
    }
}

fn decode_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<ClipboardEntry> {
    let id: String = row.get(0)?;
    let kind: String = row.get(1)?;
    let byte_count: i64 = row.get(7)?;
    let parsed_id = Uuid::parse_str(&id).map_err(|_| rusqlite::Error::InvalidQuery)?;
    let parsed_kind = ClipboardKind::parse(&kind).ok_or(rusqlite::Error::InvalidQuery)?;
    if !(1..=MAXIMUM_PAYLOAD_BYTES as i64).contains(&byte_count) {
        return Err(rusqlite::Error::InvalidQuery);
    }
    Ok(ClipboardEntry {
        id: parsed_id,
        kind: parsed_kind,
        created_at_unix_ms: row.get(2)?,
        updated_at_unix_ms: row.get(3)?,
        content_hash: row.get(4)?,
        preview: row.get(5)?,
        payload_reference: row.get(6)?,
        byte_count: byte_count as u64,
        is_pinned: row.get(8)?,
    })
}

fn parse_id(value: &str) -> Result<Uuid, String> {
    Uuid::parse_str(value).map_err(|_| "剪贴板记录无效。".into())
}

fn active_language(
    utilities: &GlobalUtilityPreferencesStore,
    general: &GeneralSettingsStore,
) -> Result<String, String> {
    general.interface_language(&utilities.active_owner()?)
}

#[cfg(any(target_os = "windows", test))]
fn should_ignore_sensitive_text(value: &str) -> bool {
    let sample = value
        .chars()
        .take(MAXIMUM_INSPECTED_CHARACTERS)
        .collect::<String>()
        .to_lowercase();
    let private_headers = [
        "-----begin private key-----",
        "-----begin rsa private key-----",
        "-----begin ec private key-----",
        "-----begin openssh private key-----",
    ];
    if private_headers.iter().any(|header| sample.contains(header)) {
        return true;
    }
    let warning = sample.contains("\"warning_banner\"")
        && sample.contains("do not share")
        && (sample.contains("\"accesstoken\"") || sample.contains("\"access_token\""));
    warning
        || (sample.contains("\"accesstoken\"") && sample.contains("\"refreshtoken\""))
        || (sample.contains("\"access_token\"") && sample.contains("\"refresh_token\""))
}

#[cfg(target_os = "windows")]
fn monitor_clipboard(
    store: ClipboardHistoryStore,
    stop: Arc<AtomicBool>,
    ignore_sequence: Arc<AtomicU32>,
) {
    let mut last_sequence = clipboard_sequence();
    let mut idle_count = 0_u8;
    while !stop.load(Ordering::Acquire) {
        let milliseconds = match idle_count {
            0..=1 => 300,
            2..=4 => 600,
            5..=9 => 1_000,
            _ => 1_500,
        };
        thread::sleep(Duration::from_millis(milliseconds));
        let sequence = clipboard_sequence();
        if sequence == last_sequence {
            idle_count = idle_count.saturating_add(1).min(32);
            continue;
        }
        last_sequence = sequence;
        idle_count = 0;
        if sequence == ignore_sequence.load(Ordering::Acquire) || contains_sensitive_format() {
            continue;
        }
        if let Some(captured) = capture_windows_clipboard() {
            let _ = store.add(captured);
        }
    }
}

#[cfg(target_os = "windows")]
fn clipboard_sequence() -> u32 {
    clipboard_win::seq_num()
        .map(std::num::NonZeroU32::get)
        .unwrap_or(0)
}

#[cfg(not(target_os = "windows"))]
fn monitor_clipboard(
    _store: ClipboardHistoryStore,
    stop: Arc<AtomicBool>,
    _ignore_sequence: Arc<AtomicU32>,
) {
    while !stop.load(Ordering::Acquire) {
        thread::sleep(Duration::from_secs(1));
    }
}

#[cfg(target_os = "windows")]
fn contains_sensitive_format() -> bool {
    use clipboard_win::{raw, Clipboard, EnumFormats};
    let Ok(_clipboard) = Clipboard::new_attempts(10) else {
        return true;
    };
    let blocked = [
        "concealed",
        "transient",
        "1password",
        "lastpass",
        "bitwarden",
        "keepass",
    ];
    EnumFormats::new().any(|format| {
        raw::format_name_big(format)
            .map(|name| {
                let lower = name.to_lowercase();
                blocked.iter().any(|fragment| lower.contains(fragment))
            })
            .unwrap_or(false)
    })
}

#[cfg(target_os = "windows")]
fn capture_windows_clipboard() -> Option<CapturedClipboard> {
    use clipboard_win::formats::FileList;
    if let Ok(paths) = clipboard_win::get_clipboard::<Vec<PathBuf>, _>(FileList) {
        if !paths.is_empty() {
            let sorted = paths
                .into_iter()
                .map(|path| path.to_string_lossy().into_owned())
                .collect::<Vec<_>>();
            let bytes = serde_json::to_vec(&sorted).ok()?;
            if bytes.len() as u64 <= MAXIMUM_PAYLOAD_BYTES {
                let preview = sorted
                    .iter()
                    .take(3)
                    .filter_map(|path| Path::new(path).file_name()?.to_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                return Some(CapturedClipboard {
                    kind: ClipboardKind::Files,
                    bytes,
                    preview: Some(preview),
                    extension: "files",
                });
            }
        }
    }
    if let Ok(mut clipboard) = arboard::Clipboard::new() {
        if let Ok(image) = clipboard.get_image() {
            if u64::try_from(image.width).ok()? * u64::try_from(image.height).ok()?
                <= MAXIMUM_SOURCE_PIXELS
            {
                let bytes =
                    encode_rgba_png(image.width, image.height, image.bytes.as_ref()).ok()?;
                if bytes.len() as u64 <= MAXIMUM_PAYLOAD_BYTES {
                    return Some(CapturedClipboard {
                        kind: ClipboardKind::Image,
                        bytes,
                        preview: None,
                        extension: "png",
                    });
                }
            }
        }
        if let Ok(text) = clipboard.get_text() {
            if text.is_empty() || should_ignore_sensitive_text(&text) {
                return None;
            }
            let bytes = text.as_bytes().to_vec();
            if bytes.len() as u64 > MAXIMUM_PAYLOAD_BYTES {
                return None;
            }
            let is_url = text.starts_with("https://") || text.starts_with("http://");
            let preview = text
                .replace(['\r', '\n'], " ")
                .trim()
                .chars()
                .take(MAXIMUM_PREVIEW_CHARACTERS)
                .collect();
            return Some(CapturedClipboard {
                kind: if is_url {
                    ClipboardKind::Url
                } else {
                    ClipboardKind::Text
                },
                bytes,
                preview: Some(preview),
                extension: if is_url { "url" } else { "txt" },
            });
        }
    }
    None
}

#[cfg(target_os = "windows")]
fn restore_payload(kind: ClipboardKind, bytes: &[u8]) -> Result<(), String> {
    match kind {
        ClipboardKind::Text | ClipboardKind::Url => {
            let text = String::from_utf8(bytes.to_vec()).map_err(|_| clipboard_error())?;
            arboard::Clipboard::new()
                .and_then(|mut clipboard| clipboard.set_text(text))
                .map_err(|_| clipboard_error())
        }
        ClipboardKind::Files => {
            use clipboard_win::Setter;
            let paths: Vec<String> =
                serde_json::from_slice(bytes).map_err(|_| clipboard_error())?;
            let _clipboard =
                clipboard_win::Clipboard::new_attempts(10).map_err(|_| clipboard_error())?;
            clipboard_win::formats::FileList
                .write_clipboard(&paths)
                .map_err(|_| clipboard_error())
        }
        ClipboardKind::Image => {
            let image = image::load_from_memory(bytes)
                .map_err(|_| clipboard_error())?
                .into_rgba8();
            let width = image.width() as usize;
            let height = image.height() as usize;
            arboard::Clipboard::new()
                .and_then(|mut clipboard| {
                    clipboard.set_image(arboard::ImageData {
                        width,
                        height,
                        bytes: std::borrow::Cow::Owned(image.into_raw()),
                    })
                })
                .map_err(|_| clipboard_error())
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn restore_payload(_kind: ClipboardKind, _bytes: &[u8]) -> Result<(), String> {
    Err("剪贴板恢复仅在 Windows 客户端中可用。".into())
}

#[cfg(target_os = "windows")]
fn encode_rgba_png(width: usize, height: usize, rgba: &[u8]) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, width as u32, height as u32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .and_then(|mut writer| writer.write_image_data(rgba))
            .map_err(|_| clipboard_error())?;
    }
    Ok(bytes)
}

#[cfg(any(target_os = "windows", test))]
fn write_private_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path.parent().ok_or_else(clipboard_error)?;
    fs::create_dir_all(parent).map_err(|_| clipboard_error())?;
    let temporary = parent.join(format!(".clipboard-{}.tmp", Uuid::new_v4()));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let result = (|| {
        let mut file = options.open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
        return Err(clipboard_error());
    }
    Ok(())
}

fn clipboard_error() -> String {
    "无法读取或更新剪贴板历史。".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_deduplicates_pins_orders_and_removes_payloads() {
        let directory = tempfile::tempdir().unwrap();
        let store = ClipboardHistoryStore::open(directory.path().into()).unwrap();
        let first = store.add(text("first")).unwrap();
        let second = store.add(text("second")).unwrap();
        let repeated = store.add(text("first")).unwrap();
        assert_eq!(first.id, repeated.id);
        assert_eq!(store.entries(500).unwrap().len(), 2);
        store.set_pinned(second.id, true).unwrap();
        assert_eq!(store.entries(500).unwrap()[0].id, second.id);
        let payload = directory.path().join(&first.payload_reference);
        assert!(payload.is_file());
        store.delete(first.id).unwrap();
        assert!(!payload.exists());
    }

    #[test]
    fn safe_projection_never_contains_hash_or_payload_reference() {
        let directory = tempfile::tempdir().unwrap();
        let manager = ClipboardHistoryManager::default();
        manager.open(directory.path().into()).unwrap();
        let entry = manager
            .store()
            .unwrap()
            .add(text("hello secret path"))
            .unwrap();
        let projection = manager.list("zh-CN".into()).unwrap();
        let json = serde_json::to_string(&projection).unwrap();
        assert!(json.contains(&entry.id.to_string()));
        assert!(!json.contains(&entry.content_hash));
        assert!(!json.contains("Payloads/"));
    }

    #[test]
    fn sensitive_policy_matches_macos_filters() {
        assert!(should_ignore_sensitive_text(
            "-----BEGIN PRIVATE KEY-----\nabc"
        ));
        assert!(should_ignore_sensitive_text(
            r#"{"access_token":"one","refresh_token":"two"}"#
        ));
        assert!(!should_ignore_sensitive_text("ordinary project notes"));
    }

    fn text(value: &str) -> CapturedClipboard {
        CapturedClipboard {
            kind: ClipboardKind::Text,
            bytes: value.as_bytes().to_vec(),
            preview: Some(value.into()),
            extension: "txt",
        }
    }
}
