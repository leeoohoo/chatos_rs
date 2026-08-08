// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{HashMap, HashSet};

use super::super::CreateTaskWithPrerequisitesItem;

pub(crate) fn ensure_client_ref_graph_acyclic(
    tasks: &[CreateTaskWithPrerequisitesItem],
) -> Result<(), String> {
    let mut graph = HashMap::<String, Vec<String>>::new();
    for task in tasks {
        graph.insert(
            task.client_ref.trim().to_string(),
            task.prerequisite_refs
                .iter()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .collect(),
        );
    }

    let mut visiting = HashSet::new();
    let mut visited = HashSet::new();
    for root in graph.keys() {
        let mut stack = vec![(root.clone(), false)];
        while let Some((current, expanded)) = stack.pop() {
            if expanded {
                visiting.remove(&current);
                visited.insert(current);
                continue;
            }
            if visited.contains(&current) {
                continue;
            }
            if !visiting.insert(current.clone()) {
                return Err(format!("前置任务不能形成循环依赖: {current}"));
            }
            stack.push((current.clone(), true));
            for prerequisite_ref in graph.get(&current).into_iter().flatten() {
                if visiting.contains(prerequisite_ref) {
                    return Err(format!(
                        "前置任务不能形成循环依赖: {} -> {}",
                        current, prerequisite_ref
                    ));
                }
                stack.push((prerequisite_ref.clone(), false));
            }
        }
    }
    Ok(())
}
