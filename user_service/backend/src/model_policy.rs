// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_service_runtime::model_workload::{default_chat_budget, ModelWorkload};

use crate::models::UserModelConfigRecord;

/// Import is where the service publishes its request budget. Runtime only freezes
/// that published record; media operations never depend on a chat token budget.
pub(crate) fn publish_missing_chat_budget(model: &mut UserModelConfigRecord) -> bool {
    if ModelWorkload::for_model(&model.model) != ModelWorkload::Chat
        || model.model_capability_revision.is_some()
        || model.context_window_tokens.is_some()
        || model.output_reserve_tokens.is_some()
        || model.protocol_overhead_estimator_id.is_some()
    {
        return false;
    }
    let budget = default_chat_budget();
    model.model_capability_revision = Some(budget.revision.clone());
    model.context_window_tokens = Some(budget.context_window_tokens);
    model.output_reserve_tokens = Some(budget.output_reserve_tokens);
    model.protocol_overhead_estimator_id = Some(budget.protocol_overhead_estimator_id.clone());
    if model.tokenizer_id.is_none() {
        model.tokenizer_id = budget.tokenizer_id.clone();
    }
    true
}
