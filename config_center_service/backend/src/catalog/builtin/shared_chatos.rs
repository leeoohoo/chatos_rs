use super::*;

#[path = "shared_chatos/chatos_features.rs"]
mod chatos_features;
#[path = "shared_chatos/chatos_runtime.rs"]
mod chatos_runtime;
#[path = "shared_chatos/shared.rs"]
mod shared;

pub(super) fn definitions(now: &str) -> Vec<ConfigDefinitionRecord> {
    let mut definitions = Vec::new();
    definitions.extend(shared::definitions(now));
    definitions.extend(chatos_runtime::definitions(now));
    definitions.extend(chatos_features::definitions(now));
    definitions
}
