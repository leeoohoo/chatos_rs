use std::sync::Arc;

use once_cell::sync::Lazy;
use tokio::sync::broadcast;

use crate::core::time::now_rfc3339;
use crate::models::terminal::Terminal;
use crate::services::memory_server_client::{ReviewRepairStatusDto, RunReviewRepairSummaryRequestDto};
use crate::services::task_manager::{TaskDraft, TaskRecord};

use super::types::{
    ChatStreamRealtimePayload,
    ContactsUpdatedRealtimePayload,
    NotepadUpdatedRealtimePayload,
    ProjectChangeSummaryRealtimePayload, ProjectRunCatalogRealtimePayload,
    ProjectMembersUpdatedRealtimePayload, ProjectRunStateRealtimePayload,
    ProjectsUpdatedRealtimePayload, RealtimeEventEnvelope,
    RealtimeEventPayload,
    RemoteConnectionsUpdatedRealtimePayload,
    SessionsUpdatedRealtimePayload,
    RemoteSftpTransferRealtimePayload, TaskBoardRealtimePayload, UiPromptRealtimePayload,
    ReviewRepairRealtimePayload, TerminalListInvalidatedRealtimePayload,
    TerminalStateRealtimePayload,
};

const REALTIME_CHANNEL_CAPACITY: usize = 512;

pub struct RealtimeHub {
    tx: broadcast::Sender<Arc<RealtimeEventEnvelope>>,
}

impl RealtimeHub {
    fn new() -> Self {
        let (tx, _) = broadcast::channel(REALTIME_CHANNEL_CAPACITY);
        Self { tx }
    }

    fn send(&self, envelope: RealtimeEventEnvelope) {
        let _ = self.tx.send(Arc::new(envelope));
    }

    fn subscribe(&self) -> broadcast::Receiver<Arc<RealtimeEventEnvelope>> {
        self.tx.subscribe()
    }

    fn has_receivers(&self) -> bool {
        self.tx.receiver_count() > 0
    }
}

static REALTIME_HUB: Lazy<RealtimeHub> = Lazy::new(RealtimeHub::new);

pub fn subscribe_user_events() -> broadcast::Receiver<Arc<RealtimeEventEnvelope>> {
    REALTIME_HUB.subscribe()
}

pub fn user_has_realtime_listeners() -> bool {
    REALTIME_HUB.has_receivers()
}

pub fn publish_review_repair_started_pending(
    user_id: &str,
    conversation_id: &str,
    scope_req: &RunReviewRepairSummaryRequestDto,
    pending_message_count: Option<i64>,
) {
    publish_review_repair_event(
        "conversation.review_repair.started",
        user_id,
        conversation_id,
        ReviewRepairRealtimePayload {
            conversation_id: conversation_id.to_string(),
            project_id: scope_req.project_id.clone().unwrap_or_default(),
            contact_id: scope_req.contact_id.clone(),
            agent_id: scope_req.agent_id.clone(),
            running: true,
            pending_message_count,
            running_job_count: None,
            scope_session_count: None,
            processed_sessions: None,
            summarized_sessions: None,
            generated_summaries: None,
            marked_messages: None,
            failed_sessions: None,
            job_type: Some("review_repair".to_string()),
            mode: Some("review_repair".to_string()),
            error: None,
        },
    );
}

pub fn publish_review_repair_completed(
    user_id: &str,
    conversation_id: &str,
    scope_req: &RunReviewRepairSummaryRequestDto,
    status: &ReviewRepairStatusDto,
) {
    publish_review_repair_event(
        "conversation.review_repair.completed",
        user_id,
        conversation_id,
        ReviewRepairRealtimePayload {
            conversation_id: conversation_id.to_string(),
            project_id: status.project_id.clone(),
            contact_id: status
                .contact_id
                .clone()
                .or_else(|| scope_req.contact_id.clone()),
            agent_id: status.agent_id.clone().or_else(|| scope_req.agent_id.clone()),
            running: false,
            pending_message_count: Some(status.pending_message_count),
            running_job_count: Some(status.running_job_count),
            scope_session_count: Some(status.scope_session_count),
            processed_sessions: None,
            summarized_sessions: None,
            generated_summaries: None,
            marked_messages: None,
            failed_sessions: None,
            job_type: Some(status.job_type.clone()),
            mode: None,
            error: None,
        },
    );
    publish_review_repair_event(
        "conversation.summaries.updated",
        user_id,
        conversation_id,
        ReviewRepairRealtimePayload {
            conversation_id: conversation_id.to_string(),
            project_id: status.project_id.clone(),
            contact_id: status
                .contact_id
                .clone()
                .or_else(|| scope_req.contact_id.clone()),
            agent_id: status.agent_id.clone().or_else(|| scope_req.agent_id.clone()),
            running: false,
            pending_message_count: Some(status.pending_message_count),
            running_job_count: Some(status.running_job_count),
            scope_session_count: Some(status.scope_session_count),
            processed_sessions: None,
            summarized_sessions: None,
            generated_summaries: None,
            marked_messages: None,
            failed_sessions: None,
            job_type: Some(status.job_type.clone()),
            mode: None,
            error: None,
        },
    );
}

pub fn publish_review_repair_failed(
    user_id: &str,
    conversation_id: &str,
    scope_req: &RunReviewRepairSummaryRequestDto,
    pending_message_count: Option<i64>,
    error: &str,
) {
    publish_review_repair_event(
        "conversation.review_repair.failed",
        user_id,
        conversation_id,
        ReviewRepairRealtimePayload {
            conversation_id: conversation_id.to_string(),
            project_id: scope_req.project_id.clone().unwrap_or_default(),
            contact_id: scope_req.contact_id.clone(),
            agent_id: scope_req.agent_id.clone(),
            running: false,
            pending_message_count,
            running_job_count: None,
            scope_session_count: None,
            processed_sessions: None,
            summarized_sessions: None,
            generated_summaries: None,
            marked_messages: None,
            failed_sessions: None,
            job_type: Some("review_repair".to_string()),
            mode: None,
            error: Some(error.to_string()),
        },
    );
}

pub fn publish_project_change_summary_updated(
    user_id: &str,
    project_id: &str,
    reason: &str,
    conversation_id: Option<&str>,
    path: Option<&str>,
) {
    REALTIME_HUB.send(RealtimeEventEnvelope {
        message_type: "event",
        event: "project.change_summary.updated",
        user_id: user_id.to_string(),
        conversation_id: conversation_id.map(|value| value.to_string()),
        project_id: Some(project_id.to_string()),
        payload: RealtimeEventPayload::ProjectChangeSummary(ProjectChangeSummaryRealtimePayload {
            project_id: project_id.to_string(),
            reason: reason.to_string(),
            conversation_id: conversation_id.map(|value| value.to_string()),
            path: path.map(|value| value.to_string()),
        }),
        ts: now_rfc3339(),
    });
}

pub fn publish_contacts_updated(user_id: &str, reason: &str, contact_id: Option<&str>) {
    REALTIME_HUB.send(RealtimeEventEnvelope {
        message_type: "event",
        event: "contacts.updated",
        user_id: user_id.to_string(),
        conversation_id: None,
        project_id: None,
        payload: RealtimeEventPayload::ContactsUpdated(ContactsUpdatedRealtimePayload {
            reason: reason.to_string(),
            contact_id: contact_id.map(|value| value.to_string()),
        }),
        ts: now_rfc3339(),
    });
}

pub fn publish_notepad_updated(
    user_id: &str,
    reason: &str,
    note_id: Option<&str>,
    folder: Option<&str>,
    from: Option<&str>,
    to: Option<&str>,
) {
    REALTIME_HUB.send(RealtimeEventEnvelope {
        message_type: "event",
        event: "notepad.updated",
        user_id: user_id.to_string(),
        conversation_id: None,
        project_id: None,
        payload: RealtimeEventPayload::NotepadUpdated(NotepadUpdatedRealtimePayload {
            reason: reason.to_string(),
            note_id: note_id.map(|value| value.to_string()),
            folder: folder.map(|value| value.to_string()),
            from: from.map(|value| value.to_string()),
            to: to.map(|value| value.to_string()),
        }),
        ts: now_rfc3339(),
    });
}

pub fn publish_projects_updated(user_id: &str, reason: &str, project_id: Option<&str>) {
    REALTIME_HUB.send(RealtimeEventEnvelope {
        message_type: "event",
        event: "projects.updated",
        user_id: user_id.to_string(),
        conversation_id: None,
        project_id: project_id.map(|value| value.to_string()),
        payload: RealtimeEventPayload::ProjectsUpdated(ProjectsUpdatedRealtimePayload {
            reason: reason.to_string(),
            project_id: project_id.map(|value| value.to_string()),
        }),
        ts: now_rfc3339(),
    });
}

pub fn publish_remote_connections_updated(
    user_id: &str,
    reason: &str,
    connection_id: Option<&str>,
) {
    REALTIME_HUB.send(RealtimeEventEnvelope {
        message_type: "event",
        event: "remote_connections.updated",
        user_id: user_id.to_string(),
        conversation_id: None,
        project_id: None,
        payload: RealtimeEventPayload::RemoteConnectionsUpdated(
            RemoteConnectionsUpdatedRealtimePayload {
                reason: reason.to_string(),
                connection_id: connection_id.map(|value| value.to_string()),
            },
        ),
        ts: now_rfc3339(),
    });
}

pub fn publish_sessions_updated(
    user_id: &str,
    reason: &str,
    session_id: Option<&str>,
    project_id: Option<&str>,
) {
    REALTIME_HUB.send(RealtimeEventEnvelope {
        message_type: "event",
        event: "sessions.updated",
        user_id: user_id.to_string(),
        conversation_id: session_id.map(|value| value.to_string()),
        project_id: project_id.map(|value| value.to_string()),
        payload: RealtimeEventPayload::SessionsUpdated(SessionsUpdatedRealtimePayload {
            reason: reason.to_string(),
            session_id: session_id.map(|value| value.to_string()),
            project_id: project_id.map(|value| value.to_string()),
        }),
        ts: now_rfc3339(),
    });
}

pub fn publish_terminal_state_changed(
    user_id: &str,
    terminal: &Terminal,
    busy: bool,
    reason: &str,
) {
    let status = terminal.status.trim();
    let payload = TerminalStateRealtimePayload {
        terminal_id: terminal.id.clone(),
        project_id: terminal.project_id.clone(),
        terminal_name: Some(terminal.name.clone()),
        cwd: Some(terminal.cwd.clone()),
        status: if status.is_empty() {
            "unknown".to_string()
        } else {
            status.to_string()
        },
        busy,
        reason: reason.to_string(),
    };
    REALTIME_HUB.send(RealtimeEventEnvelope {
        message_type: "event",
        event: "terminal.state_changed",
        user_id: user_id.to_string(),
        conversation_id: None,
        project_id: terminal.project_id.clone(),
        payload: RealtimeEventPayload::TerminalState(payload),
        ts: now_rfc3339(),
    });
}

pub fn publish_terminal_list_invalidated(
    user_id: &str,
    terminal_id: Option<&str>,
    project_id: Option<&str>,
    reason: &str,
) {
    REALTIME_HUB.send(RealtimeEventEnvelope {
        message_type: "event",
        event: "terminal.list.invalidated",
        user_id: user_id.to_string(),
        conversation_id: None,
        project_id: project_id.map(|value| value.to_string()),
        payload: RealtimeEventPayload::TerminalListInvalidated(
            TerminalListInvalidatedRealtimePayload {
                terminal_id: terminal_id.map(|value| value.to_string()),
                project_id: project_id.map(|value| value.to_string()),
                reason: reason.to_string(),
            },
        ),
        ts: now_rfc3339(),
    });
}

pub fn publish_project_run_state_changed(
    user_id: &str,
    project_id: &str,
    terminal: Option<&Terminal>,
    busy: bool,
    running: bool,
    status: &str,
    reason: &str,
) {
    let payload = ProjectRunStateRealtimePayload {
        project_id: project_id.to_string(),
        terminal_id: terminal.map(|value| value.id.clone()),
        terminal_name: terminal.map(|value| value.name.clone()),
        cwd: terminal.map(|value| value.cwd.clone()),
        status: status.to_string(),
        busy,
        running,
        reason: reason.to_string(),
    };
    REALTIME_HUB.send(RealtimeEventEnvelope {
        message_type: "event",
        event: "project.run.state_changed",
        user_id: user_id.to_string(),
        conversation_id: None,
        project_id: Some(project_id.to_string()),
        payload: RealtimeEventPayload::ProjectRunState(payload),
        ts: now_rfc3339(),
    });
}

pub fn publish_project_run_catalog_updated(
    user_id: &str,
    project_id: &str,
    reason: &str,
    path: Option<&str>,
) {
    REALTIME_HUB.send(RealtimeEventEnvelope {
        message_type: "event",
        event: "project.run.catalog.updated",
        user_id: user_id.to_string(),
        conversation_id: None,
        project_id: Some(project_id.to_string()),
        payload: RealtimeEventPayload::ProjectRunCatalog(ProjectRunCatalogRealtimePayload {
            project_id: project_id.to_string(),
            reason: reason.to_string(),
            path: path.map(|value| value.to_string()),
        }),
        ts: now_rfc3339(),
    });
}

pub fn publish_project_members_updated(
    user_id: &str,
    project_id: &str,
    reason: &str,
    contact_id: Option<&str>,
) {
    REALTIME_HUB.send(RealtimeEventEnvelope {
        message_type: "event",
        event: "project.members.updated",
        user_id: user_id.to_string(),
        conversation_id: None,
        project_id: Some(project_id.to_string()),
        payload: RealtimeEventPayload::ProjectMembersUpdated(ProjectMembersUpdatedRealtimePayload {
            project_id: project_id.to_string(),
            reason: reason.to_string(),
            contact_id: contact_id.map(|value| value.to_string()),
        }),
        ts: now_rfc3339(),
    });
}

pub fn publish_task_board_updated(
    user_id: &str,
    conversation_id: &str,
    conversation_turn_id: Option<&str>,
    review_id: Option<&str>,
    task_id: Option<&str>,
    action: &str,
    task: Option<TaskRecord>,
    draft_tasks: Option<Vec<TaskDraft>>,
    timeout_ms: Option<u64>,
) {
    REALTIME_HUB.send(RealtimeEventEnvelope {
        message_type: "event",
        event: "conversation.task_board.updated",
        user_id: user_id.to_string(),
        conversation_id: Some(conversation_id.to_string()),
        project_id: None,
        payload: RealtimeEventPayload::TaskBoard(TaskBoardRealtimePayload {
            conversation_id: conversation_id.to_string(),
            conversation_turn_id: conversation_turn_id.map(|value| value.to_string()),
            review_id: review_id.map(|value| value.to_string()),
            task_id: task_id.map(|value| value.to_string()),
            action: action.to_string(),
            task,
            draft_tasks,
            timeout_ms,
        }),
        ts: now_rfc3339(),
    });
}

pub fn publish_ui_prompt_updated(
    user_id: &str,
    conversation_id: &str,
    conversation_turn_id: Option<&str>,
    prompt_id: &str,
    action: &str,
    status: Option<&str>,
    tool_call_id: Option<&str>,
    prompt_kind: Option<&str>,
    title: Option<&str>,
    message: Option<&str>,
    allow_cancel: Option<bool>,
    timeout_ms: Option<u64>,
    payload: Option<serde_json::Value>,
) {
    REALTIME_HUB.send(RealtimeEventEnvelope {
        message_type: "event",
        event: "conversation.ui_prompt.updated",
        user_id: user_id.to_string(),
        conversation_id: Some(conversation_id.to_string()),
        project_id: None,
        payload: RealtimeEventPayload::UiPrompt(UiPromptRealtimePayload {
            conversation_id: conversation_id.to_string(),
            conversation_turn_id: conversation_turn_id.map(|value| value.to_string()),
            prompt_id: prompt_id.to_string(),
            action: action.to_string(),
            status: status.map(|value| value.to_string()),
            tool_call_id: tool_call_id.map(|value| value.to_string()),
            prompt_kind: prompt_kind.map(|value| value.to_string()),
            title: title.map(|value| value.to_string()),
            message: message.map(|value| value.to_string()),
            allow_cancel,
            timeout_ms,
            payload,
        }),
        ts: now_rfc3339(),
    });
}

pub fn publish_chat_stream_event(
    user_id: &str,
    conversation_id: &str,
    conversation_turn_id: Option<&str>,
    project_id: Option<&str>,
    user_message_id: Option<&str>,
    event: &'static str,
    stream_type: &str,
    raw: serde_json::Value,
) {
    REALTIME_HUB.send(RealtimeEventEnvelope {
        message_type: "event",
        event,
        user_id: user_id.to_string(),
        conversation_id: Some(conversation_id.to_string()),
        project_id: project_id.map(|value| value.to_string()),
        payload: RealtimeEventPayload::ChatStream(ChatStreamRealtimePayload {
            conversation_id: conversation_id.to_string(),
            conversation_turn_id: conversation_turn_id.map(|value| value.to_string()),
            project_id: project_id.map(|value| value.to_string()),
            user_message_id: user_message_id.map(|value| value.to_string()),
            stream_type: stream_type.to_string(),
            raw,
        }),
        ts: now_rfc3339(),
    });
}

pub fn publish_remote_sftp_transfer_updated(
    user_id: &str,
    payload: RemoteSftpTransferRealtimePayload,
) {
    REALTIME_HUB.send(RealtimeEventEnvelope {
        message_type: "event",
        event: "remote.sftp.transfer.updated",
        user_id: user_id.to_string(),
        conversation_id: None,
        project_id: None,
        payload: RealtimeEventPayload::RemoteSftpTransfer(payload),
        ts: now_rfc3339(),
    });
}

fn publish_review_repair_event(
    event: &'static str,
    user_id: &str,
    conversation_id: &str,
    payload: ReviewRepairRealtimePayload,
) {
    REALTIME_HUB.send(RealtimeEventEnvelope {
        message_type: "event",
        event,
        user_id: user_id.to_string(),
        conversation_id: Some(conversation_id.to_string()),
        project_id: Some(payload.project_id.clone()),
        payload: RealtimeEventPayload::ReviewRepair(payload),
        ts: now_rfc3339(),
    });
}
