use super::*;

pub(super) async fn lock_running_generation_job(
    tx: &mut Transaction<'_, Postgres>,
    generation: &GenerationCommit,
    expected_thread_id: Option<&str>,
    expected_subject_id: Option<&str>,
) -> Result<(), String> {
    let row = sqlx::query_as::<_, (String, Json<serde_json::Value>)>(
        "SELECT status,data FROM engine_job_runs \
         WHERE id=$1 AND tenant_id=$2 AND source_id=$3 FOR UPDATE",
    )
    .bind(&generation.job_run_id)
    .bind(&generation.tenant_id)
    .bind(&generation.source_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| error.to_string())?
    .ok_or_else(|| "generation job disappeared before final commit".to_string())?;
    let (stored_status, data) = row;
    let job: EngineJobRun = decode(data)?;
    if stored_status != "running"
        || job.status != "running"
        || expected_thread_id.is_some_and(|thread_id| job.thread_id.as_deref() != Some(thread_id))
        || expected_subject_id
            .is_some_and(|subject_id| job.subject_id.as_deref() != Some(subject_id))
    {
        return Err("generation job is not the running owner of the final output".to_string());
    }
    Ok(())
}

pub(super) async fn finish_generation_job(
    tx: &mut Transaction<'_, Postgres>,
    generation: &GenerationCommit,
    thread_id: &str,
    output_id: &str,
    result: FinishEngineJobRunRequest,
    now: &str,
) -> Result<(), String> {
    let row = sqlx::query_as::<_, (String, Json<serde_json::Value>)>(
        "SELECT status,data FROM engine_job_runs \
         WHERE id=$1 AND tenant_id=$2 AND source_id=$3 FOR UPDATE",
    )
    .bind(&generation.job_run_id)
    .bind(&generation.tenant_id)
    .bind(&generation.source_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| error.to_string())?
    .ok_or_else(|| "generation job disappeared during final commit".to_string())?;
    let (stored_status, data) = row;
    let mut job: EngineJobRun = decode(data)?;
    if stored_status != "running"
        || job.status != "running"
        || job.thread_id.as_deref() != Some(thread_id)
    {
        return Err("generation job is not the running owner of the final output".to_string());
    }
    job.status = result.status;
    job.input_count = result.input_count;
    job.output_count = result.output_count;
    job.processed_count = result.processed_count;
    job.success_count = result.success_count;
    job.error_count = result.error_count;
    job.metadata = result.metadata;
    if let Some(metadata) = job
        .metadata
        .as_mut()
        .and_then(serde_json::Value::as_object_mut)
    {
        metadata.insert(
            "generated_summary_id".to_string(),
            serde_json::Value::String(output_id.to_string()),
        );
    }
    job.error_message = result.error_message;
    job.finished_at = Some(now.to_string());
    sqlx::query("UPDATE engine_job_runs SET status=$2,finished_at=$3,data=$4 WHERE id=$1")
        .bind(&job.id)
        .bind(&job.status)
        .bind(timestamp(now)?)
        .bind(json(&job)?)
        .execute(&mut **tx)
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub(super) fn validate_summary_output(
    generation: &GenerationCommit,
    output: &ThreadSummaryGenerationOutput<'_>,
) -> Result<(), String> {
    let expected_kind = if output.level == 0 {
        "thread_summary"
    } else {
        "thread_rollup"
    };
    if generation.generation_kind != expected_kind {
        return Err(format!(
            "generation kind cannot produce summary level {}",
            output.level
        ));
    }
    if output.thread_id.trim().is_empty()
        || output.subject_id.trim().is_empty()
        || output.summary_type.trim().is_empty()
        || output.summary_text.trim().is_empty()
        || output.generation_reason.trim().is_empty()
        || output.generation_profile.trim().is_empty()
        || output.pipeline_revision.trim().is_empty()
    {
        return Err("thread summary generation output identity is incomplete".to_string());
    }
    if output.level < 0 {
        return Err("thread summary generation output level cannot be negative".to_string());
    }
    if output.summary_type == "thread_noop"
        && (output.level != 0 || generation.generation_kind != "thread_summary")
    {
        return Err(
            "thread no-op output is only valid for a level-0 thread generation".to_string(),
        );
    }
    if generation.project_partition_key.as_deref() != Some(output.project_partition_key) {
        return Err("thread summary output partition does not match generation".to_string());
    }
    if output.project_partition_key != "__none__"
        && !output.project_partition_key.starts_with("project:")
    {
        return Err("thread summary output partition is invalid".to_string());
    }
    if generation
        .config_snapshot
        .get("generation_profile")
        .and_then(serde_json::Value::as_str)
        != Some(output.generation_profile)
        || generation
            .config_snapshot
            .get("pipeline_revision")
            .and_then(serde_json::Value::as_str)
            != Some(output.pipeline_revision)
    {
        return Err(
            "thread summary output configuration does not match frozen generation".to_string(),
        );
    }
    if output.payload_schema != crate::models::structured_payload::MEMORY_PAYLOAD_SCHEMA {
        return Err("thread summary output payload_schema is invalid".to_string());
    }
    Ok(())
}

pub(super) async fn load_committed_thread_summary(
    db: &Db,
    generation: &GenerationCommit,
) -> Result<EngineSummary, String> {
    let stored = generation_commits::get_generation(
        db,
        &generation.tenant_id,
        &generation.source_id,
        &generation.generation_id,
    )
    .await?
    .ok_or_else(|| "generation disappeared during idempotent commit".to_string())?;
    if stored.status != "committed" || stored.output_kind.as_deref() != Some("thread_summary") {
        return Err("generation commit lease was lost before summary output committed".to_string());
    }
    let output_id = stored
        .output_id
        .as_deref()
        .ok_or_else(|| "committed summary generation is missing output_id".to_string())?;
    sqlx::query_scalar::<_, Json<serde_json::Value>>(
        "SELECT data FROM engine_summaries WHERE tenant_id=$1 AND source_id=$2 AND id=$3 \
         AND generation_id=$4",
    )
    .bind(&stored.tenant_id)
    .bind(&stored.source_id)
    .bind(output_id)
    .bind(&stored.generation_id)
    .fetch_optional(db)
    .await
    .map_err(|error| error.to_string())?
    .ok_or_else(|| "committed summary generation output is missing".to_string())
    .and_then(decode)
}

pub(super) fn validate_subject_output(
    generation: &GenerationCommit,
    output: &SubjectMemoryGenerationOutput<'_>,
) -> Result<(), String> {
    if !matches!(
        generation.generation_kind.as_str(),
        "project_memory" | "agent_memory" | "subject_memory_rollup"
    ) {
        return Err("generation kind cannot produce a subject memory".to_string());
    }
    if output.request.tenant_id != generation.tenant_id
        || output.request.source_id != generation.source_id
    {
        return Err("subject memory output scope does not match generation".to_string());
    }
    if output.subject_id.trim().is_empty()
        || output.memory_key.trim().is_empty()
        || output.generation_profile.trim().is_empty()
        || output.pipeline_revision.trim().is_empty()
    {
        return Err("subject memory generation output identity is incomplete".to_string());
    }
    if generation.project_partition_key.as_deref() != output.project_partition_key {
        return Err("subject memory output partition does not match generation".to_string());
    }
    if output.request.status.as_deref().unwrap_or("active") != "active" {
        return Err("subject memory generation output must be active".to_string());
    }
    if generation
        .config_snapshot
        .get("generation_profile")
        .and_then(serde_json::Value::as_str)
        != Some(output.generation_profile)
        || generation
            .config_snapshot
            .get("pipeline_revision")
            .and_then(serde_json::Value::as_str)
            != Some(output.pipeline_revision)
    {
        return Err(
            "subject memory output configuration does not match frozen generation".to_string(),
        );
    }
    if output.payload_schema != crate::models::structured_payload::MEMORY_PAYLOAD_SCHEMA {
        return Err("subject memory output payload_schema is invalid".to_string());
    }
    Ok(())
}

pub(super) async fn load_committed_subject_memory(
    db: &Db,
    generation: &GenerationCommit,
) -> Result<EngineSubjectMemory, String> {
    let stored = generation_commits::get_generation(
        db,
        &generation.tenant_id,
        &generation.source_id,
        &generation.generation_id,
    )
    .await?
    .ok_or_else(|| "generation disappeared during idempotent commit".to_string())?;
    if stored.status != "committed" || stored.output_kind.as_deref() != Some("subject_memory") {
        return Err("generation commit lease was lost before subject output committed".to_string());
    }
    let output_id = stored
        .output_id
        .as_deref()
        .ok_or_else(|| "committed subject generation is missing output_id".to_string())?;
    sqlx::query_scalar::<_, Json<serde_json::Value>>(
        "SELECT data FROM engine_subject_memories WHERE tenant_id=$1 AND source_id=$2 AND id=$3 \
         AND generation_id=$4",
    )
    .bind(&stored.tenant_id)
    .bind(&stored.source_id)
    .bind(output_id)
    .bind(&stored.generation_id)
    .fetch_optional(db)
    .await
    .map_err(|error| error.to_string())?
    .ok_or_else(|| "committed subject generation output is missing".to_string())
    .and_then(decode)
}
