use super::*;

#[path = "memory_engine/core.rs"]
mod core;
#[path = "memory_engine/worker_queue.rs"]
mod worker_queue;

pub(super) fn definitions(now: &str) -> Vec<ConfigDefinitionRecord> {
    let mut definitions = Vec::new();
    definitions.extend(core::definitions(now));
    definitions.extend(worker_queue::definitions(now));
    definitions
}
