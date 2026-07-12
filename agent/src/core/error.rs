// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::error::Error;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentError {
    agent_key: &'static str,
    message: String,
}

impl AgentError {
    pub fn execution(agent_key: &'static str, message: impl Into<String>) -> Self {
        Self {
            agent_key,
            message: message.into(),
        }
    }

    pub fn agent_key(&self) -> &'static str {
        self.agent_key
    }

    pub fn message(&self) -> &str {
        self.message.as_str()
    }
}

impl fmt::Display for AgentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} failed: {}", self.agent_key, self.message)
    }
}

impl Error for AgentError {}
