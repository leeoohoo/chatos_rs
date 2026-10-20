use super::models::ProjectRunEnvironmentVariable;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Display,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard},
};
use uuid::Uuid;

const CURRENT_VERSION: u32 = 1;
const MAXIMUM_PREFERENCES_BYTES: u64 = 4 * 1_024 * 1_024;
const MAXIMUM_OWNERS: usize = 32;
const MAXIMUM_PROJECTS_PER_OWNER: usize = 10_000;
const MAXIMUM_TOOLCHAINS: usize = 32;
const MAXIMUM_ENVIRONMENT_VARIABLES: usize = 128;
const MAXIMUM_RETIRED_SECRETS: usize = 1_024;
const MAXIMUM_ENVIRONMENT_KEY_BYTES: usize = 128;
const MAXIMUM_ENVIRONMENT_VALUE_BYTES: usize = 64 * 1_024;
const MAXIMUM_ENVIRONMENT_TOTAL_BYTES: usize = 512 * 1_024;
const SECRET_SERVICE: &str = "com.chatos.windows.project-run-environment";

#[derive(Clone)]
pub(crate) struct ProjectRunPreferencesStore {
    path: PathBuf,
    secrets: Arc<dyn ProjectRunSecretStore>,
    lock: Arc<Mutex<()>>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ProjectRunSelection {
    pub(crate) default_target_id: Option<String>,
    pub(crate) selected_toolchains: BTreeMap<String, String>,
    environment: BTreeMap<String, EnvironmentSecretReference>,
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct ProjectRunLaunchPreferences {
    pub(crate) selection: ProjectRunSelection,
    pub(crate) environment: BTreeMap<String, String>,
}

#[derive(Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ProjectRunEnvironmentMutation {
    pub(crate) key: String,
    pub(crate) value: Option<String>,
}

trait ProjectRunSecretStore: Send + Sync {
    fn load(&self, account: &str) -> Result<Option<String>, String>;
    fn save(&self, account: &str, value: &str) -> Result<(), String>;
    fn delete(&self, account: &str) -> Result<(), String>;
}

struct PlatformProjectRunSecretStore;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PreferencesFile {
    version: u32,
    #[serde(default)]
    owners: BTreeMap<String, OwnerPreferences>,
    #[serde(default)]
    retired_secrets: Vec<RetiredSecretReference>,
}

impl Default for PreferencesFile {
    fn default() -> Self {
        Self {
            version: CURRENT_VERSION,
            owners: BTreeMap::new(),
            retired_secrets: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OwnerPreferences {
    #[serde(default)]
    projects: BTreeMap<String, PersistedProjectRunSelection>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PersistedProjectRunSelection {
    default_target_id: Option<String>,
    #[serde(default)]
    selected_toolchains: BTreeMap<String, String>,
    #[serde(default)]
    environment: BTreeMap<String, EnvironmentSecretReference>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EnvironmentSecretReference {
    secret_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RetiredSecretReference {
    owner_user_id: String,
    project_id: String,
    key: String,
    secret_id: String,
}

impl ProjectRunPreferencesStore {
    pub(crate) fn new(app_data_directory: PathBuf) -> Self {
        Self {
            path: app_data_directory
                .join("project-run")
                .join("preferences.json"),
            secrets: Arc::new(PlatformProjectRunSecretStore),
            lock: Arc::new(Mutex::new(())),
        }
    }

    pub(crate) fn projection(
        &self,
        owner_user_id: &str,
        project_id: &str,
    ) -> Result<(ProjectRunSelection, Vec<ProjectRunEnvironmentVariable>), String> {
        let _guard = self.guard()?;
        let file = self.load_file()?;
        let selection = project_selection(&file, owner_user_id, project_id);
        let mut environment = Vec::with_capacity(selection.environment.len());
        for (key, reference) in &selection.environment {
            let account = secret_account(owner_user_id, project_id, key, &reference.secret_id);
            environment.push(ProjectRunEnvironmentVariable {
                key: key.clone(),
                has_value: self.secrets.load(&account)?.is_some(),
            });
        }
        Ok((selection, environment))
    }

    pub(crate) fn launch_preferences(
        &self,
        owner_user_id: &str,
        project_id: &str,
    ) -> Result<ProjectRunLaunchPreferences, String> {
        let _guard = self.guard()?;
        let file = self.load_file()?;
        let selection = project_selection(&file, owner_user_id, project_id);
        let mut environment = BTreeMap::new();
        for (key, reference) in &selection.environment {
            let account = secret_account(owner_user_id, project_id, key, &reference.secret_id);
            let value = self
                .secrets
                .load(&account)?
                .ok_or_else(|| "项目环境变量的安全值不可用，请重新输入。".to_owned())?;
            environment.insert(key.clone(), value);
        }
        Ok(ProjectRunLaunchPreferences {
            selection,
            environment,
        })
    }

    pub(crate) fn set_default_target(
        &self,
        owner_user_id: &str,
        project_id: &str,
        target_id: String,
    ) -> Result<(), String> {
        validate_target_id(&target_id)?;
        let _guard = self.guard()?;
        let mut file = self.load_file()?;
        let mut selection = persisted_project_selection(&file, owner_user_id, project_id);
        selection.default_target_id = Some(target_id);
        insert_project_selection(&mut file, owner_user_id, project_id, selection)?;
        self.save_file(&file)
    }

    pub(crate) fn update_environment(
        &self,
        owner_user_id: &str,
        project_id: &str,
        selected_toolchains: BTreeMap<String, String>,
        mutations: Vec<ProjectRunEnvironmentMutation>,
    ) -> Result<(), String> {
        validate_selected_toolchains(&selected_toolchains)?;
        let normalized = normalize_environment_mutations(mutations)?;
        let _guard = self.guard()?;
        let mut file = self.load_file()?;
        self.cleanup_retired_secrets(&mut file);
        let previous = persisted_project_selection(&file, owner_user_id, project_id);
        let mut next_environment = BTreeMap::new();
        let mut pending = Vec::new();
        let mut pending_values = Vec::new();
        for mutation in normalized {
            if let Some(value) = mutation.value {
                let secret_id = Uuid::new_v4().to_string();
                let reference = EnvironmentSecretReference {
                    secret_id: secret_id.clone(),
                };
                pending.push(RetiredSecretReference {
                    owner_user_id: owner_user_id.to_owned(),
                    project_id: project_id.to_owned(),
                    key: mutation.key.clone(),
                    secret_id,
                });
                pending_values.push((mutation.key.clone(), reference.clone(), value));
                next_environment.insert(mutation.key, reference);
            } else {
                let reference = previous
                    .environment
                    .get(&mutation.key)
                    .cloned()
                    .ok_or_else(|| "新增环境变量时必须提供安全值。".to_owned())?;
                next_environment.insert(mutation.key, reference);
            }
        }
        if file.retired_secrets.len().saturating_add(pending.len()) > MAXIMUM_RETIRED_SECRETS {
            return Err("项目环境变量的待清理安全记录过多。".into());
        }

        file.retired_secrets.extend(pending.iter().cloned());
        self.save_file(&file)?;
        for (key, reference, value) in &pending_values {
            let account = secret_account(owner_user_id, project_id, key, &reference.secret_id);
            if let Err(error) = self.secrets.save(&account, value) {
                self.cleanup_retired_secrets(&mut file);
                let _ = self.save_file(&file);
                return Err(error);
            }
        }

        let next_ids = next_environment
            .iter()
            .map(|(key, reference)| (key.clone(), reference.secret_id.clone()))
            .collect::<BTreeSet<_>>();
        file.retired_secrets
            .retain(|reference| !pending.iter().any(|pending| pending == reference));
        for (key, reference) in &previous.environment {
            if !next_ids.contains(&(key.clone(), reference.secret_id.clone())) {
                file.retired_secrets.push(RetiredSecretReference {
                    owner_user_id: owner_user_id.to_owned(),
                    project_id: project_id.to_owned(),
                    key: key.clone(),
                    secret_id: reference.secret_id.clone(),
                });
            }
        }
        let mut next = previous;
        next.selected_toolchains = selected_toolchains;
        next.environment = next_environment;
        insert_project_selection(&mut file, owner_user_id, project_id, next)?;
        self.save_file(&file)?;
        self.cleanup_retired_secrets(&mut file);
        let _ = self.save_file(&file);
        Ok(())
    }

    pub(crate) fn remove_project(
        &self,
        owner_user_id: &str,
        project_id: &str,
    ) -> Result<(), String> {
        let _guard = self.guard()?;
        let mut file = self.load_file()?;
        self.cleanup_retired_secrets(&mut file);
        let removed = file
            .owners
            .get_mut(owner_user_id)
            .and_then(|owner| owner.projects.remove(project_id));
        let Some(removed) = removed else {
            let _ = self.save_file(&file);
            return Ok(());
        };
        if file
            .owners
            .get(owner_user_id)
            .is_some_and(|owner| owner.projects.is_empty())
        {
            file.owners.remove(owner_user_id);
        }
        if file
            .retired_secrets
            .len()
            .saturating_add(removed.environment.len())
            > MAXIMUM_RETIRED_SECRETS
        {
            return Err("项目环境变量的待清理安全记录过多。".into());
        }
        file.retired_secrets
            .extend(removed.environment.into_iter().map(|(key, reference)| {
                RetiredSecretReference {
                    owner_user_id: owner_user_id.to_owned(),
                    project_id: project_id.to_owned(),
                    key,
                    secret_id: reference.secret_id,
                }
            }));
        self.save_file(&file)?;
        self.cleanup_retired_secrets(&mut file);
        let _ = self.save_file(&file);
        Ok(())
    }

    fn cleanup_retired_secrets(&self, file: &mut PreferencesFile) {
        file.retired_secrets.retain(|reference| {
            let account = secret_account(
                &reference.owner_user_id,
                &reference.project_id,
                &reference.key,
                &reference.secret_id,
            );
            self.secrets.delete(&account).is_err()
        });
    }

    fn guard(&self) -> Result<MutexGuard<'_, ()>, String> {
        self.lock
            .lock()
            .map_err(|_| "项目运行偏好暂时不可用。".to_owned())
    }

    fn load_file(&self) -> Result<PreferencesFile, String> {
        let file = match File::open(&self.path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(PreferencesFile::default())
            }
            Err(_) => return Err(preferences_error()),
        };
        let metadata = file.metadata().map_err(|_| preferences_error())?;
        if !metadata.is_file() || metadata.len() > MAXIMUM_PREFERENCES_BYTES {
            return Err(preferences_error());
        }
        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        file.take(MAXIMUM_PREFERENCES_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| preferences_error())?;
        if bytes.len() as u64 > MAXIMUM_PREFERENCES_BYTES {
            return Err(preferences_error());
        }
        let parsed: PreferencesFile =
            serde_json::from_slice(&bytes).map_err(|_| preferences_error())?;
        validate_file(&parsed)?;
        Ok(parsed)
    }

    fn save_file(&self, file: &PreferencesFile) -> Result<(), String> {
        validate_file(file)?;
        let bytes = serde_json::to_vec(file).map_err(|_| preferences_error())?;
        if bytes.len() as u64 > MAXIMUM_PREFERENCES_BYTES {
            return Err(preferences_error());
        }
        let parent = self.path.parent().ok_or_else(preferences_error)?;
        fs::create_dir_all(parent).map_err(|_| preferences_error())?;
        let temporary = parent.join(format!(".preferences-{}.tmp", Uuid::new_v4()));
        let result =
            write_private_file(&temporary, &bytes).and_then(|_| fs::rename(&temporary, &self.path));
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
            return Err(preferences_error());
        }
        if let Ok(directory) = File::open(parent) {
            let _ = directory.sync_all();
        }
        Ok(())
    }

    #[cfg(test)]
    fn with_secrets(path: PathBuf, secrets: Arc<dyn ProjectRunSecretStore>) -> Self {
        Self {
            path,
            secrets,
            lock: Arc::new(Mutex::new(())),
        }
    }
}

fn project_selection(
    file: &PreferencesFile,
    owner_user_id: &str,
    project_id: &str,
) -> ProjectRunSelection {
    let persisted = persisted_project_selection(file, owner_user_id, project_id);
    ProjectRunSelection {
        default_target_id: persisted.default_target_id,
        selected_toolchains: persisted.selected_toolchains,
        environment: persisted.environment,
    }
}

fn persisted_project_selection(
    file: &PreferencesFile,
    owner_user_id: &str,
    project_id: &str,
) -> PersistedProjectRunSelection {
    file.owners
        .get(owner_user_id)
        .and_then(|owner| owner.projects.get(project_id))
        .cloned()
        .unwrap_or_default()
}

fn insert_project_selection(
    file: &mut PreferencesFile,
    owner_user_id: &str,
    project_id: &str,
    selection: PersistedProjectRunSelection,
) -> Result<(), String> {
    validate_identifier(owner_user_id)?;
    validate_identifier(project_id)?;
    let owner = file.owners.entry(owner_user_id.to_owned()).or_default();
    owner.projects.insert(project_id.to_owned(), selection);
    validate_file(file)
}

fn normalize_environment_mutations(
    mutations: Vec<ProjectRunEnvironmentMutation>,
) -> Result<Vec<ProjectRunEnvironmentMutation>, String> {
    if mutations.len() > MAXIMUM_ENVIRONMENT_VARIABLES {
        return Err("项目环境变量数量超过安全上限。".into());
    }
    let mut seen = BTreeSet::new();
    let mut total = 0_usize;
    let mut normalized = Vec::with_capacity(mutations.len());
    for mutation in mutations {
        validate_environment_key(&mutation.key)?;
        let identity = mutation.key.to_ascii_uppercase();
        if !seen.insert(identity) {
            return Err("项目环境变量名称重复。".into());
        }
        if let Some(value) = &mutation.value {
            if value.len() > MAXIMUM_ENVIRONMENT_VALUE_BYTES || value.contains('\0') {
                return Err("项目环境变量的安全值超过限制。".into());
            }
            total = total.saturating_add(value.len());
            if total > MAXIMUM_ENVIRONMENT_TOTAL_BYTES {
                return Err("项目环境变量的安全值总量超过限制。".into());
            }
        }
        normalized.push(mutation);
    }
    normalized.sort_by(|left, right| left.key.cmp(&right.key));
    Ok(normalized)
}

fn validate_file(file: &PreferencesFile) -> Result<(), String> {
    if file.version != CURRENT_VERSION
        || file.owners.len() > MAXIMUM_OWNERS
        || file.retired_secrets.len() > MAXIMUM_RETIRED_SECRETS
    {
        return Err(preferences_error());
    }
    for (owner_id, owner) in &file.owners {
        validate_identifier(owner_id)?;
        if owner.projects.len() > MAXIMUM_PROJECTS_PER_OWNER {
            return Err(preferences_error());
        }
        for (project_id, selection) in &owner.projects {
            validate_identifier(project_id)?;
            if let Some(target_id) = &selection.default_target_id {
                validate_target_id(target_id)?;
            }
            validate_selected_toolchains(&selection.selected_toolchains)?;
            if selection.environment.len() > MAXIMUM_ENVIRONMENT_VARIABLES {
                return Err(preferences_error());
            }
            for (key, reference) in &selection.environment {
                validate_environment_key(key)?;
                validate_secret_id(&reference.secret_id)?;
            }
        }
    }
    for reference in &file.retired_secrets {
        validate_identifier(&reference.owner_user_id)?;
        validate_identifier(&reference.project_id)?;
        validate_environment_key(&reference.key)?;
        validate_secret_id(&reference.secret_id)?;
    }
    Ok(())
}

fn validate_selected_toolchains(values: &BTreeMap<String, String>) -> Result<(), String> {
    if values.len() > MAXIMUM_TOOLCHAINS {
        return Err("项目工具链选择超过安全上限。".into());
    }
    for (kind, option_id) in values {
        if kind.is_empty()
            || kind.len() > 64
            || !kind
                .chars()
                .all(|value| value.is_ascii_lowercase() || value.is_ascii_digit() || value == '-')
            || !option_id.starts_with("tool_")
            || option_id.len() != 29
            || !option_id[5..]
                .chars()
                .all(|value| value.is_ascii_hexdigit())
        {
            return Err("项目工具链选择无效。".into());
        }
    }
    Ok(())
}

fn validate_target_id(value: &str) -> Result<(), String> {
    if value.len() != 20 || !value.chars().all(|character| character.is_ascii_hexdigit()) {
        Err("项目运行目标无效。".into())
    } else {
        Ok(())
    }
}

fn validate_environment_key(value: &str) -> Result<(), String> {
    let mut characters = value.chars();
    let first = characters
        .next()
        .filter(|value| value.is_ascii_alphabetic() || *value == '_');
    if value.len() > MAXIMUM_ENVIRONMENT_KEY_BYTES
        || first.is_none()
        || !characters.all(|value| value.is_ascii_alphanumeric() || value == '_')
    {
        Err("项目环境变量名称无效。".into())
    } else {
        Ok(())
    }
}

fn validate_identifier(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.trim() != value
        || value.len() > 1_024
        || value.chars().any(char::is_control)
    {
        Err(preferences_error())
    } else {
        Ok(())
    }
}

fn validate_secret_id(value: &str) -> Result<(), String> {
    Uuid::parse_str(value)
        .map(|_| ())
        .map_err(|_| preferences_error())
}

fn secret_account(owner_user_id: &str, project_id: &str, key: &str, secret_id: &str) -> String {
    let mut digest = Sha256::new();
    for value in [owner_user_id, project_id, key, secret_id] {
        digest.update(value.as_bytes());
        digest.update([0]);
    }
    format!("env-{:x}", digest.finalize())
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

fn preferences_error() -> String {
    "无法读取或保存项目运行偏好。".into()
}

fn safe_secret_error(error: impl Display) -> String {
    let _ = error;
    "无法访问系统凭据存储中的项目环境变量。".into()
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
impl ProjectRunSecretStore for PlatformProjectRunSecretStore {
    fn load(&self, account: &str) -> Result<Option<String>, String> {
        let entry = keyring::Entry::new(SECRET_SERVICE, account).map_err(safe_secret_error)?;
        match entry.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(safe_secret_error(error)),
        }
    }

    fn save(&self, account: &str, value: &str) -> Result<(), String> {
        keyring::Entry::new(SECRET_SERVICE, account)
            .map_err(safe_secret_error)?
            .set_password(value)
            .map_err(safe_secret_error)
    }

    fn delete(&self, account: &str) -> Result<(), String> {
        let entry = keyring::Entry::new(SECRET_SERVICE, account).map_err(safe_secret_error)?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(safe_secret_error(error)),
        }
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
impl ProjectRunSecretStore for PlatformProjectRunSecretStore {
    fn load(&self, _account: &str) -> Result<Option<String>, String> {
        Err("当前平台没有可用的系统凭据存储。".into())
    }

    fn save(&self, _account: &str, _value: &str) -> Result<(), String> {
        Err("当前平台没有可用的系统凭据存储。".into())
    }

    fn delete(&self, _account: &str) -> Result<(), String> {
        Err("当前平台没有可用的系统凭据存储。".into())
    }
}

#[cfg(test)]
pub(crate) mod tests_support {
    use super::*;

    #[derive(Default)]
    pub(crate) struct MemoryProjectRunSecretStore {
        values: Mutex<BTreeMap<String, String>>,
    }

    impl ProjectRunSecretStore for MemoryProjectRunSecretStore {
        fn load(&self, account: &str) -> Result<Option<String>, String> {
            Ok(self.values.lock().unwrap().get(account).cloned())
        }

        fn save(&self, account: &str, value: &str) -> Result<(), String> {
            self.values
                .lock()
                .unwrap()
                .insert(account.to_owned(), value.to_owned());
            Ok(())
        }

        fn delete(&self, account: &str) -> Result<(), String> {
            self.values.lock().unwrap().remove(account);
            Ok(())
        }
    }

    pub(crate) fn memory_preferences(path: PathBuf) -> ProjectRunPreferencesStore {
        ProjectRunPreferencesStore::with_secrets(
            path,
            Arc::new(MemoryProjectRunSecretStore::default()),
        )
    }
}
