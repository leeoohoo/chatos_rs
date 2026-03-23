use serde_json::{json, Value};
use tokio::sync::broadcast;
use tokio::time::{Duration, Instant};

use crate::models::terminal_log::TerminalLog;
use crate::services::terminal_manager::TerminalEvent;

#[derive(Debug)]
pub(super) struct OutputCapture {
    pub(super) output: String,
    pub(super) truncated: bool,
    pub(super) finished_by: &'static str,
}

#[derive(Debug, Default)]
struct LogTruncationStats {
    truncated: bool,
    per_log_capped: usize,
    total_capped: bool,
    dropped_logs: usize,
    original_chars: usize,
    returned_chars: usize,
}

pub(super) fn compact_recent_logs(
    logs: &[TerminalLog],
    per_entry_max_chars: usize,
    total_max_chars: usize,
) -> (Vec<Value>, Value) {
    let mut stats = LogTruncationStats::default();
    if logs.is_empty() || total_max_chars == 0 {
        stats.truncated = !logs.is_empty();
        stats.total_capped = !logs.is_empty();
        stats.dropped_logs = logs.len();
        return (Vec::new(), stats_to_value(&stats));
    }

    let mut kept_rev: Vec<Value> = Vec::new();
    let mut total_chars = 0usize;
    let mut hit_total_limit = false;

    for (index_from_newest, log) in logs.iter().rev().enumerate() {
        let original_chars = log.content.chars().count();
        stats.original_chars += original_chars;

        let mut content = log.content.clone();
        let mut entry_truncated = false;
        if original_chars > per_entry_max_chars {
            content = truncate_keep_tail(log.content.as_str(), per_entry_max_chars);
            entry_truncated = true;
            stats.per_log_capped += 1;
        }

        let content_chars = content.chars().count();
        let remaining = total_max_chars.saturating_sub(total_chars);
        if remaining == 0 {
            hit_total_limit = true;
            stats.dropped_logs = logs.len().saturating_sub(index_from_newest);
            break;
        }

        if content_chars > remaining {
            content = truncate_keep_tail(content.as_str(), remaining);
            hit_total_limit = true;
            stats.dropped_logs = logs
                .len()
                .saturating_sub(index_from_newest)
                .saturating_sub(1);
            kept_rev.push(json!({
                "id": log.id,
                "terminal_id": log.terminal_id,
                "log_type": log.log_type,
                "content": content,
                "created_at": log.created_at,
            }));
            break;
        }

        total_chars += content_chars;
        kept_rev.push(json!({
            "id": log.id,
            "terminal_id": log.terminal_id,
            "log_type": log.log_type,
            "content": content,
            "created_at": log.created_at,
        }));

        if entry_truncated {
            stats.truncated = true;
        }
    }

    let mut kept = kept_rev;
    kept.reverse();
    stats.returned_chars = kept
        .iter()
        .map(|item| {
            item.get("content")
                .and_then(|value| value.as_str())
                .map(|value| value.chars().count())
                .unwrap_or(0)
        })
        .sum();
    stats.total_capped = hit_total_limit;
    stats.truncated = stats.truncated || hit_total_limit || stats.dropped_logs > 0;
    (kept, stats_to_value(&stats))
}

fn stats_to_value(stats: &LogTruncationStats) -> Value {
    json!({
        "truncated": stats.truncated,
        "per_log_capped": stats.per_log_capped,
        "total_capped": stats.total_capped,
        "dropped_logs": stats.dropped_logs,
        "original_chars": stats.original_chars,
        "returned_chars": stats.returned_chars
    })
}

fn truncate_keep_tail(input: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let total = input.chars().count();
    if total <= max_chars {
        return input.to_string();
    }

    let marker = format!("[...truncated {} chars...]\n", total - max_chars);
    let marker_chars = marker.chars().count();
    if marker_chars >= max_chars {
        return input
            .chars()
            .rev()
            .take(max_chars)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
    }

    let keep_tail = max_chars - marker_chars;
    let tail: String = input
        .chars()
        .rev()
        .take(keep_tail)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("{}{}", marker, tail)
}

pub(super) async fn capture_command_output(
    receiver: &mut broadcast::Receiver<TerminalEvent>,
    idle_timeout: Duration,
    max_wait: Duration,
    max_output_chars: usize,
) -> OutputCapture {
    let start = Instant::now();
    let mut last_output_at = Instant::now();
    let mut output = String::new();
    let mut truncated = false;

    let finished_by = loop {
        let elapsed = start.elapsed();
        if elapsed >= max_wait {
            break "max_wait_timeout";
        }

        let idle_elapsed = last_output_at.elapsed();
        if idle_elapsed >= idle_timeout {
            break "idle_timeout";
        }

        let until_idle = idle_timeout - idle_elapsed;
        let until_deadline = max_wait - elapsed;
        let wait_duration = std::cmp::min(until_idle, until_deadline);

        match tokio::time::timeout(wait_duration, receiver.recv()).await {
            Ok(Ok(TerminalEvent::Output(chunk))) => {
                append_tail(
                    &mut output,
                    chunk.as_str(),
                    max_output_chars,
                    &mut truncated,
                );
                last_output_at = Instant::now();
            }
            Ok(Ok(TerminalEvent::Exit(code))) => {
                append_tail(
                    &mut output,
                    format!("\n[terminal exited with code {code}]\n").as_str(),
                    max_output_chars,
                    &mut truncated,
                );
                break "terminal_exit";
            }
            Ok(Ok(TerminalEvent::State(_))) => {}
            Ok(Err(broadcast::error::RecvError::Lagged(_))) => {
                last_output_at = Instant::now();
            }
            Ok(Err(broadcast::error::RecvError::Closed)) => {
                break "receiver_closed";
            }
            Err(_) => {
                if start.elapsed() >= max_wait {
                    break "max_wait_timeout";
                }
                break "idle_timeout";
            }
        }
    };

    OutputCapture {
        output,
        truncated,
        finished_by,
    }
}

fn append_tail(output: &mut String, chunk: &str, max_chars: usize, truncated: &mut bool) {
    if chunk.is_empty() {
        return;
    }
    output.push_str(chunk);
    let char_count = output.chars().count();
    if char_count <= max_chars {
        return;
    }
    *truncated = true;
    let tail: String = output
        .chars()
        .rev()
        .take(max_chars)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    *output = tail;
}
