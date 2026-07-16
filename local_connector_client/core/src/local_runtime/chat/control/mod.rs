// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod guidance;
mod registry;

use serde::Serialize;

pub(crate) use guidance::LocalGuidanceLifecycleHook;
pub(crate) use registry::LocalTurnControlRegistry;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct LocalRuntimeGuidance {
    pub(crate) guidance_id: String,
    pub(crate) session_id: String,
    pub(crate) turn_id: String,
    pub(crate) message_id: String,
    pub(crate) content: String,
    pub(crate) status: String,
    pub(crate) created_at: String,
}
