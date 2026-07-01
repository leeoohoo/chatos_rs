// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use mongodb::options::ClientOptions;
use mongodb::{Client, Database};

use crate::config::AppConfig;

pub async fn connect_database(config: &AppConfig) -> Result<Database, String> {
    let database_url = config.database_url.trim();
    if !database_url.starts_with("mongodb://") && !database_url.starts_with("mongodb+srv://") {
        return Err(format!(
            "user_service now requires a MongoDB USER_SERVICE_DATABASE_URL, got: {database_url}"
        ));
    }

    let options = ClientOptions::parse(database_url)
        .await
        .map_err(|err| format!("parse mongodb url failed: {err}"))?;
    let client = Client::with_options(options)
        .map_err(|err| format!("create mongodb client failed: {err}"))?;
    Ok(client.database(config.mongodb_database.as_str()))
}
