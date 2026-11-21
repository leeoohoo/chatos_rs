use super::*;

#[cfg(test)]
pub async fn commit_subject_memory_generation(
    db: &Db,
    generation: &GenerationCommit,
    output: SubjectMemoryGenerationOutput<'_>,
) -> Result<EngineSubjectMemory, String> {
    commit_subject_memory_generation_inner(db, generation, output, None).await
}

pub async fn commit_subject_memory_generation_final(
    db: &Db,
    generation: &GenerationCommit,
    output: SubjectMemoryGenerationOutput<'_>,
    finalization: SubjectMemoryGenerationFinalization<'_>,
) -> Result<EngineSubjectMemory, String> {
    commit_subject_memory_generation_inner(db, generation, output, Some(finalization)).await
}

async fn commit_subject_memory_generation_inner(
    db: &Db,
    generation: &GenerationCommit,
    output: SubjectMemoryGenerationOutput<'_>,
    finalization: Option<SubjectMemoryGenerationFinalization<'_>>,
) -> Result<EngineSubjectMemory, String> {
    let current = generation_commits::get_generation(
        db,
        &generation.tenant_id,
        &generation.source_id,
        &generation.generation_id,
    )
    .await?
    .ok_or_else(|| "subject memory generation registry row is missing".to_string())?;
    validate_subject_output(&current, &output)?;
    if current.status == "committed" {
        return load_committed_subject_memory(db, &current).await;
    }
    if current.status != "running" || current.lease_token != generation.lease_token {
        return Err("subject memory generation commit lease is no longer current".to_string());
    }
    if let Some(finalization) = finalization.as_ref() {
        validate_subject_finalization(&current, &output, finalization)?;
    }
    let mut tx = db.begin().await.map_err(|error| error.to_string())?;
    if finalization.is_some() {
        lock_running_generation_job(&mut tx, &current, None, Some(output.subject_id)).await?;
    }
    lock_subject_memory_logical_key(
        &mut tx,
        &current.tenant_id,
        &current.source_id,
        output.subject_id,
        output.memory_key,
    )
    .await?;
    let rows = sqlx::query_scalar::<_, Json<serde_json::Value>>(
        "SELECT data FROM engine_subject_memories WHERE tenant_id=$1 AND source_id=$2 \
         AND subject_id=$3 AND memory_key=$4 AND status='active' AND recall_eligible FOR UPDATE",
    )
    .bind(&generation.tenant_id)
    .bind(&generation.source_id)
    .bind(output.subject_id)
    .bind(output.memory_key)
    .fetch_all(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    let now = now_rfc3339();
    for row in rows {
        let mut previous = decode::<EngineSubjectMemory>(row)?;
        previous.status = "retired".to_string();
        previous.updated_at = now.clone();
        sqlx::query(
            "UPDATE engine_subject_memories SET status='retired',recall_eligible=false, \
             retired_reason='superseded_by_generation',updated_at=$5,data=$6 \
             WHERE tenant_id=$1 AND source_id=$2 AND id=$3 AND status='active' AND memory_key=$4",
        )
        .bind(&generation.tenant_id)
        .bind(&generation.source_id)
        .bind(&previous.id)
        .bind(output.memory_key)
        .bind(timestamp(&now)?)
        .bind(json(&previous)?)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
        memory_facts::retire_output(
            &mut tx,
            &generation.tenant_id,
            &generation.source_id,
            "subject_memory",
            previous.id.as_str(),
        )
        .await?;
    }

    let mut memory = build_subject_memory(
        None,
        output.subject_id,
        output.memory_key,
        output.request,
        Some(output.source_digest),
        Some(output.rollup_status),
    );
    memory.payload_schema = Some(output.payload_schema.to_string());
    memory.structured_payload = Some(output.structured_payload.clone());
    let relation_subject_id = memory
        .metadata
        .as_ref()
        .and_then(|value| value.get("relation_subject_id"))
        .and_then(serde_json::Value::as_str);
    sqlx::query(
        "INSERT INTO engine_subject_memories(id,tenant_id,source_id,subject_id,memory_key,memory_type, \
         level,source_digest,relation_subject_id,status,rollup_status,created_at,updated_at,data, \
         generation_id,generation_profile,project_partition_key,recall_eligible,payload_schema, \
         structured_payload,pipeline_revision) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,true,$18,$19,$20)",
    )
    .bind(&memory.id)
    .bind(&memory.tenant_id)
    .bind(&memory.source_id)
    .bind(&memory.subject_id)
    .bind(&memory.memory_key)
    .bind(&memory.memory_type)
    .bind(memory.level)
    .bind(&memory.source_digest)
    .bind(relation_subject_id)
    .bind(&memory.status)
    .bind(&memory.rollup_status)
    .bind(timestamp(&memory.created_at)?)
    .bind(timestamp(&memory.updated_at)?)
    .bind(json(&memory)?)
    .bind(&generation.generation_id)
    .bind(output.generation_profile)
    .bind(output.project_partition_key)
    .bind(output.payload_schema)
    .bind(Json(output.structured_payload))
    .bind(output.pipeline_revision)
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    let (scope_kind, scope_key) = match memory.memory_type.as_str() {
        "project_summary" => (
            "project",
            output.project_partition_key.ok_or_else(|| {
                "project subject memory output has no project partition".to_string()
            })?,
        ),
        "agent_memory" => ("agent", memory.subject_id.as_str()),
        _ => return Err("subject memory output has unsupported payload scope".to_string()),
    };
    memory_facts::project_payload(
        &mut tx,
        &current,
        FactOutputScope {
            output_kind: "subject_memory",
            output_id: memory.id.as_str(),
            scope_kind,
            scope_key,
            subject_id: memory.subject_id.as_str(),
            project_partition_key: output.project_partition_key,
        },
        output.payload_schema,
        output.structured_payload,
    )
    .await?;
    derived_events::enqueue_subject_memory_rollup_request(
        &mut tx,
        &generation.tenant_id,
        &generation.source_id,
        &memory.id,
        &memory.subject_id,
        output.project_partition_key,
        &generation.generation_id,
    )
    .await?;

    if let Some(finalization) = finalization {
        finalize_subject_memory_generation(&mut tx, &current, &memory, finalization).await?;
    }

    if !generation_commits::commit_generation(
        &mut tx,
        generation,
        "subject_memory",
        memory.id.as_str(),
    )
    .await?
    {
        tx.rollback().await.map_err(|error| error.to_string())?;
        return load_committed_subject_memory(db, generation).await;
    }
    tx.commit().await.map_err(|error| error.to_string())?;
    Ok(memory)
}

fn validate_subject_finalization(
    generation: &GenerationCommit,
    output: &SubjectMemoryGenerationOutput<'_>,
    finalization: &SubjectMemoryGenerationFinalization<'_>,
) -> Result<(), String> {
    let (expected_kind, source_ids) = match &finalization.sources {
        SubjectMemoryGenerationSources::Summaries { summary_ids, .. } => {
            if output.request.level.unwrap_or(0) != 0 {
                return Err(
                    "summary sources can only finalize a level-0 subject memory".to_string()
                );
            }
            let expected = match output.request.memory_type.as_str() {
                "project_summary" => "project_memory",
                "agent_memory" => "agent_memory",
                _ => {
                    return Err(
                        "level-0 subject memory type has no generation kind mapping".to_string()
                    )
                }
            };
            (expected, *summary_ids)
        }
        SubjectMemoryGenerationSources::SubjectMemories { memory_ids } => {
            if output.request.level.unwrap_or(0) <= 0 {
                return Err("subject-memory rollup output level must be positive".to_string());
            }
            ("subject_memory_rollup", *memory_ids)
        }
    };
    if generation.generation_kind != expected_kind {
        return Err(
            "subject memory finalization source kind does not match generation".to_string(),
        );
    }
    validate_frozen_source_ids(generation, source_ids, "subject memory")?;
    if finalization.job_result.status != "done"
        || finalization.job_result.output_count != 1
        || finalization.job_result.error_count != 0
    {
        return Err(
            "subject memory finalization requires a successful terminal job result".to_string(),
        );
    }
    match (&finalization.sources, &finalization.scope_claim) {
        (SubjectMemoryGenerationSources::Summaries { scope_key, .. }, Some(scope_claim))
            if scope_key.map(str::trim) == Some(scope_claim.scope_key.trim()) => {}
        (
            SubjectMemoryGenerationSources::Summaries {
                scope_key: None, ..
            },
            None,
        ) => {}
        (SubjectMemoryGenerationSources::SubjectMemories { .. }, None)
        | (SubjectMemoryGenerationSources::SubjectMemories { .. }, Some(_)) => {}
        _ => {
            return Err(
                "subject memory scope claim must exactly match the frozen summary scope"
                    .to_string(),
            )
        }
    }
    if finalization.scope_claim.as_ref().is_some_and(|claim| {
        claim.scope_key.trim().is_empty() || claim.lock_owner.trim().is_empty()
    }) {
        return Err("subject memory scope claim identity is incomplete".to_string());
    }
    Ok(())
}

pub(super) fn validate_frozen_source_ids(
    generation: &GenerationCommit,
    source_ids: &[String],
    label: &str,
) -> Result<(), String> {
    if source_ids.is_empty() {
        return Err(format!("{label} finalization sources are empty"));
    }
    let unique = source_ids
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    if unique.len() != source_ids.len() || unique.iter().any(|value| value.trim().is_empty()) {
        return Err(format!(
            "{label} finalization source ids must be unique and non-empty"
        ));
    }
    let frozen_ids = generation
        .input_manifest
        .get("inputs")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| format!("{label} generation input manifest has no inputs"))?
        .iter()
        .map(|input| {
            input
                .get("id")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| format!("{label} generation input manifest has an invalid id"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let requested_ids = source_ids.iter().map(String::as_str).collect::<Vec<_>>();
    if frozen_ids != requested_ids {
        return Err(format!(
            "{label} finalization sources do not match frozen input order"
        ));
    }
    Ok(())
}
