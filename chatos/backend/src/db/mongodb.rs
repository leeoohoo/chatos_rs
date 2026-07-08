// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use mongodb::bson::doc;
use mongodb::options::{ClientOptions, ResolverConfig};
use mongodb::{Client, IndexModel};

use super::types::{Database, MongoConfig};

pub(super) async fn init_mongodb(cfg: &MongoConfig) -> Result<Database, String> {
    let connection_string = if let Some(conn) = cfg.connection_string.clone() {
        conn
    } else {
        let host = cfg.host.clone().unwrap_or_else(|| "localhost".to_string());
        let port = cfg.port.unwrap_or(27017);
        let database = cfg
            .database
            .clone()
            .unwrap_or_else(|| "chat_app".to_string());
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
    let db_name = cfg
        .database
        .clone()
        .unwrap_or_else(|| "chat_app".to_string());
    let db = client.database(&db_name);

    let collections = vec![
        "users",
        "auth_users",
        "agents",
        "memory_skills",
        "memory_skill_plugins",
        "chatos_contacts",
        "chatos_memory_projects",
        "chatos_project_agent_links",
        "mcp_configs",
        "mcp_change_logs",
        "task_manager_tasks",
        "ask_user_prompt_requests",
        "mcp_config_profiles",
        "system_contexts",
        "applications",
        "project_run_catalogs",
        "project_run_environment_settings",
        "terminals",
        "remote_connections",
        "terminal_logs",
        "mcp_config_applications",
        "system_context_applications",
        "session_mcp_servers",
        "session_runtime_settings",
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
        .collection::<mongodb::bson::Document>("auth_users")
        .create_index(
            IndexModel::builder()
                .keys(doc! { "user_id": 1 })
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
        .collection::<mongodb::bson::Document>("auth_users")
        .create_index(IndexModel::builder().keys(doc! { "role": 1 }).build(), None)
        .await;
    let _ = db
        .collection::<mongodb::bson::Document>("agents")
        .create_index(
            IndexModel::builder()
                .keys(doc! { "id": 1 })
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
        .collection::<mongodb::bson::Document>("chatos_contacts")
        .create_index(
            IndexModel::builder()
                .keys(doc! { "id": 1 })
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
        .collection::<mongodb::bson::Document>("session_runtime_settings")
        .create_index(
            IndexModel::builder()
                .keys(doc! { "session_id": 1 })
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
        .collection::<mongodb::bson::Document>("session_runtime_settings")
        .create_index(
            IndexModel::builder()
                .keys(doc! { "user_id": 1, "updated_at": -1 })
                .build(),
            None,
        )
        .await;
    let _ = db
        .collection::<mongodb::bson::Document>("chatos_contacts")
        .create_index(
            IndexModel::builder()
                .keys(doc! { "user_id": 1, "agent_id": 1 })
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
        .collection::<mongodb::bson::Document>("chatos_contacts")
        .create_index(
            IndexModel::builder()
                .keys(doc! { "user_id": 1, "status": 1, "updated_at": -1 })
                .build(),
            None,
        )
        .await;
    let _ = db
        .collection::<mongodb::bson::Document>("chatos_memory_projects")
        .create_index(
            IndexModel::builder()
                .keys(doc! { "id": 1 })
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
        .collection::<mongodb::bson::Document>("chatos_memory_projects")
        .create_index(
            IndexModel::builder()
                .keys(doc! { "user_id": 1, "project_id": 1 })
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
        .collection::<mongodb::bson::Document>("chatos_memory_projects")
        .create_index(
            IndexModel::builder()
                .keys(doc! { "user_id": 1, "updated_at": -1 })
                .build(),
            None,
        )
        .await;
    let _ = db
        .collection::<mongodb::bson::Document>("chatos_project_agent_links")
        .create_index(
            IndexModel::builder()
                .keys(doc! { "id": 1 })
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
        .collection::<mongodb::bson::Document>("chatos_project_agent_links")
        .create_index(
            IndexModel::builder()
                .keys(doc! { "user_id": 1, "project_id": 1 })
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
        .collection::<mongodb::bson::Document>("chatos_project_agent_links")
        .create_index(
            IndexModel::builder()
                .keys(doc! { "user_id": 1, "project_id": 1, "agent_id": 1 })
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
        .collection::<mongodb::bson::Document>("chatos_project_agent_links")
        .create_index(
            IndexModel::builder()
                .keys(doc! { "user_id": 1, "contact_id": 1, "status": 1, "last_bound_at": -1 })
                .build(),
            None,
        )
        .await;
    let _ = db
        .collection::<mongodb::bson::Document>("chatos_project_agent_links")
        .create_index(
            IndexModel::builder()
                .keys(doc! { "user_id": 1, "project_id": 1, "status": 1, "last_bound_at": -1 })
                .build(),
            None,
        )
        .await;
    let _ = db
        .collection::<mongodb::bson::Document>("agents")
        .create_index(
            IndexModel::builder()
                .keys(doc! { "user_id": 1, "updated_at": -1 })
                .build(),
            None,
        )
        .await;
    let _ = db
        .collection::<mongodb::bson::Document>("memory_skill_plugins")
        .create_index(
            IndexModel::builder()
                .keys(doc! { "id": 1 })
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
        .collection::<mongodb::bson::Document>("memory_skill_plugins")
        .create_index(
            IndexModel::builder()
                .keys(doc! { "user_id": 1, "source": 1 })
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
        .collection::<mongodb::bson::Document>("memory_skill_plugins")
        .create_index(
            IndexModel::builder()
                .keys(doc! { "user_id": 1, "updated_at": -1 })
                .build(),
            None,
        )
        .await;
    let _ = db
        .collection::<mongodb::bson::Document>("memory_skills")
        .create_index(
            IndexModel::builder()
                .keys(doc! { "id": 1 })
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
        .collection::<mongodb::bson::Document>("memory_skills")
        .create_index(
            IndexModel::builder()
                .keys(doc! { "user_id": 1, "plugin_source": 1, "source_path": 1 })
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
        .collection::<mongodb::bson::Document>("memory_skills")
        .create_index(
            IndexModel::builder()
                .keys(doc! { "user_id": 1, "plugin_source": 1, "updated_at": -1 })
                .build(),
            None,
        )
        .await;
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
        .update_many(
            doc! {
                "conversation_id": { "$exists": false },
                "session_id": { "$exists": true }
            },
            doc! { "$rename": { "session_id": "conversation_id" } },
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
            IndexModel::builder()
                .keys(doc! { "conversation_id": 1 })
                .build(),
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
                .keys(doc! { "conversation_id": 1, "conversation_turn_id": 1 })
                .build(),
            None,
        )
        .await;
    let _ = db
        .collection::<mongodb::bson::Document>("task_manager_tasks")
        .create_index(
            IndexModel::builder()
                .keys(doc! { "conversation_id": 1, "created_at": -1 })
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
        .collection::<mongodb::bson::Document>("ask_user_prompt_requests")
        .create_index(
            IndexModel::builder()
                .keys(doc! { "conversation_id": 1, "status": 1, "updated_at": -1 })
                .build(),
            None,
        )
        .await;
    let _ = db
        .collection::<mongodb::bson::Document>("ask_user_prompt_requests")
        .create_index(
            IndexModel::builder()
                .keys(doc! { "conversation_turn_id": 1, "created_at": -1 })
                .build(),
            None,
        )
        .await;
    let _ = db
        .collection::<mongodb::bson::Document>("ask_user_prompt_requests")
        .create_index(
            IndexModel::builder()
                .keys(doc! { "source": 1, "external_prompt_id": 1 })
                .build(),
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
        .collection::<mongodb::bson::Document>("project_run_environment_settings")
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
        .collection::<mongodb::bson::Document>("project_run_environment_settings")
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

    Ok(Database::Mongo {
        _client: client,
        db,
    })
}
