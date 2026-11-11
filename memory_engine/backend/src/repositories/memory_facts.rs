// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;

use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use sqlx::types::Json;
use sqlx::{Postgres, Transaction};

use crate::models::structured_payload::{
    deterministic_fact_id, validate_envelope_shape, MEMORY_PAYLOAD_SCHEMA,
};
use crate::repositories::generation_commits::GenerationCommit;

const FACT_COLLECTIONS: [&str; 8] = [
    "completed",
    "decisions",
    "open_items",
    "blockers",
    "artifacts",
    "constraints",
    "stable_preferences",
    "uncertainties",
];
const MAX_SEARCHABLE_TEXT_BYTES: usize = 16_384;
const LEXICAL_INDEX_REVISION: i64 = 22;

#[derive(Debug, Clone)]
pub(crate) struct RelevantMemoryFact {
    pub fact_id: String,
    pub output_kind: String,
    pub output_id: String,
    pub scope_kind: String,
    pub scope_key: String,
    pub project_partition_key: Option<String>,
    pub fact_key: String,
    pub normalized_value_hash: String,
    pub searchable_text: String,
    pub status: String,
    pub source_refs: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub lexical_score: f64,
}

pub(crate) struct FactOutputScope<'a> {
    pub output_kind: &'a str,
    pub output_id: &'a str,
    pub scope_kind: &'a str,
    pub scope_key: &'a str,
    pub subject_id: &'a str,
    pub project_partition_key: Option<&'a str>,
}

struct ValidatedFact {
    fact_id: String,
    fact_key: String,
    normalized_value_hash: String,
    searchable_text: String,
    status: String,
    confidence: String,
    observed_at: Option<DateTime<Utc>>,
    source_refs: Vec<String>,
    supersedes: Vec<String>,
}

pub(crate) async fn project_payload(
    tx: &mut Transaction<'_, Postgres>,
    generation: &GenerationCommit,
    scope: FactOutputScope<'_>,
    payload_schema: &str,
    payload: &serde_json::Value,
) -> Result<(), String> {
    if payload_schema != MEMORY_PAYLOAD_SCHEMA {
        return Err("generation output payload_schema must be memory_payload_v1".to_string());
    }
    validate_envelope_shape(payload)?;
    validate_scope(payload, &scope)?;
    let input_ids = frozen_input_ids(generation)?;
    let supersession_candidates = frozen_fact_ids(&generation.input_manifest);
    let facts = validate_facts(generation, payload, &input_ids, &supersession_candidates)?;

    for fact in &facts {
        sqlx::query(
            "INSERT INTO engine_memory_fact_projections(tenant_id,source_id,fact_id,output_kind, \
             output_id,scope_kind,scope_key,project_partition_key,fact_key,normalized_value_hash, \
             searchable_text,status,confidence,observed_at,source_refs) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)",
        )
        .bind(&generation.tenant_id)
        .bind(&generation.source_id)
        .bind(&fact.fact_id)
        .bind(scope.output_kind)
        .bind(scope.output_id)
        .bind(scope.scope_kind)
        .bind(scope.scope_key)
        .bind(scope.project_partition_key)
        .bind(&fact.fact_key)
        .bind(&fact.normalized_value_hash)
        .bind(&fact.searchable_text)
        .bind(&fact.status)
        .bind(&fact.confidence)
        .bind(fact.observed_at)
        .bind(Json(&fact.source_refs))
        .execute(&mut **tx)
        .await
        .map_err(|error| error.to_string())?;
    }

    for fact in &facts {
        for superseded_fact_id in &fact.supersedes {
            sqlx::query(
                "INSERT INTO engine_memory_fact_supersessions(tenant_id,source_id,newer_fact_id, \
                 superseded_fact_id) VALUES($1,$2,$3,$4)",
            )
            .bind(&generation.tenant_id)
            .bind(&generation.source_id)
            .bind(&fact.fact_id)
            .bind(superseded_fact_id)
            .execute(&mut **tx)
            .await
            .map_err(|error| error.to_string())?;
            let updated = sqlx::query(
                "UPDATE engine_memory_fact_projections SET status='superseded',retired_at=now() \
                 WHERE tenant_id=$1 AND source_id=$2 AND fact_id=$3 \
                 AND status IN ('active','uncertain')",
            )
            .bind(&generation.tenant_id)
            .bind(&generation.source_id)
            .bind(superseded_fact_id)
            .execute(&mut **tx)
            .await
            .map_err(|error| error.to_string())?;
            if updated.rows_affected() != 1 {
                return Err("superseded memory fact is not active".to_string());
            }
        }
    }
    Ok(())
}

pub(crate) async fn retire_output(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: &str,
    source_id: &str,
    output_kind: &str,
    output_id: &str,
) -> Result<u64, String> {
    sqlx::query(
        "UPDATE engine_memory_fact_projections SET status='retired',retired_at=now() \
         WHERE tenant_id=$1 AND source_id=$2 AND output_kind=$3 AND output_id=$4 \
         AND status IN ('active','uncertain')",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(output_kind)
    .bind(output_id)
    .execute(&mut **tx)
    .await
    .map(|result| result.rows_affected())
    .map_err(|error| error.to_string())
}

pub(crate) async fn search_relevant(
    db: &crate::db::Db,
    tenant_id: &str,
    source_id: &str,
    thread_id: &str,
    project_partition_key: &str,
    agent_scope_keys: &[String],
    query_text: &str,
    limit: i64,
) -> Result<Vec<RelevantMemoryFact>, String> {
    verify_lexical_index_ready(db).await?;
    let normalized_query = normalize_query(query_text);
    if normalized_query.is_empty() {
        return Ok(Vec::new());
    }
    let patterns = lexical_terms(normalized_query.as_str())
        .into_iter()
        .map(|term| format!("%{term}%"))
        .collect::<Vec<_>>();
    if patterns.is_empty() {
        return Ok(Vec::new());
    }
    let rows = sqlx::query_as::<
        _,
        (
            String,
            String,
            String,
            String,
            String,
            Option<String>,
            String,
            String,
            String,
            String,
            Json<Vec<String>>,
            DateTime<Utc>,
            f64,
        ),
    >(
        "SELECT fact.fact_id,fact.output_kind,fact.output_id,fact.scope_kind,fact.scope_key, \
         fact.project_partition_key,fact.fact_key,fact.normalized_value_hash, \
         fact.searchable_text,fact.status,fact.source_refs,fact.created_at, \
         GREATEST(similarity(fact.searchable_text,$7), \
           CASE WHEN fact.searchable_text ILIKE ANY($6) THEN 0.65 ELSE 0.0 END)::float8 AS lexical_score \
         FROM engine_memory_fact_projections fact \
         WHERE fact.tenant_id=$1 AND fact.source_id=$2 \
           AND fact.status IN ('active','uncertain') \
           AND ((fact.scope_kind='thread' AND fact.scope_key=$3 \
                 AND fact.project_partition_key=$4) \
             OR (fact.scope_kind='project' AND fact.scope_key=$4 \
                 AND fact.project_partition_key=$4) \
             OR (fact.scope_kind='agent' AND fact.scope_key=ANY($5) \
                 AND fact.project_partition_key IS NULL)) \
           AND (fact.searchable_text ILIKE ANY($6) OR similarity(fact.searchable_text,$7)>=0.08) \
           AND ((fact.output_kind='thread_summary' AND EXISTS( \
                  SELECT 1 FROM engine_summaries output WHERE output.tenant_id=fact.tenant_id \
                  AND output.source_id=fact.source_id AND output.id=fact.output_id \
                  AND output.status='done' AND output.recall_eligible)) \
             OR (fact.output_kind='subject_memory' AND EXISTS( \
                  SELECT 1 FROM engine_subject_memories output WHERE output.tenant_id=fact.tenant_id \
                  AND output.source_id=fact.source_id AND output.id=fact.output_id \
                  AND output.status='active' AND output.recall_eligible))) \
         ORDER BY lexical_score DESC,fact.created_at DESC,fact.fact_id ASC LIMIT $8",
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(thread_id)
    .bind(project_partition_key)
    .bind(agent_scope_keys)
    .bind(&patterns)
    .bind(&normalized_query)
    .bind(limit.clamp(1, 200))
    .fetch_all(db)
    .await
    .map_err(|error| error.to_string())?;
    Ok(rows
        .into_iter()
        .map(
            |(
                fact_id,
                output_kind,
                output_id,
                scope_kind,
                scope_key,
                project_partition_key,
                fact_key,
                normalized_value_hash,
                searchable_text,
                status,
                source_refs,
                created_at,
                lexical_score,
            )| RelevantMemoryFact {
                fact_id,
                output_kind,
                output_id,
                scope_kind,
                scope_key,
                project_partition_key,
                fact_key,
                normalized_value_hash,
                searchable_text,
                status,
                source_refs: source_refs.0,
                created_at,
                lexical_score,
            },
        )
        .collect())
}

pub(crate) async fn verify_lexical_index_ready(db: &crate::db::Db) -> Result<(), String> {
    let ready = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM pg_extension WHERE extname='pg_trgm') \
          AND to_regclass('public.engine_memory_fact_projections') IS NOT NULL \
          AND EXISTS(SELECT 1 FROM pg_index WHERE indexrelid= \
              to_regclass('public.engine_memory_fact_search_trgm_idx') AND indisvalid AND indisready) \
          AND EXISTS(SELECT 1 FROM _sqlx_migrations WHERE version=$1 AND success)",
    )
    .bind(LEXICAL_INDEX_REVISION)
    .fetch_one(db)
    .await
    .map_err(|error| error.to_string())?;
    if !ready {
        return Err("memory lexical index revision 22 is not ready".to_string());
    }
    Ok(())
}

pub(crate) async fn verify_memory_pipeline_ready(db: &crate::db::Db) -> Result<(), String> {
    verify_lexical_index_ready(db).await?;
    let ready = sqlx::query_scalar::<_, bool>(
        "SELECT (SELECT COUNT(*)=7 FROM _sqlx_migrations \
                  WHERE version BETWEEN 16 AND 22 AND success) \
          AND to_regclass('public.engine_generation_commits') IS NOT NULL \
          AND to_regclass('public.engine_summary_record_coverage') IS NOT NULL \
          AND to_regclass('public.engine_memory_derivation_edges') IS NOT NULL \
          AND to_regclass('public.engine_runtime_context_assertion_replays') IS NOT NULL \
          AND EXISTS(SELECT 1 FROM information_schema.columns \
                     WHERE table_schema='public' AND table_name='engine_summaries' \
                       AND column_name='structured_payload') \
          AND EXISTS(SELECT 1 FROM information_schema.columns \
                     WHERE table_schema='public' AND table_name='engine_subject_memories' \
                       AND column_name='structured_payload') \
          AND EXISTS(SELECT 1 FROM pg_trigger \
                     WHERE tgname='engine_memory_fact_output_exists' AND NOT tgisinternal) \
          AND EXISTS(SELECT 1 FROM pg_trigger \
                     WHERE tgname='engine_memory_fact_supersession_validate' AND NOT tgisinternal)",
    )
    .fetch_one(db)
    .await
    .map_err(|error| error.to_string())?;
    if !ready {
        return Err(
            "Memory Engine generation, assertion, structured payload, fact projection, or lexical pipeline is not ready"
                .to_string(),
        );
    }
    Ok(())
}

fn validate_scope(
    payload: &serde_json::Value,
    expected: &FactOutputScope<'_>,
) -> Result<(), String> {
    let scope = payload
        .get("scope")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| "memory payload scope must be an object".to_string())?;
    if scope.get("kind").and_then(serde_json::Value::as_str) != Some(expected.scope_kind)
        || scope.get("subject_id").and_then(serde_json::Value::as_str) != Some(expected.subject_id)
    {
        return Err("memory payload scope does not match generation output".to_string());
    }
    let project_partition_key = scope
        .get("project_partition_key")
        .and_then(serde_json::Value::as_str);
    if project_partition_key != expected.project_partition_key {
        return Err(
            "memory payload project partition does not match generation output".to_string(),
        );
    }
    match expected.scope_kind {
        "thread" if expected.scope_key.trim().is_empty() => {
            Err("thread memory payload scope key is empty".to_string())
        }
        "project" if !expected.scope_key.starts_with("project:") => {
            Err("project memory payload scope key is invalid".to_string())
        }
        "agent" if expected.project_partition_key.is_some() => {
            Err("agent memory payload cannot have a project partition".to_string())
        }
        "thread" | "project" | "agent" => Ok(()),
        _ => Err("memory payload scope kind is unsupported".to_string()),
    }
}

fn validate_facts(
    generation: &GenerationCommit,
    payload: &serde_json::Value,
    input_ids: &HashSet<String>,
    supersession_candidates: &HashSet<String>,
) -> Result<Vec<ValidatedFact>, String> {
    let mut result = Vec::new();
    let mut seen_ids = HashSet::new();
    let mut seen_values = HashSet::new();
    for collection in FACT_COLLECTIONS {
        let facts = payload
            .get(collection)
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| format!("memory payload {collection} must be an array"))?;
        for fact in facts {
            let object = fact
                .as_object()
                .ok_or_else(|| format!("memory payload {collection} fact must be an object"))?;
            let fact_key = required_text(object.get("fact_key"), "fact_key")?;
            if !valid_fact_key(fact_key) {
                return Err("memory fact_key has invalid characters".to_string());
            }
            let value = object
                .get("value")
                .filter(|value| !value.is_null())
                .ok_or_else(|| "memory fact value is missing".to_string())?;
            let source_refs = required_string_array(object.get("source_refs"), "source_refs")?;
            if source_refs.is_empty() || source_refs.iter().any(|value| !input_ids.contains(value))
            {
                return Err(
                    "memory fact source_refs are outside the frozen input manifest".to_string(),
                );
            }
            let expected_id = deterministic_fact_id(
                &generation.generation_id,
                collection,
                fact_key,
                value,
                &source_refs,
            )?;
            if object.get("fact_id").and_then(serde_json::Value::as_str)
                != Some(expected_id.as_str())
            {
                return Err("memory fact_id is not the deterministic server id".to_string());
            }
            if !seen_ids.insert(expected_id.clone()) {
                return Err("memory payload contains a duplicate fact_id".to_string());
            }
            let normalized = normalize_value(value)?;
            let normalized_value_hash = versioned_hash(normalized.as_bytes());
            if !seen_values.insert((fact_key.to_string(), normalized_value_hash.clone())) {
                return Err("memory payload contains an exact duplicate fact".to_string());
            }
            let searchable_text = format!("{collection} {fact_key} {normalized}");
            reject_secret_material(searchable_text.as_str())?;
            if searchable_text.len() > MAX_SEARCHABLE_TEXT_BYTES {
                return Err("memory fact searchable text exceeds 16384 bytes".to_string());
            }
            let status = required_text(object.get("status"), "status")?;
            if !matches!(status, "active" | "superseded" | "uncertain") {
                return Err("memory fact status is invalid".to_string());
            }
            let confidence = required_text(object.get("confidence"), "confidence")?;
            if !matches!(
                confidence,
                "explicit_user" | "tool_evidence" | "model_inference" | "uncertain"
            ) {
                return Err("memory fact confidence is invalid".to_string());
            }
            let supersedes = required_string_array(object.get("supersedes"), "supersedes")?;
            if supersedes.iter().any(|candidate| {
                candidate == &expected_id || !supersession_candidates.contains(candidate)
            }) {
                return Err("memory fact supersedes is outside the frozen candidates".to_string());
            }
            let observed_at = match object.get("observed_at") {
                None | Some(serde_json::Value::Null) => None,
                Some(serde_json::Value::String(value)) => Some(
                    DateTime::parse_from_rfc3339(value)
                        .map_err(|_| "memory fact observed_at is invalid".to_string())?
                        .with_timezone(&Utc),
                ),
                _ => return Err("memory fact observed_at is invalid".to_string()),
            };
            result.push(ValidatedFact {
                fact_id: expected_id,
                fact_key: fact_key.to_string(),
                normalized_value_hash,
                searchable_text,
                status: status.to_string(),
                confidence: confidence.to_string(),
                observed_at,
                source_refs,
                supersedes,
            });
        }
    }
    if result.is_empty() {
        return Err("memory payload must contain at least one fact".to_string());
    }
    Ok(result)
}

fn frozen_input_ids(generation: &GenerationCommit) -> Result<HashSet<String>, String> {
    let inputs = generation
        .input_manifest
        .get("inputs")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "generation input manifest has no inputs".to_string())?;
    let ids = inputs
        .iter()
        .map(|input| {
            input
                .get("id")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
                .ok_or_else(|| "generation input manifest contains an invalid id".to_string())
        })
        .collect::<Result<HashSet<_>, _>>()?;
    if ids.len() != inputs.len() || ids.is_empty() {
        return Err("generation input manifest ids must be unique and non-empty".to_string());
    }
    Ok(ids)
}

fn frozen_fact_ids(value: &serde_json::Value) -> HashSet<String> {
    fn visit(value: &serde_json::Value, result: &mut HashSet<String>) {
        match value {
            serde_json::Value::Object(object) => {
                if let Some(fact_id) = object.get("fact_id").and_then(serde_json::Value::as_str) {
                    result.insert(fact_id.to_string());
                }
                for value in object.values() {
                    visit(value, result);
                }
            }
            serde_json::Value::Array(values) => {
                for value in values {
                    visit(value, result);
                }
            }
            _ => {}
        }
    }
    let mut result = HashSet::new();
    visit(value, &mut result);
    result
}

fn required_text<'a>(value: Option<&'a serde_json::Value>, name: &str) -> Result<&'a str, String> {
    value
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("memory fact {name} is missing"))
}

fn required_string_array(
    value: Option<&serde_json::Value>,
    name: &str,
) -> Result<Vec<String>, String> {
    let values = value
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| format!("memory fact {name} must be an array"))?;
    let result = values
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .ok_or_else(|| format!("memory fact {name} contains an invalid id"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let unique = result.iter().collect::<HashSet<_>>();
    if unique.len() != result.len() {
        return Err(format!("memory fact {name} contains duplicate ids"));
    }
    Ok(result)
}

fn valid_fact_key(value: &str) -> bool {
    value.len() <= 256
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b':' | b'/')
        })
}

fn normalize_value(value: &serde_json::Value) -> Result<String, String> {
    let raw = match value {
        serde_json::Value::String(value) => value.clone(),
        value => serde_json::to_string(value).map_err(|error| error.to_string())?,
    };
    Ok(raw
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase())
}

fn normalize_query(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn lexical_terms(value: &str) -> Vec<String> {
    let mut terms = Vec::new();
    let mut ascii = String::new();
    let mut cjk = Vec::new();
    let flush_ascii = |ascii: &mut String, terms: &mut Vec<String>| {
        if ascii.chars().count() >= 2 {
            terms.push(std::mem::take(ascii));
        } else {
            ascii.clear();
        }
    };
    for character in value.chars() {
        if character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.' | '/') {
            ascii.push(character);
            if !cjk.is_empty() {
                append_cjk_terms(&mut terms, &mut cjk);
            }
        } else if is_cjk(character) {
            flush_ascii(&mut ascii, &mut terms);
            cjk.push(character);
        } else {
            flush_ascii(&mut ascii, &mut terms);
            append_cjk_terms(&mut terms, &mut cjk);
        }
    }
    flush_ascii(&mut ascii, &mut terms);
    append_cjk_terms(&mut terms, &mut cjk);
    terms.sort();
    terms.dedup();
    terms
}

fn append_cjk_terms(terms: &mut Vec<String>, cjk: &mut Vec<char>) {
    if cjk.len() == 1 {
        terms.push(cjk[0].to_string());
    } else {
        terms.extend(cjk.windows(2).map(|pair| pair.iter().collect::<String>()));
    }
    cjk.clear();
}

fn is_cjk(character: char) -> bool {
    matches!(
        character as u32,
        0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF
    )
}

fn versioned_hash(value: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"memory-fact-normalization-v1\0");
    hasher.update(value);
    format!("sha256-v1:{}", hex::encode(hasher.finalize()))
}

fn reject_secret_material(value: &str) -> Result<(), String> {
    let normalized = value.to_ascii_lowercase();
    let forbidden = [
        "authorization: bearer ",
        "-----begin private key-----",
        "-----begin rsa private key-----",
        "x-amz-signature=",
        "api_key=",
        "apikey=",
        "access_token=",
        "refresh_token=",
        "password=",
        "passwd=",
        "secret=",
    ];
    if forbidden.iter().any(|pattern| normalized.contains(pattern)) {
        return Err("memory fact searchable text contains secret material".to_string());
    }
    Ok(())
}
