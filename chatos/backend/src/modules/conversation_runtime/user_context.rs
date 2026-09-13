// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::json;

use crate::core::chat_context::resolve_effective_user_id;
use crate::core::internal_context_locale::{
    internal_context_locale_from_settings, InternalContextLocale,
};
use crate::services::user_settings::get_effective_user_settings;

pub async fn resolve_runtime_internal_context_locale(
    explicit_user_id: Option<String>,
    session_id: &str,
) -> InternalContextLocale {
    let effective_user_id = resolve_effective_user_id(explicit_user_id, session_id).await;
    let effective_settings = get_effective_user_settings(effective_user_id.clone())
        .await
        .unwrap_or_else(|_| json!({}));
    internal_context_locale_from_settings(&effective_settings)
}
