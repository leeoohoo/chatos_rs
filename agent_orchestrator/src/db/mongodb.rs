use std::time::Duration;

use mongodb::bson::doc;
use mongodb::options::{ClientOptions, ResolverConfig};
use mongodb::{Client, IndexModel};

use super::types::{Database, MongoConfig};

const DEFAULT_MONGO_DB_NAME: &str = "agent_orchestrator";
const EXPECTED_COLLECTIONS: [&str; 6] = [
    "projects",
    "mcp_configs",
    "terminals",
    "remote_connections",
    "system_contexts",
    "user_settings",
];

pub(super) async fn init_mongodb(cfg: &MongoConfig) -> Result<Database, String> {
    let connection_string = if let Some(conn) = cfg.connection_string.clone() {
        conn
    } else {
        let host = cfg.host.clone().unwrap_or_else(|| "localhost".to_string());
        let port = cfg.port.unwrap_or(27017);
        let database = cfg
            .database
            .clone()
            .unwrap_or_else(|| DEFAULT_MONGO_DB_NAME.to_string());
        let cred = match (&cfg.username, &cfg.password) {
            (Some(u), Some(p)) => format!("{}:{}@", urlencoding::encode(u), urlencoding::encode(p)),
            _ => "".to_string(),
        };
        format!("mongodb://{}{}:{}/{}", cred, host, port, database)
    };

    let mut options =
        ClientOptions::parse_with_resolver_config(&connection_string, ResolverConfig::cloudflare())
            .await
            .map_err(|e| format!("mongodb parse options failed: {e}"))?;
    if let Some(max_pool) = cfg.max_pool_size {
        options.max_pool_size = Some(max_pool);
    }
    if let Some(min_pool) = cfg.min_pool_size {
        options.min_pool_size = Some(min_pool);
    }
    if let Some(ms) = cfg.server_selection_timeout_ms {
        options.server_selection_timeout = Some(Duration::from_millis(ms));
    }
    if let Some(ms) = cfg.connect_timeout_ms {
        options.connect_timeout = Some(Duration::from_millis(ms));
    }
    let _ = cfg.socket_timeout_ms;

    let client =
        Client::with_options(options).map_err(|e| format!("mongodb client failed: {e}"))?;
    let requested_db_name = cfg
        .database
        .clone()
        .unwrap_or_else(|| DEFAULT_MONGO_DB_NAME.to_string());
    let db_name = resolve_database_name(&client, &requested_db_name).await;
    let db = client.database(&db_name);

    let collections = vec![
        "users",
        "mcp_configs",
        "mcp_change_logs",
        "task_manager_tasks",
        "mcp_config_profiles",
        "ai_model_configs",
        "system_contexts",
        "applications",
        "projects",
        "project_run_catalogs",
        "terminals",
        "remote_connections",
        "terminal_logs",
        "mcp_config_applications",
        "system_context_applications",
        "session_mcp_servers",
        "user_settings",
    ];
    let existing = db
        .list_collection_names(None)
        .await
        .map_err(|e| e.to_string())?;
    for name in collections {
        if !existing.contains(&name.to_string()) {
            let _ = db.create_collection(name, None).await;
        }
    }

    let _ = db
        .collection::<mongodb::bson::Document>("users")
        .create_index(
            IndexModel::builder()
                .keys(doc! { "email": 1 })
                .options(
                    mongodb::options::IndexOptions::builder()
                        .unique(true)
                        .build(),
                )
                .build(),
            None,
        )
        .await;
    let _ = db
        .collection::<mongodb::bson::Document>("mcp_change_logs")
        .create_index(
            IndexModel::builder()
                .keys(doc! { "server_name": 1 })
                .build(),
            None,
        )
        .await;
    let _ = db
        .collection::<mongodb::bson::Document>("mcp_change_logs")
        .create_index(
            IndexModel::builder().keys(doc! { "session_id": 1 }).build(),
            None,
        )
        .await;
    let _ = db
        .collection::<mongodb::bson::Document>("mcp_change_logs")
        .create_index(
            IndexModel::builder().keys(doc! { "created_at": 1 }).build(),
            None,
        )
        .await;
    let _ = db
        .collection::<mongodb::bson::Document>("mcp_change_logs")
        .create_index(
            IndexModel::builder()
                .keys(doc! { "confirmed": 1, "created_at": -1 })
                .build(),
            None,
        )
        .await;
    let _ = db
        .collection::<mongodb::bson::Document>("mcp_change_logs")
        .create_index(
            IndexModel::builder().keys(doc! { "project_id": 1 }).build(),
            None,
        )
        .await;
    let _ = db
        .collection::<mongodb::bson::Document>("mcp_change_logs")
        .create_index(IndexModel::builder().keys(doc! { "path": 1 }).build(), None)
        .await;
    let _ = db
        .collection::<mongodb::bson::Document>("task_manager_tasks")
        .create_index(
            IndexModel::builder()
                .keys(doc! { "session_id": 1, "conversation_turn_id": 1 })
                .build(),
            None,
        )
        .await;
    let _ = db
        .collection::<mongodb::bson::Document>("task_manager_tasks")
        .create_index(
            IndexModel::builder()
                .keys(doc! { "session_id": 1, "created_at": -1 })
                .build(),
            None,
        )
        .await;
    let _ = db
        .collection::<mongodb::bson::Document>("task_manager_tasks")
        .create_index(
            IndexModel::builder()
                .keys(doc! { "conversation_turn_id": 1, "created_at": -1 })
                .build(),
            None,
        )
        .await;
    let _ = db
        .collection::<mongodb::bson::Document>("projects")
        .create_index(
            IndexModel::builder().keys(doc! { "user_id": 1 }).build(),
            None,
        )
        .await;
    let _ = db
        .collection::<mongodb::bson::Document>("project_run_catalogs")
        .create_index(
            IndexModel::builder()
                .keys(doc! { "project_id": 1 })
                .options(
                    mongodb::options::IndexOptions::builder()
                        .unique(true)
                        .build(),
                )
                .build(),
            None,
        )
        .await;
    let _ = db
        .collection::<mongodb::bson::Document>("project_run_catalogs")
        .create_index(
            IndexModel::builder().keys(doc! { "user_id": 1 }).build(),
            None,
        )
        .await;
    let _ = db
        .collection::<mongodb::bson::Document>("terminals")
        .create_index(
            IndexModel::builder().keys(doc! { "user_id": 1 }).build(),
            None,
        )
        .await;
    let _ = db
        .collection::<mongodb::bson::Document>("terminals")
        .create_index(
            IndexModel::builder().keys(doc! { "project_id": 1 }).build(),
            None,
        )
        .await;
    let _ = db
        .collection::<mongodb::bson::Document>("terminals")
        .create_index(
            IndexModel::builder().keys(doc! { "status": 1 }).build(),
            None,
        )
        .await;
    let _ = db
        .collection::<mongodb::bson::Document>("remote_connections")
        .create_index(
            IndexModel::builder().keys(doc! { "user_id": 1 }).build(),
            None,
        )
        .await;
    let _ = db
        .collection::<mongodb::bson::Document>("remote_connections")
        .create_index(IndexModel::builder().keys(doc! { "host": 1 }).build(), None)
        .await;
    let _ = db
        .collection::<mongodb::bson::Document>("terminal_logs")
        .create_index(
            IndexModel::builder()
                .keys(doc! { "terminal_id": 1 })
                .build(),
            None,
        )
        .await;
    let _ = db
        .collection::<mongodb::bson::Document>("terminal_logs")
        .create_index(
            IndexModel::builder()
                .keys(doc! { "terminal_id": 1, "created_at": 1 })
                .build(),
            None,
        )
        .await;
    let _ = db
        .collection::<mongodb::bson::Document>("terminal_logs")
        .create_index(
            IndexModel::builder().keys(doc! { "created_at": 1 }).build(),
            None,
        )
        .await;

    Ok(Database::Mongo { client, db })
}

async fn resolve_database_name(client: &Client, requested: &str) -> String {
    if requested != DEFAULT_MONGO_DB_NAME {
        return requested.to_string();
    }

    let existing = match client.list_database_names(None, None).await {
        Ok(names) => names,
        Err(_) => return requested.to_string(),
    };
    if existing.iter().any(|name| name == DEFAULT_MONGO_DB_NAME) {
        return DEFAULT_MONGO_DB_NAME.to_string();
    }
    for name in existing {
        if is_system_database(name.as_str()) {
            continue;
        }
        let db = client.database(name.as_str());
        let Ok(collections) = db.list_collection_names(None).await else {
            continue;
        };
        let matched = EXPECTED_COLLECTIONS
            .iter()
            .filter(|item| collections.iter().any(|name| name == **item))
            .count();
        if matched >= 3 {
            return name;
        }
    }
    requested.to_string()
}

fn is_system_database(name: &str) -> bool {
    matches!(name, "admin" | "local" | "config")
}
