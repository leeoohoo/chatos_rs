// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chrono::{DateTime, Utc};
use sqlx::FromRow;

use crate::db::Db;
use crate::models::{AgentConversationItem, AgentDirectoryItem, AgentProjectItem};

#[derive(Debug, FromRow)]
struct AgentDirectoryRow {
    tenant_id: String,
    source_id: String,
    agent_subject_id: String,
    display_name: Option<String>,
    thread_count: i64,
    project_count: i64,
    memory_count: i64,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, FromRow)]
struct AgentProjectRow {
    tenant_id: String,
    source_id: String,
    project_subject_id: String,
    display_name: Option<String>,
    thread_count: i64,
    memory_count: i64,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, FromRow)]
struct AgentConversationRow {
    tenant_id: String,
    source_id: String,
    agent_subject_id: String,
    thread_id: String,
    project_subject_id: Option<String>,
    project_display_name: Option<String>,
    conversation_id: Option<String>,
    thread_kind: String,
    record_count: i64,
    status: String,
    summary_status: String,
    updated_at: DateTime<Utc>,
}

pub async fn list_agents(
    db: &Db,
    tenant_id: Option<&str>,
    source_id: Option<&str>,
    agent_subject_id: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<AgentDirectoryItem>, String> {
    let rows = sqlx::query_as::<_, AgentDirectoryRow>(
        "WITH thread_identities AS ( \
           SELECT thread.id,thread.tenant_id,thread.source_id,thread.updated_at, \
                  thread.subject_id AS agent_subject_id,labels.project_subject_id, \
                  COALESCE(NULLIF(BTRIM(thread.data #>> '{metadata,agent_name}'),''), \
                           NULLIF(BTRIM(subject.data->>'display_name'),'')) AS display_name \
             FROM engine_threads thread \
             LEFT JOIN engine_subjects subject \
               ON subject.tenant_id=thread.tenant_id AND subject.source_id=thread.source_id \
              AND subject.subject_id=thread.subject_id \
             LEFT JOIN LATERAL ( \
               SELECT MAX(label) FILTER (WHERE label LIKE 'project:%') AS project_subject_id \
                 FROM jsonb_array_elements_text( \
                   CASE WHEN jsonb_typeof(thread.data->'labels')='array' \
                        THEN thread.data->'labels' ELSE '[]'::jsonb END \
                 ) AS labels(label) \
             ) labels ON TRUE \
            WHERE ($1::text IS NULL OR thread.tenant_id=$1) \
              AND ($2::text IS NULL OR thread.source_id=$2) \
              AND thread.subject_id LIKE 'agent:%' \
              AND thread.subject_id NOT IN ('agent:main-chat','agent:main_chat','agent:task-execution','agent:task_execution') \
              AND thread.data->'labels' ? 'memory-schema:v2' \
              AND thread.data->'labels' ? thread.subject_id \
         ), project_identities AS ( \
           SELECT tenant_id,source_id,agent_subject_id,project_subject_id,updated_at \
             FROM thread_identities \
            WHERE agent_subject_id IS NOT NULL AND project_subject_id IS NOT NULL \
           UNION \
           SELECT thread.tenant_id,thread.source_id,thread.agent_subject_id, \
                  CASE WHEN BTRIM(record.data #>> '{metadata,memory_project_id}') LIKE 'project:%' \
                       THEN BTRIM(record.data #>> '{metadata,memory_project_id}') \
                       ELSE 'project:' || BTRIM(record.data #>> '{metadata,memory_project_id}') END, \
                  record.created_at \
             FROM engine_records record \
             JOIN thread_identities thread \
               ON thread.tenant_id=record.tenant_id AND thread.source_id=record.source_id \
              AND thread.id=record.thread_id \
            WHERE thread.agent_subject_id IS NOT NULL \
              AND NULLIF(BTRIM(COALESCE( \
                    record.data #>> '{metadata,memory_project_id}', '')), '') IS NOT NULL \
         ), agent_identities AS ( \
           SELECT tenant_id,source_id,agent_subject_id,display_name,updated_at \
             FROM thread_identities \
         ), agents AS ( \
           SELECT tenant_id,source_id,agent_subject_id, \
                  (ARRAY_AGG(display_name ORDER BY updated_at DESC) \
                    FILTER (WHERE display_name IS NOT NULL))[1] AS display_name, \
                  MAX(updated_at) AS identity_updated_at \
             FROM agent_identities \
            WHERE ($1::text IS NULL OR tenant_id=$1) \
              AND ($2::text IS NULL OR source_id=$2) \
              AND ($3::text IS NULL OR agent_subject_id=$3) \
            GROUP BY tenant_id,source_id,agent_subject_id \
         ), thread_stats AS ( \
           SELECT a.tenant_id,a.source_id,a.agent_subject_id,COUNT(DISTINCT t.id)::bigint AS thread_count, \
                  MAX(t.updated_at) AS latest_thread_at \
             FROM agents a LEFT JOIN thread_identities t \
               ON t.tenant_id=a.tenant_id AND t.source_id=a.source_id \
              AND t.agent_subject_id=a.agent_subject_id \
            GROUP BY a.tenant_id,a.source_id,a.agent_subject_id \
         ), project_stats AS ( \
           SELECT a.tenant_id,a.source_id,a.agent_subject_id, \
                  COUNT(DISTINCT t.project_subject_id)::bigint AS project_count \
             FROM agents a LEFT JOIN project_identities t \
               ON t.tenant_id=a.tenant_id AND t.source_id=a.source_id \
              AND t.agent_subject_id=a.agent_subject_id \
            GROUP BY a.tenant_id,a.source_id,a.agent_subject_id \
         ), memory_stats AS ( \
           SELECT a.tenant_id,a.source_id,a.agent_subject_id,COUNT(m.id)::bigint AS memory_count, \
                  MAX(m.updated_at) AS latest_memory_at \
             FROM agents a LEFT JOIN engine_subject_memories m \
               ON m.tenant_id=a.tenant_id AND m.source_id=a.source_id \
              AND m.subject_id=a.agent_subject_id AND m.memory_type='agent_memory' AND m.status='active' \
            GROUP BY a.tenant_id,a.source_id,a.agent_subject_id \
         ) \
         SELECT a.tenant_id,a.source_id,a.agent_subject_id,a.display_name, \
                ts.thread_count,ps.project_count,ms.memory_count, \
                GREATEST(a.identity_updated_at,ts.latest_thread_at,ms.latest_memory_at) AS updated_at \
           FROM agents a \
           JOIN thread_stats ts USING (tenant_id,source_id,agent_subject_id) \
           JOIN project_stats ps USING (tenant_id,source_id,agent_subject_id) \
           JOIN memory_stats ms USING (tenant_id,source_id,agent_subject_id) \
          ORDER BY updated_at DESC,a.agent_subject_id ASC LIMIT $4 OFFSET $5",
    )
    .bind(normalized(tenant_id))
    .bind(normalized(source_id))
    .bind(normalized(agent_subject_id))
    .bind(limit.clamp(1, 500))
    .bind(offset.max(0))
    .fetch_all(db)
    .await
    .map_err(|error| error.to_string())?;

    Ok(rows
        .into_iter()
        .map(|row| AgentDirectoryItem {
            agent_id: row
                .agent_subject_id
                .strip_prefix("agent:")
                .unwrap_or(row.agent_subject_id.as_str())
                .to_string(),
            tenant_id: row.tenant_id,
            source_id: row.source_id,
            agent_subject_id: row.agent_subject_id,
            display_name: row.display_name,
            thread_count: row.thread_count,
            project_count: row.project_count,
            memory_count: row.memory_count,
            updated_at: row.updated_at.to_rfc3339(),
        })
        .collect())
}

pub async fn list_agent_projects(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    agent_subject_id: &str,
) -> Result<Vec<AgentProjectItem>, String> {
    let rows = sqlx::query_as::<_, AgentProjectRow>(
        "WITH thread_identities AS ( \
           SELECT thread.id,thread.tenant_id,thread.source_id,thread.updated_at, \
                  thread.subject_id AS agent_subject_id,labels.project_subject_id \
             FROM engine_threads thread \
             LEFT JOIN LATERAL ( \
               SELECT MAX(label) FILTER (WHERE label LIKE 'project:%') AS project_subject_id \
                 FROM jsonb_array_elements_text( \
                   CASE WHEN jsonb_typeof(thread.data->'labels')='array' \
                        THEN thread.data->'labels' ELSE '[]'::jsonb END \
                 ) AS labels(label) \
             ) labels ON TRUE \
            WHERE thread.tenant_id=$1 AND thread.source_id=$2 \
              AND thread.subject_id LIKE 'agent:%' \
              AND thread.subject_id NOT IN ('agent:main-chat','agent:main_chat','agent:task-execution','agent:task_execution') \
              AND thread.data->'labels' ? 'memory-schema:v2' \
              AND thread.data->'labels' ? thread.subject_id \
         ), matching_threads AS ( \
           SELECT id,tenant_id,source_id,updated_at,project_subject_id \
             FROM thread_identities \
            WHERE agent_subject_id=$3 \
         ), project_threads AS ( \
           SELECT thread.tenant_id,thread.source_id,thread.id,thread.updated_at, \
                  thread.project_subject_id \
             FROM matching_threads thread \
            WHERE thread.project_subject_id IS NOT NULL \
           UNION \
           SELECT thread.tenant_id,thread.source_id,thread.id, \
                  GREATEST(thread.updated_at,record.created_at), \
                  CASE WHEN BTRIM(record.data #>> '{metadata,memory_project_id}') LIKE 'project:%' \
                       THEN BTRIM(record.data #>> '{metadata,memory_project_id}') \
                       ELSE 'project:' || BTRIM(record.data #>> '{metadata,memory_project_id}') END \
             FROM matching_threads thread \
             JOIN engine_records record \
               ON record.tenant_id=thread.tenant_id AND record.source_id=thread.source_id \
              AND record.thread_id=thread.id \
            WHERE NULLIF(BTRIM(COALESCE( \
                    record.data #>> '{metadata,memory_project_id}', '')), '') IS NOT NULL \
         ) \
         SELECT project.tenant_id,project.source_id,project.project_subject_id, \
                NULLIF(BTRIM(subject.data->>'display_name'),'') AS display_name, \
                COUNT(DISTINCT project.id)::bigint AS thread_count, \
                COUNT(DISTINCT memory.id)::bigint AS memory_count, \
                GREATEST(MAX(project.updated_at),MAX(memory.updated_at)) AS updated_at \
           FROM project_threads project \
           LEFT JOIN engine_subjects subject \
             ON subject.tenant_id=project.tenant_id AND subject.source_id=project.source_id \
            AND subject.subject_id=project.project_subject_id \
           LEFT JOIN engine_subject_memories memory \
             ON memory.tenant_id=project.tenant_id AND memory.source_id=project.source_id \
            AND memory.subject_id=project.project_subject_id \
            AND memory.memory_type='project_summary' AND memory.status='active' \
          GROUP BY project.tenant_id,project.source_id,project.project_subject_id,subject.data \
          ORDER BY updated_at DESC,project.project_subject_id ASC",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(agent_subject_id)
    .fetch_all(db)
    .await
    .map_err(|error| error.to_string())?;

    Ok(rows
        .into_iter()
        .map(|row| AgentProjectItem {
            project_id: row
                .project_subject_id
                .strip_prefix("project:")
                .unwrap_or(row.project_subject_id.as_str())
                .to_string(),
            tenant_id: row.tenant_id,
            source_id: row.source_id,
            project_subject_id: row.project_subject_id,
            display_name: row.display_name,
            thread_count: row.thread_count,
            memory_count: row.memory_count,
            updated_at: row.updated_at.to_rfc3339(),
        })
        .collect())
}

pub async fn list_agent_conversations(
    db: &Db,
    tenant_id: &str,
    source_id: &str,
    agent_subject_id: &str,
) -> Result<Vec<AgentConversationItem>, String> {
    let rows = sqlx::query_as::<_, AgentConversationRow>(
        "WITH matching_threads AS ( \
           SELECT thread.id,thread.tenant_id,thread.source_id,thread.subject_id AS agent_subject_id, \
                  thread.status,thread.summary_status,thread.updated_at, \
                  labels.project_subject_id,labels.thread_kind \
             FROM engine_threads thread \
             LEFT JOIN LATERAL ( \
               SELECT MAX(label) FILTER (WHERE label LIKE 'project:%') AS project_subject_id, \
                      COALESCE( \
                        MAX(SUBSTRING(label FROM 13)) FILTER (WHERE label LIKE 'thread-kind:%'), \
                        NULLIF(BTRIM(COALESCE(thread.data #>> '{metadata,thread_kind}','')),''), \
                        NULLIF(BTRIM(thread.thread_type),''), \
                        'client_agent' \
                      ) AS thread_kind \
                 FROM jsonb_array_elements_text( \
                   CASE WHEN jsonb_typeof(thread.data->'labels')='array' \
                        THEN thread.data->'labels' ELSE '[]'::jsonb END \
                 ) AS labels(label) \
             ) labels ON TRUE \
            WHERE thread.tenant_id=$1 AND thread.source_id=$2 AND thread.subject_id=$3 \
              AND thread.data->'labels' ? 'memory-schema:v2' \
              AND thread.data->'labels' ? thread.subject_id \
         ), grouped AS ( \
           SELECT thread.tenant_id,thread.source_id,thread.agent_subject_id,thread.id AS thread_id, \
                  CASE WHEN thread.thread_kind IN ('agent_task','todo_execution','agent_team_run') \
                       THEN thread.project_subject_id \
                       WHEN thread.thread_kind='agent_conversation' THEN NULL \
                       ELSE COALESCE( \
                    CASE WHEN BTRIM(record.data #>> '{metadata,memory_project_id}') LIKE 'project:%' \
                         THEN BTRIM(record.data #>> '{metadata,memory_project_id}') \
                         WHEN NULLIF(BTRIM(COALESCE(record.data #>> '{metadata,memory_project_id}','')),'') IS NOT NULL \
                         THEN 'project:' || BTRIM(record.data #>> '{metadata,memory_project_id}') END, \
                    thread.project_subject_id \
                  ) END AS project_subject_id, \
                  CASE WHEN thread.thread_kind IN ('agent_task','todo_execution','agent_team_run','agent_conversation') THEN NULL \
                       ELSE NULLIF(BTRIM(COALESCE(record.data #>> '{metadata,conversation_id}','')),'') END AS conversation_id, \
                  thread.thread_kind,COUNT(record.id)::bigint AS record_count,thread.status,thread.summary_status, \
                  GREATEST(thread.updated_at,COALESCE(MAX(record.created_at),thread.updated_at)) AS updated_at \
             FROM matching_threads thread \
             LEFT JOIN engine_records record \
               ON record.tenant_id=thread.tenant_id AND record.source_id=thread.source_id \
              AND record.thread_id=thread.id \
            GROUP BY thread.tenant_id,thread.source_id,thread.agent_subject_id,thread.id, \
                     thread.project_subject_id,thread.thread_kind,thread.status,thread.summary_status,thread.updated_at, \
                     CASE WHEN thread.thread_kind IN ('agent_task','todo_execution','agent_team_run') \
                          THEN thread.project_subject_id \
                          WHEN thread.thread_kind='agent_conversation' THEN NULL \
                          ELSE COALESCE( \
                       CASE WHEN BTRIM(record.data #>> '{metadata,memory_project_id}') LIKE 'project:%' \
                            THEN BTRIM(record.data #>> '{metadata,memory_project_id}') \
                            WHEN NULLIF(BTRIM(COALESCE(record.data #>> '{metadata,memory_project_id}','')),'') IS NOT NULL \
                            THEN 'project:' || BTRIM(record.data #>> '{metadata,memory_project_id}') END, \
                       thread.project_subject_id \
                     ) END, \
                     CASE WHEN thread.thread_kind IN ('agent_task','todo_execution','agent_team_run','agent_conversation') THEN NULL \
                          ELSE NULLIF(BTRIM(COALESCE(record.data #>> '{metadata,conversation_id}','')),'') END \
         ) \
         SELECT grouped.tenant_id,grouped.source_id,grouped.agent_subject_id,grouped.thread_id, \
                grouped.project_subject_id,NULLIF(BTRIM(subject.data->>'display_name'),'') AS project_display_name, \
                grouped.conversation_id,grouped.thread_kind,grouped.record_count,grouped.status, \
                grouped.summary_status,grouped.updated_at \
           FROM grouped \
           LEFT JOIN engine_subjects subject \
             ON subject.tenant_id=grouped.tenant_id AND subject.source_id=grouped.source_id \
            AND subject.subject_id=grouped.project_subject_id \
          ORDER BY grouped.project_subject_id NULLS FIRST,grouped.updated_at DESC, \
                   grouped.thread_id ASC,grouped.conversation_id ASC",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(agent_subject_id)
    .fetch_all(db)
    .await
    .map_err(|error| error.to_string())?;

    Ok(rows
        .into_iter()
        .map(|row| AgentConversationItem {
            tenant_id: row.tenant_id,
            source_id: row.source_id,
            agent_subject_id: row.agent_subject_id,
            thread_id: row.thread_id,
            project_subject_id: row.project_subject_id,
            project_display_name: row.project_display_name,
            conversation_id: row.conversation_id,
            thread_kind: row.thread_kind,
            record_count: row.record_count,
            status: row.status,
            summary_status: row.summary_status,
            updated_at: row.updated_at.to_rfc3339(),
        })
        .collect())
}

fn normalized(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}
