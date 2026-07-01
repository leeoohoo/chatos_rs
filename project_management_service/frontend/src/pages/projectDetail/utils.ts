// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { CreateWorkItemPayload, ProjectWorkItemRecord, RequirementRecord } from '../../types';
import type { RequirementTableRecord, WorkItemFormValues } from './types';

export function isSelectableRequirement(item: RequirementRecord): boolean {
  return item.status !== 'archived' && item.status !== 'cancelled';
}

export function isSelectableWorkItem(item: ProjectWorkItemRecord): boolean {
  return item.status !== 'archived' && item.status !== 'cancelled';
}

export function buildRequirementTree(items: RequirementRecord[]): RequirementTableRecord[] {
  const nodeMap = new Map<string, RequirementTableRecord>(
    items.map((item) => [item.id, { ...item }]),
  );
  const roots: RequirementTableRecord[] = [];

  items.forEach((item) => {
    const node = nodeMap.get(item.id);
    const parentId = item.parent_requirement_id?.trim();
    const parent = parentId ? nodeMap.get(parentId) : undefined;
    if (!node) {
      return;
    }
    if (parent && parent.id !== item.id) {
      parent.children ??= [];
      parent.children.push(node);
      return;
    }
    roots.push(node);
  });

  const assignTreeLevel = (nodes: RequirementTableRecord[], level: number) => {
    nodes.forEach((node) => {
      node.tree_level = level;
      if (node.children?.length) {
        assignTreeLevel(node.children, level + 1);
      }
    });
  };
  assignTreeLevel(roots, 0);

  return roots;
}

export function buildCreateWorkItemPayload(values: WorkItemFormValues): CreateWorkItemPayload {
  const tags = values.tags_text
    ?.split(',')
    .map((item) => item.trim())
    .filter(Boolean);

  return {
    title: values.title,
    description: values.description,
    task_runner_default_model_config_id: values.task_runner_default_model_config_id,
    task_runner_enabled_tool_ids: values.task_runner_enabled_tool_ids,
    task_runner_skill_ids: values.task_runner_skill_ids || [],
    status: values.status,
    priority: values.priority,
    assignee_user_id: values.assignee_user_id,
    estimate_points: values.estimate_points,
    due_at: values.due_at,
    sort_order: values.sort_order,
    tags,
  };
}
