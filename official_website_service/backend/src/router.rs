// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::{Path, State};
use axum::http::header::{AUTHORIZATION, CONTENT_TYPE};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Redirect};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::json;
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;

use crate::config::AppConfig;
use crate::registration::{register, send_registration_code};
use crate::release_storage::{unavailable_catalog, PresignReleaseRequest, ReleaseStorage};
use crate::service_status::{collect_service_status, ServiceStatusResponse};
use crate::site_manifest::{site_manifest, ServiceInfo, SiteManifest};

pub fn build_router(config: AppConfig) -> Router {
    let index_file = config.static_dir.join("index.html");
    let static_service =
        ServeDir::new(config.static_dir.clone()).not_found_service(ServeFile::new(index_file));

    Router::new()
        .route("/health", get(health))
        .route("/robots.txt", get(robots))
        .route("/sitemap.xml", get(sitemap))
        .route("/api/site/manifest", get(manifest))
        .route("/api/site/services", get(services))
        .route("/api/site/status", get(status))
        .route("/api/site/downloads", get(downloads))
        .route("/api/site/downloads/{platform}", get(download_client))
        .route(
            "/api/site/auth/register/send-code",
            post(send_registration_code),
        )
        .route("/api/site/auth/register", post(register))
        .route("/api/site/admin/releases/presign", post(presign_release))
        .fallback_service(static_service)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(config)
}

async fn health() -> impl IntoResponse {
    "ok"
}

async fn manifest(State(config): State<AppConfig>) -> Json<SiteManifest> {
    Json(site_manifest(&config))
}

async fn services(State(config): State<AppConfig>) -> Json<Vec<ServiceInfo>> {
    Json(site_manifest(&config).services)
}

async fn status() -> Json<ServiceStatusResponse> {
    Json(collect_service_status().await)
}

async fn downloads(State(config): State<AppConfig>) -> impl IntoResponse {
    let Some(storage_config) = config.release_storage else {
        return Json(unavailable_catalog());
    };
    Json(ReleaseStorage::new(storage_config).catalog().await)
}

async fn download_client(
    State(config): State<AppConfig>,
    Path(platform): Path<String>,
) -> Result<Redirect, (StatusCode, Json<serde_json::Value>)> {
    let storage_config = config.release_storage.ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"error": "client download is not configured"})),
        )
    })?;
    let url = ReleaseStorage::new(storage_config)
        .download_url(platform.as_str())
        .await
        .map_err(|err| (StatusCode::NOT_FOUND, Json(json!({"error": err}))))?;
    Ok(Redirect::temporary(url.as_str()))
}

async fn presign_release(
    State(config): State<AppConfig>,
    headers: HeaderMap,
    Json(payload): Json<PresignReleaseRequest>,
) -> Result<
    Json<crate::release_storage::PresignReleaseResponse>,
    (StatusCode, Json<serde_json::Value>),
> {
    let expected = config.release_upload_token.as_deref().ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "release publishing is not enabled"})),
        )
    })?;
    let provided = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .unwrap_or_default();
    if !constant_time_eq(expected.as_bytes(), provided.as_bytes()) {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "invalid release upload token"})),
        ));
    }
    let storage_config = config.release_storage.ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"error": "release storage is not configured"})),
        )
    })?;
    ReleaseStorage::new(storage_config)
        .presign_release(payload)
        .map(Json)
        .map_err(|err| (StatusCode::BAD_REQUEST, Json(json!({"error": err}))))
}

fn constant_time_eq(expected: &[u8], provided: &[u8]) -> bool {
    let mut diff = expected.len() ^ provided.len();
    let max_len = expected.len().max(provided.len());
    for index in 0..max_len {
        let left = expected.get(index).copied().unwrap_or_default();
        let right = provided.get(index).copied().unwrap_or_default();
        diff |= usize::from(left ^ right);
    }
    diff == 0
}

async fn robots(State(config): State<AppConfig>) -> impl IntoResponse {
    (
        [(CONTENT_TYPE, "text/plain; charset=utf-8")],
        format!(
            "User-agent: *\nAllow: /\nSitemap: {}/sitemap.xml\n",
            config.public_base_url
        ),
    )
}

async fn sitemap(State(config): State<AppConfig>) -> impl IntoResponse {
    let root = &config.public_base_url;
    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
  <url>
    <loc>{root}/</loc>
    <changefreq>weekly</changefreq>
    <priority>1.0</priority>
  </url>
</urlset>
"#
    );
    ([(CONTENT_TYPE, "application/xml; charset=utf-8")], xml)
}
