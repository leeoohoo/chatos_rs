// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

#[path = "chat_runtime_metadata.rs"]
mod chat_runtime_metadata;

pub use self::chat_runtime_metadata::{
    contact_agent_id_from_metadata, contact_id_from_metadata, normalize_project_id,
    project_id_from_metadata,
};
