use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize)]
pub struct QueryExecuteRequest {
    pub datasource_id: String,
    pub database: Option<String>,
    pub sql: String,
    pub timeout_ms: Option<u64>,
    pub max_rows: Option<u64>,
}

#[derive(Debug, Serialize, Clone)]
pub struct QueryColumn {
    pub name: String,
    #[serde(rename = "type")]
    pub type_name: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct QueryExecuteResponse {
    pub query_id: String,
    pub columns: Vec<QueryColumn>,
    pub rows: Vec<Vec<Value>>,
    pub row_count: u64,
    pub elapsed_ms: u64,
}

#[derive(Debug, Serialize)]
pub struct QueryCancelResponse {
    pub query_id: String,
    pub status: String,
}
