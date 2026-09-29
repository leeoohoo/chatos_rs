// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::{validate_identifier, LOCAL_AGENT_MAX_INPUT_BYTES};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet, VecDeque};
use std::str::FromStr;

pub const LOCAL_TASK_GRAPH_MAX_TASKS: usize = 128;
pub const LOCAL_TASK_GRAPH_MAX_DEPENDENCIES: usize = 512;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LocalTaskSpec {
    pub task_id: String,
    pub title: String,
    pub profile_key: String,
    pub model_config_ref: String,
    pub model_config_revision: String,
    pub capability_policy_revision: String,
    #[serde(default)]
    pub input: Value,
    pub max_iterations: u32,
}

impl LocalTaskSpec {
    fn validate(&self) -> Result<(), String> {
        validate_identifier("task_id", &self.task_id)?;
        let title = self.title.trim();
        if title.is_empty() || title.len() > 1_000 || title.contains('\0') {
            return Err("title must be 1..=1000 characters without NUL".to_string());
        }
        validate_identifier("profile_key", &self.profile_key)?;
        validate_identifier("model_config_ref", &self.model_config_ref)?;
        validate_identifier("model_config_revision", &self.model_config_revision)?;
        validate_identifier(
            "capability_policy_revision",
            &self.capability_policy_revision,
        )?;
        if self.max_iterations == 0 {
            return Err("max_iterations must be greater than zero".to_string());
        }
        let input_size = serde_json::to_vec(&self.input)
            .map_err(|error| format!("task input is not serializable: {error}"))?
            .len();
        if input_size > LOCAL_AGENT_MAX_INPUT_BYTES {
            return Err(format!(
                "task input exceeds the {LOCAL_AGENT_MAX_INPUT_BYTES} byte limit"
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct LocalTaskDependency {
    pub task_id: String,
    pub prerequisite_task_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CreateTaskGraphCommand {
    pub graph_id: String,
    pub owner_user_id: String,
    pub source_entity_type: String,
    pub source_entity_id: String,
    pub tasks: Vec<LocalTaskSpec>,
    #[serde(default)]
    pub dependencies: Vec<LocalTaskDependency>,
}

impl CreateTaskGraphCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_identifier("graph_id", &self.graph_id)?;
        validate_identifier("owner_user_id", &self.owner_user_id)?;
        validate_identifier("source_entity_type", &self.source_entity_type)?;
        validate_identifier("source_entity_id", &self.source_entity_id)?;
        if self.tasks.is_empty() || self.tasks.len() > LOCAL_TASK_GRAPH_MAX_TASKS {
            return Err(format!(
                "tasks must contain 1..={LOCAL_TASK_GRAPH_MAX_TASKS} items"
            ));
        }
        if self.dependencies.len() > LOCAL_TASK_GRAPH_MAX_DEPENDENCIES {
            return Err(format!(
                "dependencies must contain at most {LOCAL_TASK_GRAPH_MAX_DEPENDENCIES} items"
            ));
        }
        let mut task_ids = HashSet::new();
        for task in &self.tasks {
            task.validate()?;
            if !task_ids.insert(task.task_id.as_str()) {
                return Err(format!("task_id is duplicated: {}", task.task_id));
            }
        }
        validate_dependencies(&task_ids, &self.dependencies)
    }
}

fn validate_dependencies(
    task_ids: &HashSet<&str>,
    dependencies: &[LocalTaskDependency],
) -> Result<(), String> {
    let mut unique = HashSet::new();
    let mut indegree = task_ids
        .iter()
        .map(|task_id| ((*task_id).to_string(), 0_usize))
        .collect::<HashMap<_, _>>();
    let mut outgoing: HashMap<&str, Vec<&str>> = HashMap::new();
    for dependency in dependencies {
        validate_identifier("dependency.task_id", &dependency.task_id)?;
        validate_identifier(
            "dependency.prerequisite_task_id",
            &dependency.prerequisite_task_id,
        )?;
        if !task_ids.contains(dependency.task_id.as_str())
            || !task_ids.contains(dependency.prerequisite_task_id.as_str())
        {
            return Err("every dependency must reference tasks in the same graph".to_string());
        }
        if dependency.task_id == dependency.prerequisite_task_id {
            return Err("a task cannot depend on itself".to_string());
        }
        if !unique.insert((
            dependency.task_id.as_str(),
            dependency.prerequisite_task_id.as_str(),
        )) {
            return Err("task dependency is duplicated".to_string());
        }
        *indegree
            .get_mut(dependency.task_id.as_str())
            .expect("validated task") += 1;
        outgoing
            .entry(dependency.prerequisite_task_id.as_str())
            .or_default()
            .push(dependency.task_id.as_str());
    }
    let mut ready = indegree
        .iter()
        .filter_map(|(task_id, degree)| (*degree == 0).then_some(task_id.clone()))
        .collect::<VecDeque<_>>();
    let mut visited = 0;
    while let Some(task_id) = ready.pop_front() {
        visited += 1;
        for dependent in outgoing.get(task_id.as_str()).into_iter().flatten() {
            let degree = indegree.get_mut(*dependent).expect("validated dependent");
            *degree -= 1;
            if *degree == 0 {
                ready.push_back((*dependent).to_string());
            }
        }
    }
    if visited != task_ids.len() {
        return Err("task dependency graph contains a cycle".to_string());
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GetTaskGraphCommand {
    pub graph_id: String,
}

impl GetTaskGraphCommand {
    pub fn validate(&self) -> Result<(), String> {
        validate_identifier("graph_id", &self.graph_id)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LocalTaskStatus {
    Pending,
    Ready,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    Blocked,
}

impl LocalTaskStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Ready => "ready",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Blocked => "blocked",
        }
    }
}

impl FromStr for LocalTaskStatus {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "pending" => Ok(Self::Pending),
            "ready" => Ok(Self::Ready),
            "running" => Ok(Self::Running),
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            "blocked" => Ok(Self::Blocked),
            _ => Err(format!("unknown local task status: {value}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LocalTaskRecord {
    pub graph_id: String,
    pub owner_user_id: String,
    pub source_entity_type: String,
    pub source_entity_id: String,
    pub task_id: String,
    pub title: String,
    pub profile_key: String,
    pub model_config_ref: String,
    pub model_config_revision: String,
    pub capability_policy_revision: String,
    pub input: Value,
    pub max_iterations: u32,
    pub status: LocalTaskStatus,
    pub active_run_id: Option<String>,
    pub version: u64,
    pub created_at_unix_ms: i64,
    pub updated_at_unix_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LocalTaskGraph {
    pub graph_id: String,
    pub owner_user_id: String,
    pub source_entity_type: String,
    pub source_entity_id: String,
    pub tasks: Vec<LocalTaskRecord>,
    pub dependencies: Vec<LocalTaskDependency>,
    pub created_at_unix_ms: i64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn task(task_id: &str) -> LocalTaskSpec {
        LocalTaskSpec {
            task_id: task_id.to_string(),
            title: task_id.to_string(),
            profile_key: "task_runner".to_string(),
            model_config_ref: "model-1".to_string(),
            model_config_revision: "revision-1".to_string(),
            capability_policy_revision: "policy-1".to_string(),
            input: json!({"prompt": task_id}),
            max_iterations: 8,
        }
    }

    fn command(dependencies: Vec<LocalTaskDependency>) -> CreateTaskGraphCommand {
        CreateTaskGraphCommand {
            graph_id: "graph-1".to_string(),
            owner_user_id: "user-1".to_string(),
            source_entity_type: "conversation".to_string(),
            source_entity_id: "conversation-1".to_string(),
            tasks: vec![task("task-1"), task("task-2")],
            dependencies,
        }
    }

    #[test]
    fn validates_acyclic_graph_and_rejects_cycle() {
        assert!(command(vec![LocalTaskDependency {
            task_id: "task-2".to_string(),
            prerequisite_task_id: "task-1".to_string(),
        }])
        .validate()
        .is_ok());
        assert!(command(vec![
            LocalTaskDependency {
                task_id: "task-2".to_string(),
                prerequisite_task_id: "task-1".to_string(),
            },
            LocalTaskDependency {
                task_id: "task-1".to_string(),
                prerequisite_task_id: "task-2".to_string(),
            },
        ])
        .validate()
        .is_err());
    }
}
