use serde_json::{json, Map, Value};
use std::{
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

use crate::app_data_transaction::replace_owned_file;

const MAXIMUM_STATE_BYTES: u64 = 16 * 1_024 * 1_024;
static STATE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

pub(crate) fn ensure_owner_state(
    app_data_directory: &Path,
    owner_user_id: &str,
) -> Result<PathBuf, String> {
    let _guard = lock_state()?;
    ensure_owner_state_unlocked(app_data_directory, owner_user_id)
}

fn ensure_owner_state_unlocked(
    app_data_directory: &Path,
    owner_user_id: &str,
) -> Result<PathBuf, String> {
    let connector_directory = confined_connector_directory(app_data_directory)?;
    let state_path = connector_directory.join("state.json");
    reject_symbolic_link(&state_path)?;
    let mut state = load_state(&state_path)?;
    let previous_owner = state
        .get("user")
        .and_then(Value::as_object)
        .and_then(|user| user.get("id"))
        .and_then(Value::as_str);
    if previous_owner != Some(owner_user_id) {
        state.insert("user".into(), json!({"id": owner_user_id}));
        state.insert("deviceID".into(), Value::String(new_device_id()));
        state.insert("workspaces".into(), Value::Array(Vec::new()));
        state.insert(
            "approvalMode".into(),
            Value::String("request_approval".into()),
        );
        state.remove("commandApprovalModelConfigID");
        state.remove("commandApprovalThinkingLevel");
    } else {
        if state
            .get("deviceID")
            .and_then(Value::as_str)
            .is_none_or(|value| !valid_identifier(value, 1_024))
        {
            state.insert("deviceID".into(), Value::String(new_device_id()));
        }
        validate_workspaces(state.get("workspaces"))?;
    }
    if !state.contains_key("workspaces") {
        state.insert("workspaces".into(), Value::Array(Vec::new()));
    }
    persist_state(&connector_directory, &state_path, &state)?;
    Ok(state_path)
}

pub(crate) fn read_owner_state(
    app_data_directory: &Path,
    owner_user_id: &str,
) -> Result<Map<String, Value>, String> {
    let _guard = lock_state()?;
    let connector_directory = confined_connector_directory(app_data_directory)?;
    let state_path = connector_directory.join("state.json");
    reject_symbolic_link(&state_path)?;
    let state = load_state(&state_path)?;
    validate_owner(&state, owner_user_id)?;
    validate_workspaces(state.get("workspaces"))?;
    Ok(state)
}

pub(crate) fn update_owner_state<R>(
    app_data_directory: &Path,
    owner_user_id: &str,
    update: impl FnOnce(&mut Map<String, Value>) -> Result<R, String>,
) -> Result<R, String> {
    let _guard = lock_state()?;
    let connector_directory = confined_connector_directory(app_data_directory)?;
    let state_path = connector_directory.join("state.json");
    reject_symbolic_link(&state_path)?;
    let mut state = load_state(&state_path)?;
    validate_owner(&state, owner_user_id)?;
    validate_workspaces(state.get("workspaces"))?;
    let result = update(&mut state)?;
    persist_state(&connector_directory, &state_path, &state)?;
    Ok(result)
}

fn lock_state() -> Result<std::sync::MutexGuard<'static, ()>, String> {
    STATE_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| state_error())
}

fn new_device_id() -> String {
    format!("windows-device-{}", uuid::Uuid::new_v4())
}

fn confined_connector_directory(app_data_directory: &Path) -> Result<PathBuf, String> {
    fs::create_dir_all(app_data_directory).map_err(|_| state_error())?;
    let canonical_data = fs::canonicalize(app_data_directory).map_err(|_| state_error())?;
    let requested = canonical_data.join("NativeConnector");
    fs::create_dir_all(&requested).map_err(|_| state_error())?;
    let canonical_connector = fs::canonicalize(requested).map_err(|_| state_error())?;
    if !canonical_connector.starts_with(&canonical_data) {
        return Err(state_error());
    }
    Ok(canonical_connector)
}

fn reject_symbolic_link(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            Err(state_error())
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(state_error()),
    }
}

fn load_state(path: &Path) -> Result<Map<String, Value>, String> {
    let metadata = match path.metadata() {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Map::new()),
        Err(_) => return Err(state_error()),
    };
    if !metadata.is_file() || metadata.len() > MAXIMUM_STATE_BYTES {
        return Err(state_error());
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    File::open(path)
        .and_then(|file| file.take(MAXIMUM_STATE_BYTES + 1).read_to_end(&mut bytes))
        .map_err(|_| state_error())?;
    if bytes.len() as u64 > MAXIMUM_STATE_BYTES {
        return Err(state_error());
    }
    let value = serde_json::from_slice::<Value>(&bytes).map_err(|_| state_error())?;
    let object = value.as_object().cloned().ok_or_else(state_error)?;
    if object.get("user").is_some_and(|user| {
        user.as_object()
            .and_then(|user| user.get("id"))
            .and_then(Value::as_str)
            .is_none()
    }) {
        return Err(state_error());
    }
    Ok(object)
}

fn validate_workspaces(value: Option<&Value>) -> Result<(), String> {
    let Some(value) = value else { return Ok(()) };
    let workspaces = value.as_array().ok_or_else(state_error)?;
    if workspaces.len() > 1_024 {
        return Err(state_error());
    }
    for workspace in workspaces {
        let workspace = workspace.as_object().ok_or_else(state_error)?;
        for field in ["id", "absoluteRoot", "fingerprint"] {
            let value = workspace
                .get(field)
                .and_then(Value::as_str)
                .ok_or_else(state_error)?;
            if !valid_identifier(value, 4_096) {
                return Err(state_error());
            }
        }
    }
    Ok(())
}

fn validate_owner(state: &Map<String, Value>, owner_user_id: &str) -> Result<(), String> {
    if state
        .get("user")
        .and_then(Value::as_object)
        .and_then(|user| user.get("id"))
        .and_then(Value::as_str)
        == Some(owner_user_id)
    {
        Ok(())
    } else {
        Err(state_error())
    }
}

fn valid_identifier(value: &str, maximum_bytes: usize) -> bool {
    !value.is_empty()
        && value.trim() == value
        && value.len() <= maximum_bytes
        && !value.chars().any(char::is_control)
}

fn persist_state(
    directory: &Path,
    state_path: &Path,
    state: &Map<String, Value>,
) -> Result<(), String> {
    let mut bytes = serde_json::to_vec_pretty(state).map_err(|_| state_error())?;
    bytes.push(b'\n');
    if bytes.len() as u64 > MAXIMUM_STATE_BYTES {
        return Err(state_error());
    }
    replace_owned_file(directory, state_path, ".connector-state-", &bytes)
        .map_err(|_| state_error())
}

fn state_error() -> String {
    "无法准备账号隔离的本地工作区状态。".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initializes_owner_state_and_preserves_same_owner_grants() {
        let directory = tempfile::tempdir().unwrap();
        let state_path = ensure_owner_state(directory.path(), "owner-1").unwrap();
        let mut state = load_state(&state_path).unwrap();
        state.insert(
            "workspaces".into(),
            json!([{
                "id": "workspace-1",
                "absoluteRoot": directory.path().to_string_lossy(),
                "fingerprint": "fingerprint-1"
            }]),
        );
        state.insert("approvalMode".into(), json!("full_control"));
        state.insert("commandApprovalModelConfigID".into(), json!("model-1"));
        state.insert("commandApprovalThinkingLevel".into(), json!("high"));
        persist_state(state_path.parent().unwrap(), &state_path, &state).unwrap();

        ensure_owner_state(directory.path(), "owner-1").unwrap();
        let restored = load_state(&state_path).unwrap();
        assert_eq!(restored["user"]["id"], "owner-1");
        assert_eq!(restored["workspaces"].as_array().unwrap().len(), 1);
        assert_eq!(restored["approvalMode"], "full_control");
        assert_eq!(restored["commandApprovalModelConfigID"], "model-1");
        assert_eq!(restored["commandApprovalThinkingLevel"], "high");
        let device_id = restored["deviceID"].as_str().unwrap().to_owned();
        assert!(device_id.starts_with("windows-device-"));
        ensure_owner_state(directory.path(), "owner-1").unwrap();
        assert_eq!(load_state(&state_path).unwrap()["deviceID"], device_id);
    }

    #[test]
    fn account_change_drops_previous_workspace_grants() {
        let directory = tempfile::tempdir().unwrap();
        let state_path = ensure_owner_state(directory.path(), "owner-1").unwrap();
        let mut state = load_state(&state_path).unwrap();
        state.insert(
            "workspaces".into(),
            json!([{
                "id": "workspace-1",
                "absoluteRoot": directory.path().to_string_lossy(),
                "fingerprint": "fingerprint-1"
            }]),
        );
        state.insert("approvalMode".into(), json!("full_control"));
        state.insert("commandApprovalModelConfigID".into(), json!("model-1"));
        state.insert("commandApprovalThinkingLevel".into(), json!("high"));
        persist_state(state_path.parent().unwrap(), &state_path, &state).unwrap();
        let previous_device = state["deviceID"].as_str().unwrap().to_owned();

        ensure_owner_state(directory.path(), "owner-2").unwrap();
        let restored = load_state(&state_path).unwrap();
        assert_eq!(restored["user"]["id"], "owner-2");
        assert!(restored["workspaces"].as_array().unwrap().is_empty());
        assert_ne!(restored["deviceID"], previous_device);
        assert_eq!(restored["approvalMode"], "request_approval");
        assert!(restored.get("commandApprovalModelConfigID").is_none());
        assert!(restored.get("commandApprovalThinkingLevel").is_none());
    }

    #[test]
    fn owner_scoped_updates_preserve_unrelated_connector_state() {
        let directory = tempfile::tempdir().unwrap();
        ensure_owner_state(directory.path(), "owner-1").unwrap();
        update_owner_state(directory.path(), "owner-1", |state| {
            state.insert("approvalMode".into(), json!("full_control"));
            Ok(())
        })
        .unwrap();
        let restored = read_owner_state(directory.path(), "owner-1").unwrap();
        assert_eq!(restored["approvalMode"], "full_control");
        assert!(restored.get("deviceID").is_some());
        assert!(restored.get("workspaces").is_some());
        assert!(read_owner_state(directory.path(), "owner-2").is_err());
    }
}
