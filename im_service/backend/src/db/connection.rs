use mongodb::bson::doc;
use mongodb::options::ClientOptions;
use mongodb::Client;

use crate::config::AppConfig;

use super::Db;

pub async fn init_pool(config: &AppConfig) -> Result<Db, String> {
    let mut options = ClientOptions::parse(config.mongodb_uri.as_str())
        .await
        .map_err(|e| format!("invalid mongodb uri: {e}"))?;
    options.app_name = Some("im_service".to_string());

    let client = Client::with_options(options).map_err(|e| e.to_string())?;
    let db = client.database(config.mongodb_database.as_str());

    db.run_command(doc! { "ping": 1 })
        .await
        .map_err(|e| format!("mongodb ping failed: {e}"))?;

    Ok(db)
}
