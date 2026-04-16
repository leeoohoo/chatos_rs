use crate::{
    domain::{datasource::DataSource, query::QueryExecuteRequest},
    error::{AppError, AppResult},
};

pub async fn execute(
    _datasource: &DataSource,
    _request: &QueryExecuteRequest,
) -> AppResult<crate::domain::query::QueryExecuteResponse> {
    Err(AppError::BadRequest(
        "oracle SQL execution is not enabled in this first-stage driver".to_string(),
    ))
}
