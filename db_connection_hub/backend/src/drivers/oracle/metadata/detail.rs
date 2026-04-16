use crate::{
    domain::{datasource::DataSource, metadata::ObjectDetailResponse},
    error::{AppError, AppResult},
};

use super::super::connection::probe_tcp;
use super::projection::{build_projected_detail, parse_detail_node};

pub async fn object_detail(
    datasource: &DataSource,
    node_id: &str,
) -> AppResult<ObjectDetailResponse> {
    probe_tcp(datasource).await?;

    let parsed = parse_detail_node(node_id)
        .ok_or_else(|| AppError::NotFound(format!("unsupported oracle detail node: {node_id}")))?;

    Ok(build_projected_detail(node_id, &parsed))
}
