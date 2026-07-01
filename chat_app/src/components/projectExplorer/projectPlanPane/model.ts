// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type {
  ProjectDependencyGraphResponse,
  ProjectPlanResponse,
  ProjectRequirementResponse,
  ProjectWorkItemResponse,
} from '../../../lib/api/client/types';

export const REQUIREMENT_COLUMN_WIDTH = 320;
export const MAX_REQUIREMENT_PANE_WIDTH = 860;
export const SELECTED_WORK_ITEM_INITIAL_RENDER_LIMIT = 80;
export const SELECTED_WORK_ITEM_RENDER_INCREMENT = 80;

export type DependencyMaps = {
  requirementDependents: Map<string, string[]>;
  requirementPrerequisites: Map<string, string[]>;
  workItemDependents: Map<string, string[]>;
  workItemPrerequisites: Map<string, string[]>;
};

export type RequirementColumn = {
  id: string;
  items: ProjectRequirementResponse[];
  selectedId: string | null;
  title: string;
};

export type VisiblePlanItems<T> = {
  hasMore: boolean;
  hiddenCount: number;
  items: T[];
  totalCount: number;
};

export const readText = (value: unknown): string => (
  typeof value === 'string' ? value.trim() : ''
);

export const requirementParentId = (requirement: ProjectRequirementResponse): string => (
  readText(requirement.parent_requirement_id) || readText(requirement.parentRequirementId)
);

export const workItemRequirementId = (item: ProjectWorkItemResponse): string => (
  readText(item.requirement_id) || readText(item.requirementId)
);

export const getUpdatedAt = (value: { updated_at?: string; updatedAt?: string }): string => (
  readText(value.updated_at) || readText(value.updatedAt)
);

export const formatDateTime = (value: string): string => {
  if (!value) {
    return '-';
  }
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }
  return date.toLocaleString();
};

export const statusLabel = (status?: string): string => {
  switch (status) {
    case 'reviewing':
      return '评审中';
    case 'approved':
      return '已确认';
    case 'in_progress':
      return '进行中';
    case 'ready':
      return '就绪';
    case 'blocked':
      return '阻塞';
    case 'done':
      return '完成';
    case 'cancelled':
      return '取消';
    case 'archived':
      return '归档';
    case 'todo':
      return '待办';
    case 'draft':
    default:
      return '草稿';
  }
};

export const requirementTypeLabel = (type?: string): string => {
  switch (type) {
    case 'change':
      return '变更';
    case 'bug_fix':
      return '缺陷修复';
    case 'requirement':
    default:
      return '需求';
  }
};

export const requirementDocumentTypeLabel = (type?: string): string => {
  switch (type) {
    case 'technical_overview':
      return '技术概要';
    case 'implementation_plan':
      return '实现方案';
    case 'ui_svg_preview':
      return '前端 SVG 预览图';
    case 'architecture_diagram':
      return '架构图';
    case 'flowchart':
      return '流程图';
    case 'sequence_diagram':
      return '时序图';
    case 'api_design':
      return '接口设计';
    case 'data_model':
      return '数据模型';
    case 'risk_notes':
      return '风险说明';
    case 'other':
      return '其他';
    default:
      return readText(type) || '技术文档';
  }
};

export const canShowRequirementExecutionAction = (status?: string): boolean => {
  const normalizedStatus = readText(status);
  return !['done', 'cancelled', 'archived'].includes(normalizedStatus);
};

export const statusClassName = (status?: string): string => {
  switch (status) {
    case 'done':
      return 'border-emerald-200 bg-emerald-50 text-emerald-700 dark:border-emerald-800 dark:bg-emerald-950/30 dark:text-emerald-300';
    case 'in_progress':
    case 'reviewing':
      return 'border-blue-200 bg-blue-50 text-blue-700 dark:border-blue-800 dark:bg-blue-950/30 dark:text-blue-300';
    case 'blocked':
      return 'border-destructive/30 bg-destructive/10 text-destructive';
    case 'cancelled':
    case 'archived':
      return 'border-border bg-muted/40 text-muted-foreground';
    case 'approved':
    case 'ready':
      return 'border-amber-200 bg-amber-50 text-amber-700 dark:border-amber-800 dark:bg-amber-950/30 dark:text-amber-300';
    default:
      return 'border-border bg-muted/30 text-muted-foreground';
  }
};

export const priorityLabel = (priority?: number): string => {
  if (typeof priority !== 'number' || !Number.isFinite(priority)) {
    return 'P-';
  }
  return `P${priority}`;
};

export const createEmptyDependencyMaps = (): DependencyMaps => ({
  requirementDependents: new Map(),
  requirementPrerequisites: new Map(),
  workItemDependents: new Map(),
  workItemPrerequisites: new Map(),
});

export const graphNodeRef = (value: string): { rawId: string; type: string } | null => {
  const [type, ...rest] = value.split(':');
  const rawId = rest.join(':').trim();
  if (!type || !rawId) {
    return null;
  }
  return { rawId, type };
};

export const appendUnique = (map: Map<string, string[]>, key: string, value: string) => {
  const list = map.get(key) || [];
  if (!list.includes(value)) {
    list.push(value);
    map.set(key, list);
  }
};

export const buildDependencyMaps = (plan: ProjectPlanResponse | null): DependencyMaps => {
  const maps = createEmptyDependencyMaps();
  const graph = plan?.dependencyGraph || plan?.dependency_graph;
  const edges = Array.isArray(graph?.edges) ? graph.edges : [];

  edges.forEach((edge) => {
    const edgeType = readText(edge.edge_type) || readText(edge.edgeType);
    if (edgeType === 'contains') {
      return;
    }

    const source = graphNodeRef(edge.from);
    const target = graphNodeRef(edge.to);
    if (!source || !target || source.type !== target.type) {
      return;
    }

    if (source.type === 'requirement') {
      appendUnique(maps.requirementPrerequisites, target.rawId, source.rawId);
      appendUnique(maps.requirementDependents, source.rawId, target.rawId);
      return;
    }

    if (source.type === 'work_item') {
      appendUnique(maps.workItemPrerequisites, target.rawId, source.rawId);
      appendUnique(maps.workItemDependents, source.rawId, target.rawId);
    }
  });

  return maps;
};

export const buildDependencyMapsFromGraph = (
  dependencyGraph: ProjectDependencyGraphResponse | null | undefined,
): DependencyMaps => (
  buildDependencyMaps(dependencyGraph ? { dependencyGraph } : null)
);

export const mergeDependencyMaps = (...mapList: DependencyMaps[]): DependencyMaps => {
  const merged = createEmptyDependencyMaps();
  mapList.forEach((maps) => {
    maps.requirementDependents.forEach((values, key) => {
      values.forEach((value) => appendUnique(merged.requirementDependents, key, value));
    });
    maps.requirementPrerequisites.forEach((values, key) => {
      values.forEach((value) => appendUnique(merged.requirementPrerequisites, key, value));
    });
    maps.workItemDependents.forEach((values, key) => {
      values.forEach((value) => appendUnique(merged.workItemDependents, key, value));
    });
    maps.workItemPrerequisites.forEach((values, key) => {
      values.forEach((value) => appendUnique(merged.workItemPrerequisites, key, value));
    });
  });
  return merged;
};

export const buildRequirementChildrenMap = (
  requirements: ProjectRequirementResponse[],
): Map<string, ProjectRequirementResponse[]> => {
  const byParent = new Map<string, ProjectRequirementResponse[]>();
  const byId = new Map<string, ProjectRequirementResponse>();
  requirements.forEach((requirement) => {
    byId.set(requirement.id, requirement);
  });
  requirements.forEach((requirement) => {
    const parentId = requirementParentId(requirement);
    const key = parentId && byId.has(parentId) ? parentId : '';
    const list = byParent.get(key) || [];
    list.push(requirement);
    byParent.set(key, list);
  });

  return byParent;
};

const requirementStatusIsDone = (status: unknown): boolean => (
  ['done', 'succeeded', 'success', 'completed'].includes(readText(status).toLowerCase())
);

const expandRequirementDescendants = (
  requirements: ProjectRequirementResponse[],
  scope: Set<string>,
) => {
  let changed = true;
  while (changed) {
    changed = false;
    requirements.forEach((requirement) => {
      const parentId = requirementParentId(requirement);
      if (parentId && scope.has(parentId) && !scope.has(requirement.id)) {
        scope.add(requirement.id);
        changed = true;
      }
    });
  }
};

const expandRequirementDependents = (
  dependencyMaps: DependencyMaps,
  scope: Set<string>,
) => {
  let changed = true;
  while (changed) {
    changed = false;
    dependencyMaps.requirementPrerequisites.forEach((prerequisiteIds, requirementId) => {
      if (prerequisiteIds.some((prerequisiteId) => scope.has(prerequisiteId))
        && !scope.has(requirementId)) {
        scope.add(requirementId);
        changed = true;
      }
    });
  }
};

export const buildRequirementExecutionScope = ({
  dependencyMaps,
  includePrerequisiteDependents = false,
  requirements,
  rootId,
}: {
  dependencyMaps: DependencyMaps;
  includePrerequisiteDependents?: boolean;
  requirements: ProjectRequirementResponse[];
  rootId: string | null;
}): string[] => {
  const normalizedRootId = readText(rootId);
  if (!normalizedRootId) {
    return [];
  }

  const downstreamIds = buildRequirementDownstreamScope({
    dependencyMaps,
    requirements,
    rootId: normalizedRootId,
  });
  if (downstreamIds.length === 0) {
    return [];
  }

  const scope = new Set<string>(downstreamIds);
  const requirementById = new Map(requirements.map((requirement) => [requirement.id, requirement]));
  let changed = true;
  while (changed) {
    changed = false;
    Array.from(scope).forEach((requirementId) => {
      const prerequisiteIds = dependencyMaps.requirementPrerequisites.get(requirementId) || [];
      prerequisiteIds.forEach((prerequisiteId) => {
        if (scope.has(prerequisiteId)) {
          return;
        }
        const prerequisite = requirementById.get(prerequisiteId);
        if (!prerequisite || requirementStatusIsDone(prerequisite.status)) {
          return;
        }
        scope.add(prerequisiteId);
        changed = true;
      });
    });
    const before = scope.size;
    expandRequirementDescendants(requirements, scope);
    if (includePrerequisiteDependents) {
      expandRequirementDependents(dependencyMaps, scope);
      expandRequirementDescendants(requirements, scope);
    }
    if (scope.size !== before) {
      changed = true;
    }
  }

  return [
    normalizedRootId,
    ...requirements
      .map((requirement) => requirement.id)
      .filter((requirementId) => requirementId !== normalizedRootId && scope.has(requirementId)),
  ];
};

export const buildRequirementDownstreamScope = ({
  dependencyMaps,
  requirements,
  rootId,
}: {
  dependencyMaps: DependencyMaps;
  requirements: ProjectRequirementResponse[];
  rootId: string | null;
}): string[] => {
  const normalizedRootId = readText(rootId);
  if (!normalizedRootId) {
    return [];
  }
  const scope = new Set<string>([normalizedRootId]);
  expandRequirementDescendants(requirements, scope);
  expandRequirementDependents(dependencyMaps, scope);
  expandRequirementDescendants(requirements, scope);
  return [
    normalizedRootId,
    ...requirements
      .map((requirement) => requirement.id)
      .filter((requirementId) => requirementId !== normalizedRootId && scope.has(requirementId)),
  ];
};

export const buildRequirementPath = (
  requirementId: string | null,
  requirementById: Map<string, ProjectRequirementResponse>,
): string[] => {
  if (!requirementId) {
    return [];
  }

  const path: string[] = [];
  const visited = new Set<string>();
  let current = requirementById.get(requirementId);

  while (current && !visited.has(current.id)) {
    visited.add(current.id);
    path.unshift(current.id);
    const parentId = requirementParentId(current);
    current = parentId ? requirementById.get(parentId) : undefined;
  }

  return path;
};

export const buildRequirementColumns = ({
  childrenMap,
  path,
  requirementById,
}: {
  childrenMap: Map<string, ProjectRequirementResponse[]>;
  path: string[];
  requirementById: Map<string, ProjectRequirementResponse>;
}): RequirementColumn[] => {
  const rootItems = childrenMap.get('') || [];
  const columns: RequirementColumn[] = [{
    id: 'root',
    items: rootItems,
    selectedId: path[0] || null,
    title: '主需求',
  }];

  path.forEach((requirementId, index) => {
    const children = childrenMap.get(requirementId) || [];
    if (children.length === 0) {
      return;
    }
    columns.push({
      id: requirementId,
      items: children,
      selectedId: path[index + 1] || null,
      title: requirementById.get(requirementId)?.title || '子需求',
    });
  });

  return columns;
};

export const groupWorkItemsByRequirement = (
  workItems: ProjectWorkItemResponse[],
): Map<string, ProjectWorkItemResponse[]> => {
  const map = new Map<string, ProjectWorkItemResponse[]>();
  workItems.forEach((item) => {
    const requirementId = workItemRequirementId(item);
    if (!requirementId) {
      return;
    }
    const list = map.get(requirementId) || [];
    list.push(item);
    map.set(requirementId, list);
  });
  map.forEach((items) => {
    items.sort((a, b) => {
      const orderA = typeof a.sort_order === 'number' ? a.sort_order : (a.sortOrder || 0);
      const orderB = typeof b.sort_order === 'number' ? b.sort_order : (b.sortOrder || 0);
      if (orderA !== orderB) {
        return orderA - orderB;
      }
      return getUpdatedAt(b).localeCompare(getUpdatedAt(a));
    });
  });
  return map;
};

export const countOpenItems = (items: ProjectWorkItemResponse[]): number => (
  items.filter((item) => item.status !== 'done').length
);

export const buildVisiblePlanItems = <T>(
  items: T[],
  visibleLimit: number,
): VisiblePlanItems<T> => {
  const totalCount = items.length;
  const normalizedLimit = Number.isFinite(visibleLimit)
    ? Math.max(1, Math.floor(visibleLimit))
    : totalCount;
  const visibleItems = totalCount > normalizedLimit
    ? items.slice(0, normalizedLimit)
    : items;
  const hiddenCount = Math.max(0, totalCount - visibleItems.length);

  return {
    hasMore: hiddenCount > 0,
    hiddenCount,
    items: visibleItems,
    totalCount,
  };
};

export const sortWorkItemsByDependencies = (
  items: ProjectWorkItemResponse[],
  dependencies: Map<string, string[]>,
): ProjectWorkItemResponse[] => {
  const idSet = new Set(items.map((item) => item.id));
  const baseOrder = new Map(items.map((item, index) => [item.id, index]));
  const indegree = new Map<string, number>();
  const outgoing = new Map<string, string[]>();

  items.forEach((item) => {
    indegree.set(item.id, 0);
  });

  items.forEach((item) => {
    const prerequisites = dependencies.get(item.id) || [];
    prerequisites.forEach((prerequisiteId) => {
      if (!idSet.has(prerequisiteId)) {
        return;
      }
      indegree.set(item.id, (indegree.get(item.id) || 0) + 1);
      const list = outgoing.get(prerequisiteId) || [];
      list.push(item.id);
      outgoing.set(prerequisiteId, list);
    });
  });

  const queue = items
    .filter((item) => (indegree.get(item.id) || 0) === 0)
    .sort((a, b) => (baseOrder.get(a.id) || 0) - (baseOrder.get(b.id) || 0));
  const byId = new Map(items.map((item) => [item.id, item]));
  const result: ProjectWorkItemResponse[] = [];

  while (queue.length > 0) {
    const current = queue.shift();
    if (!current) {
      break;
    }
    result.push(current);
    (outgoing.get(current.id) || []).forEach((nextId) => {
      const nextIndegree = (indegree.get(nextId) || 0) - 1;
      indegree.set(nextId, nextIndegree);
      if (nextIndegree === 0) {
        const next = byId.get(nextId);
        if (next) {
          queue.push(next);
          queue.sort((a, b) => (baseOrder.get(a.id) || 0) - (baseOrder.get(b.id) || 0));
        }
      }
    });
  }

  if (result.length !== items.length) {
    const emitted = new Set(result.map((item) => item.id));
    items.forEach((item) => {
      if (!emitted.has(item.id)) {
        result.push(item);
      }
    });
  }

  return result;
};
