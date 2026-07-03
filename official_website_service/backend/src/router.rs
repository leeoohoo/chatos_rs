// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::extract::State;
use axum::http::header::CONTENT_TYPE;
use axum::{response::IntoResponse, routing::get, Json, Router};
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;

use crate::config::AppConfig;
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
        .fallback_service(static_service)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(config)
}

async fn health() -> impl IntoResponse {
    "ok"
}

async fn manifest() -> Json<SiteManifest> {
    Json(site_manifest())
}

async fn services() -> Json<Vec<ServiceInfo>> {
    Json(site_manifest().services)
}

async fn status() -> Json<ServiceStatusResponse> {
    Json(collect_service_status().await)
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
