// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::db::Db;
use crate::models::EngineJobPolicy;

use super::common::{default_job_policy, default_job_types, normalize_job_policy};

pub async fn list_job_policies(_db: &Db) -> Result<Vec<EngineJobPolicy>, String> {
    let mut policies = Vec::with_capacity(default_job_types().len());
    for job_type in default_job_types() {
        policies.push(get_effective_job_policy(_db, job_type).await?);
    }
    Ok(policies)
}

pub async fn count_job_policies(_db: &Db) -> Result<i64, String> {
    Ok(default_job_types().len() as i64)
}

pub async fn get_effective_job_policy(
    _db: &Db,
    job_type: &str,
) -> Result<EngineJobPolicy, String> {
    if !default_job_types().contains(&job_type.trim()) {
        return Err(format!("unsupported Memory Engine policy {job_type}"));
    }
    let mut policy = default_job_policy(job_type);
    super::managed_memory_policy::apply_to_engine_job_policy(&mut policy).await?;
    normalize_job_policy(&mut policy);
    Ok(policy)
}
