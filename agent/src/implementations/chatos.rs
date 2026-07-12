// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use async_trait::async_trait;
use chatos_plugin_management_sdk::SystemAgentKey;

use crate::{agent_descriptor, AgentDescriptor, AgentIdentity};

pub const CHATOS_ASYNC_PLANNER_TOOL_PROFILE: &str = "chatos_async_planner";
pub const PROJECT_REQUIREMENT_EXECUTION_PLANNER_TOOL_PROFILE: &str =
    "project_requirement_execution_planner";
pub const CHATOS_PLAN_TASK_PROFILE: &str = "chatos_plan";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChatosAgentProfile {
    key: SystemAgentKey,
    requires_concrete_project: bool,
    task_runner_tool_profile: &'static str,
    task_runner_task_profile: Option<&'static str>,
    plan_mode_header: bool,
    requires_project_management_mcp: bool,
}

impl ChatosAgentProfile {
    pub fn from_flags(plan_mode: bool, project_requirement_execution_planner: bool) -> Self {
        let key = if project_requirement_execution_planner {
            SystemAgentKey::ProjectRequirementExecutionPlannerAgent
        } else if plan_mode {
            SystemAgentKey::ChatosPlanningAgent
        } else {
            SystemAgentKey::ChatosConversationAgent
        };
        Self {
            key,
            requires_concrete_project: plan_mode || project_requirement_execution_planner,
            task_runner_tool_profile: if project_requirement_execution_planner {
                PROJECT_REQUIREMENT_EXECUTION_PLANNER_TOOL_PROFILE
            } else {
                CHATOS_ASYNC_PLANNER_TOOL_PROFILE
            },
            task_runner_task_profile: plan_mode.then_some(CHATOS_PLAN_TASK_PROFILE),
            plan_mode_header: plan_mode,
            requires_project_management_mcp: project_requirement_execution_planner,
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

    pub fn task_runner_task_profile(self) -> Option<&'static str> {
        self.task_runner_task_profile
    }

    pub fn plan_mode_header(self) -> bool {
        self.plan_mode_header
    }

    pub fn requires_project_management_mcp(self) -> bool {
        self.requires_project_management_mcp
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
    fn resolves_conversation_planning_and_requirement_profiles() {
        let conversation = ChatosAgentProfile::from_flags(false, false);
        assert_eq!(conversation.key(), SystemAgentKey::ChatosConversationAgent);
        assert!(!conversation.requires_concrete_project());
        assert_eq!(
            conversation.task_runner_tool_profile(),
            CHATOS_ASYNC_PLANNER_TOOL_PROFILE
        );
        assert_eq!(conversation.task_runner_task_profile(), None);

        let planning = ChatosAgentProfile::from_flags(true, false);
        assert_eq!(planning.key(), SystemAgentKey::ChatosPlanningAgent);
        assert!(planning.requires_concrete_project());
        assert_eq!(
            planning.task_runner_task_profile(),
            Some(CHATOS_PLAN_TASK_PROFILE)
        );
        assert!(planning.plan_mode_header());

        let requirement = ChatosAgentProfile::from_flags(false, true);
        assert_eq!(
            requirement.key(),
            SystemAgentKey::ProjectRequirementExecutionPlannerAgent
        );
        assert_eq!(
            requirement.task_runner_tool_profile(),
            PROJECT_REQUIREMENT_EXECUTION_PLANNER_TOOL_PROFILE
        );
        assert!(requirement.requires_project_management_mcp());
    }

    #[test]
    fn preserves_legacy_combined_flag_behavior() {
        let profile = ChatosAgentProfile::from_flags(true, true);

        assert_eq!(
            profile.key(),
            SystemAgentKey::ProjectRequirementExecutionPlannerAgent
        );
        assert_eq!(
            profile.task_runner_tool_profile(),
            PROJECT_REQUIREMENT_EXECUTION_PLANNER_TOOL_PROFILE
        );
        assert_eq!(
            profile.task_runner_task_profile(),
            Some(CHATOS_PLAN_TASK_PROFILE)
        );
        assert!(profile.plan_mode_header());
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
        let profile = ChatosAgentProfile::from_flags(true, false);
        let mut agent = ChatosStreamAgent::new(profile, FakeStreamRuntime);

        let output = agent
            .execute("session-1", "hello", 3)
            .await
            .expect("stream execution");

        assert_eq!(agent.profile(), profile);
        assert_eq!(output, "session-1:hello:3");
    }
}
