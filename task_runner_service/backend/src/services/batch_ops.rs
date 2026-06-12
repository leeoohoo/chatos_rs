use crate::models::{BatchTaskOperationItem, BatchTaskOperationResponse};

pub(super) fn normalize_batch_task_ids(task_ids: Vec<String>) -> Result<Vec<String>, String> {
    let task_ids = task_ids
        .into_iter()
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect::<Vec<_>>();
    if task_ids.is_empty() {
        Err("task_ids 不能为空".to_string())
    } else {
        Ok(task_ids)
    }
}

pub(super) fn sanitize_id_list(ids: Vec<String>) -> Vec<String> {
    ids.into_iter()
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .take(200)
        .collect()
}

pub(super) fn normalize_prerequisite_task_ids(ids: Vec<String>) -> Vec<String> {
    let mut out = Vec::new();
    for id in ids {
        let id = id.trim().to_string();
        if id.is_empty() || out.iter().any(|item| item == &id) {
            continue;
        }
        out.push(id);
        if out.len() >= 50 {
            break;
        }
    }
    out
}

pub(super) fn summarize_batch_results(
    results: Vec<BatchTaskOperationItem>,
) -> BatchTaskOperationResponse {
    let total = results.len();
    let succeeded = results.iter().filter(|item| item.ok).count();
    let failed = total.saturating_sub(succeeded);
    BatchTaskOperationResponse {
        total,
        succeeded,
        failed,
        results,
    }
}

pub(super) fn normalize_tags(tags: Option<Vec<String>>) -> Vec<String> {
    tags.unwrap_or_default()
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect()
}
