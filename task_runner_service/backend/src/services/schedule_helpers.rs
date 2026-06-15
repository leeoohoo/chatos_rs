use chrono::{DateTime, Duration as ChronoDuration, Utc};

use crate::models::{TaskScheduleConfig, TaskScheduleMode, now_rfc3339};

use super::normalized_optional;

pub(super) fn sanitize_task_schedule_config(
    mut schedule: TaskScheduleConfig,
    existing: Option<&TaskScheduleConfig>,
) -> Result<TaskScheduleConfig, String> {
    schedule.run_at = normalized_optional(schedule.run_at);
    schedule.next_run_at = normalized_optional(schedule.next_run_at);
    schedule.last_scheduled_at = existing
        .and_then(|item| item.last_scheduled_at.clone())
        .or(schedule.last_scheduled_at);

    match schedule.mode {
        TaskScheduleMode::Manual => {
            schedule.run_at = None;
            schedule.interval_seconds = None;
            schedule.next_run_at = None;
            schedule.last_scheduled_at = existing.and_then(|item| item.last_scheduled_at.clone());
        }
        TaskScheduleMode::Once => {
            let run_at = schedule
                .run_at
                .clone()
                .ok_or_else(|| "一次性调度必须提供执行时间".to_string())?;
            ensure_rfc3339("schedule.run_at", &run_at)?;
            schedule.interval_seconds = None;
            schedule.next_run_at = Some(run_at);
        }
        TaskScheduleMode::Interval => {
            let seconds = schedule
                .interval_seconds
                .ok_or_else(|| "循环调度必须提供间隔秒数".to_string())?;
            if seconds < 60 {
                return Err("循环调度的最小间隔为 60 秒".to_string());
            }
            if let Some(run_at) = schedule.run_at.clone() {
                ensure_rfc3339("schedule.run_at", &run_at)?;
                if schedule.next_run_at.is_none() {
                    schedule.next_run_at = Some(run_at);
                }
            }
            if let Some(next_run_at) = schedule.next_run_at.clone() {
                ensure_rfc3339("schedule.next_run_at", &next_run_at)?;
            } else {
                schedule.next_run_at = existing
                    .and_then(|item| item.next_run_at.clone())
                    .or_else(|| Some(now_rfc3339()));
            }
        }
        TaskScheduleMode::ContactAsync => {
            let run_at = schedule
                .run_at
                .clone()
                .ok_or_else(|| "联系人异步调度必须提供执行时间".to_string())?;
            ensure_rfc3339("schedule.run_at", &run_at)?;
            schedule.interval_seconds = None;
            schedule.next_run_at = Some(run_at);
        }
    }

    Ok(schedule)
}

pub(super) fn advance_task_schedule_after_dispatch(
    schedule: &TaskScheduleConfig,
    started_at: DateTime<Utc>,
) -> Result<TaskScheduleConfig, String> {
    let mut next = schedule.clone();
    next.last_scheduled_at = Some(started_at.to_rfc3339());
    match next.mode {
        TaskScheduleMode::Manual => {
            next.next_run_at = None;
        }
        TaskScheduleMode::Once => {
            next.next_run_at = None;
        }
        TaskScheduleMode::Interval => {
            let seconds = next
                .interval_seconds
                .ok_or_else(|| "循环调度缺少 interval_seconds".to_string())?;
            next.next_run_at = Some((started_at + ChronoDuration::seconds(seconds)).to_rfc3339());
        }
        TaskScheduleMode::ContactAsync => {
            next.next_run_at = None;
        }
    }
    Ok(next)
}

fn parse_rfc3339(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|item| item.with_timezone(&Utc))
}

fn ensure_rfc3339(label: &str, value: &str) -> Result<(), String> {
    if parse_rfc3339(value).is_some() {
        Ok(())
    } else {
        Err(format!("{label} 必须是 RFC3339 时间"))
    }
}
