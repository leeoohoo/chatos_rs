// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod execute;
mod stop;

use serde::Deserialize;

pub(super) use execute::execute_requirement;
pub(super) use stop::stop_requirement;

#[derive(Debug, Default, Deserialize)]
pub(super) struct ExecuteRequirementPayload {
    #[allow(dead_code)]
    contact_id: Option<String>,
    #[serde(default, alias = "includePrerequisiteDependents")]
    #[allow(dead_code)]
    include_prerequisite_dependents: bool,
}
