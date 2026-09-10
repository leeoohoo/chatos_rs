// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskExecutionScope {
    UserConversation {
        tenant_id: String,
        owner_user_id: String,
    },
    Project {
        tenant_id: String,
        owner_user_id: String,
        project_id: String,
    },
}

impl TaskExecutionScope {
    pub fn workspace_project_id(&self) -> Option<&str> {
        match self {
            Self::UserConversation { .. } => None,
            Self::Project { project_id, .. } => Some(project_id.as_str()),
        }
    }

    pub fn owner_user_id(&self) -> &str {
        match self {
            Self::UserConversation { owner_user_id, .. } | Self::Project { owner_user_id, .. } => {
                owner_user_id.as_str()
            }
        }
    }
}

pub fn resolve_task_execution_scope(
    project_id: Option<&str>,
    tenant_id: &str,
    owner_user_id: &str,
) -> TaskExecutionScope {
    let project_id = normalize_project_id(project_id.map(ToOwned::to_owned));
    let tenant_id = tenant_id.trim().to_string();
    let owner_user_id = owner_user_id.trim().to_string();
    match project_id {
        Some(project_id) => TaskExecutionScope::Project {
            tenant_id,
            owner_user_id,
            project_id,
        },
        None => TaskExecutionScope::UserConversation {
            tenant_id,
            owner_user_id,
        },
    }
}

pub fn normalize_project_id(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod execution_scope_tests {
    use super::*;

    #[test]
    fn user_conversation_scope_is_owner_scoped_instead_of_globally_shared() {
        let first = resolve_task_execution_scope(None, "tenant-1", "user-1");
        let second = resolve_task_execution_scope(None, "tenant-1", "user-2");

        assert_ne!(first, second);
        assert_eq!(first.workspace_project_id(), None);
        assert_eq!(first.owner_user_id(), "user-1");
    }

    #[test]
    fn concrete_project_scope_exposes_workspace_identity() {
        let scope = resolve_task_execution_scope(Some(" project-1 "), "tenant-1", "user-1");

        assert_eq!(scope.workspace_project_id(), Some("project-1"));
        assert_eq!(scope.owner_user_id(), "user-1");
    }
}
