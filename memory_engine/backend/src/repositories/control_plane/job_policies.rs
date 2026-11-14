// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::db::Db;

pub async fn count_job_policies(_db: &Db) -> Result<i64, String> {
    Ok(super::common::default_job_types().len() as i64)
}
