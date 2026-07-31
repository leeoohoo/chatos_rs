// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::config::AppConfig;
use crate::routing::RoutingEngine;

#[derive(Clone)]
pub struct AppState {
    pub config: AppConfig,
    pub routing: RoutingEngine,
}

impl AppState {
    pub fn new(config: AppConfig) -> Self {
        Self {
            config,
            routing: RoutingEngine,
        }
    }
}
