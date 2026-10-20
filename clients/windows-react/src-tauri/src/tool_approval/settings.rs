use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::Read,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use crate::{connector_state, control_plane::ControlPlaneModelOption};

use super::models::{
    ToolApproval, ToolApprovalDecision, ToolApprovalHistoryEntry, ToolApprovalMode,
    ToolApprovalModelOption, ToolApprovalSettings,
};

const MAXIMUM_HISTORY_BYTES: u64 = 16 * 1_024 * 1_024;
const MAXIMUM_HISTORY_ENTRIES: usize = 1_000;
const HISTORY_FILENAME: &str = "approval-activity.json";
const AUTOMATIC_REVIEWER_DETAIL: &str =
    "Windows 本机审批 Agent runtime 尚未完成准入；自动审批暂时按 macOS 降级规则转交用户确认。";

#[derive(Clone)]
pub(crate) struct ApprovalSettingsStore {
    app_data_directory: PathBuf,
    lock: Arc<Mutex<()>>,
}

impl ApprovalSettingsStore {
    pub(crate) fn new(app_data_directory: PathBuf) -> Self {
        Self {
            app_data_directory,
            lock: Arc::new(Mutex::new(())),
        }
    }

    pub(crate) fn load(
        &self,
        owner_user_id: &str,
        models: &[ControlPlaneModelOption],
    ) -> Result<ToolApprovalSettings, String> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| "审批设置暂时不可用。".to_owned())?;
        let mut state = connector_state::read_owner_state(&self.app_data_directory, owner_user_id)?;
        let mode = mode_from_state(&state)?;
        let mut selected_model_id = optional_identifier(
            state.get("commandApprovalModelConfigID"),
            "commandApprovalModelConfigID",
        )?;
        let mut selected_thinking_level = optional_identifier(
            state.get("commandApprovalThinkingLevel"),
            "commandApprovalThinkingLevel",
        )?;
        let selected_is_current = selected_model_id
            .as_deref()
            .is_none_or(|selected| models.iter().any(|model| model.id == selected));
        if !selected_is_current {
            state.remove("commandApprovalModelConfigID");
            state.remove("commandApprovalThinkingLevel");
            connector_state::update_owner_state(
                &self.app_data_directory,
                owner_user_id,
                |persisted| {
                    persisted.remove("commandApprovalModelConfigID");
                    persisted.remove("commandApprovalThinkingLevel");
                    Ok(())
                },
            )?;
            selected_model_id = None;
            selected_thinking_level = None;
        }
        Ok(ToolApprovalSettings {
            default_mode: mode,
            selected_model_id,
            selected_thinking_level,
            models: models.iter().map(project_model).collect(),
            automatic_reviewer_ready: false,
            automatic_reviewer_detail: AUTOMATIC_REVIEWER_DETAIL.into(),
            history: self.load_history(owner_user_id)?,
        })
    }

    pub(crate) fn mode(&self, owner_user_id: &str) -> Result<ToolApprovalMode, String> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| "审批设置暂时不可用。".to_owned())?;
        let state = connector_state::read_owner_state(&self.app_data_directory, owner_user_id)?;
        mode_from_state(&state)
    }

    pub(crate) fn update_mode(
        &self,
        owner_user_id: &str,
        mode: ToolApprovalMode,
        risk_acknowledged: bool,
    ) -> Result<(), String> {
        if mode != ToolApprovalMode::RequestApproval && !risk_acknowledged {
            return Err("提高审批权限前需要明确确认风险。".into());
        }
        let _guard = self
            .lock
            .lock()
            .map_err(|_| "审批设置暂时不可用。".to_owned())?;
        connector_state::update_owner_state(&self.app_data_directory, owner_user_id, |state| {
            if mode == ToolApprovalMode::AutoApproval
                && optional_identifier(
                    state.get("commandApprovalModelConfigID"),
                    "commandApprovalModelConfigID",
                )?
                .is_none()
            {
                return Err("请先选择本机审批 Agent 模型。".into());
            }
            state.insert("approvalMode".into(), mode.as_str().into());
            Ok(())
        })
    }

    pub(crate) fn update_model(
        &self,
        owner_user_id: &str,
        selected_model_id: Option<String>,
        selected_thinking_level: Option<String>,
        models: &[ControlPlaneModelOption],
    ) -> Result<(), String> {
        let selection =
            normalized_model_selection(selected_model_id, selected_thinking_level, models)?;
        let _guard = self
            .lock
            .lock()
            .map_err(|_| "审批设置暂时不可用。".to_owned())?;
        connector_state::update_owner_state(&self.app_data_directory, owner_user_id, |state| {
            match selection.0.as_deref() {
                Some(model_id) => {
                    state.insert("commandApprovalModelConfigID".into(), model_id.into());
                }
                None => {
                    state.remove("commandApprovalModelConfigID");
                }
            }
            match selection.1.as_deref() {
                Some(level) => {
                    state.insert("commandApprovalThinkingLevel".into(), level.into());
                }
                None => {
                    state.remove("commandApprovalThinkingLevel");
                }
            }
            Ok(())
        })
    }

    pub(crate) fn record_decision(
        &self,
        owner_user_id: &str,
        approval: &ToolApproval,
        mode: ToolApprovalMode,
        decision: ToolApprovalDecision,
        decided_at_unix_ms: i64,
    ) -> Result<(), String> {
        if decided_at_unix_ms <= 0 {
            return Err("审批记录时间无效。".into());
        }
        let _guard = self
            .lock
            .lock()
            .map_err(|_| "审批记录暂时不可用。".to_owned())?;
        let mut history = self.load_history(owner_user_id)?;
        let id = history_id(
            owner_user_id,
            &approval.invocation_id,
            approval.invocation_version,
        );
        let entry = ToolApprovalHistoryEntry {
            id: id.clone(),
            invocation_id: approval.invocation_id.clone(),
            invocation_version: approval.invocation_version,
            command: approval.command.clone(),
            purpose: approval.purpose.clone(),
            cwd: approval.cwd.clone(),
            source: approval.source.clone(),
            mode,
            decision,
            risk: approval.risk.clone(),
            reason: approval.reason.clone(),
            created_at_unix_ms: decided_at_unix_ms,
        };
        if let Some(existing) = history.iter().find(|existing| existing.id == id) {
            return if existing == &entry {
                Ok(())
            } else {
                Err("审批记录身份冲突。".into())
            };
        }
        history.insert(0, entry);
        history.truncate(MAXIMUM_HISTORY_ENTRIES);
        self.save_history(owner_user_id, &history)
    }

    fn load_history(&self, owner_user_id: &str) -> Result<Vec<ToolApprovalHistoryEntry>, String> {
        let path = self.history_path(owner_user_id)?;
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(_) => return Err("无法读取审批记录。".into()),
        };
        if metadata.file_type().is_symlink()
            || !metadata.is_file()
            || metadata.len() > MAXIMUM_HISTORY_BYTES
        {
            return Err("审批记录文件无效。".into());
        }
        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        File::open(&path)
            .and_then(|file| file.take(MAXIMUM_HISTORY_BYTES + 1).read_to_end(&mut bytes))
            .map_err(|_| "无法读取审批记录。".to_owned())?;
        if bytes.len() as u64 > MAXIMUM_HISTORY_BYTES {
            return Err("审批记录文件过大。".into());
        }
        let file = serde_json::from_slice::<ApprovalHistoryFile>(&bytes)
            .map_err(|_| "审批记录文件无效。".to_owned())?;
        if file.owner_user_id != owner_user_id {
            return Ok(Vec::new());
        }
        if file.entries.len() > MAXIMUM_HISTORY_ENTRIES
            || file.entries.iter().any(|entry| !valid_history_entry(entry))
        {
            return Err("审批记录文件无效。".into());
        }
        Ok(file.entries)
    }

    fn save_history(
        &self,
        owner_user_id: &str,
        entries: &[ToolApprovalHistoryEntry],
    ) -> Result<(), String> {
        let path = self.history_path(owner_user_id)?;
        let directory = path
            .parent()
            .ok_or_else(|| "审批记录路径无效。".to_owned())?;
        let bytes = serde_json::to_vec_pretty(&ApprovalHistoryFile {
            owner_user_id: owner_user_id.to_owned(),
            entries: entries.to_vec(),
        })
        .map_err(|_| "无法编码审批记录。".to_owned())?;
        if bytes.len() as u64 > MAXIMUM_HISTORY_BYTES {
            return Err("审批记录文件过大。".into());
        }
        let mut temporary =
            tempfile::NamedTempFile::new_in(directory).map_err(|_| "无法写入审批记录。")?;
        std::io::Write::write_all(&mut temporary, &bytes)
            .and_then(|()| temporary.as_file().sync_all())
            .map_err(|_| "无法写入审批记录。".to_owned())?;
        temporary
            .persist(path)
            .map_err(|_| "无法发布审批记录。".to_owned())?;
        Ok(())
    }

    fn history_path(&self, owner_user_id: &str) -> Result<PathBuf, String> {
        let state_path =
            connector_state::ensure_owner_state(&self.app_data_directory, owner_user_id)?;
        Ok(state_path
            .parent()
            .ok_or_else(|| "审批记录路径无效。".to_owned())?
            .join(HISTORY_FILENAME))
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct ApprovalHistoryFile {
    owner_user_id: String,
    entries: Vec<ToolApprovalHistoryEntry>,
}

fn mode_from_state(
    state: &serde_json::Map<String, serde_json::Value>,
) -> Result<ToolApprovalMode, String> {
    match state
        .get("approvalMode")
        .and_then(serde_json::Value::as_str)
    {
        None | Some("request_approval") => Ok(ToolApprovalMode::RequestApproval),
        Some("auto_approval") => Ok(ToolApprovalMode::AutoApproval),
        Some("full_control") => Ok(ToolApprovalMode::FullControl),
        Some(_) => Err("本地审批模式无效。".into()),
    }
}

fn optional_identifier(
    value: Option<&serde_json::Value>,
    field: &str,
) -> Result<Option<String>, String> {
    let Some(value) = value else { return Ok(None) };
    let value = value
        .as_str()
        .ok_or_else(|| format!("{field} is invalid"))?;
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    if value.len() > 1_024 || value.chars().any(char::is_control) {
        return Err(format!("{field} is invalid"));
    }
    Ok(Some(value.to_owned()))
}

fn normalized_model_selection(
    selected_model_id: Option<String>,
    selected_thinking_level: Option<String>,
    models: &[ControlPlaneModelOption],
) -> Result<(Option<String>, Option<String>), String> {
    let selected_model_id = trimmed_optional(selected_model_id, "selected_model_id")?;
    let selected_thinking_level =
        trimmed_optional(selected_thinking_level, "selected_thinking_level")?;
    let Some(model_id) = selected_model_id else {
        if selected_thinking_level.is_some() {
            return Err("未选择模型时不能设置推理等级。".into());
        }
        return Ok((None, None));
    };
    let model = models
        .iter()
        .find(|model| model.id == model_id)
        .ok_or_else(|| "选择的审批模型当前不可用。".to_owned())?;
    let thinking = if model.supports_reasoning {
        let value = selected_thinking_level.unwrap_or_else(|| model.default_thinking_level.clone());
        if !model.thinking_levels.contains(&value) {
            return Err("选择的审批模型不支持该推理等级。".into());
        }
        Some(value)
    } else {
        None
    };
    Ok((Some(model_id), thinking))
}

fn trimmed_optional(value: Option<String>, field: &str) -> Result<Option<String>, String> {
    let Some(value) = value else { return Ok(None) };
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.len() > 1_024 || trimmed.chars().any(char::is_control) {
        return Err(format!("{field} is invalid"));
    }
    Ok(Some(trimmed.to_owned()))
}

fn project_model(model: &ControlPlaneModelOption) -> ToolApprovalModelOption {
    ToolApprovalModelOption {
        id: model.id.clone(),
        display_name: model.display_name.clone(),
        model_name: model.model_name.clone(),
        provider: model.provider.clone(),
        supports_reasoning: model.supports_reasoning,
        thinking_levels: model.thinking_levels.clone(),
        default_thinking_level: model.default_thinking_level.clone(),
    }
}

fn history_id(owner_user_id: &str, invocation_id: &str, version: u64) -> String {
    let mut digest = Sha256::new();
    for value in [owner_user_id, invocation_id, version.to_string().as_str()] {
        digest.update(value.len().to_le_bytes());
        digest.update(value.as_bytes());
    }
    format!("approval:{:x}", digest.finalize())
}

fn valid_history_entry(entry: &ToolApprovalHistoryEntry) -> bool {
    !entry.id.is_empty()
        && entry.id.len() <= 1_024
        && !entry.invocation_id.is_empty()
        && entry.invocation_id.len() <= 1_024
        && entry.invocation_version > 0
        && !entry.command.is_empty()
        && entry.command.chars().count() <= 4_000
        && entry
            .purpose
            .as_ref()
            .is_none_or(|value| value.chars().count() <= 500)
        && !entry.cwd.is_empty()
        && entry.cwd.chars().count() <= 1_000
        && !entry.source.is_empty()
        && entry.source.len() <= 1_024
        && matches!(entry.risk.as_str(), "low" | "medium" | "high")
        && entry
            .reason
            .as_ref()
            .is_none_or(|value| value.chars().count() <= 4_000)
        && entry.created_at_unix_ms > 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn enforces_risk_confirmation_and_model_requirement() {
        let directory = tempfile::tempdir().unwrap();
        connector_state::ensure_owner_state(directory.path(), "owner-1").unwrap();
        let store = ApprovalSettingsStore::new(directory.path().into());
        assert!(store
            .update_mode("owner-1", ToolApprovalMode::FullControl, false)
            .is_err());
        assert!(store
            .update_mode("owner-1", ToolApprovalMode::AutoApproval, true)
            .is_err());
        store
            .update_mode("owner-1", ToolApprovalMode::FullControl, true)
            .unwrap();
        assert_eq!(
            store.mode("owner-1").unwrap(),
            ToolApprovalMode::FullControl
        );
        store
            .update_mode("owner-1", ToolApprovalMode::RequestApproval, false)
            .unwrap();
    }

    #[test]
    fn validates_and_persists_approval_model_selection() {
        let directory = tempfile::tempdir().unwrap();
        connector_state::ensure_owner_state(directory.path(), "owner-1").unwrap();
        let store = ApprovalSettingsStore::new(directory.path().into());
        let models = vec![model("model-1", true)];
        store
            .update_model(
                "owner-1",
                Some("model-1".into()),
                Some("high".into()),
                &models,
            )
            .unwrap();
        store
            .update_mode("owner-1", ToolApprovalMode::AutoApproval, true)
            .unwrap();
        let snapshot = store.load("owner-1", &models).unwrap();
        assert_eq!(snapshot.selected_model_id.as_deref(), Some("model-1"));
        assert_eq!(snapshot.selected_thinking_level.as_deref(), Some("high"));
        assert_eq!(snapshot.default_mode, ToolApprovalMode::AutoApproval);
        assert!(!snapshot.automatic_reviewer_ready);
        assert!(store
            .update_model("owner-1", Some("missing".into()), None, &models)
            .is_err());
    }

    #[test]
    fn history_is_owner_scoped_persistent_and_idempotent() {
        let directory = tempfile::tempdir().unwrap();
        connector_state::ensure_owner_state(directory.path(), "owner-1").unwrap();
        let store = ApprovalSettingsStore::new(directory.path().into());
        let approval = approval();
        store
            .record_decision(
                "owner-1",
                &approval,
                ToolApprovalMode::RequestApproval,
                ToolApprovalDecision::AcceptForSession,
                12,
            )
            .unwrap();
        store
            .record_decision(
                "owner-1",
                &approval,
                ToolApprovalMode::RequestApproval,
                ToolApprovalDecision::AcceptForSession,
                12,
            )
            .unwrap();
        let reopened = ApprovalSettingsStore::new(directory.path().into());
        let snapshot = reopened.load("owner-1", &[]).unwrap();
        assert_eq!(snapshot.history.len(), 1);
        assert_eq!(
            snapshot.history[0].decision,
            ToolApprovalDecision::AcceptForSession
        );
        assert_eq!(snapshot.history[0].created_at_unix_ms, 12);

        connector_state::ensure_owner_state(directory.path(), "owner-2").unwrap();
        assert!(reopened.load("owner-2", &[]).unwrap().history.is_empty());
    }

    fn model(id: &str, supports_reasoning: bool) -> ControlPlaneModelOption {
        ControlPlaneModelOption {
            id: id.into(),
            display_name: "GPT".into(),
            model_name: "gpt-test".into(),
            provider: "openai".into(),
            default_thinking_level: if supports_reasoning { "medium" } else { "none" }.into(),
            supports_reasoning,
            thinking_levels: if supports_reasoning {
                vec!["low".into(), "medium".into(), "high".into()]
            } else {
                Vec::new()
            },
            model_config_revision: "revision-1".into(),
        }
    }

    fn approval() -> ToolApproval {
        ToolApproval {
            invocation_id: "invocation-1".into(),
            invocation_version: 2,
            run_id: "run-1".into(),
            tool_name: "execute_command".into(),
            command: "cargo test".into(),
            purpose: Some("验证".into()),
            cwd: ".".into(),
            source: "Local Agent Task".into(),
            risk: "medium".into(),
            reason: Some("可能修改文件".into()),
            created_at_unix_ms: 10,
            updated_at_unix_ms: 11,
            available_decisions: vec![ToolApprovalDecision::Decline, ToolApprovalDecision::Accept],
        }
    }

    #[test]
    fn rejects_corrupt_history_instead_of_exposing_untrusted_content() {
        let directory = tempfile::tempdir().unwrap();
        let state = connector_state::ensure_owner_state(directory.path(), "owner-1").unwrap();
        fs::write(
            state.parent().unwrap().join(HISTORY_FILENAME),
            serde_json::to_vec(&json!({
                "ownerUserId": "owner-1",
                "entries": [{"command": "missing identity"}]
            }))
            .unwrap(),
        )
        .unwrap();
        let store = ApprovalSettingsStore::new(directory.path().into());
        assert!(store.load("owner-1", &[]).is_err());
    }
}
