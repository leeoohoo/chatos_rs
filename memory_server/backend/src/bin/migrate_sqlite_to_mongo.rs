use std::env;

use mongodb::bson::{doc, Bson, Document};
use mongodb::options::ClientOptions;
use mongodb::{Client, Collection, Database};
use rusqlite::Connection;

#[derive(Debug, Clone)]
struct CliArgs {
    sqlite_path: String,
    mongo_uri: String,
    mongo_db: String,
    drop_target: bool,
}

#[tokio::main]
async fn main() -> Result<(), String> {
    let args = parse_args()?;

    println!("[MIGRATE] sqlite path = {}", args.sqlite_path);
    println!("[MIGRATE] mongo uri   = {}", args.mongo_uri);
    println!("[MIGRATE] mongo db    = {}", args.mongo_db);
    println!("[MIGRATE] drop target = {}", args.drop_target);

    let conn = Connection::open(args.sqlite_path.as_str()).map_err(|e| e.to_string())?;

    let mut options = ClientOptions::parse(args.mongo_uri.as_str())
        .await
        .map_err(|e| format!("invalid mongo uri: {e}"))?;
    options.app_name = Some("memory_server_migrator".to_string());
    let client = Client::with_options(options).map_err(|e| e.to_string())?;
    let db = client.database(args.mongo_db.as_str());

    db.run_command(doc! {"ping": 1})
        .await
        .map_err(|e| format!("mongo ping failed: {e}"))?;

    if args.drop_target {
        drop_target_collections(&db).await?;
    }

    let sessions_count = migrate_sessions(&conn, &db).await?;
    let messages_count = migrate_messages(&conn, &db).await?;
    let summaries_count = migrate_summaries(&conn, &db).await?;
    let model_configs_count = migrate_ai_model_configs(&conn, &db).await?;
    let auth_users_count = migrate_auth_users(&conn, &db).await?;
    let summary_job_cfg_count = migrate_summary_job_configs(&conn, &db).await?;
    let summary_rollup_job_cfg_count = migrate_summary_rollup_job_configs(&conn, &db).await?;
    let job_runs_count = migrate_job_runs(&conn, &db).await?;

    println!("[MIGRATE] done");
    println!("  sessions: {}", sessions_count);
    println!("  messages: {}", messages_count);
    println!("  summaries: {}", summaries_count);
    println!("  ai_model_configs: {}", model_configs_count);
    println!("  auth_users: {}", auth_users_count);
    println!("  summary_job_configs: {}", summary_job_cfg_count);
    println!(
        "  summary_rollup_job_configs: {}",
        summary_rollup_job_cfg_count
    );
    println!("  job_runs: {}", job_runs_count);

    Ok(())
}

fn parse_args() -> Result<CliArgs, String> {
    let mut sqlite_path = env::var("MEMORY_SERVER_SQLITE_PATH")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| "data/memory_server.db".to_string());

    // Backward-compat: support legacy sqlite url env.
    if let Some(v) = env::var("MEMORY_SERVER_DATABASE_URL").ok() {
        if v.starts_with("sqlite://") {
            sqlite_path = v.trim_start_matches("sqlite://").to_string();
        }
    }

    let mut mongo_uri = env::var("MEMORY_SERVER_MONGODB_URI")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| "mongodb://admin:admin@127.0.0.1:27018/admin".to_string());

    let mut mongo_db = env::var("MEMORY_SERVER_MONGODB_DATABASE")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| "memory_server".to_string());

    let mut drop_target = false;

    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--sqlite" => {
                sqlite_path = args
                    .next()
                    .ok_or_else(|| "--sqlite requires value".to_string())?;
            }
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
            "--drop-target" => {
                drop_target = true;
            }
            "--help" | "-h" => {
                print_usage();
                std::process::exit(0);
            }
            _ => {
                return Err(format!("unknown arg: {arg}"));
            }
        }
    }

    Ok(CliArgs {
        sqlite_path,
        mongo_uri,
        mongo_db,
        drop_target,
    })
}

fn print_usage() {
    println!(
        "Usage:\n  cargo run --bin migrate_sqlite_to_mongo -- \\\n    [--sqlite <path>] [--mongo-uri <uri>] [--mongo-db <name>] [--drop-target]\n\nExamples:\n  cargo run --bin migrate_sqlite_to_mongo -- --sqlite data/memory_server.db --mongo-uri mongodb://admin:admin@127.0.0.1:27018/admin --mongo-db memory_server\n  cargo run --bin migrate_sqlite_to_mongo -- --drop-target"
    );
}

async fn drop_target_collections(db: &Database) -> Result<(), String> {
    let names = [
        "sessions",
        "messages",
        "session_summaries_v2",
        "ai_model_configs",
        "auth_users",
        "summary_job_configs",
        "summary_rollup_job_configs",
        "job_runs",
    ];

    for name in names {
        match db.collection::<Document>(name).drop().await {
            Ok(_) => println!("[MIGRATE] dropped collection: {name}"),
            Err(err) => {
                let msg = err.to_string();
                if !msg.contains("NamespaceNotFound") {
                    return Err(format!("drop {name} failed: {err}"));
                }
            }
        }
    }

    Ok(())
}

async fn migrate_sessions(conn: &Connection, db: &Database) -> Result<usize, String> {
    let coll = db.collection::<Document>("sessions");
    let mut stmt = conn
        .prepare(
            "SELECT id, user_id, project_id, title, status, created_at, updated_at, archived_at FROM sessions",
        )
        .map_err(|e| e.to_string())?;
    let mut rows = stmt.query([]).map_err(|e| e.to_string())?;

    let mut count = 0usize;
    while let Some(row) = rows.next().map_err(|e| e.to_string())? {
        let id: String = row.get(0).map_err(to_row_err("sessions.id"))?;
        let doc = doc! {
            "id": &id,
            "user_id": row.get::<_, String>(1).map_err(to_row_err("sessions.user_id"))?,
            "project_id": to_bson_opt_string(row.get(2).map_err(to_row_err("sessions.project_id"))?),
            "title": to_bson_opt_string(row.get(3).map_err(to_row_err("sessions.title"))?),
            "status": row.get::<_, String>(4).map_err(to_row_err("sessions.status"))?,
            "created_at": row.get::<_, String>(5).map_err(to_row_err("sessions.created_at"))?,
            "updated_at": row.get::<_, String>(6).map_err(to_row_err("sessions.updated_at"))?,
            "archived_at": to_bson_opt_string(row.get(7).map_err(to_row_err("sessions.archived_at"))?),
        };
        upsert(&coll, doc! {"id": &id}, doc).await?;
        count += 1;
    }

    println!("[MIGRATE] sessions: {}", count);
    Ok(count)
}

async fn migrate_messages(conn: &Connection, db: &Database) -> Result<usize, String> {
    let coll = db.collection::<Document>("messages");
    let mut stmt = conn
        .prepare(
            "SELECT id, session_id, role, content, message_mode, message_source, tool_calls, tool_call_id, reasoning, metadata, summary_status, summary_id, summarized_at, created_at FROM messages",
        )
        .map_err(|e| e.to_string())?;
    let mut rows = stmt.query([]).map_err(|e| e.to_string())?;

    let mut count = 0usize;
    while let Some(row) = rows.next().map_err(|e| e.to_string())? {
        let id: String = row.get(0).map_err(to_row_err("messages.id"))?;
        let doc = doc! {
            "id": &id,
            "session_id": row.get::<_, String>(1).map_err(to_row_err("messages.session_id"))?,
            "role": row.get::<_, String>(2).map_err(to_row_err("messages.role"))?,
            "content": row.get::<_, String>(3).map_err(to_row_err("messages.content"))?,
            "message_mode": to_bson_opt_string(row.get(4).map_err(to_row_err("messages.message_mode"))?),
            "message_source": to_bson_opt_string(row.get(5).map_err(to_row_err("messages.message_source"))?),
            "tool_calls": to_bson_opt_json(row.get(6).map_err(to_row_err("messages.tool_calls"))?),
            "tool_call_id": to_bson_opt_string(row.get(7).map_err(to_row_err("messages.tool_call_id"))?),
            "reasoning": to_bson_opt_string(row.get(8).map_err(to_row_err("messages.reasoning"))?),
            "metadata": to_bson_opt_json(row.get(9).map_err(to_row_err("messages.metadata"))?),
            "summary_status": default_string(row.get(10).map_err(to_row_err("messages.summary_status"))?, "pending"),
            "summary_id": to_bson_opt_string(row.get(11).map_err(to_row_err("messages.summary_id"))?),
            "summarized_at": to_bson_opt_string(row.get(12).map_err(to_row_err("messages.summarized_at"))?),
            "created_at": row.get::<_, String>(13).map_err(to_row_err("messages.created_at"))?,
        };
        upsert(&coll, doc! {"id": &id}, doc).await?;
        count += 1;
    }

    println!("[MIGRATE] messages: {}", count);
    Ok(count)
}

async fn migrate_summaries(conn: &Connection, db: &Database) -> Result<usize, String> {
    let coll = db.collection::<Document>("session_summaries_v2");
    let mut stmt = conn
        .prepare(
            "SELECT id, session_id, summary_text, summary_model, trigger_type, source_start_message_id, source_end_message_id, source_message_count, source_estimated_tokens, status, error_message, level, rollup_status, rollup_summary_id, rolled_up_at, created_at, updated_at FROM session_summaries_v2",
        )
        .map_err(|e| e.to_string())?;
    let mut rows = stmt.query([]).map_err(|e| e.to_string())?;

    let mut count = 0usize;
    while let Some(row) = rows.next().map_err(|e| e.to_string())? {
        let id: String = row.get(0).map_err(to_row_err("summaries.id"))?;
        let doc = doc! {
            "id": &id,
            "session_id": row.get::<_, String>(1).map_err(to_row_err("summaries.session_id"))?,
            "summary_text": row.get::<_, String>(2).map_err(to_row_err("summaries.summary_text"))?,
            "summary_model": row.get::<_, String>(3).map_err(to_row_err("summaries.summary_model"))?,
            "trigger_type": row.get::<_, String>(4).map_err(to_row_err("summaries.trigger_type"))?,
            "source_start_message_id": to_bson_opt_string(row.get(5).map_err(to_row_err("summaries.source_start_message_id"))?),
            "source_end_message_id": to_bson_opt_string(row.get(6).map_err(to_row_err("summaries.source_end_message_id"))?),
            "source_message_count": row.get::<_, i64>(7).map_err(to_row_err("summaries.source_message_count"))?,
            "source_estimated_tokens": row.get::<_, i64>(8).map_err(to_row_err("summaries.source_estimated_tokens"))?,
            "status": row.get::<_, String>(9).map_err(to_row_err("summaries.status"))?,
            "error_message": to_bson_opt_string(row.get(10).map_err(to_row_err("summaries.error_message"))?),
            "level": row.get::<_, i64>(11).map_err(to_row_err("summaries.level"))?,
            "rollup_status": default_string(row.get(12).map_err(to_row_err("summaries.rollup_status"))?, "pending"),
            "rollup_summary_id": to_bson_opt_string(row.get(13).map_err(to_row_err("summaries.rollup_summary_id"))?),
            "rolled_up_at": to_bson_opt_string(row.get(14).map_err(to_row_err("summaries.rolled_up_at"))?),
            "created_at": row.get::<_, String>(15).map_err(to_row_err("summaries.created_at"))?,
            "updated_at": row.get::<_, String>(16).map_err(to_row_err("summaries.updated_at"))?,
        };
        upsert(&coll, doc! {"id": &id}, doc).await?;
        count += 1;
    }

    println!("[MIGRATE] session_summaries_v2: {}", count);
    Ok(count)
}

async fn migrate_ai_model_configs(conn: &Connection, db: &Database) -> Result<usize, String> {
    let coll = db.collection::<Document>("ai_model_configs");
    let mut stmt = conn
        .prepare(
            "SELECT id, user_id, name, provider, model, base_url, api_key, COALESCE(supports_images, 0), COALESCE(supports_reasoning, 0), COALESCE(supports_responses, 0), temperature, thinking_level, enabled, created_at, updated_at FROM ai_model_configs",
        )
        .map_err(|e| e.to_string())?;
    let mut rows = stmt.query([]).map_err(|e| e.to_string())?;

    let mut count = 0usize;
    while let Some(row) = rows.next().map_err(|e| e.to_string())? {
        let id: String = row.get(0).map_err(to_row_err("ai_model_configs.id"))?;
        let doc = doc! {
            "id": &id,
            "user_id": row.get::<_, String>(1).map_err(to_row_err("ai_model_configs.user_id"))?,
            "name": row.get::<_, String>(2).map_err(to_row_err("ai_model_configs.name"))?,
            "provider": row.get::<_, String>(3).map_err(to_row_err("ai_model_configs.provider"))?,
            "model": row.get::<_, String>(4).map_err(to_row_err("ai_model_configs.model"))?,
            "base_url": to_bson_opt_string(row.get(5).map_err(to_row_err("ai_model_configs.base_url"))?),
            "api_key": to_bson_opt_string(row.get(6).map_err(to_row_err("ai_model_configs.api_key"))?),
            "supports_images": row.get::<_, i64>(7).map_err(to_row_err("ai_model_configs.supports_images"))?,
            "supports_reasoning": row.get::<_, i64>(8).map_err(to_row_err("ai_model_configs.supports_reasoning"))?,
            "supports_responses": row.get::<_, i64>(9).map_err(to_row_err("ai_model_configs.supports_responses"))?,
            "temperature": to_bson_opt_f64(row.get(10).map_err(to_row_err("ai_model_configs.temperature"))?),
            "thinking_level": to_bson_opt_string(row.get(11).map_err(to_row_err("ai_model_configs.thinking_level"))?),
            "enabled": row.get::<_, i64>(12).map_err(to_row_err("ai_model_configs.enabled"))?,
            "created_at": row.get::<_, String>(13).map_err(to_row_err("ai_model_configs.created_at"))?,
            "updated_at": row.get::<_, String>(14).map_err(to_row_err("ai_model_configs.updated_at"))?,
        };
        upsert(&coll, doc! {"id": &id}, doc).await?;
        count += 1;
    }

    println!("[MIGRATE] ai_model_configs: {}", count);
    Ok(count)
}

async fn migrate_auth_users(conn: &Connection, db: &Database) -> Result<usize, String> {
    let coll = db.collection::<Document>("auth_users");
    let mut stmt = conn
        .prepare("SELECT user_id, password_hash, role, created_at, updated_at FROM auth_users")
        .map_err(|e| e.to_string())?;
    let mut rows = stmt.query([]).map_err(|e| e.to_string())?;

    let mut count = 0usize;
    while let Some(row) = rows.next().map_err(|e| e.to_string())? {
        let user_id: String = row.get(0).map_err(to_row_err("auth_users.user_id"))?;
        let doc = doc! {
            "user_id": &user_id,
            "password_hash": row.get::<_, String>(1).map_err(to_row_err("auth_users.password_hash"))?,
            "role": row.get::<_, String>(2).map_err(to_row_err("auth_users.role"))?,
            "created_at": row.get::<_, String>(3).map_err(to_row_err("auth_users.created_at"))?,
            "updated_at": row.get::<_, String>(4).map_err(to_row_err("auth_users.updated_at"))?,
        };
        upsert(&coll, doc! {"user_id": &user_id}, doc).await?;
        count += 1;
    }

    println!("[MIGRATE] auth_users: {}", count);
    Ok(count)
}

async fn migrate_summary_job_configs(conn: &Connection, db: &Database) -> Result<usize, String> {
    let coll = db.collection::<Document>("summary_job_configs");
    let mut stmt = conn
        .prepare(
            "SELECT user_id, enabled, summary_model_config_id, token_limit, round_limit, target_summary_tokens, job_interval_seconds, max_sessions_per_tick, updated_at FROM summary_job_configs",
        )
        .map_err(|e| e.to_string())?;
    let mut rows = stmt.query([]).map_err(|e| e.to_string())?;

    let mut count = 0usize;
    while let Some(row) = rows.next().map_err(|e| e.to_string())? {
        let user_id: String = row.get(0).map_err(to_row_err("summary_job_configs.user_id"))?;
        let doc = doc! {
            "user_id": &user_id,
            "enabled": row.get::<_, i64>(1).map_err(to_row_err("summary_job_configs.enabled"))?,
            "summary_model_config_id": to_bson_opt_string(row.get(2).map_err(to_row_err("summary_job_configs.summary_model_config_id"))?),
            "token_limit": row.get::<_, i64>(3).map_err(to_row_err("summary_job_configs.token_limit"))?,
            "round_limit": row.get::<_, i64>(4).map_err(to_row_err("summary_job_configs.round_limit"))?,
            "target_summary_tokens": row.get::<_, i64>(5).map_err(to_row_err("summary_job_configs.target_summary_tokens"))?,
            "job_interval_seconds": row.get::<_, i64>(6).map_err(to_row_err("summary_job_configs.job_interval_seconds"))?,
            "max_sessions_per_tick": row.get::<_, i64>(7).map_err(to_row_err("summary_job_configs.max_sessions_per_tick"))?,
            "updated_at": row.get::<_, String>(8).map_err(to_row_err("summary_job_configs.updated_at"))?,
        };
        upsert(&coll, doc! {"user_id": &user_id}, doc).await?;
        count += 1;
    }

    println!("[MIGRATE] summary_job_configs: {}", count);
    Ok(count)
}

async fn migrate_summary_rollup_job_configs(
    conn: &Connection,
    db: &Database,
) -> Result<usize, String> {
    let coll = db.collection::<Document>("summary_rollup_job_configs");
    let mut stmt = conn
        .prepare(
            "SELECT user_id, enabled, summary_model_config_id, token_limit, round_limit, target_summary_tokens, job_interval_seconds, keep_raw_level0_count, max_level, max_sessions_per_tick, updated_at FROM summary_rollup_job_configs",
        )
        .map_err(|e| e.to_string())?;
    let mut rows = stmt.query([]).map_err(|e| e.to_string())?;

    let mut count = 0usize;
    while let Some(row) = rows.next().map_err(|e| e.to_string())? {
        let user_id: String = row
            .get(0)
            .map_err(to_row_err("summary_rollup_job_configs.user_id"))?;
        let doc = doc! {
            "user_id": &user_id,
            "enabled": row.get::<_, i64>(1).map_err(to_row_err("summary_rollup_job_configs.enabled"))?,
            "summary_model_config_id": to_bson_opt_string(row.get(2).map_err(to_row_err("summary_rollup_job_configs.summary_model_config_id"))?),
            "token_limit": row.get::<_, i64>(3).map_err(to_row_err("summary_rollup_job_configs.token_limit"))?,
            "round_limit": row.get::<_, i64>(4).map_err(to_row_err("summary_rollup_job_configs.round_limit"))?,
            "target_summary_tokens": row.get::<_, i64>(5).map_err(to_row_err("summary_rollup_job_configs.target_summary_tokens"))?,
            "job_interval_seconds": row.get::<_, i64>(6).map_err(to_row_err("summary_rollup_job_configs.job_interval_seconds"))?,
            "keep_raw_level0_count": row.get::<_, i64>(7).map_err(to_row_err("summary_rollup_job_configs.keep_raw_level0_count"))?,
            "max_level": row.get::<_, i64>(8).map_err(to_row_err("summary_rollup_job_configs.max_level"))?,
            "max_sessions_per_tick": row.get::<_, i64>(9).map_err(to_row_err("summary_rollup_job_configs.max_sessions_per_tick"))?,
            "updated_at": row.get::<_, String>(10).map_err(to_row_err("summary_rollup_job_configs.updated_at"))?,
        };
        upsert(&coll, doc! {"user_id": &user_id}, doc).await?;
        count += 1;
    }

    println!("[MIGRATE] summary_rollup_job_configs: {}", count);
    Ok(count)
}

async fn migrate_job_runs(conn: &Connection, db: &Database) -> Result<usize, String> {
    let coll = db.collection::<Document>("job_runs");
    let mut stmt = conn
        .prepare(
            "SELECT id, job_type, session_id, status, trigger_type, input_count, output_count, error_message, started_at, finished_at FROM job_runs",
        )
        .map_err(|e| e.to_string())?;
    let mut rows = stmt.query([]).map_err(|e| e.to_string())?;

    let mut count = 0usize;
    while let Some(row) = rows.next().map_err(|e| e.to_string())? {
        let id: String = row.get(0).map_err(to_row_err("job_runs.id"))?;
        let doc = doc! {
            "id": &id,
            "job_type": row.get::<_, String>(1).map_err(to_row_err("job_runs.job_type"))?,
            "session_id": to_bson_opt_string(row.get(2).map_err(to_row_err("job_runs.session_id"))?),
            "status": row.get::<_, String>(3).map_err(to_row_err("job_runs.status"))?,
            "trigger_type": to_bson_opt_string(row.get(4).map_err(to_row_err("job_runs.trigger_type"))?),
            "input_count": row.get::<_, i64>(5).map_err(to_row_err("job_runs.input_count"))?,
            "output_count": row.get::<_, i64>(6).map_err(to_row_err("job_runs.output_count"))?,
            "error_message": to_bson_opt_string(row.get(7).map_err(to_row_err("job_runs.error_message"))?),
            "started_at": row.get::<_, String>(8).map_err(to_row_err("job_runs.started_at"))?,
            "finished_at": to_bson_opt_string(row.get(9).map_err(to_row_err("job_runs.finished_at"))?),
        };
        upsert(&coll, doc! {"id": &id}, doc).await?;
        count += 1;
    }

    println!("[MIGRATE] job_runs: {}", count);
    Ok(count)
}

async fn upsert(coll: &Collection<Document>, filter: Document, replacement: Document) -> Result<(), String> {
    coll.replace_one(filter, replacement)
        .upsert(true)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn to_bson_opt_string(v: Option<String>) -> Bson {
    match v.and_then(|x| {
        let t = x.trim().to_string();
        if t.is_empty() {
            None
        } else {
            Some(t)
        }
    }) {
        Some(s) => Bson::String(s),
        None => Bson::Null,
    }
}

fn to_bson_opt_f64(v: Option<f64>) -> Bson {
    v.map(Bson::Double).unwrap_or(Bson::Null)
}

fn to_bson_opt_json(v: Option<String>) -> Bson {
    let Some(raw) = v else {
        return Bson::Null;
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Bson::Null;
    }

    match serde_json::from_str::<serde_json::Value>(trimmed) {
        Ok(value) => mongodb::bson::to_bson(&value).unwrap_or_else(|_| Bson::String(raw)),
        Err(_) => Bson::String(raw),
    }
}

fn default_string(v: Option<String>, default_value: &str) -> String {
    v.map(|x| x.trim().to_string())
        .filter(|x| !x.is_empty())
        .unwrap_or_else(|| default_value.to_string())
}

fn to_row_err(field: &'static str) -> impl FnOnce(rusqlite::Error) -> String {
    move |err| format!("row read failed ({field}): {err}")
}
