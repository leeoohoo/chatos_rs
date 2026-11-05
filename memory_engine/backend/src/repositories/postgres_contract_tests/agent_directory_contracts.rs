// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use crate::models::{
    EngineRecord, UpsertSubjectMemoryRequest, UpsertSubjectRequest, UpsertThreadRequest,
};

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
    let record_project_id = format!("record-project-{suffix}");
    let record_project_subject = format!("project:{record_project_id}");
    let thread_id = format!("thread-{suffix}");

    for (subject_id, subject_type, display_name) in [
        (agent_subject.as_str(), "agent", "Contract Agent"),
        (project_subject.as_str(), "project", "Contract Project"),
        (
            record_project_subject.as_str(),
            "project",
            "Record Contract Project",
        ),
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

    let shared_thread_id = format!("shared-main-chat-{suffix}");
    crate::repositories::threads::upsert_thread(
        &pool,
        &shared_thread_id,
        UpsertThreadRequest {
            tenant_id: tenant.clone(),
            source_id: source.clone(),
            subject_id: agent_subject.clone(),
            thread_type: "conversation".to_string(),
            external_thread_id: Some(format!("shared-main-chat-external-{suffix}")),
            title: Some("Shared main chat".to_string()),
            labels: Some(vec![
                "memory-schema:v2".to_string(),
                agent_subject.clone(),
                "thread-kind:main_chat".to_string(),
            ]),
            metadata: Some(json!({
                "agent_id": agent_id.clone(),
                "thread_kind": "main_chat",
            })),
            status: None,
            created_at: None,
            updated_at: None,
            archived_at: None,
        },
    )
    .await
    .expect("upsert shared main chat thread");

    let mut tx = pool
        .begin()
        .await
        .expect("begin project record transaction");
    crate::repositories::records::upsert_record_row(
        &mut tx,
        &EngineRecord {
            id: format!("record-project-message-{suffix}"),
            thread_id: shared_thread_id,
            tenant_id: tenant.clone(),
            source_id: source.clone(),
            external_record_id: None,
            role: "user".to_string(),
            record_type: "message".to_string(),
            content: "Project-scoped main chat message".to_string(),
            structured_payload: None,
            metadata: Some(json!({"memory_project_id": record_project_id.clone()})),
            summary_status: "pending".to_string(),
            summary_id: None,
            summarized_at: None,
            created_at: chrono::Utc::now().to_rfc3339(),
        },
    )
    .await
    .expect("upsert project-scoped main chat record");
    tx.commit()
        .await
        .expect("commit project record transaction");

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
    assert_eq!(agents[0].project_count, 2);
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
    .expect("list canonical Agent threads");
    assert_eq!(agent_threads.len(), 2);

    let projects = crate::repositories::agent_directory::list_agent_projects(
        &pool,
        &tenant,
        &source,
        &agent_subject,
    )
    .await
    .expect("list agent projects");
    assert_eq!(projects.len(), 2);
    let project = projects
        .iter()
        .find(|item| item.project_id == project_id)
        .expect("thread-scoped project");
    assert_eq!(project.project_id, project_id);
    assert_eq!(project.display_name.as_deref(), Some("Contract Project"));
    assert_eq!(project.thread_count, 1);
    assert_eq!(project.memory_count, 1);
    let record_project = projects
        .iter()
        .find(|item| item.project_id == record_project_id)
        .expect("record-scoped project");
    assert_eq!(
        record_project.display_name.as_deref(),
        Some("Record Contract Project")
    );
    assert_eq!(record_project.thread_count, 1);
    assert_eq!(record_project.memory_count, 0);
}
