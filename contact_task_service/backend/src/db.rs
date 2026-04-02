use mongodb::bson::doc;
use mongodb::{options::ClientOptions, Client, Database};

pub type Db = Database;

pub async fn init_pool(config: &crate::config::AppConfig) -> Result<Db, String> {
    let client_options = ClientOptions::parse(config.mongo_url.as_str())
        .await
        .map_err(|e| e.to_string())?;
    let client = Client::with_options(client_options).map_err(|e| e.to_string())?;
    Ok(client.database(config.mongo_db.as_str()))
}

pub async fn init_schema(db: &Db) -> Result<(), String> {
    use mongodb::IndexModel;

    async fn ensure_index(
        collection: mongodb::Collection<mongodb::bson::Document>,
        keys: mongodb::bson::Document,
        unique: bool,
    ) -> Result<(), String> {
        let options = mongodb::options::IndexOptions::builder()
            .unique(Some(unique))
            .build();
        collection
            .create_index(IndexModel::builder().keys(keys).options(options).build())
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    ensure_index(db.collection("auth_users"), doc! {"user_id": 1}, true).await?;
    ensure_index(db.collection("contact_tasks"), doc! {"id": 1}, true).await?;
    ensure_index(
        db.collection("contact_tasks"),
        doc! {"user_id": 1, "contact_agent_id": 1, "project_id": 1, "status": 1, "created_at": 1},
        false,
    )
    .await?;
    ensure_index(
        db.collection("contact_tasks"),
        doc! {"status": 1, "priority_rank": 1, "created_at": 1},
        false,
    )
    .await?;
    ensure_index(
        db.collection("contact_task_scope_runtimes"),
        doc! {"scope_key": 1},
        true,
    )
    .await?;
    Ok(())
}
