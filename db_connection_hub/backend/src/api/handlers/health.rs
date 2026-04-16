use axum::Json;
use chrono::Utc;
use serde::Serialize;

#[derive(Serialize)]
pub struct HealthResponse {
    pub ok: bool,
    pub service: String,
    pub ts: String,
}

pub async fn health_check() -> Json<HealthResponse> {
    Json(HealthResponse {
        ok: true,
        service: "db_connection_hub_backend".to_string(),
        ts: Utc::now().to_rfc3339(),
    })
}
