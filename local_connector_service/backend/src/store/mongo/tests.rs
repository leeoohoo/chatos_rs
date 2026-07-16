// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::managed_requirements::collect_applicable_managed_requirements_layers;
use super::*;

use mongodb::bson::{self, Bson};

fn policy(id: &str, enabled: bool) -> ManagedRequirementsPolicy {
    ManagedRequirementsPolicy {
        id: id.to_string(),
        name: id.to_string(),
        description: None,
        requirements_toml: String::new(),
        content_sha256: "sha256:empty".to_string(),
        version: 1,
        enabled,
        created_by: "admin-1".to_string(),
        updated_by: "admin-1".to_string(),
        created_at: "2026-07-15T00:00:00Z".to_string(),
        updated_at: "2026-07-15T00:00:00Z".to_string(),
    }
}

fn assignment(
    id: &str,
    policy_id: &str,
    scope: &str,
    subject: Option<&str>,
    priority: i32,
    enabled: bool,
) -> ManagedRequirementsAssignment {
    ManagedRequirementsAssignment {
        id: id.to_string(),
        policy_id: policy_id.to_string(),
        scope: scope.to_string(),
        subject: subject.map(str::to_string),
        priority,
        enabled,
        created_by: "admin-1".to_string(),
        updated_by: "admin-1".to_string(),
        created_at: "2026-07-15T00:00:00Z".to_string(),
        updated_at: "2026-07-15T00:00:00Z".to_string(),
    }
}

#[test]
fn applicable_layers_are_global_then_role_then_user_and_priority_ascending() {
    let assignments = vec![
        assignment("user", "policy-user", "user", Some("user-1"), -100, true),
        assignment(
            "role-high",
            "policy-role-high",
            "role",
            Some("admin"),
            10,
            true,
        ),
        assignment("global", "policy-global", "global", None, 100, true),
        assignment(
            "role-low",
            "policy-role-low",
            "role",
            Some("admin"),
            -10,
            true,
        ),
    ];
    let policies = [
        policy("policy-user", true),
        policy("policy-role-high", true),
        policy("policy-global", true),
        policy("policy-role-low", true),
    ]
    .into_iter()
    .map(|policy| (policy.id.clone(), policy))
    .collect();

    let layers = collect_applicable_managed_requirements_layers(assignments, policies);
    let ids = layers
        .iter()
        .map(|layer| layer.assignment.id.as_str())
        .collect::<Vec<_>>();

    assert_eq!(ids, vec!["global", "role-low", "role-high", "user"]);
}

#[test]
fn disabled_assignments_and_policies_do_not_produce_layers() {
    let assignments = vec![
        assignment("enabled", "policy-enabled", "global", None, 0, true),
        assignment(
            "disabled-assignment",
            "policy-enabled",
            "global",
            None,
            1,
            false,
        ),
        assignment(
            "disabled-policy",
            "policy-disabled",
            "user",
            Some("user-1"),
            0,
            true,
        ),
    ];
    let policies = [
        policy("policy-enabled", true),
        policy("policy-disabled", false),
    ]
    .into_iter()
    .map(|policy| (policy.id.clone(), policy))
    .collect();

    let layers = collect_applicable_managed_requirements_layers(assignments, policies);

    assert_eq!(layers.len(), 1);
    assert_eq!(layers[0].assignment.id, "enabled");
}

#[test]
fn global_assignment_subject_is_serialized_as_null_for_the_unique_compound_index() {
    let document = bson::to_document(&assignment(
        "global",
        "policy-global",
        "global",
        None,
        0,
        true,
    ))
    .unwrap();

    assert_eq!(document.get("subject"), Some(&Bson::Null));
}
