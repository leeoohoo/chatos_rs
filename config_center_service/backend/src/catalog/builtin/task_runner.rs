use super::*;

#[path = "task_runner/downstream.rs"]
mod downstream;
#[path = "task_runner/execution.rs"]
mod execution;
#[path = "task_runner/operations.rs"]
mod operations;
#[path = "task_runner/queue.rs"]
mod queue;
#[path = "task_runner/runtime.rs"]
mod runtime;

pub(super) fn definitions(now: &str) -> Vec<ConfigDefinitionRecord> {
    let mut definitions = Vec::new();
    definitions.extend(execution::definitions(now));
    definitions.extend(queue::definitions(now));
    definitions.extend(operations::definitions(now));
    definitions.extend(downstream::definitions(now));
    definitions.extend(runtime::definitions(now));
    definitions
}
