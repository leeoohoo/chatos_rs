mod budget;
mod execution_plan;
mod items;

#[cfg(test)]
mod tests;

pub use budget::{
    sanitize_tool_results_for_model, sanitize_tool_results_for_model_with_budget,
    ToolResultModelBudget, ToolResultModelBudgetLimits, DEFAULT_TOOL_RESULTS_MODEL_TOTAL_MAX_CHARS,
    DEFAULT_TOOL_RESULT_MODEL_MAX_CHARS, TOOL_RESULTS_MODEL_TOTAL_MAX_CHARS_ENV,
    TOOL_RESULT_MODEL_MAX_CHARS_ENV,
};
pub use execution_plan::{
    build_tool_call_execution_plan, expand_tool_results_with_aliases, ToolCallExecutionPlan,
};
pub use items::{
    append_responses_tool_results, append_responses_tool_results_with_budget, append_tool_results,
    append_tool_results_with_budget, append_tool_turn_items, build_tool_call_items,
    build_tool_output_items, build_tool_output_items_for_calls_with_budget,
    build_tool_output_items_with_budget, merge_missing_tool_turn_items,
    merge_pending_tool_turn_items,
};
