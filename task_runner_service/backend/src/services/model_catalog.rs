// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use chatos_ai_runtime::model_config::{
    default_base_url_for_provider, normalize_provider, normalize_thinking_level,
};
use serde_json::Value;
use tracing::{info, warn};

use crate::models::{now_rfc3339, ModelCatalogResponse, ModelConfigRecord, ProviderModelRecord};

use super::normalized_optional;

mod fetching;
mod normalization;

pub(in crate::services) use self::fetching::fetch_model_catalog_for_record;
pub(in crate::services) use self::normalization::{
    normalize_model_base_url_input, normalize_model_config_record, normalize_model_provider_input,
    normalize_model_thinking_level_input,
};
