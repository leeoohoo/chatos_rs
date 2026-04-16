use std::time::Instant;

use mongodb::bson::{self, doc, Bson, Document};

use crate::{
    domain::{
        datasource::DataSource,
        query::{QueryColumn, QueryExecuteRequest, QueryExecuteResponse},
    },
    error::{AppError, AppResult},
};

use super::connection::{connect_client, map_db_error, target_database};

pub async fn execute(
    datasource: &DataSource,
    request: &QueryExecuteRequest,
) -> AppResult<QueryExecuteResponse> {
    let sql = request.sql.trim();
    if sql.is_empty() {
        return Err(AppError::BadRequest("command cannot be empty".to_string()));
    }

    let command = parse_command(sql)?;
    let client = connect_client(datasource).await?;
    let db_name = target_database(datasource, request.database.as_deref());
    let db = client.database(db_name.as_str());
    let start = Instant::now();

    let result = db
        .run_command(command, None)
        .await
        .map_err(|err| map_db_error("query", err.to_string()))?;

    let json_value = bson::from_bson::<serde_json::Value>(Bson::Document(result.clone()))
        .unwrap_or_else(|_| serde_json::json!({ "raw_result": format!("{result:?}") }));

    Ok(QueryExecuteResponse {
        query_id: format!("q_{}", uuid::Uuid::new_v4().simple()),
        columns: vec![QueryColumn {
            name: "result".to_string(),
            type_name: "json".to_string(),
        }],
        rows: vec![vec![json_value]],
        row_count: 1,
        elapsed_ms: start.elapsed().as_millis() as u64,
    })
}

fn parse_command(raw: &str) -> AppResult<Document> {
    if raw.eq_ignore_ascii_case("ping") {
        return Ok(doc! { "ping": 1 });
    }

    let value: serde_json::Value = serde_json::from_str(raw).map_err(|err| {
        AppError::BadRequest(format!(
            "mongodb command must be a JSON object, for example {{\"ping\":1}}: {err}"
        ))
    })?;

    let bson_value = bson::to_bson(&value).map_err(|err| {
        AppError::BadRequest(format!("failed to convert command JSON to BSON: {err}"))
    })?;

    bson_value
        .as_document()
        .cloned()
        .ok_or_else(|| AppError::BadRequest("mongodb command must be a JSON object".to_string()))
}
