use mongodb::{Client, Database, options::ClientOptions};

use crate::config::AppConfig;

pub async fn init_pool(config: &AppConfig) -> Result<Database, String> {
    let options = ClientOptions::parse(config.mongodb_uri.as_str())
        .await
        .map_err(|err| err.to_string())?;
    let client = Client::with_options(options).map_err(|err| err.to_string())?;
    Ok(client.database(config.mongodb_database.as_str()))
}
