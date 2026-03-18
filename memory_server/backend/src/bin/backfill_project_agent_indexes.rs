use std::collections::HashMap;
use std::env;

use futures_util::TryStreamExt;
use mongodb::bson::{doc, Bson, Document};
use mongodb::options::{ClientOptions, FindOptions};
use mongodb::{Client, Collection, Database};
use uuid::Uuid;

#[derive(Debug, Clone)]
struct CliArgs {
    mongo_uri: String,
    mongo_db: String,
    dry_run: bool,
}

#[derive(Debug, Default)]
struct BackfillStats {
    session_rows: usize,
    memory_rows: usize,
    projects_upserted: usize,
    links_upserted: usize,
    skipped_rows: usize,
}

#[tokio::main]
async fn main() -> Result<(), String> {
    let args = parse_args()?;
    println!("[BACKFILL] mongo uri = {}", args.mongo_uri);
    println!("[BACKFILL] mongo db  = {}", args.mongo_db);
    println!("[BACKFILL] dry run   = {}", args.dry_run);

    let mut options = ClientOptions::parse(args.mongo_uri.as_str())
        .await
        .map_err(|e| format!("invalid mongo uri: {e}"))?;
    options.app_name = Some("memory_project_agent_backfill".to_string());
    let client = Client::with_options(options).map_err(|e| e.to_string())?;
    let db = client.database(args.mongo_db.as_str());

    db.run_command(doc! {"ping": 1})
        .await
        .map_err(|e| format!("mongo ping failed: {e}"))?;

    let contact_map = load_contact_map(&db).await?;
    println!("[BACKFILL] contacts loaded: {}", contact_map.len());

    let mut stats = BackfillStats::default();
    backfill_from_sessions(&db, &contact_map, args.dry_run, &mut stats).await?;
    backfill_from_project_memories(&db, args.dry_run, &mut stats).await?;

    println!("[BACKFILL] done");
    println!("  session_rows: {}", stats.session_rows);
    println!("  memory_rows: {}", stats.memory_rows);
    println!("  projects_upserted: {}", stats.projects_upserted);
    println!("  links_upserted: {}", stats.links_upserted);
    println!("  skipped_rows: {}", stats.skipped_rows);
    Ok(())
}

fn parse_args() -> Result<CliArgs, String> {
    let mut mongo_uri = env::var("MEMORY_SERVER_MONGODB_URI")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| "mongodb://admin:admin@127.0.0.1:27018/admin".to_string());
    let mut mongo_db = env::var("MEMORY_SERVER_MONGODB_DATABASE")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| "memory_server".to_string());
    let mut dry_run = false;

    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--mongo-uri" => {
                mongo_uri = args
                    .next()
                    .ok_or_else(|| "--mongo-uri requires value".to_string())?;
            }
            "--mongo-db" => {
                mongo_db = args
                    .next()
                    .ok_or_else(|| "--mongo-db requires value".to_string())?;
            }
            "--dry-run" => {
                dry_run = true;
            }
            "--help" | "-h" => {
                print_usage();
                std::process::exit(0);
            }
            _ => return Err(format!("unknown arg: {arg}")),
        }
    }

    Ok(CliArgs {
        mongo_uri,
        mongo_db,
        dry_run,
    })
}

fn print_usage() {
    println!(
        "Usage:\n  cargo run --bin backfill_project_agent_indexes -- [--mongo-uri <uri>] [--mongo-db <name>] [--dry-run]"
    );
}

async fn load_contact_map(db: &Database) -> Result<HashMap<(String, String), String>, String> {
    let coll: Collection<Document> = db.collection("contacts");
    let mut cursor = coll
        .find(doc! {"status": {"$ne": "deleted"}})
        .await
        .map_err(|e| e.to_string())?;
    let mut out = HashMap::new();
    while let Some(row) = cursor.try_next().await.map_err(|e| e.to_string())? {
        let user_id = doc_string(&row, "user_id");
        let agent_id = doc_string(&row, "agent_id");
        let contact_id = doc_string(&row, "id");
        if let (Some(user_id), Some(agent_id), Some(contact_id)) = (user_id, agent_id, contact_id) {
            out.insert((user_id, agent_id), contact_id);
        }
    }
    Ok(out)
}

async fn backfill_from_sessions(
    db: &Database,
    contact_map: &HashMap<(String, String), String>,
    dry_run: bool,
    stats: &mut BackfillStats,
) -> Result<(), String> {
    let coll: Collection<Document> = db.collection("sessions");
    let options = FindOptions::builder()
        .sort(doc! {"updated_at": 1, "created_at": 1})
        .build();
    let mut cursor = coll
        .find(doc! {})
        .with_options(options)
        .await
        .map_err(|e| e.to_string())?;

    while let Some(row) = cursor.try_next().await.map_err(|e| e.to_string())? {
        stats.session_rows += 1;
        let Some(user_id) = doc_string(&row, "user_id") else {
            stats.skipped_rows += 1;
            continue;
        };
        let project_id = normalize_project_id(doc_opt_string(&row, "project_id"));
        let session_id = doc_string(&row, "id");
        let updated_at = doc_string(&row, "updated_at").unwrap_or_else(now_rfc3339);
        let status = doc_string(&row, "status").unwrap_or_else(|| "active".to_string());
        let metadata = row.get("metadata").and_then(Bson::as_document);

        upsert_project_index(
            db,
            UpsertProjectIndexInput {
                user_id: user_id.clone(),
                project_id: project_id.clone(),
                name: default_project_name(project_id.as_str()),
                root_path: None,
                description: None,
                status: Some("active".to_string()),
                is_virtual: Some(project_id == "0"),
            },
            dry_run,
            stats,
        )
        .await?;

        let agent_id = metadata_string(metadata, &["contact", "agent_id"])
            .or_else(|| metadata_string(metadata, &["ui_contact", "agent_id"]))
            .or_else(|| metadata_string(metadata, &["ui_chat_selection", "selected_agent_id"]))
            .or_else(|| metadata_string(metadata, &["ui_chat_selection", "selectedAgentId"]));
        let Some(agent_id) = agent_id else {
            continue;
        };
        let contact_id = metadata_string(metadata, &["contact", "contact_id"])
            .or_else(|| metadata_string(metadata, &["ui_contact", "contact_id"]))
            .or_else(|| contact_map.get(&(user_id.clone(), agent_id.clone())).cloned());

        upsert_project_agent_link(
            db,
            UpsertProjectAgentLinkInput {
                user_id,
                project_id,
                agent_id,
                contact_id,
                latest_session_id: session_id,
                last_bound_at: updated_at,
                status: Some(status),
            },
            dry_run,
            stats,
        )
        .await?;
    }
    Ok(())
}

async fn backfill_from_project_memories(
    db: &Database,
    dry_run: bool,
    stats: &mut BackfillStats,
) -> Result<(), String> {
    let coll: Collection<Document> = db.collection("project_memories");
    let options = FindOptions::builder().sort(doc! {"updated_at": 1}).build();
    let mut cursor = coll
        .find(doc! {})
        .with_options(options)
        .await
        .map_err(|e| e.to_string())?;

    while let Some(row) = cursor.try_next().await.map_err(|e| e.to_string())? {
        stats.memory_rows += 1;
        let Some(user_id) = doc_string(&row, "user_id") else {
            stats.skipped_rows += 1;
            continue;
        };
        let Some(agent_id) = doc_string(&row, "agent_id") else {
            stats.skipped_rows += 1;
            continue;
        };
        let Some(contact_id) = doc_string(&row, "contact_id") else {
            stats.skipped_rows += 1;
            continue;
        };
        let project_id = normalize_project_id(doc_opt_string(&row, "project_id"));
        let updated_at = doc_string(&row, "updated_at").unwrap_or_else(now_rfc3339);

        upsert_project_index(
            db,
            UpsertProjectIndexInput {
                user_id: user_id.clone(),
                project_id: project_id.clone(),
                name: default_project_name(project_id.as_str()),
                root_path: None,
                description: None,
                status: Some("active".to_string()),
                is_virtual: Some(project_id == "0"),
            },
            dry_run,
            stats,
        )
        .await?;

        upsert_project_agent_link(
            db,
            UpsertProjectAgentLinkInput {
                user_id,
                project_id,
                agent_id,
                contact_id: Some(contact_id),
                latest_session_id: None,
                last_bound_at: updated_at,
                status: Some("active".to_string()),
            },
            dry_run,
            stats,
        )
        .await?;
    }
    Ok(())
}

struct UpsertProjectIndexInput {
    user_id: String,
    project_id: String,
    name: String,
    root_path: Option<String>,
    description: Option<String>,
    status: Option<String>,
    is_virtual: Option<bool>,
}

async fn upsert_project_index(
    db: &Database,
    input: UpsertProjectIndexInput,
    dry_run: bool,
    stats: &mut BackfillStats,
) -> Result<(), String> {
    let now = now_rfc3339();
    let status = input.status.unwrap_or_else(|| "active".to_string());
    let archived_at = if status == "archived" || status == "deleted" {
        Some(now.clone())
    } else {
        None
    };

    if dry_run {
        stats.projects_upserted += 1;
        return Ok(());
    }

    let coll: Collection<Document> = db.collection("memory_projects");
    let mut set_doc = doc! {
        "user_id": input.user_id.as_str(),
        "project_id": input.project_id.as_str(),
        "name": input.name.as_str(),
        "status": status.as_str(),
        "is_virtual": if input.is_virtual.unwrap_or(false) { 1 } else { 0 },
        "updated_at": now.as_str(),
    };
    if let Some(root_path) = input.root_path {
        set_doc.insert("root_path", root_path);
    } else {
        set_doc.insert("root_path", Bson::Null);
    }
    if let Some(description) = input.description {
        set_doc.insert("description", description);
    } else {
        set_doc.insert("description", Bson::Null);
    }
    if let Some(archived_at) = archived_at {
        set_doc.insert("archived_at", archived_at);
    } else {
        set_doc.insert("archived_at", Bson::Null);
    }

    coll.update_one(
        doc! {
            "user_id": input.user_id.as_str(),
            "project_id": input.project_id.as_str(),
        },
        doc! {
            "$set": set_doc,
            "$setOnInsert": {
                "id": Uuid::new_v4().to_string(),
                "created_at": now.as_str(),
            }
        },
    )
    .upsert(true)
    .await
    .map_err(|e| e.to_string())?;
    stats.projects_upserted += 1;
    Ok(())
}

struct UpsertProjectAgentLinkInput {
    user_id: String,
    project_id: String,
    agent_id: String,
    contact_id: Option<String>,
    latest_session_id: Option<String>,
    last_bound_at: String,
    status: Option<String>,
}

async fn upsert_project_agent_link(
    db: &Database,
    input: UpsertProjectAgentLinkInput,
    dry_run: bool,
    stats: &mut BackfillStats,
) -> Result<(), String> {
    if dry_run {
        stats.links_upserted += 1;
        return Ok(());
    }

    let now = now_rfc3339();
    let status = input.status.unwrap_or_else(|| "active".to_string());
    let coll: Collection<Document> = db.collection("memory_project_agent_links");
    let mut set_doc = doc! {
        "user_id": input.user_id.as_str(),
        "project_id": input.project_id.as_str(),
        "agent_id": input.agent_id.as_str(),
        "status": status.as_str(),
        "last_bound_at": input.last_bound_at.as_str(),
        "updated_at": now.as_str(),
    };
    if let Some(contact_id) = input.contact_id {
        set_doc.insert("contact_id", contact_id);
    } else {
        set_doc.insert("contact_id", Bson::Null);
    }
    if let Some(session_id) = input.latest_session_id {
        set_doc.insert("latest_session_id", session_id);
    } else {
        set_doc.insert("latest_session_id", Bson::Null);
    }

    coll.update_one(
        doc! {
            "user_id": input.user_id.as_str(),
            "project_id": input.project_id.as_str(),
            "agent_id": input.agent_id.as_str(),
        },
        doc! {
            "$set": set_doc,
            "$setOnInsert": {
                "id": Uuid::new_v4().to_string(),
                "first_bound_at": input.last_bound_at.as_str(),
                "created_at": now.as_str(),
            }
        },
    )
    .upsert(true)
    .await
    .map_err(|e| e.to_string())?;
    stats.links_upserted += 1;
    Ok(())
}

fn doc_string(doc: &Document, key: &str) -> Option<String> {
    doc.get_str(key)
        .ok()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToOwned::to_owned)
}

fn doc_opt_string(doc: &Document, key: &str) -> Option<String> {
    match doc.get(key) {
        Some(Bson::String(value)) => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        _ => None,
    }
}

fn metadata_string(metadata: Option<&Document>, path: &[&str]) -> Option<String> {
    let mut cursor = metadata?;
    for (index, key) in path.iter().enumerate() {
        let value = cursor.get(*key)?;
        if index == path.len() - 1 {
            return match value {
                Bson::String(raw) => {
                    let trimmed = raw.trim();
                    if trimmed.is_empty() {
                        None
                    } else {
                        Some(trimmed.to_string())
                    }
                }
                _ => None,
            };
        }
        cursor = value.as_document()?;
    }
    None
}

fn normalize_project_id(raw: Option<String>) -> String {
    let value = raw.unwrap_or_default();
    let trimmed = value.trim();
    if trimmed.is_empty() {
        "0".to_string()
    } else {
        trimmed.to_string()
    }
}

fn default_project_name(project_id: &str) -> String {
    if project_id == "0" {
        "未指定项目".to_string()
    } else {
        format!("项目 {}", project_id)
    }
}

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

