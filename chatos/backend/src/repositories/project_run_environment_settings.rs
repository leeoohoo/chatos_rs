// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use mongodb::bson::{doc, Bson, Document};

use crate::core::values::optional_string_bson;
use crate::models::project_run_environment::ProjectRunEnvironmentSelection;
use crate::repositories::db::{mongo_find_one_doc, mongo_upsert_set_doc, with_db};

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
    with_db(|db| {
        let project_id = project_id.to_string();
        Box::pin(async move {
            let doc = mongo_find_one_doc(
                db,
                "project_run_environment_settings",
                doc! { "project_id": &project_id },
            )
            .await?;
            Ok(doc.as_ref().and_then(normalize_doc))
        })
    })
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

    with_db(|db| {
        let selection = selection.clone();
        let selected_toolchains_json = selected_toolchains_json.clone();
        let custom_toolchains_json = custom_toolchains_json.clone();
        let env_vars_json = env_vars_json.clone();
        Box::pin(async move {
            let selected_toolchains_bson = mongodb::bson::to_bson(&selection.selected_toolchains)
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

            mongo_upsert_set_doc(
                db,
                "project_run_environment_settings",
                doc! { "project_id": &selection.project_id },
                set_doc,
            )
            .await?;
            Ok(selection)
        })
    })
    .await
}
