use rusqlite::{params, Connection, OpenFlags, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Cursor,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use tauri::{http, State};

use crate::{
    authentication::{AuthenticationManager, GatewayModelConfig},
    host_lifecycle::validate_owner,
    media_studio::MediaStudioManager,
};

const DATABASE_FILENAME: &str = "AgentGroupChat.sqlite3";
const MAXIMUM_AVATAR_BYTES: i64 = 512 * 1_024;
const STAFF_HIRE: &str = "agent.staff.hire";
const STAFF_TERMINATE: &str = "agent.staff.terminate";
const LOCAL_PROJECT_LIST: &str = "local.project.list";
const LEGACY_PROJECT_STEWARD: &str = "builtin.project-steward";
const SKILL_CATALOG: &str =
    include_str!("../../../macos/Sources/ChatOSCore/AgentSkills/Catalog/ChatOSSkillCatalog.json");
const MODEL_REASONING_CAPABILITIES: &str = include_str!(
    "../../../macos/Sources/ChatOSModelCapabilities/Resources/ModelReasoningCapabilities.json"
);
const DEFAULT_AGENT_ROLE_PROMPT: &str =
    include_str!("../../../macos/Sources/ChatOSCore/Resources/AgentPrompt.Agent.DefaultRole.md");

const SCHEMA: &str = r#"
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;
CREATE TABLE IF NOT EXISTS local_agent_profiles (
    owner_user_id TEXT NOT NULL,
    id TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT NOT NULL,
    role_prompt TEXT NOT NULL,
    model_config_id TEXT NOT NULL,
    thinking_level TEXT,
    profession_key TEXT NOT NULL DEFAULT 'general_member',
    default_plugin_ids_json TEXT NOT NULL,
    default_skill_ids_json TEXT NOT NULL,
    heartbeat_enabled INTEGER NOT NULL DEFAULT 0 CHECK(heartbeat_enabled IN (0, 1)),
    heartbeat_interval_seconds INTEGER NOT NULL DEFAULT 900
        CHECK(heartbeat_interval_seconds BETWEEN 60 AND 86400),
    heartbeat_prompt TEXT NOT NULL DEFAULT '',
    last_heartbeat_at_unix_ms INTEGER,
    next_heartbeat_at_unix_ms INTEGER,
    avatar_data BLOB,
    status TEXT NOT NULL CHECK(status IN ('active', 'archived')),
    created_at_unix_ms INTEGER NOT NULL,
    updated_at_unix_ms INTEGER NOT NULL,
    PRIMARY KEY(owner_user_id, id)
);
"#;

const PROFILE_COLUMNS: &str = r#"
id, name, description, role_prompt, model_config_id, thinking_level,
profession_key, default_plugin_ids_json, default_skill_ids_json,
heartbeat_enabled, heartbeat_interval_seconds, heartbeat_prompt,
last_heartbeat_at_unix_ms, next_heartbeat_at_unix_ms,
length(avatar_data), created_at_unix_ms, updated_at_unix_ms
"#;

#[derive(Clone)]
pub(crate) struct AgentProfileStore {
    database: PathBuf,
    preview_session: Arc<Mutex<Option<AgentAvatarPreviewSession>>>,
}

#[derive(Clone)]
struct AgentAvatarPreviewSession {
    owner_user_id: String,
    authentication_generation: u64,
    token: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentProfileSummary {
    id: String,
    name: String,
    description: String,
    model_config_id: String,
    thinking_level: Option<String>,
    profession_key: String,
    heartbeat_enabled: bool,
    heartbeat_interval_seconds: i64,
    last_heartbeat_at_unix_ms: Option<i64>,
    next_heartbeat_at_unix_ms: Option<i64>,
    can_manage_staff: bool,
    can_access_local_projects: bool,
    has_avatar: bool,
    avatar_preview_url: Option<String>,
    created_at_unix_ms: i64,
    updated_at_unix_ms: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentProfileEditorCatalog {
    models: Vec<AgentModelOption>,
    professions: Vec<AgentProfessionOption>,
    default_role_prompt: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentProfileEditorData {
    profile: AgentProfileSummary,
    role_prompt: String,
    heartbeat_prompt: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AgentModelOption {
    id: String,
    display_name: String,
    model_name: String,
    provider: String,
    default_thinking_level: Option<String>,
    supports_reasoning: bool,
    thinking_levels: Vec<String>,
}

#[derive(Deserialize)]
struct ModelReasoningCapability {
    model: String,
    levels: Vec<String>,
}

#[derive(Deserialize)]
struct ModelReasoningCatalog {
    models: Vec<ModelReasoningCapability>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct AgentProfessionOption {
    key: String,
    label: String,
    description: String,
    #[serde(rename(deserialize = "category_key", serialize = "categoryKey"))]
    category_key: String,
    #[serde(rename(deserialize = "category_label", serialize = "categoryLabel"))]
    category_label: String,
}

#[derive(Deserialize)]
struct SkillCatalogPayload {
    professions: Vec<AgentProfessionOption>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
enum AgentAvatarMutationInput {
    Preserve,
    Remove,
    Staged {
        presentation_id: String,
        reference_id: String,
    },
    History {
        record_id: String,
        content_token: String,
    },
}

enum ResolvedAgentAvatar {
    Preserve,
    Remove,
    Replace(Vec<u8>),
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AgentProfileMutationInput {
    agent_id: Option<String>,
    expected_updated_at_unix_ms: Option<i64>,
    name: String,
    description: String,
    role_prompt: String,
    model_config_id: String,
    thinking_level: Option<String>,
    profession_key: String,
    can_manage_staff: bool,
    can_access_local_projects: bool,
    heartbeat_enabled: bool,
    heartbeat_interval_seconds: i64,
    heartbeat_prompt: String,
    avatar: AgentAvatarMutationInput,
}

impl AgentProfileStore {
    pub(crate) fn new(app_data_directory: PathBuf) -> Result<Self, String> {
        let connector = confined_connector_directory(&app_data_directory)?;
        let database = connector.join(DATABASE_FILENAME);
        reject_link_or_non_file(&database)?;
        let connection = open_database(&database, false)?;
        connection
            .execute_batch(SCHEMA)
            .map_err(|_| store_error())?;
        Ok(Self {
            database,
            preview_session: Arc::new(Mutex::new(None)),
        })
    }

    pub(crate) fn from_runtime() -> Result<Self, String> {
        let app_data_directory = dirs::data_local_dir()
            .ok_or_else(store_error)?
            .join("com.chatos.windows");
        Self::new(app_data_directory)
    }

    fn activate_preview_session(
        &self,
        owner_user_id: &str,
        authentication_generation: u64,
    ) -> Result<String, String> {
        let mut session = self.preview_session.lock().map_err(|_| store_error())?;
        if let Some(active) = session.as_ref() {
            if active.owner_user_id == owner_user_id
                && active.authentication_generation == authentication_generation
            {
                return Ok(active.token.clone());
            }
        }
        let token = uuid::Uuid::new_v4().to_string().to_lowercase();
        *session = Some(AgentAvatarPreviewSession {
            owner_user_id: owner_user_id.to_owned(),
            authentication_generation,
            token: token.clone(),
        });
        Ok(token)
    }

    fn present_profile(
        &self,
        mut profile: AgentProfileSummary,
        session_token: &str,
    ) -> AgentProfileSummary {
        if profile.has_avatar {
            profile.avatar_preview_url = Some(agent_avatar_preview_url(
                session_token,
                &profile.id,
                profile.updated_at_unix_ms,
            ));
        }
        profile
    }

    pub(crate) fn reset(&self) {
        if let Ok(mut session) = self.preview_session.lock() {
            *session = None;
        }
    }

    fn protocol_response(&self, request: http::Request<Vec<u8>>) -> http::Response<Vec<u8>> {
        match self.protocol_body(request.uri().path()) {
            Ok(body) => http::Response::builder()
                .status(http::StatusCode::OK)
                .header(http::header::CONTENT_TYPE, "image/png")
                .header("X-Content-Type-Options", "nosniff")
                .header("Cache-Control", "no-store")
                .body(body)
                .expect("valid Agent avatar response"),
            Err(status) => http::Response::builder()
                .status(status)
                .header(http::header::CONTENT_TYPE, "text/plain; charset=utf-8")
                .header("Cache-Control", "no-store")
                .body(b"Agent avatar unavailable".to_vec())
                .expect("valid Agent avatar error response"),
        }
    }

    fn protocol_body(&self, path: &str) -> Result<Vec<u8>, http::StatusCode> {
        let parts = path.trim_start_matches('/').split('/').collect::<Vec<_>>();
        if parts.len() != 3 || uuid::Uuid::parse_str(parts[0]).is_err() {
            return Err(http::StatusCode::BAD_REQUEST);
        }
        validate_agent_identifier(parts[1]).map_err(|_| http::StatusCode::BAD_REQUEST)?;
        let expected_revision = parts[2]
            .parse::<i64>()
            .ok()
            .filter(|value| *value >= 0)
            .ok_or(http::StatusCode::BAD_REQUEST)?;
        let owner_user_id = {
            let session = self
                .preview_session
                .lock()
                .map_err(|_| http::StatusCode::SERVICE_UNAVAILABLE)?;
            let active = session.as_ref().ok_or(http::StatusCode::UNAUTHORIZED)?;
            if active.token != parts[0] {
                return Err(http::StatusCode::UNAUTHORIZED);
            }
            active.owner_user_id.clone()
        };
        reject_link_or_non_file(&self.database)
            .map_err(|_| http::StatusCode::SERVICE_UNAVAILABLE)?;
        let connection = open_database(&self.database, true)
            .map_err(|_| http::StatusCode::SERVICE_UNAVAILABLE)?;
        let bytes: Vec<u8> = connection
            .query_row(
                "SELECT avatar_data FROM local_agent_profiles WHERE owner_user_id = ?1 AND id = ?2 AND status = 'active' AND updated_at_unix_ms = ?3 AND avatar_data IS NOT NULL",
                params![owner_user_id, parts[1], expected_revision],
                |row| row.get(0),
            )
            .map_err(|_| http::StatusCode::NOT_FOUND)?;
        validate_avatar_png(&bytes).map_err(|_| http::StatusCode::NOT_FOUND)?;
        Ok(bytes)
    }

    fn list_active(&self, owner_user_id: &str) -> Result<Vec<AgentProfileSummary>, String> {
        reject_link_or_non_file(&self.database)?;
        let connection = open_database(&self.database, true)?;
        let mut statement = connection
            .prepare(
                r#"
                SELECT id, name, description, role_prompt, model_config_id, thinking_level,
                       profession_key, default_plugin_ids_json, default_skill_ids_json,
                       heartbeat_enabled, heartbeat_interval_seconds, heartbeat_prompt,
                       last_heartbeat_at_unix_ms, next_heartbeat_at_unix_ms,
                       length(avatar_data), created_at_unix_ms, updated_at_unix_ms
                FROM local_agent_profiles
                WHERE owner_user_id = ?1 AND status = 'active'
                ORDER BY name, id
                "#,
            )
            .map_err(|_| store_error())?;
        let rows = statement
            .query_map(params![owner_user_id], map_profile)
            .map_err(|_| store_error())?;
        let mut profiles = Vec::new();
        for row in rows {
            profiles.push(row.map_err(|_| store_error())?);
        }
        Ok(profiles)
    }

    fn save(
        &self,
        owner_user_id: &str,
        input: AgentProfileMutationInput,
        avatar: ResolvedAgentAvatar,
    ) -> Result<AgentProfileSummary, String> {
        validate_mutation_input(&input)?;
        reject_link_or_non_file(&self.database)?;
        let mut connection = open_database(&self.database, false)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| store_error())?;
        let wall_clock_now = chrono::Utc::now().timestamp_millis().max(0);
        let profile_id;
        if let Some(agent_id) = input.agent_id.as_deref() {
            validate_agent_identifier(agent_id).map_err(|_| invalid_input())?;
            let expected = input
                .expected_updated_at_unix_ms
                .filter(|value| *value >= 0)
                .ok_or_else(invalid_input)?;
            let now = wall_clock_now.max(expected.saturating_add(1));
            let existing: Option<(String, bool, i64, Option<i64>)> = transaction
                .query_row(
                    "SELECT default_skill_ids_json, heartbeat_enabled, heartbeat_interval_seconds, next_heartbeat_at_unix_ms FROM local_agent_profiles WHERE owner_user_id = ?1 AND id = ?2 AND status = 'active' AND updated_at_unix_ms = ?3",
                    params![owner_user_id, agent_id, expected],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )
                .optional()
                .map_err(|_| store_error())?;
            let Some((skill_json, previous_enabled, previous_interval, previous_next)) = existing
            else {
                return Err("Agent 状态已经变化，请刷新后重试。".into());
            };
            let skills = normalized_permissions(
                parse_identifiers(skill_json).map_err(|_| store_error())?,
                input.can_manage_staff,
                input.can_access_local_projects,
            );
            let next_heartbeat = next_heartbeat_at(
                input.heartbeat_enabled,
                input.heartbeat_interval_seconds,
                previous_enabled,
                previous_interval,
                previous_next,
                now,
            );
            let (avatar_changed, avatar_data) = resolved_avatar_parameters(avatar);
            let changed = transaction
                .execute(
                    r#"
                UPDATE local_agent_profiles
                SET name = ?1, description = ?2, role_prompt = ?3, model_config_id = ?4,
                    thinking_level = ?5, profession_key = ?6, default_plugin_ids_json = '[]',
                    default_skill_ids_json = ?7, heartbeat_enabled = ?8,
                    heartbeat_interval_seconds = ?9, heartbeat_prompt = ?10,
                    next_heartbeat_at_unix_ms = ?11,
                    avatar_data = CASE WHEN ?12 = 0 THEN avatar_data ELSE ?13 END,
                    updated_at_unix_ms = ?14
                WHERE owner_user_id = ?15 AND id = ?16 AND status = 'active'
                  AND updated_at_unix_ms = ?17
                "#,
                    params![
                        input.name.trim(),
                        input.description.trim(),
                        input.role_prompt.trim(),
                        input.model_config_id.trim(),
                        normalized_optional(input.thinking_level.as_deref()),
                        input.profession_key,
                        serde_json::to_string(&skills).map_err(|_| store_error())?,
                        input.heartbeat_enabled,
                        input.heartbeat_interval_seconds,
                        input.heartbeat_prompt.trim(),
                        next_heartbeat,
                        avatar_changed,
                        avatar_data,
                        now,
                        owner_user_id,
                        agent_id,
                        expected,
                    ],
                )
                .map_err(|_| store_error())?;
            if changed != 1 {
                return Err("Agent 状态已经变化，请刷新后重试。".into());
            }
            profile_id = agent_id.to_owned();
        } else {
            if input.expected_updated_at_unix_ms.is_some() {
                return Err(invalid_input());
            }
            profile_id = uuid::Uuid::new_v4().to_string().to_lowercase();
            let now = wall_clock_now;
            let skills = normalized_permissions(
                Vec::new(),
                input.can_manage_staff,
                input.can_access_local_projects,
            );
            let next_heartbeat = next_heartbeat_at(
                input.heartbeat_enabled,
                input.heartbeat_interval_seconds,
                false,
                input.heartbeat_interval_seconds,
                None,
                now,
            );
            let (_, avatar_data) = resolved_avatar_parameters(avatar);
            transaction
                .execute(
                    r#"
                INSERT INTO local_agent_profiles (
                    owner_user_id, id, name, description, role_prompt, model_config_id,
                    thinking_level, profession_key, default_plugin_ids_json, default_skill_ids_json,
                    heartbeat_enabled, heartbeat_interval_seconds, heartbeat_prompt,
                    last_heartbeat_at_unix_ms, next_heartbeat_at_unix_ms, avatar_data,
                    status, created_at_unix_ms, updated_at_unix_ms
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, '[]', ?9, ?10, ?11, ?12,
                          NULL, ?13, ?14, 'active', ?15, ?15)
                "#,
                    params![
                        owner_user_id,
                        profile_id,
                        input.name.trim(),
                        input.description.trim(),
                        input.role_prompt.trim(),
                        input.model_config_id.trim(),
                        normalized_optional(input.thinking_level.as_deref()),
                        input.profession_key,
                        serde_json::to_string(&skills).map_err(|_| store_error())?,
                        input.heartbeat_enabled,
                        input.heartbeat_interval_seconds,
                        input.heartbeat_prompt.trim(),
                        next_heartbeat,
                        avatar_data,
                        now,
                    ],
                )
                .map_err(|_| store_error())?;
        }
        transaction.commit().map_err(|_| store_error())?;
        self.load_active(owner_user_id, &profile_id)
    }

    fn load_active(
        &self,
        owner_user_id: &str,
        agent_id: &str,
    ) -> Result<AgentProfileSummary, String> {
        let connection = open_database(&self.database, true)?;
        connection.query_row(
            &format!("SELECT {PROFILE_COLUMNS} FROM local_agent_profiles WHERE owner_user_id = ?1 AND id = ?2 AND status = 'active'"),
            params![owner_user_id, agent_id],
            map_profile,
        ).map_err(|_| store_error())
    }

    fn load_editor(
        &self,
        owner_user_id: &str,
        agent_id: &str,
    ) -> Result<AgentProfileEditorData, String> {
        validate_agent_identifier(agent_id).map_err(|_| invalid_input())?;
        reject_link_or_non_file(&self.database)?;
        let connection = open_database(&self.database, true)?;
        connection
            .query_row(
                &format!("SELECT {PROFILE_COLUMNS} FROM local_agent_profiles WHERE owner_user_id = ?1 AND id = ?2 AND status = 'active'"),
                params![owner_user_id, agent_id],
                |row| {
                    let role_prompt: String = row.get(3)?;
                    let heartbeat_prompt: String = row.get(11)?;
                    validate_text(&role_prompt, 32_000, false)?;
                    validate_text(&heartbeat_prompt, 8_000, true)?;
                    Ok(AgentProfileEditorData {
                        profile: map_profile(row)?,
                        role_prompt,
                        heartbeat_prompt,
                    })
                },
            )
            .map_err(|_| store_error())
    }

    #[cfg(test)]
    fn database(&self) -> &Path {
        &self.database
    }
}

#[tauri::command]
pub(crate) async fn list_agent_profiles(
    owner_user_id: String,
    authentication: State<'_, AuthenticationManager>,
    store: State<'_, AgentProfileStore>,
) -> Result<Vec<AgentProfileSummary>, String> {
    validate_owner(&owner_user_id)?;
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    let store = store.inner().clone();
    let query_store = store.clone();
    let owner = owner_user_id.clone();
    let profiles = tauri::async_runtime::spawn_blocking(move || query_store.list_active(&owner))
        .await
        .map_err(|_| "Agent 管理读取任务异常退出。".to_owned())??;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    let preview_session = store.activate_preview_session(&owner_user_id, generation)?;
    Ok(profiles
        .into_iter()
        .map(|profile| store.present_profile(profile, &preview_session))
        .collect())
}

#[tauri::command]
pub(crate) async fn load_agent_profile_editor_catalog(
    owner_user_id: String,
    authentication: State<'_, AuthenticationManager>,
) -> Result<AgentProfileEditorCatalog, String> {
    validate_owner(&owner_user_id)?;
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    let mut models: Vec<_> = authentication
        .model_catalog_for_owner(&owner_user_id)
        .await
        .map_err(|error| error.message)?
        .into_iter()
        .filter(|model| model.enabled && model.has_api_key)
        .map(|model| {
            let mut option = model_option_from_gateway(&model);
            if option.supports_reasoning
                && !option.thinking_levels.iter().any(|level| level == "auto")
            {
                option.thinking_levels.insert(0, "auto".into());
            }
            option
        })
        .collect();
    models.sort_by(|left, right| {
        left.display_name
            .to_lowercase()
            .cmp(&right.display_name.to_lowercase())
            .then_with(|| left.id.cmp(&right.id))
    });
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    let payload: SkillCatalogPayload =
        serde_json::from_str(SKILL_CATALOG).map_err(|_| "Agent 职业目录不可用。".to_owned())?;
    if payload.professions.len() != PROFESSION_KEYS.len() {
        return Err("Agent 职业目录不可用。".into());
    }
    Ok(AgentProfileEditorCatalog {
        models,
        professions: payload.professions,
        default_role_prompt: DEFAULT_AGENT_ROLE_PROMPT.trim().to_owned(),
    })
}

#[tauri::command]
pub(crate) async fn load_agent_profile_editor(
    owner_user_id: String,
    agent_id: String,
    authentication: State<'_, AuthenticationManager>,
    store: State<'_, AgentProfileStore>,
) -> Result<AgentProfileEditorData, String> {
    validate_owner(&owner_user_id)?;
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    let store = store.inner().clone();
    let query_store = store.clone();
    let owner = owner_user_id.clone();
    let mut editor =
        tauri::async_runtime::spawn_blocking(move || query_store.load_editor(&owner, &agent_id))
            .await
            .map_err(|_| "Agent 编辑资料读取任务异常退出。".to_owned())??;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    let preview_session = store.activate_preview_session(&owner_user_id, generation)?;
    editor.profile = store.present_profile(editor.profile, &preview_session);
    Ok(editor)
}

#[tauri::command]
pub(crate) async fn save_agent_profile(
    owner_user_id: String,
    input: AgentProfileMutationInput,
    authentication: State<'_, AuthenticationManager>,
    store: State<'_, AgentProfileStore>,
    media_studio: State<'_, MediaStudioManager>,
) -> Result<AgentProfileSummary, String> {
    validate_owner(&owner_user_id)?;
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    let models = authentication
        .model_catalog_for_owner(&owner_user_id)
        .await
        .map_err(|error| error.message)?;
    let selected = models
        .iter()
        .find(|model| {
            model.id == input.model_config_id.trim() && model.enabled && model.has_api_key
        })
        .ok_or_else(|| "所选 Agent 模型不可用，请刷新后重试。".to_owned())?;
    let option = model_option_from_gateway(selected);
    let thinking = normalized_optional(input.thinking_level.as_deref());
    if thinking.as_deref().is_some_and(|level| {
        level != "auto"
            && (!option.supports_reasoning || !option.thinking_levels.contains(&level.to_owned()))
    }) {
        return Err("Agent 思考等级无效，请重新选择。".into());
    }
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    let avatar_cleanup = match &input.avatar {
        AgentAvatarMutationInput::Staged {
            presentation_id, ..
        } => Some(presentation_id.clone()),
        _ => None,
    };
    let media = media_studio.inner().clone();
    let avatar_owner = owner_user_id.clone();
    let avatar_input = input.avatar.clone();
    let avatar = tauri::async_runtime::spawn_blocking(move || {
        resolve_avatar_mutation(&media, &avatar_owner, generation, &avatar_input)
    })
    .await
    .map_err(|_| "头像处理任务异常退出。".to_owned())??;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    let store = store.inner().clone();
    let save_store = store.clone();
    let owner = owner_user_id.clone();
    let saved =
        tauri::async_runtime::spawn_blocking(move || save_store.save(&owner, input, avatar))
            .await
            .map_err(|_| "Agent 保存任务异常退出。".to_owned())??;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    if let Some(presentation_id) = avatar_cleanup {
        let _ = media_studio.discard_agent_avatar_presentation(
            &owner_user_id,
            generation,
            &presentation_id,
        );
    }
    let preview_session = store.activate_preview_session(&owner_user_id, generation)?;
    Ok(store.present_profile(saved, &preview_session))
}

fn map_profile(row: &rusqlite::Row<'_>) -> rusqlite::Result<AgentProfileSummary> {
    let id: String = row.get(0)?;
    let name: String = row.get(1)?;
    let description: String = row.get(2)?;
    let role_prompt: String = row.get(3)?;
    let model_config_id: String = row.get(4)?;
    let thinking_level: Option<String> = row.get(5)?;
    let profession_key: String = row.get(6)?;
    let plugin_ids = parse_identifiers(row.get::<_, String>(7)?)?;
    let skill_ids = parse_identifiers(row.get::<_, String>(8)?)?;
    let heartbeat_enabled = integer_bool(row.get(9)?)?;
    let heartbeat_interval_seconds: i64 = row.get(10)?;
    let heartbeat_prompt: String = row.get(11)?;
    let last_heartbeat_at_unix_ms: Option<i64> = row.get(12)?;
    let next_heartbeat_at_unix_ms: Option<i64> = row.get(13)?;
    let avatar_bytes: Option<i64> = row.get(14)?;
    let created_at_unix_ms: i64 = row.get(15)?;
    let updated_at_unix_ms: i64 = row.get(16)?;

    validate_agent_identifier(&id)?;
    validate_text(&name, 120, false)?;
    validate_text(&description, 2_000, true)?;
    validate_text(&role_prompt, 32_000, false)?;
    validate_identifier(&model_config_id)?;
    if thinking_level.as_deref().is_some_and(|value| {
        !matches!(
            value,
            "auto" | "none" | "minimal" | "low" | "medium" | "high" | "xhigh" | "max"
        )
    }) {
        return Err(invalid_row());
    }
    if !PROFESSION_KEYS.contains(&profession_key.as_str()) {
        return Err(invalid_row());
    }
    if plugin_ids.len() > 100 || skill_ids.len() > 100 {
        return Err(invalid_row());
    }
    validate_text(&heartbeat_prompt, 8_000, true)?;
    if !(60..=86_400).contains(&heartbeat_interval_seconds)
        || last_heartbeat_at_unix_ms.is_some_and(|value| value < 0)
        || next_heartbeat_at_unix_ms.is_some_and(|value| value < 0)
        || avatar_bytes.is_some_and(|value| !(1..=MAXIMUM_AVATAR_BYTES).contains(&value))
        || created_at_unix_ms < 0
        || updated_at_unix_ms < created_at_unix_ms
    {
        return Err(invalid_row());
    }

    let has_legacy_permission = skill_ids
        .iter()
        .any(|value| value == LEGACY_PROJECT_STEWARD);
    let can_manage_staff = has_legacy_permission
        || (skill_ids.iter().any(|value| value == STAFF_HIRE)
            && skill_ids.iter().any(|value| value == STAFF_TERMINATE));
    let can_access_local_projects =
        has_legacy_permission || skill_ids.iter().any(|value| value == LOCAL_PROJECT_LIST);

    Ok(AgentProfileSummary {
        id,
        name,
        description,
        model_config_id,
        thinking_level,
        profession_key,
        heartbeat_enabled,
        heartbeat_interval_seconds,
        last_heartbeat_at_unix_ms,
        next_heartbeat_at_unix_ms,
        can_manage_staff,
        can_access_local_projects,
        has_avatar: avatar_bytes.is_some(),
        avatar_preview_url: None,
        created_at_unix_ms,
        updated_at_unix_ms,
    })
}

fn resolve_avatar_mutation(
    media_studio: &MediaStudioManager,
    owner_user_id: &str,
    authentication_generation: u64,
    input: &AgentAvatarMutationInput,
) -> Result<ResolvedAgentAvatar, String> {
    match input {
        AgentAvatarMutationInput::Preserve => Ok(ResolvedAgentAvatar::Preserve),
        AgentAvatarMutationInput::Remove => Ok(ResolvedAgentAvatar::Remove),
        AgentAvatarMutationInput::Staged {
            presentation_id,
            reference_id,
        } => {
            let bytes = media_studio.read_agent_avatar_reference(
                owner_user_id,
                authentication_generation,
                presentation_id,
                reference_id,
            )?;
            normalize_avatar(bytes).map(ResolvedAgentAvatar::Replace)
        }
        AgentAvatarMutationInput::History {
            record_id,
            content_token,
        } => normalize_avatar(media_studio.read_story_history_image(
            owner_user_id,
            record_id,
            content_token,
        )?)
        .map(ResolvedAgentAvatar::Replace),
    }
}

fn normalize_avatar(bytes: Vec<u8>) -> Result<Vec<u8>, String> {
    let format = image::guess_format(&bytes).map_err(|_| invalid_avatar())?;
    if !matches!(
        format,
        image::ImageFormat::Png | image::ImageFormat::Jpeg | image::ImageFormat::WebP
    ) {
        return Err(invalid_avatar());
    }
    let decoded =
        image::load_from_memory_with_format(&bytes, format).map_err(|_| invalid_avatar())?;
    let side = decoded.width().min(decoded.height());
    if side == 0 {
        return Err(invalid_avatar());
    }
    let cropped = decoded.crop_imm(
        (decoded.width() - side) / 2,
        (decoded.height() - side) / 2,
        side,
        side,
    );
    let normalized = cropped.resize_exact(256, 256, image::imageops::FilterType::Lanczos3);
    let mut output = Cursor::new(Vec::new());
    normalized
        .write_to(&mut output, image::ImageFormat::Png)
        .map_err(|_| invalid_avatar())?;
    let output = output.into_inner();
    if output.is_empty() || output.len() > MAXIMUM_AVATAR_BYTES as usize {
        return Err(invalid_avatar());
    }
    Ok(output)
}

fn validate_avatar_png(bytes: &[u8]) -> Result<(), String> {
    if bytes.is_empty() || bytes.len() > MAXIMUM_AVATAR_BYTES as usize {
        return Err(invalid_avatar());
    }
    let format = image::guess_format(bytes).map_err(|_| invalid_avatar())?;
    if format != image::ImageFormat::Png {
        return Err(invalid_avatar());
    }
    let (width, height) = image::ImageReader::with_format(Cursor::new(bytes), format)
        .into_dimensions()
        .map_err(|_| invalid_avatar())?;
    if width != 256 || height != 256 {
        return Err(invalid_avatar());
    }
    Ok(())
}

fn resolved_avatar_parameters(avatar: ResolvedAgentAvatar) -> (i64, Option<Vec<u8>>) {
    match avatar {
        ResolvedAgentAvatar::Preserve => (0, None),
        ResolvedAgentAvatar::Remove => (1, None),
        ResolvedAgentAvatar::Replace(bytes) => (1, Some(bytes)),
    }
}

fn invalid_avatar() -> String {
    "头像不是有效图片或处理后超过 512 KB。".into()
}

fn agent_avatar_preview_url(session_token: &str, agent_id: &str, revision: i64) -> String {
    if cfg!(target_os = "windows") {
        format!("http://chatos-agent.localhost/{session_token}/{agent_id}/{revision}")
    } else {
        format!("chatos-agent://localhost/{session_token}/{agent_id}/{revision}")
    }
}

pub(crate) fn agent_avatar_protocol(
    store: AgentProfileStore,
    request: http::Request<Vec<u8>>,
) -> http::Response<Vec<u8>> {
    store.protocol_response(request)
}

fn parse_identifiers(json: String) -> rusqlite::Result<Vec<String>> {
    let values: Vec<String> = serde_json::from_str(&json).map_err(|_| invalid_row())?;
    if values.len() > 100 {
        return Err(invalid_row());
    }
    let mut unique = std::collections::HashSet::new();
    for value in &values {
        validate_identifier(value)?;
        if !unique.insert(value) {
            return Err(invalid_row());
        }
    }
    Ok(values)
}

fn integer_bool(value: i64) -> rusqlite::Result<bool> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(invalid_row()),
    }
}

fn validate_identifier(value: &str) -> rusqlite::Result<()> {
    if value.is_empty()
        || value.chars().count() > 512
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        return Err(invalid_row());
    }
    Ok(())
}

fn validate_agent_identifier(value: &str) -> rusqlite::Result<()> {
    validate_identifier(value)?;
    if value.chars().any(|character| {
        !(character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.'))
    }) {
        return Err(invalid_row());
    }
    Ok(())
}

fn validate_text(value: &str, maximum: usize, optional: bool) -> rusqlite::Result<()> {
    if (!optional && value.is_empty())
        || value.chars().count() > maximum
        || value.trim() != value
        || value.contains('\0')
    {
        return Err(invalid_row());
    }
    Ok(())
}

fn validate_mutation_input(input: &AgentProfileMutationInput) -> Result<(), String> {
    validate_text(input.name.trim(), 120, false).map_err(|_| invalid_input())?;
    validate_text(input.description.trim(), 2_000, true).map_err(|_| invalid_input())?;
    validate_text(input.role_prompt.trim(), 32_000, false).map_err(|_| invalid_input())?;
    validate_identifier(input.model_config_id.trim()).map_err(|_| invalid_input())?;
    if let Some(thinking) = normalized_optional(input.thinking_level.as_deref()) {
        if !matches!(
            thinking.as_str(),
            "auto" | "none" | "minimal" | "low" | "medium" | "high" | "xhigh" | "max"
        ) {
            return Err(invalid_input());
        }
    }
    if !PROFESSION_KEYS.contains(&input.profession_key.as_str())
        || !(60..=86_400).contains(&input.heartbeat_interval_seconds)
    {
        return Err(invalid_input());
    }
    validate_text(input.heartbeat_prompt.trim(), 8_000, true).map_err(|_| invalid_input())?;
    Ok(())
}

fn model_option_from_gateway(model: &GatewayModelConfig) -> AgentModelOption {
    let thinking_levels = if model.supports_reasoning {
        agent_thinking_levels(&model.provider, &model.model)
    } else {
        Vec::new()
    };
    let default_thinking_level = model
        .task_thinking_level
        .as_deref()
        .map(str::trim)
        .filter(|value| thinking_levels.iter().any(|candidate| candidate == *value))
        .map(str::to_owned);
    AgentModelOption {
        id: model.id.clone(),
        display_name: if model.name.trim().is_empty() {
            model.model.clone()
        } else {
            model.name.trim().to_owned()
        },
        model_name: model.model.clone(),
        provider: model.provider.clone(),
        default_thinking_level,
        supports_reasoning: model.supports_reasoning,
        thinking_levels,
    }
}

fn agent_thinking_levels(provider: &str, model: &str) -> Vec<String> {
    static CATALOG: std::sync::OnceLock<ModelReasoningCatalog> = std::sync::OnceLock::new();
    let catalog = CATALOG.get_or_init(|| {
        serde_json::from_str(MODEL_REASONING_CAPABILITIES)
            .expect("shared model reasoning capabilities must be valid")
    });
    let normalized_model = model.trim().to_ascii_lowercase();
    if let Some(capability) = catalog.models.iter().find(|capability| {
        normalized_model == capability.model
            || normalized_model
                .strip_prefix(&format!("{}-", capability.model))
                .is_some_and(|suffix| {
                    suffix.len() == 10
                        && suffix
                            .chars()
                            .all(|value| value.is_ascii_digit() || value == '-')
                })
    }) {
        return std::iter::once("auto".to_owned())
            .chain(capability.levels.iter().cloned())
            .collect();
    }
    let provider = provider.trim().to_ascii_lowercase().replace('-', "_");
    let values: &[&str] = match provider.as_str() {
        "deepseek" => &["auto", "none", "high", "max"],
        "kimi" | "kimik2" | "moonshot" => &["auto", "none"],
        "glm" | "zhipu" | "zai" => &["auto", "none", "low", "medium", "high", "xhigh"],
        _ => &["auto", "none", "minimal", "low", "medium", "high", "xhigh"],
    };
    values.iter().map(|value| (*value).to_owned()).collect()
}

fn normalized_optional(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn normalized_permissions(
    permissions: Vec<String>,
    can_manage_staff: bool,
    can_access_local_projects: bool,
) -> Vec<String> {
    let mut values: std::collections::BTreeSet<_> = permissions.into_iter().collect();
    values.remove(LEGACY_PROJECT_STEWARD);
    values.remove(STAFF_HIRE);
    values.remove(STAFF_TERMINATE);
    values.remove(LOCAL_PROJECT_LIST);
    if can_manage_staff {
        values.insert(STAFF_HIRE.into());
        values.insert(STAFF_TERMINATE.into());
    }
    if can_access_local_projects {
        values.insert(LOCAL_PROJECT_LIST.into());
    }
    values.into_iter().collect()
}

fn next_heartbeat_at(
    enabled: bool,
    interval_seconds: i64,
    previous_enabled: bool,
    previous_interval_seconds: i64,
    previous_next: Option<i64>,
    now: i64,
) -> Option<i64> {
    enabled.then(|| {
        if !previous_enabled || interval_seconds != previous_interval_seconds {
            now.saturating_add(interval_seconds.saturating_mul(1_000))
        } else {
            previous_next
                .unwrap_or_else(|| now.saturating_add(interval_seconds.saturating_mul(1_000)))
        }
    })
}

fn invalid_input() -> String {
    "Agent 资料无效，请检查后重试。".into()
}

fn open_database(path: &Path, read_only: bool) -> Result<Connection, String> {
    let flags = if read_only {
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX
    } else {
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
    };
    let connection = Connection::open_with_flags(path, flags).map_err(|_| store_error())?;
    connection
        .busy_timeout(std::time::Duration::from_secs(2))
        .map_err(|_| store_error())?;
    Ok(connection)
}

fn confined_connector_directory(app_data_directory: &Path) -> Result<PathBuf, String> {
    fs::create_dir_all(app_data_directory).map_err(|_| store_error())?;
    let app_data = fs::canonicalize(app_data_directory).map_err(|_| store_error())?;
    let requested = app_data.join("NativeConnector");
    fs::create_dir_all(&requested).map_err(|_| store_error())?;
    let connector = fs::canonicalize(requested).map_err(|_| store_error())?;
    if !connector.starts_with(&app_data) {
        return Err(store_error());
    }
    Ok(connector)
}

fn reject_link_or_non_file(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            Err(store_error())
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(store_error()),
    }
}

fn invalid_row() -> rusqlite::Error {
    rusqlite::Error::InvalidQuery
}

fn store_error() -> String {
    "Agent 管理数据不可用，请检查本机存储后重试。".into()
}

const PROFESSION_KEYS: &[&str] = &[
    "project_manager",
    "product_manager",
    "technical_manager",
    "solution_architect",
    "security_engineer",
    "qa_engineer",
    "software_engineer",
    "fullstack_engineer",
    "frontend_engineer",
    "backend_engineer",
    "mobile_engineer",
    "desktop_engineer",
    "game_engineer",
    "embedded_iot_engineer",
    "database_engineer",
    "devops_engineer",
    "data_engineer",
    "data_analyst",
    "machine_learning_engineer",
    "research_specialist",
    "product_designer",
    "ui_designer",
    "ux_designer",
    "game_designer",
    "technical_writer",
    "business_analyst",
    "implementation_consultant",
    "erp_consultant",
    "wms_consultant",
    "domain_expert",
    "operations_specialist",
    "growth_marketing_specialist",
    "general_member",
];

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, AgentProfileStore) {
        let directory = tempfile::tempdir().unwrap();
        let store = AgentProfileStore::new(directory.path().to_path_buf()).unwrap();
        (directory, store)
    }

    fn insert(connection: &Connection, owner: &str, id: &str, name: &str, status: &str) {
        connection
            .execute(
                r#"
            INSERT INTO local_agent_profiles (
                owner_user_id, id, name, description, role_prompt, model_config_id,
                thinking_level, profession_key, default_plugin_ids_json, default_skill_ids_json,
                heartbeat_enabled, heartbeat_interval_seconds, heartbeat_prompt,
                last_heartbeat_at_unix_ms, next_heartbeat_at_unix_ms, avatar_data,
                status, created_at_unix_ms, updated_at_unix_ms
            ) VALUES (?1, ?2, ?3, '说明', '内部角色提示', 'model-1', 'high', 'general_member',
                      '[]', '["agent.staff.hire","agent.staff.terminate","local.project.list"]',
                      1, 900, '检查未读消息', 100, 200, X'89504E47', ?4, 10, 20)
            "#,
                params![owner, id, name, status],
            )
            .unwrap();
    }

    #[test]
    fn lists_only_active_owner_profiles_in_macos_order() {
        let (_directory, store) = store();
        let connection = Connection::open(store.database()).unwrap();
        insert(&connection, "alice", "b", "乙", "active");
        insert(&connection, "alice", "a", "甲", "active");
        insert(&connection, "alice", "archived", "丙", "archived");
        insert(&connection, "bob", "foreign", "零", "active");

        let profiles = store.list_active("alice").unwrap();
        assert_eq!(
            profiles
                .iter()
                .map(|profile| profile.id.as_str())
                .collect::<Vec<_>>(),
            vec!["b", "a"]
        );
        assert!(profiles[0].can_manage_staff);
        assert!(profiles[0].can_access_local_projects);
        assert!(profiles[0].has_avatar);
    }

    #[test]
    fn projection_does_not_serialize_prompt_permissions_or_avatar_bytes() {
        let (_directory, store) = store();
        let connection = Connection::open(store.database()).unwrap();
        insert(&connection, "alice", "agent", "Agent", "active");

        let value = serde_json::to_value(&store.list_active("alice").unwrap()[0]).unwrap();
        assert!(value.get("rolePrompt").is_none());
        assert!(value.get("defaultSkillIds").is_none());
        assert!(value.get("avatarData").is_none());
        assert_eq!(value["hasAvatar"], true);
    }

    #[test]
    fn editor_projection_returns_private_prompts_only_for_the_requested_owner_profile() {
        let (_directory, store) = store();
        let connection = Connection::open(store.database()).unwrap();
        insert(&connection, "alice", "agent", "Agent", "active");
        insert(&connection, "bob", "agent", "Foreign", "active");

        let editor = store.load_editor("alice", "agent").unwrap();
        assert_eq!(editor.profile.name, "Agent");
        assert_eq!(editor.role_prompt, "内部角色提示");
        assert_eq!(editor.heartbeat_prompt, "检查未读消息");
        assert!(store.load_editor("alice", "missing").is_err());
    }

    #[test]
    fn malformed_profile_fails_closed_instead_of_becoming_partial_ui() {
        let (_directory, store) = store();
        let connection = Connection::open(store.database()).unwrap();
        insert(&connection, "alice", "agent", "Agent", "active");
        connection.execute(
            "UPDATE local_agent_profiles SET default_skill_ids_json = '[\"duplicate\",\"duplicate\"]' WHERE owner_user_id = 'alice'",
            [],
        ).unwrap();

        assert_eq!(store.list_active("alice").unwrap_err(), store_error());
    }

    #[test]
    fn creates_and_updates_profiles_with_macos_permission_and_heartbeat_rules() {
        let (_directory, store) = store();
        let created = store
            .save("alice", mutation(), ResolvedAgentAvatar::Preserve)
            .unwrap();
        assert!(created.can_manage_staff);
        assert!(created.can_access_local_projects);
        assert!(created.next_heartbeat_at_unix_ms.is_some());

        let connection = Connection::open(store.database()).unwrap();
        connection.execute(
            "UPDATE local_agent_profiles SET default_skill_ids_json = '[\"custom.skill\",\"builtin.project-steward\"]' WHERE owner_user_id = 'alice' AND id = ?1",
            params![created.id],
        ).unwrap();
        let mut update = mutation();
        update.agent_id = Some(created.id.clone());
        update.expected_updated_at_unix_ms = Some(created.updated_at_unix_ms);
        update.can_manage_staff = false;
        update.can_access_local_projects = true;
        update.heartbeat_interval_seconds = 300;
        let saved = store
            .save("alice", update, ResolvedAgentAvatar::Preserve)
            .unwrap();
        assert!(!saved.can_manage_staff);
        assert!(saved.can_access_local_projects);

        let skills: String = connection.query_row(
            "SELECT default_skill_ids_json FROM local_agent_profiles WHERE owner_user_id = 'alice' AND id = ?1",
            params![created.id],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(skills, r#"["custom.skill","local.project.list"]"#);
        assert_ne!(
            saved.next_heartbeat_at_unix_ms,
            created.next_heartbeat_at_unix_ms
        );
    }

    #[test]
    fn rejects_stale_profile_updates() {
        let (_directory, store) = store();
        let created = store
            .save("alice", mutation(), ResolvedAgentAvatar::Preserve)
            .unwrap();
        let mut update = mutation();
        update.agent_id = Some(created.id);
        update.expected_updated_at_unix_ms = Some(created.updated_at_unix_ms.saturating_sub(1));
        assert_eq!(
            store
                .save("alice", update, ResolvedAgentAvatar::Preserve)
                .unwrap_err(),
            "Agent 状态已经变化，请刷新后重试。"
        );
    }

    #[test]
    fn bundled_profession_catalog_matches_the_macos_contract() {
        let payload: SkillCatalogPayload = serde_json::from_str(SKILL_CATALOG).unwrap();
        assert_eq!(payload.professions.len(), 33);
        assert_eq!(payload.professions.first().unwrap().key, "project_manager");
        assert_eq!(payload.professions.last().unwrap().key, "general_member");
    }

    #[test]
    fn avatar_replacement_is_normalized_and_removal_is_explicit() {
        let (_directory, store) = store();
        let source = image::RgbImage::from_pixel(400, 200, image::Rgb([20, 40, 60]));
        let mut encoded = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(source)
            .write_to(&mut encoded, image::ImageFormat::Jpeg)
            .unwrap();
        let avatar = normalize_avatar(encoded.into_inner()).unwrap();
        let created = store
            .save(
                "alice",
                mutation(),
                ResolvedAgentAvatar::Replace(avatar.clone()),
            )
            .unwrap();
        assert!(created.has_avatar);

        let connection = Connection::open(store.database()).unwrap();
        let stored: Vec<u8> = connection
            .query_row(
                "SELECT avatar_data FROM local_agent_profiles WHERE owner_user_id = 'alice' AND id = ?1",
                params![created.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(stored, avatar);
        assert_eq!(image::load_from_memory(&stored).unwrap().width(), 256);

        let mut preserve = mutation();
        preserve.agent_id = Some(created.id.clone());
        preserve.expected_updated_at_unix_ms = Some(created.updated_at_unix_ms);
        let preserved = store
            .save("alice", preserve, ResolvedAgentAvatar::Preserve)
            .unwrap();
        assert!(preserved.has_avatar);
        assert!(preserved.updated_at_unix_ms > created.updated_at_unix_ms);

        let mut update = mutation();
        update.agent_id = Some(created.id);
        update.expected_updated_at_unix_ms = Some(preserved.updated_at_unix_ms);
        let removed = store
            .save("alice", update, ResolvedAgentAvatar::Remove)
            .unwrap();
        assert!(!removed.has_avatar);
    }

    #[test]
    fn avatar_preview_is_owner_revision_and_session_scoped() {
        let (_directory, store) = store();
        let source = image::RgbImage::from_pixel(12, 18, image::Rgb([70, 80, 90]));
        let mut encoded = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(source)
            .write_to(&mut encoded, image::ImageFormat::Png)
            .unwrap();
        let avatar = normalize_avatar(encoded.into_inner()).unwrap();
        let alice = store
            .save(
                "alice",
                mutation(),
                ResolvedAgentAvatar::Replace(avatar.clone()),
            )
            .unwrap();
        let connection = Connection::open(store.database()).unwrap();
        insert(&connection, "bob", &alice.id, "Foreign", "active");

        let session = store.activate_preview_session("alice", 7).unwrap();
        let path = format!("/{session}/{}/{}", alice.id, alice.updated_at_unix_ms);
        assert_eq!(store.protocol_body(&path).unwrap(), avatar);
        assert_eq!(
            store
                .protocol_body(&format!(
                    "/{session}/{}/{}",
                    alice.id,
                    alice.updated_at_unix_ms.saturating_add(1)
                ))
                .unwrap_err(),
            http::StatusCode::NOT_FOUND
        );

        store.reset();
        assert_eq!(
            store.protocol_body(&path).unwrap_err(),
            http::StatusCode::UNAUTHORIZED
        );
    }

    fn mutation() -> AgentProfileMutationInput {
        AgentProfileMutationInput {
            agent_id: None,
            expected_updated_at_unix_ms: None,
            name: "Agent".into(),
            description: "说明".into(),
            role_prompt: "负责完成分配的工作".into(),
            model_config_id: "model-1".into(),
            thinking_level: Some("high".into()),
            profession_key: "general_member".into(),
            can_manage_staff: true,
            can_access_local_projects: true,
            heartbeat_enabled: true,
            heartbeat_interval_seconds: 900,
            heartbeat_prompt: "检查未读消息".into(),
            avatar: AgentAvatarMutationInput::Preserve,
        }
    }
}
