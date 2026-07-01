// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::{
    domain::{datasource::DataSource, metadata::ObjectStatsResponse},
    error::AppResult,
};

use super::super::connection::probe_tcp;
use super::{common::derive_schemas, projection::projected_object_stats};

pub async fn object_stats(
    datasource: &DataSource,
    database: &str,
) -> AppResult<ObjectStatsResponse> {
    probe_tcp(datasource).await?;
    let schema_count = derive_schemas(datasource).len() as u64;

    Ok(projected_object_stats(database, schema_count))
}
