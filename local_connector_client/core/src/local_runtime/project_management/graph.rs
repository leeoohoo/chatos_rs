// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;

use super::{
    LocalDependencyGraph, LocalDependencyGraphEdge, LocalDependencyGraphNode,
    LocalRequirementRecord, LocalWorkItemRecord,
};

pub(crate) fn build_local_dependency_graph(
    project_id: &str,
    requirements: &[LocalRequirementRecord],
    work_items: &[LocalWorkItemRecord],
    requirement_dependencies: &[(String, String, String)],
    work_item_dependencies: &[(String, String, String)],
) -> LocalDependencyGraph {
    let requirement_ids = requirements
        .iter()
        .map(|record| record.id.as_str())
        .collect::<HashSet<_>>();
    let work_item_ids = work_items
        .iter()
        .map(|record| record.id.as_str())
        .collect::<HashSet<_>>();
    let mut nodes = requirements
        .iter()
        .map(requirement_node)
        .chain(work_items.iter().map(work_item_node))
        .collect::<Vec<_>>();
    let mut edges = Vec::new();
    for (requirement_id, prerequisite_id, relation_type) in requirement_dependencies {
        if requirement_ids.contains(requirement_id.as_str())
            && requirement_ids.contains(prerequisite_id.as_str())
        {
            edges.push(LocalDependencyGraphEdge {
                from: format!("requirement:{prerequisite_id}"),
                to: format!("requirement:{requirement_id}"),
                edge_type: relation_type.clone(),
            });
        }
    }
    for item in work_items {
        if requirement_ids.contains(item.requirement_id.as_str()) {
            edges.push(LocalDependencyGraphEdge {
                from: format!("requirement:{}", item.requirement_id),
                to: format!("work_item:{}", item.id),
                edge_type: "contains".to_string(),
            });
        }
    }
    for (work_item_id, prerequisite_id, relation_type) in work_item_dependencies {
        if work_item_ids.contains(work_item_id.as_str())
            && work_item_ids.contains(prerequisite_id.as_str())
        {
            edges.push(LocalDependencyGraphEdge {
                from: format!("work_item:{prerequisite_id}"),
                to: format!("work_item:{work_item_id}"),
                edge_type: relation_type.clone(),
            });
        }
    }
    nodes.sort_by(|left, right| left.id.cmp(&right.id));
    LocalDependencyGraph {
        root_id: Some(format!("project:{project_id}")),
        nodes,
        edges,
        blocked_by: Vec::new(),
        ready: true,
    }
}

fn requirement_node(record: &LocalRequirementRecord) -> LocalDependencyGraphNode {
    LocalDependencyGraphNode {
        id: format!("requirement:{}", record.id),
        node_type: "requirement".to_string(),
        label: record.title.clone(),
        status: record.status.clone(),
        parent_id: record.parent_requirement_id.clone(),
        raw_id: record.id.clone(),
    }
}

fn work_item_node(record: &LocalWorkItemRecord) -> LocalDependencyGraphNode {
    LocalDependencyGraphNode {
        id: format!("work_item:{}", record.id),
        node_type: "work_item".to_string(),
        label: record.title.clone(),
        status: record.status.clone(),
        parent_id: Some(record.requirement_id.clone()),
        raw_id: record.id.clone(),
    }
}
