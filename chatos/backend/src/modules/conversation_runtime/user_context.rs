// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};

use crate::core::chat_context::resolve_effective_user_id;
use crate::core::internal_context_locale::{
    internal_context_locale_from_settings, ui_locale_from_settings, InternalContextLocale,
};
use crate::services::user_settings::get_effective_user_settings;

#[derive(Debug, Clone)]
pub struct ConversationRuntimeUserContext {
    pub effective_user_id: Option<String>,
    pub effective_settings: Value,
    pub internal_context_locale: InternalContextLocale,
    pub ui_locale: InternalContextLocale,
}

pub async fn load_runtime_user_context(
    explicit_user_id: Option<String>,
    session_id: &str,
) -> ConversationRuntimeUserContext {
    let effective_user_id = resolve_effective_user_id(explicit_user_id, session_id).await;
    let effective_settings = get_effective_user_settings(effective_user_id.clone())
        .await
        .unwrap_or_else(|_| json!({}));
    let internal_context_locale = internal_context_locale_from_settings(&effective_settings);
    let ui_locale = ui_locale_from_settings(&effective_settings);

    ConversationRuntimeUserContext {
        effective_user_id,
        effective_settings,
        internal_context_locale,
        ui_locale,
    }
}
