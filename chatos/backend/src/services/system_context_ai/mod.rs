// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod normalization;
mod operations;
mod parsing;
mod types;

pub use self::operations::{evaluate_draft, generate_draft, optimize_draft};
pub use self::types::{
    EvaluateDraftInput, GenerateDraftInput, OptimizeDraftInput, PromptRuntimeOverrides,
    SystemContextAiError,
};
