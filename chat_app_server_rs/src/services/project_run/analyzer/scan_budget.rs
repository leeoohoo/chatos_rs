use std::time::{Duration, Instant};

pub(super) use crate::services::project_run::file_limits::{
    read_to_string_limited, MAX_MANIFEST_BYTES, MAX_SOURCE_PROBE_BYTES,
};

const DEFAULT_MAX_ENTRIES: usize = 20_000;
const DEFAULT_DEADLINE: Duration = Duration::from_secs(5);

#[derive(Debug, Clone)]
pub(super) struct ScanBudget {
    started_at: Instant,
    deadline: Duration,
    max_entries: usize,
    entries_seen: usize,
}

impl ScanBudget {
    pub(super) fn for_project_run_analysis() -> Self {
        Self {
            started_at: Instant::now(),
            deadline: DEFAULT_DEADLINE,
            max_entries: DEFAULT_MAX_ENTRIES,
            entries_seen: 0,
        }
    }

    #[cfg(test)]
    pub(super) fn for_test(max_entries: usize, deadline: Duration) -> Self {
        Self {
            started_at: Instant::now(),
            deadline,
            max_entries,
            entries_seen: 0,
        }
    }

    pub(super) fn account_entry(&mut self) -> Result<(), String> {
        self.entries_seen = self.entries_seen.saturating_add(1);
        self.check()
    }

    pub(super) fn check(&self) -> Result<(), String> {
        if self.started_at.elapsed() > self.deadline {
            return Err(format!(
                "project run analysis exceeded {}ms deadline",
                self.deadline.as_millis()
            ));
        }
        if self.entries_seen > self.max_entries {
            return Err(format!(
                "project run analysis exceeded {} filesystem entries",
                self.max_entries
            ));
        }
        Ok(())
    }
}
