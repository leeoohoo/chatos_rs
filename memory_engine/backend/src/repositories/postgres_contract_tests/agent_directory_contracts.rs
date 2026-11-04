// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use crate::models::{UpsertSubjectMemoryRequest, UpsertSubjectRequest, UpsertThreadRequest};

#[tokio::test]
async fn postgres_agent_directory_groups_memory_projects_and_threads() {
    let Some(pool) = pool().await else {
        return;
    };
    let suffix = Uuid::new_v4().to_string();
    let tenant = format!("tenant-{suffix}");
    let source = format!("source-{suffix}");
    let agent_id = format!("agent-{suffix}");
    let agent_subject = format!("agent:{agent_id}");
    let project_id = format!("project-{suffix}");
    let project_subject = format!("project:{project_id}");
    let thread_id = format!("thread-{suffix}");

    for (subject_id, subject_type, display_name) in [
        (agent_subject.as_str(), "agent", "Contract Agent"),
        (project_subject.as_str(), "project", "Contract Project"),
    ] {
        crate::repositories::subjects::upsert_subject(
            &pool,
            subject_id,
            UpsertSubjectRequest {
                tenant_id: tenant.clone(),
                source_id: source.clone(),
                subject_type: subject_type.to_string(),
                display_name: Some(display_name.to_string()),
                attributes: None,
                status: None,
            },
        )
        .await
        .expect("upsert identity subject");
    }

    crate::repositories::threads::upsert_thread(
        &pool,
        &thread_id,
        UpsertThreadRequest {
            tenant_id: tenant.clone(),
            source_id: source.clone(),
            subject_id: agent_subject.clone(),
            thread_type: "agent_task".to_string(),
            external_thread_id: Some(format!("external-{suffix}")),
            title: Some("Contract task".to_string()),
            labels: Some(vec![
                "memory-schema:v2".to_string(),
                agent_subject.clone(),
                project_subject.clone(),
            ]),
            metadata: None,
            status: None,
            created_at: None,
            updated_at: None,
            archived_at: None,
        },
    )
    .await
    .expect("upsert agent thread");

    crate::repositories::threads::upsert_thread(
        &pool,
        &format!("legacy-thread-{suffix}"),
        UpsertThreadRequest {
            tenant_id: tenant.clone(),
            source_id: source.clone(),
            subject_id: format!("session:{suffix}"),
            thread_type: "approval_run".to_string(),
            external_thread_id: Some(format!("legacy-external-{suffix}")),
            title: Some("Legacy approval".to_string()),
            labels: None,
            metadata: Some(json!({
                "legacy_session_mapping": {
                    "agent_id": agent_id,
                    "project_id": project_id,
                }
            })),
            status: None,
            created_at: None,
            updated_at: None,
            archived_at: None,
        },
    )
    .await
    .expect("upsert legacy agent thread");

    for (subject_id, memory_key, memory_type, text) in [
        (
            agent_subject.as_str(),
            "agent-memory",
            "agent_memory",
            "Agent durable memory",
        ),
        (
            project_subject.as_str(),
            "project-summary",
            "project_summary",
            "Project shared summary",
        ),
    ] {
        crate::repositories::subject_memories::upsert_subject_memory(
            &pool,
            subject_id,
            memory_key,
            UpsertSubjectMemoryRequest {
                id: None,
                tenant_id: tenant.clone(),
                source_id: source.clone(),
                memory_type: memory_type.to_string(),
                text: text.to_string(),
                level: Some(0),
                source_digest: None,
                confidence: None,
                last_seen_at: None,
                metadata: None,
                rollup_status: None,
                rollup_memory_key: None,
                rolled_up_at: None,
                status: None,
                created_at: None,
                updated_at: None,
            },
        )
        .await
        .expect("upsert identity memory");
    }

    let agents = crate::repositories::agent_directory::list_agents(
        &pool,
        Some(&tenant),
        Some(&source),
        Some(&agent_subject),
        10,
        0,
    )
    .await
    .expect("list agent directory");
    assert_eq!(agents.len(), 1);
    assert_eq!(agents[0].agent_id, agent_id);
    assert_eq!(agents[0].display_name.as_deref(), Some("Contract Agent"));
    assert_eq!(agents[0].thread_count, 2);
    assert_eq!(agents[0].project_count, 1);
    assert_eq!(agents[0].memory_count, 1);

    let agent_threads = crate::repositories::threads::list_threads(
        &pool,
        crate::repositories::threads::ListThreadsQuery {
            tenant_id: Some(&tenant),
            source_id: Some(&source),
            agent_id: Some(&agent_id),
            limit: 10,
            ..Default::default()
        },
    )
    .await
    .expect("list current and legacy agent threads");
    assert_eq!(agent_threads.len(), 2);

    let projects = crate::repositories::agent_directory::list_agent_projects(
        &pool,
        &tenant,
        &source,
        &agent_subject,
    )
    .await
    .expect("list agent projects");
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0].project_id, project_id);
    assert_eq!(
        projects[0].display_name.as_deref(),
        Some("Contract Project")
    );
    assert_eq!(projects[0].thread_count, 2);
    assert_eq!(projects[0].memory_count, 1);
}
