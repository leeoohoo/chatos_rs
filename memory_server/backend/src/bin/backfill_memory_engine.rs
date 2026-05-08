use futures_util::TryStreamExt;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;
use mongodb::{Collection, Database};
use reqwest::Client;
use serde::Deserialize;
use serde::Serialize;

#[path = "../bin_support/mongo_maintenance.rs"]
mod mongo_maintenance;

#[derive(Debug, Clone)]
struct CliArgs {
    mongo: mongo_maintenance::MongoCliArgs,
    engine_base_url: String,
    batch_size: usize,
    run_summary: bool,
}

#[derive(Debug, Default)]
struct BackfillStats {
    sessions_seen: usize,
    sessions_synced: usize,
    messages_seen: usize,
    message_batches_synced: usize,
    summaries_seen: usize,
    summaries_synced: usize,
    subject_memories_seen: usize,
    subject_memories_synced: usize,
    summaries_triggered: usize,
    skipped_sessions: usize,
    skipped_messages: usize,
    skipped_summaries: usize,
    skipped_subject_memories: usize,
}

#[derive(Debug, Clone, Serialize)]
struct EngineUpsertThreadRequest {
    tenant_id: String,
    source_id: String,
    subject_id: String,
    thread_type: String,
    external_thread_id: Option<String>,
    title: Option<String>,
    labels: Option<Vec<String>>,
    metadata: Option<serde_json::Value>,
    status: Option<String>,
    created_at: Option<String>,
    updated_at: Option<String>,
    archived_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct EngineBatchSyncRecordsRequest {
    tenant_id: String,
    source_id: String,
    records: Vec<EngineUpsertRecordInput>,
}

#[derive(Debug, Clone, Serialize)]
struct EngineUpsertRecordInput {
    id: String,
    external_record_id: Option<String>,
    role: String,
    record_type: String,
    content: String,
    structured_payload: Option<serde_json::Value>,
    metadata: Option<serde_json::Value>,
    summary_status: Option<String>,
    summary_id: Option<String>,
    summarized_at: Option<String>,
    created_at: String,
}

#[derive(Debug, Clone, Serialize)]
struct EngineUpsertSummaryRequest {
    tenant_id: String,
    source_id: String,
    subject_id: String,
    summary_type: String,
    level: Option<i64>,
    source_digest: Option<String>,
    summary_text: String,
    source_record_start_id: Option<String>,
    source_record_end_id: Option<String>,
    source_record_count: Option<i64>,
    status: Option<String>,
    rollup_status: Option<String>,
    rollup_summary_id: Option<String>,
    rolled_up_at: Option<String>,
    subject_memory_summarized: Option<i64>,
    subject_memory_summarized_at: Option<String>,
    metadata: Option<serde_json::Value>,
    created_at: Option<String>,
    updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct EngineUpsertSubjectMemoryRequest {
    id: Option<String>,
    tenant_id: String,
    source_id: String,
    memory_type: String,
    text: String,
    level: Option<i64>,
    source_digest: Option<String>,
    confidence: Option<f64>,
    last_seen_at: Option<String>,
    metadata: Option<serde_json::Value>,
    rollup_status: Option<String>,
    rollup_memory_key: Option<String>,
    rolled_up_at: Option<String>,
    status: Option<String>,
    created_at: Option<String>,
    updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LegacySession {
    id: String,
    user_id: String,
    project_id: Option<String>,
    title: Option<String>,
    metadata: Option<serde_json::Value>,
    status: String,
    created_at: String,
    updated_at: String,
    archived_at: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), String> {
    let args = parse_args()?;
    mongo_maintenance::print_mongo_cli_header("BACKFILL-MEMORY-ENGINE", &args.mongo);
    println!(
        "[BACKFILL-MEMORY-ENGINE] engine base url = {}",
        args.engine_base_url
    );
    println!(
        "[BACKFILL-MEMORY-ENGINE] batch size      = {}",
        args.batch_size
    );
    println!(
        "[BACKFILL-MEMORY-ENGINE] run summary     = {}",
        args.run_summary
    );

    let db = mongo_maintenance::connect_database(
        &args.mongo.target,
        "memory_engine_backfill",
    )
    .await?;

    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|err| err.to_string())?;

    let mut stats = BackfillStats::default();
    backfill_sessions(&db, &client, &args, &mut stats).await?;
    backfill_messages(&db, &client, &args, &mut stats).await?;
    backfill_summaries(&db, &client, &args, &mut stats).await?;
    backfill_subject_memories(&db, &client, &args, &mut stats).await?;

    println!("[BACKFILL-MEMORY-ENGINE] done");
    println!("  sessions_seen: {}", stats.sessions_seen);
    println!("  sessions_synced: {}", stats.sessions_synced);
    println!("  messages_seen: {}", stats.messages_seen);
    println!("  message_batches_synced: {}", stats.message_batches_synced);
    println!("  summaries_seen: {}", stats.summaries_seen);
    println!("  summaries_synced: {}", stats.summaries_synced);
    println!("  subject_memories_seen: {}", stats.subject_memories_seen);
    println!("  subject_memories_synced: {}", stats.subject_memories_synced);
    println!("  summaries_triggered: {}", stats.summaries_triggered);
    println!("  skipped_sessions: {}", stats.skipped_sessions);
    println!("  skipped_messages: {}", stats.skipped_messages);
    println!("  skipped_summaries: {}", stats.skipped_summaries);
    println!(
        "  skipped_subject_memories: {}",
        stats.skipped_subject_memories
    );
    Ok(())
}

fn parse_args() -> Result<CliArgs, String> {
    let mongo = mongo_maintenance::parse_mongo_cli_args("backfill_memory_engine")?;
    let engine_base_url = std::env::var("MEMORY_ENGINE_BASE_URL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "http://127.0.0.1:7081".to_string());
    let batch_size = std::env::var("MEMORY_ENGINE_BACKFILL_BATCH_SIZE")
        .ok()
        .and_then(|value| value.trim().parse::<usize>().ok())
        .unwrap_or(200)
        .max(1);
    let run_summary = std::env::var("MEMORY_ENGINE_BACKFILL_RUN_SUMMARY")
        .ok()
        .map(|value| value.to_lowercase() != "false")
        .unwrap_or(true);
    Ok(CliArgs {
        mongo,
        engine_base_url,
        batch_size,
        run_summary,
    })
}

async fn backfill_sessions(
    db: &Database,
    client: &Client,
    args: &CliArgs,
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
        .map_err(|err| err.to_string())?;

    while let Some(row) = cursor.try_next().await.map_err(|err| err.to_string())? {
        stats.sessions_seen += 1;

        let Some(session_id) = doc_string(&row, "id") else {
            stats.skipped_sessions += 1;
            continue;
        };
        let Some(user_id) = doc_string(&row, "user_id") else {
            stats.skipped_sessions += 1;
            continue;
        };
        if user_id.trim().is_empty() {
            stats.skipped_sessions += 1;
            continue;
        }

        let session = parse_legacy_session(&row)?;
        let request = EngineUpsertThreadRequest {
            tenant_id: user_id,
            source_id: "memory_server".to_string(),
            subject_id: format!("session:{session_id}"),
            thread_type: "chat".to_string(),
            external_thread_id: Some(session_id.clone()),
            title: session.title.clone(),
            labels: build_thread_labels(&session),
            metadata: build_session_mapping_metadata(&session),
            status: Some(session.status.clone()),
            created_at: Some(session.created_at.clone()),
            updated_at: Some(session.updated_at.clone()),
            archived_at: session.archived_at.clone(),
        };

        if args.mongo.dry_run {
            stats.sessions_synced += 1;
            continue;
        }

        put_json(
            client,
            format!(
                "{}/api/memory-engine/v1/threads/{}",
                args.engine_base_url.trim_end_matches('/'),
                urlencoding::encode(session_id.as_str())
            )
            .as_str(),
            &request,
        )
        .await?;
        stats.sessions_synced += 1;
    }

    Ok(())
}

async fn backfill_messages(
    db: &Database,
    client: &Client,
    args: &CliArgs,
    stats: &mut BackfillStats,
) -> Result<(), String> {
    let sessions_coll: Collection<Document> = db.collection("sessions");
    let messages_coll: Collection<Document> = db.collection("messages");
    let options = FindOptions::builder()
        .sort(doc! {"updated_at": 1, "created_at": 1})
        .build();
    let mut session_cursor = sessions_coll
        .find(doc! {})
        .with_options(options)
        .await
        .map_err(|err| err.to_string())?;

    while let Some(session) = session_cursor
        .try_next()
        .await
        .map_err(|err| err.to_string())?
    {
        let Some(session_id) = doc_string(&session, "id") else {
            continue;
        };
        let Some(user_id) = doc_string(&session, "user_id") else {
            continue;
        };
        if user_id.trim().is_empty() {
            continue;
        }

        let msg_options = FindOptions::builder()
            .sort(doc! {"created_at": 1})
            .build();
        let mut msg_cursor = messages_coll
            .find(doc! {"session_id": session_id.as_str()})
            .with_options(msg_options)
            .await
            .map_err(|err| err.to_string())?;

        let mut batch: Vec<EngineUpsertRecordInput> = Vec::new();
        while let Some(row) = msg_cursor.try_next().await.map_err(|err| err.to_string())? {
            stats.messages_seen += 1;
            let Some(message_id) = doc_string(&row, "id") else {
                stats.skipped_messages += 1;
                continue;
            };
            let Some(role) = doc_string(&row, "role") else {
                stats.skipped_messages += 1;
                continue;
            };
            let Some(content) = doc_string(&row, "content") else {
                stats.skipped_messages += 1;
                continue;
            };
            let created_at = doc_string(&row, "created_at").unwrap_or_else(now_rfc3339);

            batch.push(EngineUpsertRecordInput {
                id: message_id.clone(),
                external_record_id: Some(message_id),
                role,
                record_type: "message".to_string(),
                content,
                structured_payload: None,
                metadata: build_record_metadata(
                    doc_opt_string(&row, "message_mode"),
                    doc_opt_string(&row, "message_source"),
                    bson_json_value(row.get("tool_calls")),
                    doc_opt_string(&row, "tool_call_id"),
                    doc_opt_string(&row, "reasoning"),
                    bson_json_value(row.get("metadata")),
                ),
                summary_status: doc_opt_string(&row, "summary_status")
                    .or_else(|| Some("pending".to_string())),
                summary_id: doc_opt_string(&row, "summary_id"),
                summarized_at: doc_opt_string(&row, "summarized_at"),
                created_at,
            });

            if batch.len() >= args.batch_size {
                flush_message_batch(
                    client,
                    args,
                    session_id.as_str(),
                    user_id.as_str(),
                    &batch,
                )
                .await?;
                stats.message_batches_synced += 1;
                batch.clear();
            }
        }

        if !batch.is_empty() {
            flush_message_batch(
                client,
                args,
                session_id.as_str(),
                user_id.as_str(),
                &batch,
            )
            .await?;
            stats.message_batches_synced += 1;
        }

        if args.run_summary && !session_has_any_summaries(db, session_id.as_str()).await? {
            trigger_thread_summary(client, args, session_id.as_str(), user_id.as_str()).await?;
            stats.summaries_triggered += 1;
        }
    }

    Ok(())
}

async fn backfill_summaries(
    db: &Database,
    client: &Client,
    args: &CliArgs,
    stats: &mut BackfillStats,
) -> Result<(), String> {
    let coll: Collection<Document> = db.collection("session_summaries_v2");
    let options = FindOptions::builder()
        .sort(doc! {"created_at": 1, "updated_at": 1})
        .build();
    let mut cursor = coll
        .find(doc! {})
        .with_options(options)
        .await
        .map_err(|err| err.to_string())?;

    while let Some(row) = cursor.try_next().await.map_err(|err| err.to_string())? {
        stats.summaries_seen += 1;

        let Some(summary_id) = doc_string(&row, "id") else {
            stats.skipped_summaries += 1;
            continue;
        };
        let Some(session_id) = doc_string(&row, "session_id") else {
            stats.skipped_summaries += 1;
            continue;
        };
        let Some(summary_text) = doc_string(&row, "summary_text") else {
            stats.skipped_summaries += 1;
            continue;
        };

        let Some(session) = load_session_by_id(db, session_id.as_str()).await? else {
            stats.skipped_summaries += 1;
            continue;
        };
        let tenant_id = session.user_id.trim();
        if tenant_id.is_empty() {
            stats.skipped_summaries += 1;
            continue;
        }

        let trigger_type = doc_opt_string(&row, "trigger_type")
            .unwrap_or_else(|| "thread_incremental".to_string());
        let request = EngineUpsertSummaryRequest {
            tenant_id: tenant_id.to_string(),
            source_id: "memory_server".to_string(),
            subject_id: format!("session:{}", session.id),
            summary_type: map_trigger_type_to_summary_type(trigger_type.as_str()),
            level: Some(doc_i64(&row, "level").unwrap_or(0).max(0)),
            source_digest: doc_opt_string(&row, "source_digest"),
            summary_text,
            source_record_start_id: doc_opt_string(&row, "source_start_message_id"),
            source_record_end_id: doc_opt_string(&row, "source_end_message_id"),
            source_record_count: Some(doc_i64(&row, "source_message_count").unwrap_or(0).max(0)),
            status: Some(map_legacy_summary_status(
                doc_opt_string(&row, "status").as_deref(),
            )),
            rollup_status: Some(normalize_rollup_status(
                doc_opt_string(&row, "rollup_status").as_deref(),
            )),
            rollup_summary_id: doc_opt_string(&row, "rollup_summary_id"),
            rolled_up_at: doc_opt_string(&row, "rolled_up_at"),
            subject_memory_summarized: Some(doc_i64(&row, "agent_memory_summarized").unwrap_or(0)),
            subject_memory_summarized_at: doc_opt_string(&row, "agent_memory_summarized_at"),
            metadata: Some(build_summary_metadata(&row, trigger_type.as_str())),
            created_at: Some(
                doc_string(&row, "created_at").unwrap_or_else(now_rfc3339),
            ),
            updated_at: Some(
                doc_string(&row, "updated_at").unwrap_or_else(now_rfc3339),
            ),
        };

        if args.mongo.dry_run {
            stats.summaries_synced += 1;
            continue;
        }

        put_json(
            client,
            format!(
                "{}/api/memory-engine/v1/threads/{}/summaries/{}",
                args.engine_base_url.trim_end_matches('/'),
                urlencoding::encode(session_id.as_str()),
                urlencoding::encode(summary_id.as_str())
            )
            .as_str(),
            &request,
        )
        .await?;
        stats.summaries_synced += 1;
    }

    Ok(())
}

async fn backfill_subject_memories(
    db: &Database,
    client: &Client,
    args: &CliArgs,
    stats: &mut BackfillStats,
) -> Result<(), String> {
    backfill_agent_recalls(db, client, args, stats).await?;
    backfill_project_memories(db, client, args, stats).await
}

async fn backfill_agent_recalls(
    db: &Database,
    client: &Client,
    args: &CliArgs,
    stats: &mut BackfillStats,
) -> Result<(), String> {
    let coll: Collection<Document> = db.collection("agent_recalls");
    let options = FindOptions::builder().sort(doc! {"updated_at": 1}).build();
    let mut cursor = coll
        .find(doc! {})
        .with_options(options)
        .await
        .map_err(|err| err.to_string())?;

    while let Some(row) = cursor.try_next().await.map_err(|err| err.to_string())? {
        stats.subject_memories_seen += 1;

        let Some(id) = doc_string(&row, "id") else {
            stats.skipped_subject_memories += 1;
            continue;
        };
        let Some(user_id) = doc_string(&row, "user_id") else {
            stats.skipped_subject_memories += 1;
            continue;
        };
        let Some(agent_id) = doc_string(&row, "agent_id") else {
            stats.skipped_subject_memories += 1;
            continue;
        };
        let Some(recall_key) = doc_string(&row, "recall_key") else {
            stats.skipped_subject_memories += 1;
            continue;
        };
        let Some(recall_text) = doc_string(&row, "recall_text") else {
            stats.skipped_subject_memories += 1;
            continue;
        };

        let subject_id = format!("agent:{agent_id}");
        let request = EngineUpsertSubjectMemoryRequest {
            id: Some(id),
            tenant_id: user_id,
            source_id: "memory_server".to_string(),
            memory_type: "agent_recall".to_string(),
            text: recall_text,
            level: Some(doc_i64(&row, "level").unwrap_or(0).max(0)),
            source_digest: doc_opt_string(&row, "source_digest"),
            confidence: doc_f64(&row, "confidence"),
            last_seen_at: doc_opt_string(&row, "last_seen_at"),
            metadata: Some(serde_json::json!({
                "relation_subject_id": subject_id,
                "rollup_recall_key": doc_opt_string(&row, "rollup_recall_key"),
            })),
            rollup_status: Some(if doc_i64(&row, "rolled_up").unwrap_or(0) == 1 {
                "done".to_string()
            } else {
                "pending".to_string()
            }),
            rollup_memory_key: doc_opt_string(&row, "rollup_recall_key"),
            rolled_up_at: doc_opt_string(&row, "rolled_up_at"),
            status: Some("active".to_string()),
            created_at: Some(doc_string(&row, "updated_at").unwrap_or_else(now_rfc3339)),
            updated_at: Some(doc_string(&row, "updated_at").unwrap_or_else(now_rfc3339)),
        };

        if args.mongo.dry_run {
            stats.subject_memories_synced += 1;
            continue;
        }

        put_json(
            client,
            format!(
                "{}/api/memory-engine/v1/subjects/{}/memories/{}",
                args.engine_base_url.trim_end_matches('/'),
                urlencoding::encode(subject_id.as_str()),
                urlencoding::encode(recall_key.as_str())
            )
            .as_str(),
            &request,
        )
        .await?;
        stats.subject_memories_synced += 1;
    }

    Ok(())
}

async fn backfill_project_memories(
    db: &Database,
    client: &Client,
    args: &CliArgs,
    stats: &mut BackfillStats,
) -> Result<(), String> {
    let coll: Collection<Document> = db.collection("project_memories");
    let options = FindOptions::builder().sort(doc! {"updated_at": 1}).build();
    let mut cursor = coll
        .find(doc! {})
        .with_options(options)
        .await
        .map_err(|err| err.to_string())?;

    while let Some(row) = cursor.try_next().await.map_err(|err| err.to_string())? {
        let Some(id) = doc_string(&row, "id") else {
            stats.skipped_subject_memories += 1;
            continue;
        };
        let Some(user_id) = doc_string(&row, "user_id") else {
            stats.skipped_subject_memories += 1;
            continue;
        };
        let contact_id = doc_opt_string(&row, "contact_id").unwrap_or_default();
        let agent_id = doc_opt_string(&row, "agent_id").unwrap_or_default();
        let project_id = normalize_project_id(doc_opt_string(&row, "project_id"));
        let Some(memory_text) = doc_string(&row, "memory_text") else {
            stats.skipped_subject_memories += 1;
            continue;
        };

        let related = build_project_memory_subject_targets(
            contact_id.as_str(),
            agent_id.as_str(),
            project_id.as_str(),
        );
        if related.is_empty() {
            stats.skipped_subject_memories += 1;
            continue;
        }

        for (subject_id, memory_key) in related {
            stats.subject_memories_seen += 1;

            let request = EngineUpsertSubjectMemoryRequest {
                id: Some(format!("{id}:{memory_key}")),
                tenant_id: user_id.clone(),
                source_id: "memory_server".to_string(),
                memory_type: "project_memory".to_string(),
                text: memory_text.clone(),
                level: Some(0),
                source_digest: None,
                confidence: None,
                last_seen_at: doc_opt_string(&row, "last_source_at"),
                metadata: Some(serde_json::json!({
                    "legacy_session_mapping": {
                        "session_id": serde_json::Value::Null,
                        "project_id": project_id,
                        "contact_id": if contact_id.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(contact_id.clone()) },
                        "agent_id": if agent_id.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(agent_id.clone()) },
                    }
                })),
                rollup_status: Some(if doc_i64(&row, "recall_summarized").unwrap_or(0) == 1 {
                    "done".to_string()
                } else {
                    "pending".to_string()
                }),
                rollup_memory_key: None,
                rolled_up_at: doc_opt_string(&row, "recall_summarized_at"),
                status: Some("active".to_string()),
                created_at: Some(doc_string(&row, "updated_at").unwrap_or_else(now_rfc3339)),
                updated_at: Some(doc_string(&row, "updated_at").unwrap_or_else(now_rfc3339)),
            };

            if args.mongo.dry_run {
                stats.subject_memories_synced += 1;
                continue;
            }

            put_json(
                client,
                format!(
                    "{}/api/memory-engine/v1/subjects/{}/memories/{}",
                    args.engine_base_url.trim_end_matches('/'),
                    urlencoding::encode(subject_id.as_str()),
                    urlencoding::encode(memory_key.as_str())
                )
                .as_str(),
                &request,
            )
            .await?;
            stats.subject_memories_synced += 1;
        }
    }

    Ok(())
}

async fn flush_message_batch(
    client: &Client,
    args: &CliArgs,
    session_id: &str,
    user_id: &str,
    batch: &[EngineUpsertRecordInput],
) -> Result<(), String> {
    if args.mongo.dry_run {
        return Ok(());
    }

    let request = EngineBatchSyncRecordsRequest {
        tenant_id: user_id.to_string(),
        source_id: "memory_server".to_string(),
        records: batch.to_vec(),
    };

    put_json(
        client,
        format!(
            "{}/api/memory-engine/v1/threads/{}/records/batch-sync",
            args.engine_base_url.trim_end_matches('/'),
            urlencoding::encode(session_id)
        )
        .as_str(),
        &request,
    )
    .await
}

async fn put_json<T: Serialize>(client: &Client, url: &str, body: &T) -> Result<(), String> {
    let response = client
        .put(url)
        .json(body)
        .send()
        .await
        .map_err(|err| format!("request failed: {err}"))?;

    if response.status().is_success() {
        return Ok(());
    }

    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    Err(format!("request failed: status={} body={}", status, body))
}

async fn post_json<T: Serialize>(client: &Client, url: &str, body: &T) -> Result<(), String> {
    let response = client
        .post(url)
        .json(body)
        .send()
        .await
        .map_err(|err| format!("request failed: {err}"))?;

    if response.status().is_success() {
        return Ok(());
    }

    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    Err(format!("request failed: status={} body={}", status, body))
}

async fn trigger_thread_summary(
    client: &Client,
    args: &CliArgs,
    session_id: &str,
    user_id: &str,
) -> Result<(), String> {
    if args.mongo.dry_run {
        return Ok(());
    }

    let request = serde_json::json!({
        "tenant_id": user_id,
        "source_id": "memory_server",
        "max_records": 200
    });

    post_json(
        client,
        format!(
            "{}/api/memory-engine/v1/threads/{}/summaries/run",
            args.engine_base_url.trim_end_matches('/'),
            urlencoding::encode(session_id)
        )
        .as_str(),
        &request,
    )
    .await
}

fn doc_string(row: &Document, key: &str) -> Option<String> {
    row.get_str(key).ok().map(|value| value.to_string())
}

fn doc_opt_string(row: &Document, key: &str) -> Option<String> {
    row.get(key).and_then(|value| value.as_str()).map(|value| value.to_string())
}

fn doc_i64(row: &Document, key: &str) -> Option<i64> {
    row.get(key).and_then(|value| match value {
        mongodb::bson::Bson::Int32(v) => Some(*v as i64),
        mongodb::bson::Bson::Int64(v) => Some(*v),
        mongodb::bson::Bson::Double(v) => Some(*v as i64),
        _ => None,
    })
}

fn doc_f64(row: &Document, key: &str) -> Option<f64> {
    row.get(key).and_then(|value| match value {
        mongodb::bson::Bson::Int32(v) => Some(*v as f64),
        mongodb::bson::Bson::Int64(v) => Some(*v as f64),
        mongodb::bson::Bson::Double(v) => Some(*v),
        _ => None,
    })
}

fn normalized_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToOwned::to_owned)
}

fn metadata_string(metadata: Option<&serde_json::Value>, path: &[&str]) -> Option<String> {
    let mut cursor = metadata?;
    for key in path {
        cursor = cursor.get(*key)?;
    }
    normalized_text(cursor.as_str())
}

fn contact_id_from_metadata(metadata: Option<&serde_json::Value>) -> Option<String> {
    metadata_string(metadata, &["contact", "contact_id"])
        .or_else(|| metadata_string(metadata, &["contact", "contactId"]))
        .or_else(|| metadata_string(metadata, &["ui_contact", "contact_id"]))
        .or_else(|| metadata_string(metadata, &["ui_contact", "contactId"]))
        .or_else(|| metadata_string(metadata, &["chat_runtime", "contact_id"]))
        .or_else(|| metadata_string(metadata, &["chat_runtime", "contactId"]))
}

fn agent_id_from_metadata(metadata: Option<&serde_json::Value>) -> Option<String> {
    metadata_string(metadata, &["contact", "agent_id"])
        .or_else(|| metadata_string(metadata, &["contact", "agentId"]))
        .or_else(|| metadata_string(metadata, &["ui_contact", "agent_id"]))
        .or_else(|| metadata_string(metadata, &["ui_contact", "agentId"]))
        .or_else(|| metadata_string(metadata, &["ui_chat_selection", "selected_agent_id"]))
        .or_else(|| metadata_string(metadata, &["ui_chat_selection", "selectedAgentId"]))
        .or_else(|| metadata_string(metadata, &["chat_runtime", "contact_agent_id"]))
        .or_else(|| metadata_string(metadata, &["chat_runtime", "contactAgentId"]))
}

fn project_id_from_metadata(metadata: Option<&serde_json::Value>) -> Option<String> {
    metadata_string(metadata, &["chat_runtime", "project_id"])
        .or_else(|| metadata_string(metadata, &["chat_runtime", "projectId"]))
}

fn build_session_mapping_metadata(session: &LegacySession) -> Option<serde_json::Value> {
    let original = session.metadata.clone();
    let metadata_ref = original.as_ref();
    let project_id = normalized_text(session.project_id.as_deref())
        .or_else(|| project_id_from_metadata(metadata_ref));
    let contact_id = contact_id_from_metadata(metadata_ref);
    let agent_id = agent_id_from_metadata(metadata_ref);

    Some(serde_json::json!({
        "legacy_session_mapping": {
            "session_id": session.id,
            "project_id": project_id,
            "contact_id": contact_id,
            "agent_id": agent_id,
        },
        "source_metadata": original
    }))
}

fn build_thread_labels(session: &LegacySession) -> Option<Vec<String>> {
    let metadata_ref = session.metadata.as_ref();
    let project_id = normalized_text(session.project_id.as_deref())
        .or_else(|| project_id_from_metadata(metadata_ref));
    let contact_id = contact_id_from_metadata(metadata_ref);
    let agent_id = agent_id_from_metadata(metadata_ref);

    let mut labels = Vec::new();
    if let Some(project_id) = project_id.clone() {
        labels.push(format!("project:{project_id}"));
        if let Some(contact_id) = contact_id.clone() {
            labels.push(format!("contact_project:{contact_id}:{project_id}"));
        }
        if let Some(agent_id) = agent_id.clone() {
            labels.push(format!("agent_project:{agent_id}:{project_id}"));
        }
    }
    if let Some(contact_id) = contact_id {
        labels.push(format!("contact:{contact_id}"));
    }
    if let Some(agent_id) = agent_id {
        labels.push(format!("agent:{agent_id}"));
    }

    if labels.is_empty() {
        None
    } else {
        Some(labels)
    }
}

fn build_record_metadata(
    message_mode: Option<String>,
    message_source: Option<String>,
    tool_calls: Option<serde_json::Value>,
    tool_call_id: Option<String>,
    reasoning: Option<String>,
    metadata: Option<serde_json::Value>,
) -> Option<serde_json::Value> {
    let mut merged = match metadata {
        Some(serde_json::Value::Object(map)) => map,
        Some(other) => {
            let mut map = serde_json::Map::new();
            map.insert("legacy_metadata".to_string(), other);
            map
        }
        None => serde_json::Map::new(),
    };

    if let Some(value) = message_mode.filter(|v| !v.trim().is_empty()) {
        merged.insert("message_mode".to_string(), serde_json::Value::String(value));
    }
    if let Some(value) = message_source.filter(|v| !v.trim().is_empty()) {
        merged.insert("message_source".to_string(), serde_json::Value::String(value));
    }
    if let Some(value) = tool_calls {
        merged.insert("tool_calls".to_string(), value);
    }
    if let Some(value) = tool_call_id.filter(|v| !v.trim().is_empty()) {
        merged.insert("tool_call_id".to_string(), serde_json::Value::String(value));
    }
    if let Some(value) = reasoning.filter(|v| !v.trim().is_empty()) {
        merged.insert("reasoning".to_string(), serde_json::Value::String(value));
    }

    if merged.is_empty() {
        None
    } else {
        Some(serde_json::Value::Object(merged))
    }
}

fn build_summary_metadata(row: &Document, trigger_type: &str) -> serde_json::Value {
    serde_json::json!({
        "summary_model": doc_opt_string(row, "summary_model").unwrap_or_else(|| "legacy".to_string()),
        "legacy_trigger_type": trigger_type,
        "legacy_status": doc_opt_string(row, "status"),
        "legacy_error_message": doc_opt_string(row, "error_message"),
        "source_estimated_tokens": doc_i64(row, "source_estimated_tokens").unwrap_or(0),
    })
}

fn map_trigger_type_to_summary_type(trigger_type: &str) -> String {
    match trigger_type.trim() {
        "review_repair" => "thread_repair".to_string(),
        other if !other.is_empty() => "thread_incremental".to_string(),
        _ => "thread_incremental".to_string(),
    }
}

fn map_legacy_summary_status(status: Option<&str>) -> String {
    match status.map(str::trim).filter(|value| !value.is_empty()) {
        Some("summarized") => "done".to_string(),
        Some("done") => "done".to_string(),
        Some("failed") => "failed".to_string(),
        Some("pending") => "pending".to_string(),
        Some(other) => other.to_string(),
        None => "done".to_string(),
    }
}

fn normalize_rollup_status(status: Option<&str>) -> String {
    match status.map(str::trim).filter(|value| !value.is_empty()) {
        Some("summarized") => "done".to_string(),
        Some("done") => "done".to_string(),
        Some("pending") => "pending".to_string(),
        Some(other) => other.to_string(),
        None => "pending".to_string(),
    }
}

fn normalize_project_id(project_id: Option<String>) -> String {
    project_id
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "0".to_string())
}

fn build_project_memory_subject_targets(
    contact_id: &str,
    agent_id: &str,
    project_id: &str,
) -> Vec<(String, String)> {
    let mut out = Vec::new();
    if !contact_id.trim().is_empty() {
        out.push((
            format!("contact_project:{}:{}", contact_id.trim(), project_id),
            format!("project_memory:contact:{}:{}", contact_id.trim(), project_id),
        ));
    }
    if !agent_id.trim().is_empty() {
        out.push((
            format!("agent_project:{}:{}", agent_id.trim(), project_id),
            format!("project_memory:agent:{}:{}", agent_id.trim(), project_id),
        ));
    }
    out.push((
        format!("project:{project_id}"),
        format!("project_memory:project:{project_id}"),
    ));
    out
}

async fn load_session_by_id(db: &Database, session_id: &str) -> Result<Option<LegacySession>, String> {
    let coll: Collection<Document> = db.collection("sessions");
    let row = coll
        .find_one(doc! {"id": session_id})
        .await
        .map_err(|err| err.to_string())?;
    row.map(|doc| parse_legacy_session(&doc)).transpose()
}

fn parse_legacy_session(row: &Document) -> Result<LegacySession, String> {
    Ok(LegacySession {
        id: doc_string(row, "id").ok_or_else(|| "session.id missing".to_string())?,
        user_id: doc_string(row, "user_id").ok_or_else(|| "session.user_id missing".to_string())?,
        project_id: doc_opt_string(row, "project_id"),
        title: doc_opt_string(row, "title"),
        metadata: bson_json_value(row.get("metadata")),
        status: doc_string(row, "status").unwrap_or_else(|| "active".to_string()),
        created_at: doc_string(row, "created_at").unwrap_or_else(now_rfc3339),
        updated_at: doc_string(row, "updated_at").unwrap_or_else(now_rfc3339),
        archived_at: doc_opt_string(row, "archived_at"),
    })
}

async fn session_has_any_summaries(db: &Database, session_id: &str) -> Result<bool, String> {
    let coll: Collection<Document> = db.collection("session_summaries_v2");
    coll.find_one(doc! {"session_id": session_id})
        .await
        .map(|value| value.is_some())
        .map_err(|err| err.to_string())
}

fn bson_json_value(value: Option<&mongodb::bson::Bson>) -> Option<serde_json::Value> {
    value.and_then(|item| mongodb::bson::from_bson(item.clone()).ok())
}

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}
