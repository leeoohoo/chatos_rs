// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use futures_util::TryStreamExt;
use mongodb::bson::doc;
use task_runner_service_backend::{load_task_runner_dotenv, AppConfig};

fn optional_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn env_flag(name: &str) -> bool {
    matches!(
        optional_env(name)
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str(),
        "1" | "true" | "yes" | "on"
    )
}

#[tokio::main]
async fn main() -> Result<(), String> {
    load_task_runner_dotenv();
    let config = AppConfig::from_env()?;
    let source_user_message_id = optional_env("TASK_RUNNER_REPAIR_SOURCE_USER_MESSAGE_ID")
        .ok_or_else(|| "TASK_RUNNER_REPAIR_SOURCE_USER_MESSAGE_ID is required".to_string())?;
    let source_turn_id = optional_env("TASK_RUNNER_REPAIR_SOURCE_TURN_ID")
        .unwrap_or_else(|| source_user_message_id.clone());
    let apply = env_flag("TASK_RUNNER_REPAIR_APPLY");

    let client = mongodb::Client::with_uri_str(config.database_url.as_str())
        .await
        .map_err(|err| err.to_string())?;
    let database = client
        .default_database()
        .ok_or_else(|| "mongodb connection string must include a database name".to_string())?;
    let tasks = database.collection::<mongodb::bson::Document>("tasks");
    let filter = doc! {
        "source_user_message_id": &source_user_message_id,
        "$or": [
            { "source_turn_id": { "$exists": false } },
            { "source_turn_id": "" },
            { "source_turn_id": null },
        ],
    };
    let mut cursor = tasks
        .find(filter.clone(), None)
        .await
        .map_err(|err| err.to_string())?;

    let mut matched = 0usize;
    while let Some(task) = cursor.try_next().await.map_err(|err| err.to_string())? {
        matched += 1;
        let task_id = task.get_str("id").unwrap_or("<missing>");
        let title = task.get_str("title").unwrap_or("<missing>");
        println!(
            "{} task_id={} title={}",
            if apply { "repair" } else { "dry-run" },
            task_id,
            title
        );
    }

    let mut updated = 0u64;
    if apply && matched > 0 {
        let result = tasks
            .update_many(
                filter,
                doc! {
                    "$set": {
                        "source_turn_id": &source_turn_id,
                    }
                },
                None,
            )
            .await
            .map_err(|err| err.to_string())?;
        updated = result.modified_count;
    }

    println!(
        "repair_source_turn_id complete apply={} source_user_message_id={} source_turn_id={} matched={} updated={}",
        apply,
        source_user_message_id,
        source_turn_id,
        matched,
        updated
    );
    Ok(())
}
