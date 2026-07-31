// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use futures_util::TryStreamExt;
use mongodb::bson::{self, doc, Bson, Document};
use task_runner_service_backend::{load_task_runner_dotenv, AppConfig};
use uuid::Uuid;

const REPAIR_REASON: &str = "历史版本错误地将未完成的必需清单记为 waived，并将父任务标记为 succeeded；现已纠正为 blocked_terminal";

fn optional_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn env_flag(name: &str) -> bool {
    matches!(
        optional_env(name)
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str(),
        "1" | "true" | "yes" | "on"
    )
}

fn comma_separated_env(name: &str) -> Vec<String> {
    optional_env(name)
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn value_at_path<'a>(document: &'a Document, path: &str) -> Option<&'a Bson> {
    let mut segments = path.split('.');
    let first = segments.next()?;
    let mut value = document.get(first)?;
    for segment in segments {
        value = value.as_document()?.get(segment)?;
    }
    Some(value)
}

fn string_at_path<'a>(document: &'a Document, path: &str) -> Option<&'a str> {
    value_at_path(document, path)
        .and_then(Bson::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn bool_at_path(document: &Document, path: &str) -> Option<bool> {
    value_at_path(document, path).and_then(Bson::as_bool)
}

fn child_is_current_waived_required_checklist(child: &Document, current_run_id: &str) -> bool {
    let session_id = string_at_path(child, "task_tool_state.task_session_id")
        .or_else(|| string_at_path(child, "source_run_id"));
    if session_id != Some(current_run_id) {
        return false;
    }

    let is_run_checklist = match string_at_path(child, "task_tool_state.manager_scope") {
        Some("run_checklist") => true,
        Some(_) => false,
        None => {
            string_at_path(child, "parent_task_id").is_some()
                && string_at_path(child, "source_run_id").is_some()
        }
    };
    is_run_checklist
        && bool_at_path(child, "task_tool_state.required_for_parent_completion").unwrap_or(true)
        && string_at_path(child, "task_tool_state.closure_state") == Some("waived")
}

fn parent_and_run_are_current_succeeded(
    task: &Document,
    run: &Document,
    current_run_id: &str,
) -> bool {
    string_at_path(task, "status") == Some("succeeded")
        && string_at_path(task, "last_run_id") == Some(current_run_id)
        && string_at_path(run, "id") == Some(current_run_id)
        && string_at_path(run, "task_id") == string_at_path(task, "id")
        && string_at_path(run, "status") == Some("succeeded")
}

fn candidate_task_filter(execution_group_id: Option<&str>, task_ids: &[String]) -> Document {
    let mut filter = doc! {
        "status": "succeeded",
        "deleted_at": Bson::Null,
        "last_run_id": { "$exists": true, "$nin": [Bson::Null, Bson::String(String::new())] },
    };
    if let Some(execution_group_id) = execution_group_id {
        filter.insert(
            "$or",
            vec![
                doc! { "input_payload.execution_group_id": execution_group_id },
                doc! { "source_user_message_id": execution_group_id },
            ],
        );
    }
    if !task_ids.is_empty() {
        filter.insert("id", doc! { "$in": task_ids });
    }
    filter
}

fn current_session_child_filter(task_id: &str, run_id: &str) -> Document {
    doc! {
        "parent_task_id": task_id,
        "$or": [
            { "task_tool_state.task_session_id": run_id },
            {
                "task_tool_state.task_session_id": { "$exists": false },
                "source_run_id": run_id,
            },
            {
                "task_tool_state.task_session_id": Bson::Null,
                "source_run_id": run_id,
            },
        ],
    }
}

fn callback_delivery(now: &str) -> Result<Bson, String> {
    bson::to_bson(&doc! {
        "event": "task.blocked",
        "status": "pending",
        "attempt_count": 0_i32,
        "next_attempt_at": now,
        "last_error": Bson::Null,
        "updated_at": now,
    })
    .map_err(|err| err.to_string())
}

#[tokio::main]
async fn main() -> Result<(), String> {
    load_task_runner_dotenv();
    let config = AppConfig::from_env()?;
    let execution_group_id = optional_env("TASK_RUNNER_REPAIR_EXECUTION_GROUP_ID");
    let task_ids = comma_separated_env("TASK_RUNNER_REPAIR_TASK_IDS");
    let apply = env_flag("TASK_RUNNER_REPAIR_APPLY");

    if execution_group_id.is_none() && task_ids.is_empty() {
        return Err(
            "set TASK_RUNNER_REPAIR_EXECUTION_GROUP_ID or TASK_RUNNER_REPAIR_TASK_IDS to limit the repair scope"
                .to_string(),
        );
    }

    let client = mongodb::Client::with_uri_str(config.database_url.as_str())
        .await
        .map_err(|err| err.to_string())?;
    let database = client
        .default_database()
        .ok_or_else(|| "mongodb connection string must include a database name".to_string())?;
    let tasks = database.collection::<Document>("tasks");
    let runs = database.collection::<Document>("task_runs");
    let run_events = database.collection::<Document>("task_run_events");

    let mut cursor = tasks
        .find(
            candidate_task_filter(execution_group_id.as_deref(), &task_ids),
            None,
        )
        .await
        .map_err(|err| err.to_string())?;
    let mut candidates = Vec::<(Document, Document, Vec<Document>)>::new();

    while let Some(task) = cursor.try_next().await.map_err(|err| err.to_string())? {
        let Some(task_id) = string_at_path(&task, "id") else {
            continue;
        };
        let Some(run_id) = string_at_path(&task, "last_run_id") else {
            continue;
        };
        let Some(run) = runs
            .find_one(doc! { "id": run_id }, None)
            .await
            .map_err(|err| err.to_string())?
        else {
            continue;
        };
        if !parent_and_run_are_current_succeeded(&task, &run, run_id) {
            continue;
        }

        let mut child_cursor = tasks
            .find(current_session_child_filter(task_id, run_id), None)
            .await
            .map_err(|err| err.to_string())?;
        let mut repairable_children = Vec::new();
        while let Some(child) = child_cursor
            .try_next()
            .await
            .map_err(|err| err.to_string())?
        {
            if child_is_current_waived_required_checklist(&child, run_id) {
                repairable_children.push(child);
            }
        }
        if !repairable_children.is_empty() {
            candidates.push((task, run, repairable_children));
        }
    }

    candidates
        .sort_by(|left, right| string_at_path(&left.0, "id").cmp(&string_at_path(&right.0, "id")));

    let matched_children = candidates
        .iter()
        .map(|(_, _, children)| children.len())
        .sum::<usize>();
    for (task, run, children) in &candidates {
        let task_id = string_at_path(task, "id").unwrap_or("<missing>");
        let run_id = string_at_path(run, "id").unwrap_or("<missing>");
        let title = string_at_path(task, "title").unwrap_or("<missing>");
        println!(
            "{} task_id={} run_id={} waived_required={} title={}",
            if apply { "repair" } else { "dry-run" },
            task_id,
            run_id,
            children.len(),
            title
        );
        for child in children {
            println!(
                "  checklist_id={} title={}",
                string_at_path(child, "id").unwrap_or("<missing>"),
                string_at_path(child, "title").unwrap_or("<missing>")
            );
        }
    }

    if apply {
        for (task, run, children) in &candidates {
            let task_id = string_at_path(task, "id").ok_or_else(|| {
                "repair candidate is missing task id after classification".to_string()
            })?;
            let run_id = string_at_path(run, "id").ok_or_else(|| {
                "repair candidate is missing run id after classification".to_string()
            })?;
            let now = chrono::Utc::now().to_rfc3339();
            let summary = format!(
                "历史任务状态已纠正：上次运行仍有 {} 个必需清单未完成，不能标记为成功。请修复前置实现后重试。",
                children.len()
            );
            let child_ids = children
                .iter()
                .filter_map(|child| string_at_path(child, "id"))
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>();

            for child_id in &child_ids {
                let result = tasks
                    .update_one(
                        doc! {
                            "id": child_id,
                            "parent_task_id": task_id,
                            "task_tool_state.closure_state": "waived",
                            "task_tool_state.required_for_parent_completion": { "$ne": false },
                            "$or": [
                                { "task_tool_state.task_session_id": run_id },
                                {
                                    "task_tool_state.task_session_id": { "$exists": false },
                                    "source_run_id": run_id,
                                },
                                {
                                    "task_tool_state.task_session_id": Bson::Null,
                                    "source_run_id": run_id,
                                },
                            ],
                        },
                        doc! {
                            "$set": {
                                "status": "blocked",
                                "task_tool_state.closure_state": "blocked_terminal",
                                "task_tool_state.closure_reason": REPAIR_REASON,
                                "task_tool_state.blocker_reason": REPAIR_REASON,
                                "task_tool_state.blocker_kind": "historical_completion_gate_repair",
                                "task_tool_state.completed_at": &now,
                                "task_tool_state.lifecycle_updated_at": &now,
                                "updated_at": &now,
                            }
                        },
                        None,
                    )
                    .await
                    .map_err(|err| err.to_string())?;
                if result.modified_count != 1 {
                    return Err(format!(
                        "checklist changed while applying repair: task_id={task_id} run_id={run_id} checklist_id={child_id}"
                    ));
                }
            }

            let run_result = runs
                .update_one(
                    doc! {
                        "id": run_id,
                        "task_id": task_id,
                        "status": "succeeded",
                    },
                    doc! {
                        "$set": {
                            "status": "blocked",
                            "result_summary": &summary,
                            "error_message": &summary,
                            "cancel_requested": false,
                            "chatos_callback_delivery": callback_delivery(&now)?,
                            "updated_at": &now,
                        },
                        "$unset": {
                            "claim_token": "",
                            "claim_until": "",
                        }
                    },
                    None,
                )
                .await
                .map_err(|err| err.to_string())?;
            if run_result.modified_count != 1 {
                return Err(format!(
                    "run changed while applying repair: task_id={task_id} run_id={run_id}"
                ));
            }

            let task_result = tasks
                .update_one(
                    doc! {
                        "id": task_id,
                        "last_run_id": run_id,
                        "status": "succeeded",
                    },
                    doc! {
                        "$set": {
                            "status": "blocked",
                            "result_summary": &summary,
                            "task_tool_state.blocker_reason": &summary,
                            "task_tool_state.blocker_kind": "historical_completion_gate_repair",
                            "task_tool_state.blocker_needs": &child_ids,
                            "updated_at": &now,
                        }
                    },
                    None,
                )
                .await
                .map_err(|err| err.to_string())?;
            if task_result.modified_count != 1 {
                return Err(format!(
                    "parent task changed while applying repair: task_id={task_id} run_id={run_id}"
                ));
            }

            run_events
                .insert_one(
                    doc! {
                        "id": Uuid::new_v4().to_string(),
                        "run_id": run_id,
                        "event_type": "historical_required_checklist_repaired",
                        "message": &summary,
                        "payload": {
                            "reason": "legacy_required_checklist_waived",
                            "previous_task_status": "succeeded",
                            "previous_run_status": "succeeded",
                            "repaired_task_status": "blocked",
                            "repaired_run_status": "blocked",
                            "repaired_checklist_ids": &child_ids,
                        },
                        "created_at": &now,
                    },
                    None,
                )
                .await
                .map_err(|err| err.to_string())?;
        }
    }

    println!(
        "repair_waived_required_successes complete apply={} execution_group_id={} scoped_task_ids={} matched_tasks={} matched_checklists={}",
        apply,
        execution_group_id.as_deref().unwrap_or("<none>"),
        task_ids.len(),
        candidates.len(),
        matched_children
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parent(run_id: &str) -> Document {
        doc! {
            "id": "parent-1",
            "status": "succeeded",
            "last_run_id": run_id,
        }
    }

    fn run(run_id: &str) -> Document {
        doc! {
            "id": run_id,
            "task_id": "parent-1",
            "status": "succeeded",
        }
    }

    fn checklist(run_id: &str, required: bool, closure_state: &str) -> Document {
        doc! {
            "id": "child-1",
            "parent_task_id": "parent-1",
            "source_run_id": run_id,
            "task_tool_state": {
                "manager_scope": "run_checklist",
                "task_session_id": run_id,
                "required_for_parent_completion": required,
                "closure_state": closure_state,
            }
        }
    }

    #[test]
    fn required_waived_current_checklist_is_repairable() {
        assert!(parent_and_run_are_current_succeeded(
            &parent("run-1"),
            &run("run-1"),
            "run-1"
        ));
        assert!(child_is_current_waived_required_checklist(
            &checklist("run-1", true, "waived"),
            "run-1"
        ));
    }

    #[test]
    fn optional_waived_checklist_is_not_repairable() {
        assert!(!child_is_current_waived_required_checklist(
            &checklist("run-1", false, "waived"),
            "run-1"
        ));
    }

    #[test]
    fn satisfied_required_checklist_is_not_repairable() {
        assert!(!child_is_current_waived_required_checklist(
            &checklist("run-1", true, "satisfied"),
            "run-1"
        ));
    }

    #[test]
    fn historical_session_does_not_pollute_current_run() {
        assert!(!child_is_current_waived_required_checklist(
            &checklist("run-old", true, "waived"),
            "run-current"
        ));
    }

    #[test]
    fn completed_review_task_does_not_match_without_waived_required_checklist() {
        assert!(parent_and_run_are_current_succeeded(
            &parent("review-run"),
            &run("review-run"),
            "review-run"
        ));
        assert!(!child_is_current_waived_required_checklist(
            &checklist("review-run", true, "satisfied"),
            "review-run"
        ));
    }

    #[test]
    fn parent_last_run_must_match_candidate_run() {
        assert!(!parent_and_run_are_current_succeeded(
            &parent("run-current"),
            &run("run-old"),
            "run-old"
        ));
    }
}
