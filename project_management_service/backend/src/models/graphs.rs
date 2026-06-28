use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyGraphNode {
    pub id: String,
    pub node_type: String,
    pub label: String,
    pub status: String,
    pub parent_id: Option<String>,
    pub raw_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyGraphEdge {
    pub from: String,
    pub to: String,
    pub edge_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyGraphResponse {
    pub root_id: Option<String>,
    pub nodes: Vec<DependencyGraphNode>,
    pub edges: Vec<DependencyGraphEdge>,
    pub blocked_by: Vec<DependencyGraphNode>,
    pub ready: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRunnerExecutionOptionRecord {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRunnerExecutionOptionsResponse {
    pub model_configs: Vec<TaskRunnerExecutionOptionRecord>,
    pub tools: Vec<TaskRunnerExecutionOptionRecord>,
}
