use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use tauri::State;
use unicode_segmentation::UnicodeSegmentation;

use crate::{authentication::AuthenticationManager, host_lifecycle::validate_owner};

const SKILL_CATALOG: &str =
    include_str!("../../../macos/Sources/ChatOSCore/AgentSkills/Catalog/ChatOSSkillCatalog.json");
const MAXIMUM_PAYLOAD_BYTES: u64 = 256 * 1_024 * 1_024;

#[derive(Clone)]
pub(crate) struct AgentSkillStore {
    file: PathBuf,
    payload: Arc<Mutex<OverridePayload>>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct OverridePayload {
    #[serde(default = "schema_version")]
    schema_version: u8,
    #[serde(default)]
    accounts: HashMap<String, AccountOverrides>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct AccountOverrides {
    #[serde(default)]
    professions: HashMap<String, ProfessionOverride>,
    #[serde(default)]
    project_types: HashMap<String, ProjectTypeOverride>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProfessionOverride {
    label: String,
    description: String,
    skill_markdown: String,
    #[serde(rename = "labelEN", default)]
    label_en: Option<String>,
    #[serde(rename = "descriptionEN", default)]
    description_en: Option<String>,
    #[serde(rename = "skillMarkdownEN", default)]
    skill_markdown_en: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectTypeOverride {
    label: String,
    description: String,
    rule_markdown: String,
    #[serde(rename = "labelEN", default)]
    label_en: Option<String>,
    #[serde(rename = "descriptionEN", default)]
    description_en: Option<String>,
    #[serde(rename = "ruleMarkdownEN", default)]
    rule_markdown_en: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CatalogPayload {
    professions: Vec<CatalogProfession>,
    project_types: Vec<CatalogProjectType>,
}

#[derive(Clone, Debug, Deserialize)]
struct CatalogProfession {
    key: String,
    label: String,
    label_en: String,
    description: String,
    description_en: String,
    category_key: String,
    category_label: String,
    category_label_en: String,
    skill_markdown: String,
    skill_markdown_en: String,
}

#[derive(Clone, Debug, Deserialize)]
struct CatalogProjectType {
    key: String,
    label: String,
    label_en: String,
    description: String,
    description_en: String,
    category_key: String,
    category_label: String,
    category_label_en: String,
    rule_markdown: String,
    rule_markdown_en: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum AgentSkillKind {
    Profession,
    ProjectType,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentSkillDefinition {
    kind: AgentSkillKind,
    key: String,
    label: String,
    label_en: String,
    description: String,
    description_en: String,
    category_key: String,
    category_label: String,
    category_label_en: String,
    markdown: String,
    markdown_en: String,
    customized: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AgentSkillMutationInput {
    kind: AgentSkillKind,
    key: String,
    label: String,
    label_en: String,
    description: String,
    description_en: String,
    markdown: String,
    markdown_en: String,
}

fn schema_version() -> u8 {
    2
}

impl AgentSkillStore {
    pub(crate) fn new(file: PathBuf) -> Result<Self, String> {
        let payload = load_payload(&file)?;
        Ok(Self {
            file,
            payload: Arc::new(Mutex::new(payload)),
        })
    }

    pub(crate) fn from_runtime() -> Result<Self, String> {
        let file = dirs::data_local_dir()
            .ok_or_else(storage_error)?
            .join("com.chatos.windows")
            .join("connector")
            .join("AgentSkillOverrides.json");
        Self::new(file)
    }

    fn catalog(&self, owner_user_id: &str) -> Result<Vec<AgentSkillDefinition>, String> {
        let catalog = catalog_payload()?;
        let payload = self.payload.lock().map_err(|_| storage_error())?;
        let account = payload.accounts.get(owner_user_id);
        Ok(merged_catalog(&catalog, account))
    }

    fn save(
        &self,
        owner_user_id: &str,
        input: AgentSkillMutationInput,
    ) -> Result<AgentSkillDefinition, String> {
        let catalog = catalog_payload()?;
        let base = catalog_item(&catalog, input.kind, input.key.trim())
            .ok_or_else(|| "Skill 标识无效，请刷新后重试。".to_owned())?;
        let label = validate_field(&input.label, "名称", 120)?;
        let label_en = validate_field(&input.label_en, "英文名称", 120)?;
        let description = validate_field(&input.description, "说明", 4_000)?;
        let description_en = validate_field(&input.description_en, "英文说明", 4_000)?;
        let markdown = validate_field(&input.markdown, "正文", 500_000)?;
        let markdown_en = validate_field(&input.markdown_en, "英文正文", 500_000)?;
        let mut payload = self.payload.lock().map_err(|_| storage_error())?;
        let mut next = payload.clone();
        next.schema_version = 2;
        let account = next.accounts.entry(owner_user_id.to_owned()).or_default();
        let saved = match input.kind {
            AgentSkillKind::Profession => {
                let value = ProfessionOverride {
                    label,
                    description,
                    skill_markdown: markdown,
                    label_en: Some(label_en),
                    description_en: Some(description_en),
                    skill_markdown_en: Some(markdown_en),
                };
                account.professions.insert(base.key.clone(), value.clone());
                apply_profession_override(base, Some(&value))
            }
            AgentSkillKind::ProjectType => {
                let value = ProjectTypeOverride {
                    label,
                    description,
                    rule_markdown: markdown,
                    label_en: Some(label_en),
                    description_en: Some(description_en),
                    rule_markdown_en: Some(markdown_en),
                };
                account
                    .project_types
                    .insert(base.key.clone(), value.clone());
                apply_project_type_override(base, Some(&value))
            }
        };
        persist_payload(&self.file, &next)?;
        *payload = next;
        Ok(saved)
    }

    fn reset(
        &self,
        owner_user_id: &str,
        kind: AgentSkillKind,
        key: &str,
    ) -> Result<AgentSkillDefinition, String> {
        let catalog = catalog_payload()?;
        let base = catalog_item(&catalog, kind, key.trim())
            .ok_or_else(|| "Skill 标识无效，请刷新后重试。".to_owned())?;
        let mut payload = self.payload.lock().map_err(|_| storage_error())?;
        let mut next = payload.clone();
        if let Some(account) = next.accounts.get_mut(owner_user_id) {
            match kind {
                AgentSkillKind::Profession => {
                    account.professions.remove(base.key.as_str());
                }
                AgentSkillKind::ProjectType => {
                    account.project_types.remove(base.key.as_str());
                }
            }
            if account.professions.is_empty() && account.project_types.is_empty() {
                next.accounts.remove(owner_user_id);
            }
        }
        persist_payload(&self.file, &next)?;
        *payload = next;
        Ok(base)
    }
}

#[tauri::command]
pub(crate) async fn load_agent_skill_catalog(
    owner_user_id: String,
    authentication: State<'_, AuthenticationManager>,
    store: State<'_, AgentSkillStore>,
) -> Result<Vec<AgentSkillDefinition>, String> {
    validate_owner(&owner_user_id)?;
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    let query_store = store.inner().clone();
    let owner = owner_user_id.clone();
    let catalog = tauri::async_runtime::spawn_blocking(move || query_store.catalog(&owner))
        .await
        .map_err(|_| "Skill 目录读取任务异常退出。".to_owned())??;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    Ok(catalog)
}

#[tauri::command]
pub(crate) async fn save_agent_skill_override(
    owner_user_id: String,
    input: AgentSkillMutationInput,
    authentication: State<'_, AuthenticationManager>,
    store: State<'_, AgentSkillStore>,
) -> Result<AgentSkillDefinition, String> {
    validate_owner(&owner_user_id)?;
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    let mutation_store = store.inner().clone();
    let owner = owner_user_id.clone();
    tauri::async_runtime::spawn_blocking(move || mutation_store.save(&owner, input))
        .await
        .map_err(|_| "Skill 保存任务异常退出。".to_owned())?
}

#[tauri::command]
pub(crate) async fn reset_agent_skill_override(
    owner_user_id: String,
    kind: AgentSkillKind,
    key: String,
    authentication: State<'_, AuthenticationManager>,
    store: State<'_, AgentSkillStore>,
) -> Result<AgentSkillDefinition, String> {
    validate_owner(&owner_user_id)?;
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    let mutation_store = store.inner().clone();
    let owner = owner_user_id.clone();
    tauri::async_runtime::spawn_blocking(move || mutation_store.reset(&owner, kind, &key))
        .await
        .map_err(|_| "Skill 恢复任务异常退出。".to_owned())?
}

fn catalog_payload() -> Result<CatalogPayload, String> {
    serde_json::from_str(SKILL_CATALOG).map_err(|_| "Skill 内置目录不可用。".to_owned())
}

fn merged_catalog(
    catalog: &CatalogPayload,
    account: Option<&AccountOverrides>,
) -> Vec<AgentSkillDefinition> {
    catalog
        .professions
        .iter()
        .cloned()
        .map(AgentSkillDefinition::from)
        .map(|base| {
            let value = account.and_then(|account| account.professions.get(&base.key));
            apply_profession_override(base, value)
        })
        .chain(
            catalog
                .project_types
                .iter()
                .cloned()
                .map(AgentSkillDefinition::from)
                .map(|base| {
                    let value = account.and_then(|account| account.project_types.get(&base.key));
                    apply_project_type_override(base, value)
                }),
        )
        .collect()
}

fn catalog_item(
    catalog: &CatalogPayload,
    kind: AgentSkillKind,
    key: &str,
) -> Option<AgentSkillDefinition> {
    match kind {
        AgentSkillKind::Profession => catalog
            .professions
            .iter()
            .find(|value| value.key == key)
            .cloned()
            .map(Into::into),
        AgentSkillKind::ProjectType => catalog
            .project_types
            .iter()
            .find(|value| value.key == key)
            .cloned()
            .map(Into::into),
    }
}

fn apply_profession_override(
    mut base: AgentSkillDefinition,
    value: Option<&ProfessionOverride>,
) -> AgentSkillDefinition {
    if let Some(value) = value {
        base.label.clone_from(&value.label);
        base.description.clone_from(&value.description);
        base.markdown.clone_from(&value.skill_markdown);
        if let Some(value) = &value.label_en {
            base.label_en.clone_from(value);
        }
        if let Some(value) = &value.description_en {
            base.description_en.clone_from(value);
        }
        if let Some(value) = &value.skill_markdown_en {
            base.markdown_en.clone_from(value);
        }
        base.customized = true;
    }
    base
}

fn apply_project_type_override(
    mut base: AgentSkillDefinition,
    value: Option<&ProjectTypeOverride>,
) -> AgentSkillDefinition {
    if let Some(value) = value {
        base.label.clone_from(&value.label);
        base.description.clone_from(&value.description);
        base.markdown.clone_from(&value.rule_markdown);
        if let Some(value) = &value.label_en {
            base.label_en.clone_from(value);
        }
        if let Some(value) = &value.description_en {
            base.description_en.clone_from(value);
        }
        if let Some(value) = &value.rule_markdown_en {
            base.markdown_en.clone_from(value);
        }
        base.customized = true;
    }
    base
}

impl From<CatalogProfession> for AgentSkillDefinition {
    fn from(value: CatalogProfession) -> Self {
        Self {
            kind: AgentSkillKind::Profession,
            key: value.key,
            label: value.label,
            label_en: value.label_en,
            description: value.description,
            description_en: value.description_en,
            category_key: value.category_key,
            category_label: value.category_label,
            category_label_en: value.category_label_en,
            markdown: value.skill_markdown,
            markdown_en: value.skill_markdown_en,
            customized: false,
        }
    }
}

impl From<CatalogProjectType> for AgentSkillDefinition {
    fn from(value: CatalogProjectType) -> Self {
        Self {
            kind: AgentSkillKind::ProjectType,
            key: value.key,
            label: value.label,
            label_en: value.label_en,
            description: value.description,
            description_en: value.description_en,
            category_key: value.category_key,
            category_label: value.category_label,
            category_label_en: value.category_label_en,
            markdown: value.rule_markdown,
            markdown_en: value.rule_markdown_en,
            customized: false,
        }
    }
}

fn validate_field(raw: &str, field: &str, maximum_length: usize) -> Result<String, String> {
    let value = raw.trim();
    if value.is_empty() || value.contains('\0') || value.graphemes(true).count() > maximum_length {
        return Err(format!("无效的 Skill 字段：{field}"));
    }
    Ok(value.to_owned())
}

fn load_payload(file: &Path) -> Result<OverridePayload, String> {
    let Some(metadata) = symlink_safe_metadata(file)? else {
        return Ok(OverridePayload {
            schema_version: 2,
            accounts: HashMap::new(),
        });
    };
    if !metadata.is_file() || metadata.len() > MAXIMUM_PAYLOAD_BYTES {
        return Err("Skill 配置不是普通文件或超过本地存储上限。".to_owned());
    }
    let bytes = fs::read(file).map_err(|_| storage_error())?;
    let mut payload: OverridePayload =
        serde_json::from_slice(&bytes).map_err(|_| storage_error())?;
    if !(1..=2).contains(&payload.schema_version) {
        return Err("Skill 配置版本不受支持。".to_owned());
    }
    payload.schema_version = 2;
    Ok(payload)
}

fn persist_payload(file: &Path, payload: &OverridePayload) -> Result<(), String> {
    let parent = file.parent().ok_or_else(storage_error)?;
    fs::create_dir_all(parent).map_err(|_| storage_error())?;
    reject_symlink(parent)?;
    if symlink_safe_metadata(file)?.is_some() {
        reject_symlink(file)?;
    }
    let bytes = serde_json::to_vec_pretty(payload).map_err(|_| storage_error())?;
    if bytes.len() as u64 > MAXIMUM_PAYLOAD_BYTES {
        return Err("Skill 配置超过本地存储上限。".to_owned());
    }
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(|_| storage_error())?;
    temporary.write_all(&bytes).map_err(|_| storage_error())?;
    temporary
        .as_file()
        .sync_all()
        .map_err(|_| storage_error())?;
    replace_file(temporary.into_temp_path().as_ref(), file)?;
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn replace_file(source: &Path, destination: &Path) -> Result<(), String> {
    fs::rename(source, destination).map_err(|_| storage_error())
}

#[cfg(target_os = "windows")]
fn replace_file(source: &Path, destination: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };
    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    let result = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        Err(storage_error())
    } else {
        Ok(())
    }
}

fn symlink_safe_metadata(path: &Path) -> Result<Option<fs::Metadata>, String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(storage_error()),
        Ok(metadata) => Ok(Some(metadata)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(storage_error()),
    }
}

fn reject_symlink(path: &Path) -> Result<(), String> {
    if fs::symlink_metadata(path)
        .map_err(|_| storage_error())?
        .file_type()
        .is_symlink()
    {
        Err(storage_error())
    } else {
        Ok(())
    }
}

fn storage_error() -> String {
    "无法保存本地 Skill。".to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overrides_are_account_scoped_and_persisted() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("AgentSkillOverrides.json");
        let store = AgentSkillStore::new(file.clone()).unwrap();
        let saved = store.save("alice", mutation("自定义职业")).unwrap();
        assert_eq!(saved.label, "自定义职业");
        assert!(saved.customized);
        assert_eq!(store.catalog("bob").unwrap()[0].label, "项目经理");

        let reopened = AgentSkillStore::new(file).unwrap();
        assert_eq!(reopened.catalog("alice").unwrap()[0].label, "自定义职业");
        assert_eq!(reopened.catalog("bob").unwrap()[0].label, "项目经理");

        let persisted = fs::read_to_string(reopened.file.as_path()).unwrap();
        assert!(persisted.contains("\"labelEN\""));
        assert!(persisted.contains("\"skillMarkdownEN\""));
        assert!(!persisted.contains("\"labelEn\""));
    }

    #[test]
    fn reset_restores_the_bundled_definition() {
        let directory = tempfile::tempdir().unwrap();
        let store = AgentSkillStore::new(directory.path().join("overrides.json")).unwrap();
        store.save("alice", mutation("自定义职业")).unwrap();
        let reset = store
            .reset("alice", AgentSkillKind::Profession, "project_manager")
            .unwrap();
        assert_eq!(reset.label, "项目经理");
        assert!(!reset.customized);
    }

    #[test]
    fn rejects_unknown_keys_and_empty_bilingual_fields() {
        let directory = tempfile::tempdir().unwrap();
        let store = AgentSkillStore::new(directory.path().join("overrides.json")).unwrap();
        let mut unknown = mutation("自定义职业");
        unknown.key = "unknown".into();
        assert!(store.save("alice", unknown).is_err());
        let mut empty = mutation("自定义职业");
        empty.label_en = "  ".into();
        assert!(store.save("alice", empty).is_err());
    }

    fn mutation(label: &str) -> AgentSkillMutationInput {
        AgentSkillMutationInput {
            kind: AgentSkillKind::Profession,
            key: "project_manager".into(),
            label: label.into(),
            label_en: "Custom role".into(),
            description: "自定义说明".into(),
            description_en: "Custom description".into(),
            markdown: "# 自定义正文".into(),
            markdown_en: "# Custom instructions".into(),
        }
    }
}
