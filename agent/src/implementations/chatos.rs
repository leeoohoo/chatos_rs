// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use async_trait::async_trait;
use chatos_plugin_management_sdk::SystemAgentKey;

use crate::{agent_descriptor, AgentDescriptor, AgentIdentity, CHATOS_ASYNC_PLANNER_TOOL_PROFILE};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChatosAgentProfile {
    key: SystemAgentKey,
    requires_concrete_project: bool,
    task_runner_tool_profile: &'static str,
}

impl ChatosAgentProfile {
    pub fn for_runtime() -> Self {
        Self {
            key: SystemAgentKey::ChatosConversationAgent,
            requires_concrete_project: false,
            task_runner_tool_profile: CHATOS_ASYNC_PLANNER_TOOL_PROFILE,
        }
    }

    pub fn key(self) -> SystemAgentKey {
        self.key
    }

    pub fn requires_concrete_project(self) -> bool {
        self.requires_concrete_project
    }

    pub fn task_runner_tool_profile(self) -> &'static str {
        self.task_runner_tool_profile
    }
}

impl AgentIdentity for ChatosAgentProfile {
    fn descriptor(&self) -> &'static AgentDescriptor {
        agent_descriptor(self.key)
    }
}

#[async_trait]
pub trait ChatosStreamRuntime: Send {
    type Options: Send;
    type Output: Send;

    async fn execute(
        &mut self,
        conversation_id: &str,
        user_message: &str,
        options: Self::Options,
    ) -> Result<Self::Output, String>;
}

pub struct ChatosStreamAgent<R> {
    profile: ChatosAgentProfile,
    runtime: R,
}

impl<R> ChatosStreamAgent<R> {
    pub fn new(profile: ChatosAgentProfile, runtime: R) -> Self {
        Self { profile, runtime }
    }

    pub fn profile(&self) -> ChatosAgentProfile {
        self.profile
    }

    pub fn runtime(&self) -> &R {
        &self.runtime
    }

    pub fn runtime_mut(&mut self) -> &mut R {
        &mut self.runtime
    }

    pub fn into_runtime(self) -> R {
        self.runtime
    }
}

impl<R> ChatosStreamAgent<R>
where
    R: ChatosStreamRuntime,
{
    pub async fn execute(
        &mut self,
        conversation_id: &str,
        user_message: &str,
        options: R::Options,
    ) -> Result<R::Output, String> {
        self.runtime
            .execute(conversation_id, user_message, options)
            .await
    }
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;

    use super::*;

    #[test]
    fn runtime_profile_is_the_conversation_agent() {
        assert_eq!(
            ChatosAgentProfile::for_runtime().key(),
            SystemAgentKey::ChatosConversationAgent
        );
    }

    struct FakeStreamRuntime;

    #[async_trait]
    impl ChatosStreamRuntime for FakeStreamRuntime {
        type Options = usize;
        type Output = String;

        async fn execute(
            &mut self,
            conversation_id: &str,
            user_message: &str,
            options: Self::Options,
        ) -> Result<Self::Output, String> {
            Ok(format!("{conversation_id}:{user_message}:{options}"))
        }
    }

    #[tokio::test]
    async fn stream_agent_preserves_profile_and_delegates_execution() {
        let profile = ChatosAgentProfile::for_runtime();
        let mut agent = ChatosStreamAgent::new(profile, FakeStreamRuntime);

        let output = agent
            .execute("session-1", "hello", 3)
            .await
            .expect("stream execution");

        assert_eq!(agent.profile(), profile);
        assert_eq!(output, "session-1:hello:3");
    }
}
