// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

#[test]
fn project_scoped_binding_only_matches_cloud_project_context() {
    let conditions = BindingConditions {
        project_source_type: Some("cloud".to_string()),
        ..BindingConditions::default()
    };
    assert!(binding_matches_runtime_context(
        &conditions,
        &BindingConditions {
            project_source_type: Some("CLOUD".to_string()),
            ..BindingConditions::default()
        }
    ));
    assert!(!binding_matches_runtime_context(
        &conditions,
        &BindingConditions {
            project_source_type: Some("public".to_string()),
            ..BindingConditions::default()
        }
    ));
    assert!(!binding_matches_runtime_context(
        &conditions,
        &BindingConditions::default()
    ));
}

#[test]
fn unconditional_binding_matches_every_runtime_context() {
    assert!(binding_matches_runtime_context(
        &BindingConditions::default(),
        &BindingConditions {
            task_profile: Some("default".to_string()),
            project_source_type: Some("public".to_string()),
            schedule_mode: Some("contact_async".to_string()),
            ..BindingConditions::default()
        }
    ));
}
