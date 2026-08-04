// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use chrono::Utc;
use serde_json::Value;
use tracing::{info, warn};

use crate::api::projects::{
    cloud_execution_planner_message_is_stale, execution_message_status,
    repair_stale_cloud_execution_planner_message_for_reconciler,
};
use crate::config::Config;
use crate::models::message::Message;
use crate::repositories::auth_users;
use crate::services::{chatos_sessions, task_runner_api_client};

const REQUIREMENT_EXECUTION_RECONCILE_INTERVAL: Duration = Duration::from_secs(60);
const REQUIREMENT_EXECUTION_SESSION_PAGE_SIZE: i64 = 100;
const REQUIREMENT_EXECUTION_MESSAGE_PAGE_SIZE: i64 = 200;

pub fn start_requirement_execution_reconciler() {
    tokio::spawn(async move {
        loop {
            match reconcile_once().await {
                Ok(repaired) if repaired > 0 => {
                    info!(
                        repaired_count = repaired,
                        "requirement execution planner reconciler repaired stale planning messages"
                    );
                }
                Ok(_) => {}
                Err(err) => {
                    warn!(
                        error = err.as_str(),
                        "requirement execution planner reconciler failed"
                    );
                }
            }
            tokio::time::sleep(REQUIREMENT_EXECUTION_RECONCILE_INTERVAL).await;
        }
    });
}

async fn reconcile_once() -> Result<usize, String> {
    let config = Config::try_get()?;
    let users = auth_users::list_users().await?;
    let mut repaired = 0usize;
    for user in users {
        repaired +=
            reconcile_user_sessions(user.user_id.as_str(), config.task_runner_base_url.as_str())
                .await;
    }
    Ok(repaired)
}

async fn reconcile_user_sessions(user_id: &str, task_runner_base_url: &str) -> usize {
    let mut repaired = 0usize;
    let mut offset = 0i64;
    loop {
        let sessions = match chatos_sessions::list_sessions(
            Some(user_id),
            None,
            Some(REQUIREMENT_EXECUTION_SESSION_PAGE_SIZE),
            offset,
            false,
            false,
        )
        .await
        {
            Ok(items) => items,
            Err(err) => {
                warn!(
                    user_id,
                    error = err.as_str(),
                    "requirement execution reconciler failed to list user sessions"
                );
                break;
            }
        };
        let count = sessions.len();
        for session in sessions {
            repaired += reconcile_session_messages(&session.id, task_runner_base_url).await;
        }
        offset += count as i64;
        if count < REQUIREMENT_EXECUTION_SESSION_PAGE_SIZE as usize {
            break;
        }
    }
    repaired
}

async fn reconcile_session_messages(session_id: &str, task_runner_base_url: &str) -> usize {
    let mut repaired = 0usize;
    let mut offset = 0i64;
    loop {
        let messages = match chatos_sessions::list_messages_including_hidden(
            session_id,
            Some(REQUIREMENT_EXECUTION_MESSAGE_PAGE_SIZE),
            offset,
            false,
        )
        .await
        {
            Ok(items) => items,
            Err(err) => {
                warn!(
                    session_id,
                    error = err.as_str(),
                    "requirement execution reconciler failed to list session messages"
                );
                break;
            }
        };
        let count = messages.len();
        for message in messages {
            if !should_check_message(&message) {
                continue;
            }
            if !message_task_graph_is_empty(session_id, message.id.as_str(), task_runner_base_url)
                .await
            {
                continue;
            }
            let previous_status = execution_message_status(&message);
            match repair_stale_cloud_execution_planner_message_for_reconciler(message, true).await {
                Ok(updated)
                    if previous_status != "failed"
                        && execution_message_status(&updated) == "failed" =>
                {
                    repaired += 1;
                }
                Ok(_) => {}
                Err(err) => {
                    warn!(
                        session_id,
                        error = err.as_str(),
                        "requirement execution reconciler failed to repair stale planning message"
                    );
                }
            }
        }
        offset += count as i64;
        if count < REQUIREMENT_EXECUTION_MESSAGE_PAGE_SIZE as usize {
            break;
        }
    }
    repaired
}

fn should_check_message(message: &Message) -> bool {
    if !message.role.trim().eq_ignore_ascii_case("user") {
        return false;
    }
    if !message
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("project_requirement_execution"))
        .is_some()
    {
        return false;
    }
    cloud_execution_planner_message_is_stale(message, false, Utc::now())
}

async fn message_task_graph_is_empty(
    session_id: &str,
    message_id: &str,
    task_runner_base_url: &str,
) -> bool {
    match task_runner_api_client::list_message_tasks(
        task_runner_base_url,
        session_id,
        Some(message_id),
        None,
    )
    .await
    {
        Ok(payload) => task_payload_is_empty(&payload),
        Err(err) => {
            warn!(
                session_id,
                message_id,
                error = err.as_str(),
                "requirement execution reconciler failed to query task runner message tasks"
            );
            false
        }
    }
}

fn task_payload_is_empty(payload: &Value) -> bool {
    payload
        .get("items")
        .and_then(Value::as_array)
        .is_none_or(|items| items.is_empty())
}

#[cfg(test)]
mod tests {
    use super::{should_check_message, task_payload_is_empty};
    use crate::models::message::Message;
    use serde_json::json;

    #[test]
    fn task_payload_empty_detection_matches_internal_response_shape() {
        assert!(task_payload_is_empty(&json!({ "items": [] })));
        assert!(task_payload_is_empty(&json!({})));
        assert!(!task_payload_is_empty(&json!({
            "items": [{ "id": "task-1" }]
        })));
    }

    #[test]
    fn only_stale_requirement_execution_user_messages_are_checked() {
        let mut message = Message::new(
            "session-1".to_string(),
            "user".to_string(),
            "execute".to_string(),
        );
        message.id = "group-1".to_string();
        message.created_at = "2026-08-03T00:00:00Z".to_string();
        message.metadata = Some(json!({
            "project_requirement_execution": {
                "project_id": "project-1",
                "requirement_id": "requirement-1"
            },
            "task_runner_async": {
                "overall_status": "planning_started"
            }
        }));

        assert!(should_check_message(&message));

        let mut assistant_message = message.clone();
        assistant_message.role = "assistant".to_string();
        assert!(!should_check_message(&assistant_message));

        let mut finished = message.clone();
        finished.metadata = Some(json!({
            "project_requirement_execution": {
                "project_id": "project-1",
                "requirement_id": "requirement-1"
            },
            "task_runner_async": {
                "overall_status": "failed"
            }
        }));
        assert!(!should_check_message(&finished));
    }
}
