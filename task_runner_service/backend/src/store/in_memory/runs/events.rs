// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl InMemoryStore {
    pub(in crate::store) fn list_run_events(&self, run_id: &str) -> Vec<TaskRunEventRecord> {
        self.inner
            .read()
            .run_events
            .get(run_id)
            .cloned()
            .unwrap_or_default()
    }

    pub(in crate::store) fn get_run_event(
        &self,
        run_id: &str,
        event_id: &str,
    ) -> Option<TaskRunEventRecord> {
        self.inner
            .read()
            .run_events
            .get(run_id)
            .and_then(|events| events.iter().find(|event| event.id == event_id))
            .cloned()
    }

    pub(in crate::store) fn list_run_events_after(
        &self,
        run_id: &str,
        after_created_at: Option<&str>,
        after_id: Option<&str>,
        limit: usize,
    ) -> Vec<TaskRunEventRecord> {
        let events = self
            .inner
            .read()
            .run_events
            .get(run_id)
            .cloned()
            .unwrap_or_default();
        let mut items = events
            .into_iter()
            .filter(|event| run_event_is_after_cursor(event, after_created_at, after_id))
            .collect::<Vec<_>>();
        items.sort_by(|left, right| {
            left.created_at
                .cmp(&right.created_at)
                .then(left.id.cmp(&right.id))
        });
        items.truncate(limit);
        items
    }

    pub(in crate::store) fn latest_run_event_cursor(
        &self,
        run_id: &str,
    ) -> Option<(String, String)> {
        self.inner.read().run_events.get(run_id).and_then(|events| {
            events
                .iter()
                .max_by(|left, right| {
                    left.created_at
                        .cmp(&right.created_at)
                        .then(left.id.cmp(&right.id))
                })
                .map(|event| (event.created_at.clone(), event.id.clone()))
        })
    }

    pub(in crate::store) fn prune_terminal_run_events_before(
        &self,
        cutoff: &str,
        candidate_limit: usize,
    ) -> RunEventPruneResult {
        let mut data = self.inner.write();
        let mut eligible_run_ids = data
            .run_events
            .iter()
            .filter(|(run_id, events)| {
                events
                    .iter()
                    .any(|event| event.created_at.as_str() < cutoff)
                    && data
                        .runs
                        .get(run_id.as_str())
                        .is_some_and(|run| task_run_status_is_terminal(run.status))
            })
            .map(|(run_id, _)| run_id.clone())
            .collect::<Vec<_>>();
        eligible_run_ids.sort();
        eligible_run_ids.truncate(candidate_limit);

        let mut deleted_events = 0_u64;
        for run_id in &eligible_run_ids {
            let remove_entry = if let Some(events) = data.run_events.get_mut(run_id) {
                let previous_len = events.len();
                events.retain(|event| event.created_at.as_str() >= cutoff);
                deleted_events =
                    deleted_events.saturating_add(previous_len.saturating_sub(events.len()) as u64);
                events.is_empty()
            } else {
                false
            };
            if remove_entry {
                data.run_events.remove(run_id);
            }
        }

        RunEventPruneResult {
            eligible_runs: eligible_run_ids.len(),
            deleted_events,
        }
    }

    pub(in crate::store) fn append_run_event(&self, event: TaskRunEventRecord) {
        let mut data = self.inner.write();
        data.run_events
            .entry(event.run_id.clone())
            .or_default()
            .push(event);
    }
}

fn run_event_is_after_cursor(
    event: &TaskRunEventRecord,
    after_created_at: Option<&str>,
    after_id: Option<&str>,
) -> bool {
    match (after_created_at, after_id) {
        (Some(created_at), Some(id)) => {
            event.created_at.as_str() > created_at
                || (event.created_at.as_str() == created_at && event.id.as_str() > id)
        }
        _ => true,
    }
}
