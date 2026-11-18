use crate::{
    host_lifecycle::HostManager,
    machine_resource_governor::{
        MachineConcurrencyAppSnapshot, MachineConcurrencyBudget, MachineResourceGovernor,
    },
};
use chatos_local_agent_protocol::LocalAgentHostSchedulerSnapshot;
use std::{future::Future, time::Duration};
use tauri::{AppHandle, Manager};

const REFRESH_INTERVAL: Duration = Duration::from_secs(1);
const RESOURCE_UNITS_PER_APP_JOB: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MachineConcurrencyTargets {
    host: usize,
    app: usize,
}

fn desired_targets(
    total: usize,
    base_host: usize,
    host_active: usize,
    host_queued: usize,
    app_used_resource_units: usize,
    app_queued_resource_units: usize,
) -> MachineConcurrencyTargets {
    let base = MachineConcurrencyTargets {
        host: base_host,
        app: total - base_host,
    };
    if host_queued > 0 && app_queued_resource_units == 0 {
        let app_keep = app_used_resource_units
            .div_ceil(RESOURCE_UNITS_PER_APP_JOB)
            .max(1);
        let host_demand = base.host.max(host_active.saturating_add(host_queued));
        let host = host_demand.min(total.saturating_sub(app_keep));
        return MachineConcurrencyTargets {
            host,
            app: total - host,
        };
    }
    if app_queued_resource_units > 0 && host_queued == 0 {
        let host_keep = host_active.max(1);
        let app_demand = base.app.max(
            app_used_resource_units
                .saturating_add(app_queued_resource_units)
                .div_ceil(RESOURCE_UNITS_PER_APP_JOB),
        );
        let app = app_demand.min(total.saturating_sub(host_keep));
        return MachineConcurrencyTargets {
            host: total - app,
            app,
        };
    }
    base
}

struct MachineConcurrencyAllocator {
    budget: MachineConcurrencyBudget,
    pending_borrow: Option<MachineConcurrencyTargets>,
    pending_borrow_observations: usize,
}

impl MachineConcurrencyAllocator {
    fn new(budget: MachineConcurrencyBudget) -> Self {
        Self {
            budget,
            pending_borrow: None,
            pending_borrow_observations: 0,
        }
    }

    fn base_targets(&self) -> MachineConcurrencyTargets {
        MachineConcurrencyTargets {
            host: self.budget.base_host_work_items,
            app: self.budget.base_app_jobs,
        }
    }

    fn stabilized(
        &mut self,
        desired: MachineConcurrencyTargets,
    ) -> Option<MachineConcurrencyTargets> {
        if desired == self.base_targets() {
            self.pending_borrow = None;
            self.pending_borrow_observations = 0;
            return Some(desired);
        }
        if self.pending_borrow != Some(desired) {
            self.pending_borrow = Some(desired);
            self.pending_borrow_observations = 1;
            return None;
        }
        self.pending_borrow_observations = self.pending_borrow_observations.saturating_add(1);
        (self.pending_borrow_observations >= 2).then_some(desired)
    }

    async fn rebalance<F, Fut>(
        &mut self,
        host: Option<LocalAgentHostSchedulerSnapshot>,
        resources: &MachineResourceGovernor,
        update_host_target: F,
    ) where
        F: FnMut(usize) -> Fut,
        Fut: Future<Output = bool>,
    {
        let app = resources.concurrency_snapshot();
        let Some(host) = host else {
            self.pending_borrow = None;
            self.pending_borrow_observations = 0;
            let base_app_units = self
                .budget
                .base_app_jobs
                .saturating_mul(RESOURCE_UNITS_PER_APP_JOB);
            if app.target_resource_units > base_app_units {
                let _ = resources.update_target_resource_units(base_app_units);
            }
            return;
        };
        let host_queued = usize::try_from(host.queued_ready_work_items).unwrap_or(usize::MAX);
        let desired = if host.accepting_new_work {
            desired_targets(
                self.budget.total_logical_jobs,
                self.budget.base_host_work_items,
                host.active_work_items,
                host_queued,
                app.used_resource_units,
                app.queued_resource_units,
            )
        } else {
            self.base_targets()
        };
        let Some(stable_desired) = self.stabilized(desired) else {
            return;
        };
        self.apply(stable_desired, host, app, resources, update_host_target)
            .await;
    }

    async fn apply<F, Fut>(
        &self,
        desired: MachineConcurrencyTargets,
        host: LocalAgentHostSchedulerSnapshot,
        app: MachineConcurrencyAppSnapshot,
        resources: &MachineResourceGovernor,
        mut update_host_target: F,
    ) where
        F: FnMut(usize) -> Fut,
        Fut: Future<Output = bool>,
    {
        let mut host_target = host.target_concurrent_work_items;
        let mut app_target = app.target_resource_units / RESOURCE_UNITS_PER_APP_JOB;
        let mut shrank = false;

        if desired.host < host_target {
            if !update_host_target(desired.host).await {
                return;
            }
            host_target = desired.host;
            shrank = true;
        }
        if desired.app < app_target {
            if !resources.update_target_resource_units(
                desired.app.saturating_mul(RESOURCE_UNITS_PER_APP_JOB),
            ) {
                return;
            }
            app_target = desired.app;
            shrank = true;
        }
        if shrank
            || host.active_work_items > host_target
            || app.used_resource_units > app_target.saturating_mul(RESOURCE_UNITS_PER_APP_JOB)
        {
            return;
        }

        if desired.host > host_target {
            if desired.host.saturating_add(app_target) > self.budget.total_logical_jobs
                || !update_host_target(desired.host).await
            {
                return;
            }
            host_target = desired.host;
        }
        if desired.app > app_target
            && host_target.saturating_add(desired.app) <= self.budget.total_logical_jobs
        {
            let _ = resources.update_target_resource_units(
                desired.app.saturating_mul(RESOURCE_UNITS_PER_APP_JOB),
            );
        }
    }
}

pub(crate) fn start_allocator(app: AppHandle) {
    let resources = app.state::<MachineResourceGovernor>().inner().clone();
    let budget = resources.concurrency_budget();
    if !budget.allows_dynamic_borrowing {
        return;
    }
    tauri::async_runtime::spawn(async move {
        let mut allocator = MachineConcurrencyAllocator::new(budget);
        loop {
            let host_snapshot = app.state::<HostManager>().scheduler_snapshot().await;
            let host_app = app.clone();
            allocator
                .rebalance(host_snapshot, &resources, move |target| {
                    let host_app = host_app.clone();
                    async move {
                        host_app
                            .state::<HostManager>()
                            .update_scheduler_target(target)
                            .await
                            .unwrap_or(false)
                    }
                })
                .await;
            tokio::time::sleep(REFRESH_INTERVAL).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use chatos_local_agent_protocol::LocalAgentResourcePressureLevel;
    use std::sync::{Arc, Mutex};

    fn targets(
        host_active: usize,
        host_queued: usize,
        app_used: usize,
        app_queued: usize,
    ) -> MachineConcurrencyTargets {
        desired_targets(10, 5, host_active, host_queued, app_used, app_queued)
    }

    fn budget() -> MachineConcurrencyBudget {
        MachineConcurrencyBudget {
            total_logical_jobs: 10,
            base_host_work_items: 5,
            base_app_jobs: 5,
            maximum_host_work_items: 9,
            allows_dynamic_borrowing: true,
        }
    }

    fn host(target: usize, active: usize) -> LocalAgentHostSchedulerSnapshot {
        LocalAgentHostSchedulerSnapshot {
            maximum_concurrent_work_items: 9,
            target_concurrent_work_items: target,
            effective_maximum_concurrent_work_items: target,
            pressure_level: LocalAgentResourcePressureLevel::Normal,
            active_work_items: active,
            idle_workers: 9 - active,
            queued_ready_work_items: 0,
            accepting_new_work: true,
            active_memory_sync_items: 0,
            admission_in_flight_work_items: 0,
        }
    }

    #[test]
    fn balanced_demand_uses_the_baseline_split() {
        assert_eq!(
            targets(5, 1, 10, 2),
            MachineConcurrencyTargets { host: 5, app: 5 }
        );
    }

    #[test]
    fn host_and_app_borrow_only_idle_jobs() {
        assert_eq!(
            targets(5, 4, 0, 0),
            MachineConcurrencyTargets { host: 9, app: 1 }
        );
        assert_eq!(
            targets(5, 4, 6, 0),
            MachineConcurrencyTargets { host: 7, app: 3 }
        );
        assert_eq!(
            targets(0, 0, 10, 8),
            MachineConcurrencyTargets { host: 1, app: 9 }
        );
        assert_eq!(
            targets(3, 0, 10, 8),
            MachineConcurrencyTargets { host: 3, app: 7 }
        );
    }

    #[test]
    fn every_plan_preserves_one_job_per_side_and_the_total_budget() {
        for host_active in 0..=9 {
            for host_queued in 0..=9 {
                for app_used in 0..=18 {
                    let result = targets(
                        host_active,
                        host_queued,
                        app_used,
                        if host_queued == 0 { 4 } else { 0 },
                    );
                    assert!(result.host >= 1);
                    assert!(result.app >= 1);
                    assert_eq!(result.host + result.app, 10);
                }
            }
        }
    }

    #[test]
    fn borrowing_requires_two_matching_observations_but_baseline_is_immediate() {
        let mut allocator = MachineConcurrencyAllocator::new(budget());
        let borrowed = MachineConcurrencyTargets { host: 9, app: 1 };
        assert_eq!(allocator.stabilized(borrowed), None);
        assert_eq!(allocator.stabilized(borrowed), Some(borrowed));
        assert_eq!(
            allocator.stabilized(allocator.base_targets()),
            Some(MachineConcurrencyTargets { host: 5, app: 5 })
        );
    }

    #[tokio::test]
    async fn allocator_shrinks_and_drains_before_growing_the_borrower() {
        let resources = MachineResourceGovernor::new_for_concurrency_tests(2, 18, 10, 5);
        let allocator = MachineConcurrencyAllocator::new(budget());
        let recorded = Arc::new(Mutex::new(Vec::new()));

        let first_recorded = Arc::clone(&recorded);
        allocator
            .apply(
                allocator.base_targets(),
                host(9, 7),
                resources.concurrency_snapshot(),
                &resources,
                move |target| {
                    let recorded = Arc::clone(&first_recorded);
                    async move {
                        recorded.lock().unwrap().push(target);
                        true
                    }
                },
            )
            .await;
        assert_eq!(*recorded.lock().unwrap(), vec![5]);
        assert_eq!(resources.concurrency_snapshot().target_resource_units, 2);

        allocator
            .apply(
                allocator.base_targets(),
                host(5, 6),
                resources.concurrency_snapshot(),
                &resources,
                |_| async { true },
            )
            .await;
        assert_eq!(resources.concurrency_snapshot().target_resource_units, 2);

        allocator
            .apply(
                allocator.base_targets(),
                host(5, 5),
                resources.concurrency_snapshot(),
                &resources,
                |_| async { true },
            )
            .await;
        assert_eq!(resources.concurrency_snapshot().target_resource_units, 10);
    }

    #[tokio::test]
    async fn missing_host_snapshot_never_expands_app_capacity() {
        let resources = MachineResourceGovernor::new_for_concurrency_tests(10, 18, 10, 5);
        assert!(resources.update_target_resource_units(2));
        let mut allocator = MachineConcurrencyAllocator::new(budget());
        allocator
            .rebalance(None, &resources, |_| async { false })
            .await;
        assert_eq!(resources.concurrency_snapshot().target_resource_units, 2);
    }
}
