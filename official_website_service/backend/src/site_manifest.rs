// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::Serialize;

use crate::config::AppConfig;

#[derive(Debug, Clone, Serialize)]
pub struct SiteManifest {
    pub product_name: &'static str,
    pub tagline: &'static str,
    pub app_url: String,
    pub registration_enabled: bool,
    pub downloads_enabled: bool,
    pub default_ports: Vec<DefaultPort>,
    pub services: Vec<ServiceInfo>,
    pub showcase_images: Vec<ShowcaseImage>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DefaultPort {
    pub name: &'static str,
    pub backend: Option<u16>,
    pub frontend: Option<u16>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ServiceInfo {
    pub name: &'static str,
    pub directory: &'static str,
    pub role: &'static str,
    pub capability: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct ShowcaseImage {
    pub id: &'static str,
    pub title: &'static str,
    pub path: &'static str,
    pub source_url: &'static str,
}

pub fn site_manifest(config: &AppConfig) -> SiteManifest {
    SiteManifest {
        product_name: "叽咕狸",
        tagline: "给你的项目一位真正能动手的 AI 搭档。",
        app_url: config.app_url.clone(),
        registration_enabled: !config.user_service_base_url.is_empty(),
        downloads_enabled: config.release_storage.is_some(),
        // Public website metadata must not expose internal service dashboards or ports.
        // The product page renders native-client surfaces directly from the frontend.
        default_ports: vec![],
        services: vec![],
        showcase_images: vec![],
    }
}
