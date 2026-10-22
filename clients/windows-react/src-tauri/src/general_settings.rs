use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use tauri::State;
use uuid::Uuid;

use crate::{authentication::AuthenticationManager, host_lifecycle::validate_owner};

const FILE_VERSION: u32 = 1;
const MAXIMUM_FILE_BYTES: u64 = 2 * 1_024 * 1_024;

#[derive(Clone)]
pub(crate) struct GeneralSettingsStore {
    path: PathBuf,
    lock: Arc<Mutex<()>>,
    power: IdleSleepController,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct GeneralSettingsSnapshot {
    interface_language: String,
    context_language: String,
    interface_font_size: u8,
    prevents_idle_system_sleep: bool,
    revision: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct UpdateGeneralSettingsInput {
    owner_user_id: String,
    interface_language: String,
    context_language: String,
    interface_font_size: u8,
    prevents_idle_system_sleep: bool,
    expected_revision: u64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GeneralSettingsFile {
    version: u32,
    owners: BTreeMap<String, GeneralSettingsSnapshot>,
}

impl Default for GeneralSettingsFile {
    fn default() -> Self {
        Self {
            version: FILE_VERSION,
            owners: BTreeMap::new(),
        }
    }
}

impl Default for GeneralSettingsSnapshot {
    fn default() -> Self {
        Self {
            interface_language: "zh-CN".into(),
            context_language: "zh-CN".into(),
            interface_font_size: 14,
            prevents_idle_system_sleep: false,
            revision: 1,
        }
    }
}

impl GeneralSettingsStore {
    pub(crate) fn new(app_data_directory: PathBuf) -> Self {
        Self {
            path: app_data_directory
                .join("preferences")
                .join("general-settings.json"),
            lock: Arc::new(Mutex::new(())),
            power: IdleSleepController::new(),
        }
    }

    fn snapshot(&self, owner_user_id: &str) -> Result<GeneralSettingsSnapshot, String> {
        let _guard = self.lock.lock().map_err(|_| settings_error())?;
        let file = self.load()?;
        let snapshot = file.owners.get(owner_user_id).cloned().unwrap_or_default();
        validate_snapshot(&snapshot)?;
        self.power.set(snapshot.prevents_idle_system_sleep)?;
        Ok(snapshot)
    }

    pub(crate) fn context_language(&self, owner_user_id: &str) -> Result<String, String> {
        let _guard = self.lock.lock().map_err(|_| settings_error())?;
        let file = self.load()?;
        let snapshot = file.owners.get(owner_user_id).cloned().unwrap_or_default();
        validate_snapshot(&snapshot)?;
        Ok(snapshot.context_language)
    }

    pub(crate) fn interface_language(&self, owner_user_id: &str) -> Result<String, String> {
        let _guard = self.lock.lock().map_err(|_| settings_error())?;
        let file = self.load()?;
        let snapshot = file.owners.get(owner_user_id).cloned().unwrap_or_default();
        validate_snapshot(&snapshot)?;
        Ok(snapshot.interface_language)
    }

    fn update(&self, input: UpdateGeneralSettingsInput) -> Result<GeneralSettingsSnapshot, String> {
        let _guard = self.lock.lock().map_err(|_| settings_error())?;
        let mut file = self.load()?;
        let current = file
            .owners
            .get(&input.owner_user_id)
            .cloned()
            .unwrap_or_default();
        if current.revision != input.expected_revision {
            return Err("常规设置已经变化，请刷新后重试。".into());
        }
        let next = GeneralSettingsSnapshot {
            interface_language: input.interface_language,
            context_language: input.context_language,
            interface_font_size: input.interface_font_size,
            prevents_idle_system_sleep: input.prevents_idle_system_sleep,
            revision: current.revision.checked_add(1).ok_or_else(settings_error)?,
        };
        validate_snapshot(&next)?;
        file.owners.insert(input.owner_user_id, next.clone());
        self.save(&file)?;
        self.power.set(next.prevents_idle_system_sleep)?;
        Ok(next)
    }

    fn load(&self) -> Result<GeneralSettingsFile, String> {
        let file = match File::open(&self.path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(GeneralSettingsFile::default())
            }
            Err(_) => return Err(settings_error()),
        };
        let metadata = file.metadata().map_err(|_| settings_error())?;
        if !metadata.is_file() || metadata.len() > MAXIMUM_FILE_BYTES {
            return Err(settings_error());
        }
        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        file.take(MAXIMUM_FILE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| settings_error())?;
        let parsed: GeneralSettingsFile =
            serde_json::from_slice(&bytes).map_err(|_| settings_error())?;
        validate_file(&parsed)?;
        Ok(parsed)
    }

    fn save(&self, file: &GeneralSettingsFile) -> Result<(), String> {
        validate_file(file)?;
        let bytes = serde_json::to_vec(file).map_err(|_| settings_error())?;
        if bytes.len() as u64 > MAXIMUM_FILE_BYTES {
            return Err(settings_error());
        }
        let parent = self.path.parent().ok_or_else(settings_error)?;
        fs::create_dir_all(parent).map_err(|_| settings_error())?;
        let temporary = parent.join(format!(".general-settings-{}.tmp", Uuid::new_v4()));
        let result =
            write_private_file(&temporary, &bytes).and_then(|_| fs::rename(&temporary, &self.path));
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
            return Err(settings_error());
        }
        if let Ok(directory) = File::open(parent) {
            let _ = directory.sync_all();
        }
        Ok(())
    }
}

#[tauri::command]
pub(crate) async fn load_general_settings(
    owner_user_id: String,
    authentication: State<'_, AuthenticationManager>,
    store: State<'_, GeneralSettingsStore>,
) -> Result<GeneralSettingsSnapshot, String> {
    validate_owner(&owner_user_id)?;
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    let store = store.inner().clone();
    let owner = owner_user_id.clone();
    let snapshot = tauri::async_runtime::spawn_blocking(move || store.snapshot(&owner))
        .await
        .map_err(|_| settings_error())??;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    Ok(snapshot)
}

#[tauri::command]
pub(crate) async fn update_general_settings(
    input: UpdateGeneralSettingsInput,
    authentication: State<'_, AuthenticationManager>,
    store: State<'_, GeneralSettingsStore>,
) -> Result<GeneralSettingsSnapshot, String> {
    validate_owner(&input.owner_user_id)?;
    validate_update(&input)?;
    let owner_user_id = input.owner_user_id.clone();
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    let store = store.inner().clone();
    let snapshot = tauri::async_runtime::spawn_blocking(move || store.update(input))
        .await
        .map_err(|_| settings_error())??;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    Ok(snapshot)
}

fn validate_update(input: &UpdateGeneralSettingsInput) -> Result<(), String> {
    if input.expected_revision == 0 {
        return Err("常规设置版本无效。".into());
    }
    validate_language(&input.interface_language)?;
    validate_language(&input.context_language)?;
    if !(12..=18).contains(&input.interface_font_size) {
        return Err("界面字体大小必须在 12 到 18 之间。".into());
    }
    Ok(())
}

fn validate_snapshot(snapshot: &GeneralSettingsSnapshot) -> Result<(), String> {
    validate_language(&snapshot.interface_language)?;
    validate_language(&snapshot.context_language)?;
    if !(12..=18).contains(&snapshot.interface_font_size) || snapshot.revision == 0 {
        return Err(settings_error());
    }
    Ok(())
}

fn validate_file(file: &GeneralSettingsFile) -> Result<(), String> {
    if file.version != FILE_VERSION || file.owners.len() > 10_000 {
        return Err(settings_error());
    }
    for (owner, snapshot) in &file.owners {
        validate_owner(owner).map_err(|_| settings_error())?;
        validate_snapshot(snapshot)?;
    }
    Ok(())
}

fn validate_language(value: &str) -> Result<(), String> {
    if matches!(value, "zh-CN" | "en-US") {
        Ok(())
    } else {
        Err("语言设置无效。".into())
    }
}

#[cfg(target_os = "windows")]
#[derive(Clone)]
struct IdleSleepController {
    sender: std::sync::mpsc::SyncSender<bool>,
}

#[cfg(target_os = "windows")]
impl IdleSleepController {
    fn new() -> Self {
        let (sender, receiver) = std::sync::mpsc::sync_channel(4);
        std::thread::Builder::new()
            .name("chatos-idle-sleep".into())
            .spawn(move || {
                use windows_sys::Win32::System::Power::{
                    SetThreadExecutionState, ES_CONTINUOUS, ES_SYSTEM_REQUIRED,
                };
                while let Ok(enabled) = receiver.recv() {
                    let flags = if enabled {
                        ES_CONTINUOUS | ES_SYSTEM_REQUIRED
                    } else {
                        ES_CONTINUOUS
                    };
                    let _ = unsafe { SetThreadExecutionState(flags) };
                }
                let _ = unsafe { SetThreadExecutionState(ES_CONTINUOUS) };
            })
            .expect("failed to start idle sleep controller");
        Self { sender }
    }

    fn set(&self, enabled: bool) -> Result<(), String> {
        self.sender
            .send(enabled)
            .map_err(|_| "无法更新 Windows 休眠策略。".into())
    }
}

#[cfg(not(target_os = "windows"))]
#[derive(Clone, Default)]
struct IdleSleepController;

#[cfg(not(target_os = "windows"))]
impl IdleSleepController {
    fn new() -> Self {
        Self
    }

    fn set(&self, _enabled: bool) -> Result<(), String> {
        Ok(())
    }
}

fn write_private_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

fn settings_error() -> String {
    "无法读取或保存常规设置。".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_are_owner_scoped_and_versioned() {
        let directory = tempfile::tempdir().unwrap();
        let store = GeneralSettingsStore::new(directory.path().to_path_buf());
        let alice = store.snapshot("alice").unwrap();
        assert_eq!(alice.interface_language, "zh-CN");
        let updated = store
            .update(UpdateGeneralSettingsInput {
                owner_user_id: "alice".into(),
                interface_language: "en-US".into(),
                context_language: "zh-CN".into(),
                interface_font_size: 16,
                prevents_idle_system_sleep: true,
                expected_revision: alice.revision,
            })
            .unwrap();
        assert_eq!(updated.revision, 2);
        assert_eq!(store.snapshot("alice").unwrap(), updated);
        assert_eq!(
            store.snapshot("bob").unwrap(),
            GeneralSettingsSnapshot::default()
        );
        assert!(store
            .update(UpdateGeneralSettingsInput {
                owner_user_id: "alice".into(),
                interface_language: "zh-CN".into(),
                context_language: "zh-CN".into(),
                interface_font_size: 14,
                prevents_idle_system_sleep: false,
                expected_revision: 1,
            })
            .is_err());
    }

    #[test]
    fn rejects_unknown_languages_and_out_of_range_fonts() {
        let input = UpdateGeneralSettingsInput {
            owner_user_id: "alice".into(),
            interface_language: "fr-FR".into(),
            context_language: "zh-CN".into(),
            interface_font_size: 22,
            prevents_idle_system_sleep: false,
            expected_revision: 1,
        };
        assert!(validate_update(&input).is_err());
    }

    #[test]
    fn exposes_owner_scoped_languages_to_native_runtimes() {
        let directory = tempfile::tempdir().unwrap();
        let store = GeneralSettingsStore::new(directory.path().to_path_buf());
        let initial = store.snapshot("alice").unwrap();
        store
            .update(UpdateGeneralSettingsInput {
                owner_user_id: "alice".into(),
                interface_language: "en-US".into(),
                context_language: "en-US".into(),
                interface_font_size: 14,
                prevents_idle_system_sleep: false,
                expected_revision: initial.revision,
            })
            .unwrap();

        assert_eq!(store.context_language("alice").unwrap(), "en-US");
        assert_eq!(store.context_language("bob").unwrap(), "zh-CN");
        assert_eq!(store.interface_language("alice").unwrap(), "en-US");
        assert_eq!(store.interface_language("bob").unwrap(), "zh-CN");
    }
}
