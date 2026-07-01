// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod config_handlers;
mod model;
mod provider_handlers;
mod provider_models;
mod settings_handlers;
mod user_service_proxy;

pub(super) use config_handlers::{
    create_ai_model_config, delete_ai_model_config, get_ai_model_config, list_ai_model_configs,
    list_ai_provider_models, refresh_ai_model_config, update_ai_model_config,
};
pub(super) use provider_handlers::{
    create_ai_model_provider, delete_ai_model_provider, get_ai_model_provider,
    list_ai_model_providers, refresh_ai_model_provider, update_ai_model_provider,
};
pub(super) use settings_handlers::{get_ai_model_settings, put_ai_model_settings};
