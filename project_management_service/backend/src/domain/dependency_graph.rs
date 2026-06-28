use std::collections::HashSet;

use crate::models::{
    DbStatus, DependencyGraphEdge, DependencyGraphNode, DependencyGraphResponse,
    ProjectWorkItemRecord, RequirementDependencyRecord, RequirementRecord,
    WorkItemDependencyRecord,
};

pub fn project_dependency_graph(
    project_id: &str,
    requirements: &[RequirementRecord],
    work_items: &[ProjectWorkItemRecord],
    requirement_dependencies: &[RequirementDependencyRecord],
    work_item_dependencies: &[WorkItemDependencyRecord],
) -> DependencyGraphResponse {
    let requirement_ids = requirements
        .iter()
        .map(|item| item.id.as_str())
        .collect::<HashSet<_>>();
    let work_item_ids = work_items
        .iter()
        .map(|item| item.id.as_str())
        .collect::<HashSet<_>>();

    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    for requirement in requirements {
        nodes.push(requirement_node(requirement));
    }
    for dependency in requirement_dependencies {
        if requirement_ids.contains(dependency.requirement_id.as_str())
            && requirement_ids.contains(dependency.prerequisite_requirement_id.as_str())
        {
            edges.push(DependencyGraphEdge {
                from: format!("requirement:{}", dependency.prerequisite_requirement_id),
                to: format!("requirement:{}", dependency.requirement_id),
                edge_type: dependency.relation_type.clone(),
            });
        }
    }

    for item in work_items {
        nodes.push(work_item_node(item));
        if requirement_ids.contains(item.requirement_id.as_str()) {
            edges.push(DependencyGraphEdge {
                from: format!("requirement:{}", item.requirement_id),
                to: format!("work_item:{}", item.id),
                edge_type: "contains".to_string(),
            });
        }
    }
    for dependency in work_item_dependencies {
        if work_item_ids.contains(dependency.work_item_id.as_str())
            && work_item_ids.contains(dependency.prerequisite_work_item_id.as_str())
        {
            edges.push(DependencyGraphEdge {
                from: format!("work_item:{}", dependency.prerequisite_work_item_id),
                to: format!("work_item:{}", dependency.work_item_id),
                edge_type: dependency.relation_type.clone(),
            });
        }
    }

    DependencyGraphResponse {
        root_id: Some(format!("project:{project_id}")),
        nodes,
        edges,
        blocked_by: Vec::new(),
        ready: true,
    }
}

pub fn requirement_node(requirement: &RequirementRecord) -> DependencyGraphNode {
    DependencyGraphNode {
        id: format!("requirement:{}", requirement.id),
        raw_id: requirement.id.clone(),
        node_type: "requirement".to_string(),
        label: requirement.title.clone(),
        status: requirement.status.as_str().to_string(),
        parent_id: requirement.parent_requirement_id.clone(),
    }
}

pub fn work_item_node(item: &ProjectWorkItemRecord) -> DependencyGraphNode {
    DependencyGraphNode {
        id: format!("work_item:{}", item.id),
        raw_id: item.id.clone(),
        node_type: "work_item".to_string(),
        label: item.title.clone(),
        status: item.status.as_str().to_string(),
        parent_id: Some(item.requirement_id.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ProjectWorkItemStatus, RequirementStatus, RequirementType};

    #[test]
    fn project_graph_keeps_only_edges_inside_visible_sets() {
        let requirements = vec![
            requirement_record("req-a", None),
            requirement_record("req-b", Some("req-a")),
        ];
        let work_items = vec![work_item_record("item-a", "req-a")];
        let requirement_dependencies = vec![
            RequirementDependencyRecord {
                requirement_id: "req-b".to_string(),
                prerequisite_requirement_id: "req-a".to_string(),
                relation_type: "blocks".to_string(),
                created_at: "now".to_string(),
            },
            RequirementDependencyRecord {
                requirement_id: "req-b".to_string(),
                prerequisite_requirement_id: "hidden-req".to_string(),
                relation_type: "blocks".to_string(),
                created_at: "now".to_string(),
            },
        ];
        let work_item_dependencies = vec![WorkItemDependencyRecord {
            work_item_id: "item-a".to_string(),
            prerequisite_work_item_id: "hidden-item".to_string(),
            relation_type: "blocks".to_string(),
            created_at: "now".to_string(),
        }];

        let graph = project_dependency_graph(
            "project-1",
            &requirements,
            &work_items,
            &requirement_dependencies,
            &work_item_dependencies,
        );

        assert_eq!(graph.nodes.len(), 3);
        assert!(graph
            .edges
            .iter()
            .any(|edge| { edge.from == "requirement:req-a" && edge.to == "requirement:req-b" }));
        assert!(graph
            .edges
            .iter()
            .any(|edge| { edge.from == "requirement:req-a" && edge.to == "work_item:item-a" }));
        assert!(!graph
            .edges
            .iter()
            .any(|edge| edge.from.contains("hidden") || edge.to.contains("hidden")));
    }

    fn requirement_record(id: &str, parent_requirement_id: Option<&str>) -> RequirementRecord {
        RequirementRecord {
            id: id.to_string(),
            project_id: "project-1".to_string(),
            parent_requirement_id: parent_requirement_id.map(ToOwned::to_owned),
            requirement_type: RequirementType::Requirement,
            title: id.to_string(),
            summary: None,
            detail: None,
            business_value: None,
            acceptance_criteria: None,
            source: None,
            priority: 0,
            status: RequirementStatus::Draft,
            creator_user_id: None,
            creator_username: None,
            creator_display_name: None,
            owner_user_id: Some("user-1".to_string()),
            owner_username: Some("owner".to_string()),
            owner_display_name: Some("Owner".to_string()),
            assignee_user_id: None,
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
            archived_at: None,
        }
    }

    fn work_item_record(id: &str, requirement_id: &str) -> ProjectWorkItemRecord {
        ProjectWorkItemRecord {
            id: id.to_string(),
            project_id: "project-1".to_string(),
            requirement_id: requirement_id.to_string(),
            title: id.to_string(),
            description: None,
            task_runner_default_model_config_id: "model-1".to_string(),
            task_runner_enabled_tool_ids: vec!["tool-1".to_string()],
            status: ProjectWorkItemStatus::Todo,
            priority: 0,
            assignee_user_id: None,
            estimate_points: None,
            due_at: None,
            sort_order: 0,
            tags: Vec::new(),
            creator_user_id: None,
            creator_username: None,
            creator_display_name: None,
            owner_user_id: Some("user-1".to_string()),
            owner_username: Some("owner".to_string()),
            owner_display_name: Some("Owner".to_string()),
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
            archived_at: None,
        }
    }
}
