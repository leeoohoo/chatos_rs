// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::config::AppConfig;
use crate::db::Db;

#[derive(Clone)]
pub struct AppState {
    pub pool: Db,
    pub config: AppConfig,
    pub user_service_http: reqwest::Client,
}
