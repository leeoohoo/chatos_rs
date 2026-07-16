// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeSet, HashMap, HashSet};

use anyhow::Result;

pub(super) fn normalized_ids(values: Vec<String>) -> Vec<String> {
    values
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

pub(super) fn ensure_acyclic(
    target: &str,
    prerequisites: &[String],
    edges: &[(String, String)],
) -> Result<()> {
    let mut adjacency = HashMap::<&str, Vec<&str>>::new();
    for (from, to) in edges {
        adjacency
            .entry(from.as_str())
            .or_default()
            .push(to.as_str());
    }
    for prerequisite in prerequisites {
        if prerequisite == target || reachable(target, prerequisite.as_str(), &adjacency) {
            return Err(anyhow::anyhow!("local project dependency cycle detected"));
        }
    }
    Ok(())
}

fn reachable(start: &str, target: &str, adjacency: &HashMap<&str, Vec<&str>>) -> bool {
    let mut pending = vec![start];
    let mut visited = HashSet::new();
    while let Some(current) = pending.pop() {
        if current == target {
            return true;
        }
        if visited.insert(current) {
            pending.extend(adjacency.get(current).into_iter().flatten().copied());
        }
    }
    false
}
