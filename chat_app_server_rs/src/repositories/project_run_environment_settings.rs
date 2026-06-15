use mongodb::bson::{doc, Bson, Document};
use sqlx::Row;

use crate::core::values::optional_string_bson;
use crate::models::project_run_environment::{
    ProjectRunCustomToolchain, ProjectRunEnvironmentSelection,
};
use crate::repositories::db::{to_doc, with_db};

fn normalize_doc(doc: &Document) -> Option<ProjectRunEnvironmentSelection> {
    Some(ProjectRunEnvironmentSelection {
        project_id: doc.get_str("project_id").ok()?.to_string(),
        user_id: doc.get_str("user_id").ok().map(|v| v.to_string()),
        selected_toolchains: doc
            .get("selected_toolchains")
            .cloned()
            .and_then(|value| mongodb::bson::from_bson(value).ok())
            .unwrap_or_default(),
        custom_toolchains: doc
            .get("custom_toolchains")
            .cloned()
            .and_then(|value| mongodb::bson::from_bson(value).ok())
            .unwrap_or_default(),
        env_vars: doc
            .get("env_vars")
            .cloned()
            .and_then(|value| mongodb::bson::from_bson(value).ok())
            .unwrap_or_default(),
        terminal_ui_enabled: doc.get_bool("terminal_ui_enabled").unwrap_or(true),
        updated_at: doc.get_str("updated_at").unwrap_or("").to_string(),
    })
}

pub async fn get_by_project_id(
    project_id: &str,
) -> Result<Option<ProjectRunEnvironmentSelection>, String> {
    with_db(
        |db| {
            let project_id = project_id.to_string();
            Box::pin(async move {
                let doc = db
                    .collection::<Document>("project_run_environment_settings")
                    .find_one(doc! { "project_id": &project_id }, None)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(doc.as_ref().and_then(normalize_doc))
            })
        },
        |pool| {
            let project_id = project_id.to_string();
            Box::pin(async move {
                let row = sqlx::query(
                    "SELECT project_id, user_id, selected_toolchains_json, custom_toolchains_json, env_vars_json, terminal_ui_enabled, updated_at \
                     FROM project_run_environment_settings WHERE project_id = ?",
                )
                .bind(&project_id)
                .fetch_optional(pool)
                .await
                .map_err(|e| e.to_string())?;
                let Some(row) = row else {
                    return Ok(None);
                };
                let selected_toolchains_json: String = row
                    .try_get("selected_toolchains_json")
                    .unwrap_or_else(|_| "{}".to_string());
                let custom_toolchains_json: String = row
                    .try_get("custom_toolchains_json")
                    .unwrap_or_else(|_| "{}".to_string());
                let env_vars_json: String = row
                    .try_get("env_vars_json")
                    .unwrap_or_else(|_| "{}".to_string());
                Ok(Some(ProjectRunEnvironmentSelection {
                    project_id: row.try_get::<String, _>("project_id").unwrap_or_default(),
                    user_id: row.try_get::<Option<String>, _>("user_id").unwrap_or(None),
                    selected_toolchains: serde_json::from_str(&selected_toolchains_json)
                        .unwrap_or_default(),
                    custom_toolchains: serde_json::from_str::<
                        std::collections::HashMap<String, ProjectRunCustomToolchain>,
                    >(&custom_toolchains_json)
                    .unwrap_or_default(),
                    env_vars: serde_json::from_str(&env_vars_json).unwrap_or_default(),
                    terminal_ui_enabled: row
                        .try_get::<i64, _>("terminal_ui_enabled")
                        .unwrap_or(1)
                        != 0,
                    updated_at: row.try_get::<String, _>("updated_at").unwrap_or_default(),
                }))
            })
        },
    )
    .await
}

pub async fn upsert(
    selection: &ProjectRunEnvironmentSelection,
) -> Result<ProjectRunEnvironmentSelection, String> {
    let selected_toolchains_json =
        serde_json::to_string(&selection.selected_toolchains).map_err(|e| e.to_string())?;
    let custom_toolchains_json =
        serde_json::to_string(&selection.custom_toolchains).map_err(|e| e.to_string())?;
    let env_vars_json = serde_json::to_string(&selection.env_vars).map_err(|e| e.to_string())?;

    with_db(
        |db| {
            let selection = selection.clone();
            let selected_toolchains_json = selected_toolchains_json.clone();
            let custom_toolchains_json = custom_toolchains_json.clone();
            let env_vars_json = env_vars_json.clone();
            Box::pin(async move {
                let selected_toolchains_bson =
                    mongodb::bson::to_bson(&selection.selected_toolchains)
                        .unwrap_or(Bson::Document(Document::new()));
                let custom_toolchains_bson = mongodb::bson::to_bson(&selection.custom_toolchains)
                    .unwrap_or(Bson::Document(Document::new()));
                let env_vars_bson = mongodb::bson::to_bson(&selection.env_vars)
                    .unwrap_or(Bson::Document(Document::new()));
                let mut set_doc = Document::new();
                set_doc.insert("project_id", selection.project_id.clone());
                set_doc.insert("user_id", optional_string_bson(selection.user_id.clone()));
                set_doc.insert("selected_toolchains", selected_toolchains_bson);
                set_doc.insert("selected_toolchains_json", selected_toolchains_json);
                set_doc.insert("custom_toolchains", custom_toolchains_bson);
                set_doc.insert("custom_toolchains_json", custom_toolchains_json);
                set_doc.insert("env_vars", env_vars_bson);
                set_doc.insert("env_vars_json", env_vars_json);
                set_doc.insert("terminal_ui_enabled", selection.terminal_ui_enabled);
                set_doc.insert("updated_at", selection.updated_at.clone());

                db.collection::<Document>("project_run_environment_settings")
                    .update_one(
                        doc! { "project_id": &selection.project_id },
                        doc! { "$set": to_doc(set_doc) },
                        mongodb::options::UpdateOptions::builder()
                            .upsert(true)
                            .build(),
                    )
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(selection)
            })
        },
        |pool| {
            let selection = selection.clone();
            let selected_toolchains_json = selected_toolchains_json.clone();
            let custom_toolchains_json = custom_toolchains_json.clone();
            let env_vars_json = env_vars_json.clone();
            Box::pin(async move {
                sqlx::query(
                    "INSERT INTO project_run_environment_settings \
                    (project_id, user_id, selected_toolchains_json, custom_toolchains_json, env_vars_json, terminal_ui_enabled, updated_at) \
                    VALUES (?, ?, ?, ?, ?, ?, ?) \
                    ON CONFLICT(project_id) DO UPDATE SET \
                    user_id=excluded.user_id, \
                    selected_toolchains_json=excluded.selected_toolchains_json, \
                    custom_toolchains_json=excluded.custom_toolchains_json, \
                    env_vars_json=excluded.env_vars_json, \
                    terminal_ui_enabled=excluded.terminal_ui_enabled, \
                    updated_at=excluded.updated_at",
                )
                .bind(&selection.project_id)
                .bind(&selection.user_id)
                .bind(&selected_toolchains_json)
                .bind(&custom_toolchains_json)
                .bind(&env_vars_json)
                .bind(selection.terminal_ui_enabled as i64)
                .bind(&selection.updated_at)
                .execute(pool)
                .await
                .map_err(|e| e.to_string())?;
                Ok(selection)
            })
        },
    )
    .await
}
