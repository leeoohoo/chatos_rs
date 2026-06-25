import React, { useCallback, useEffect, useMemo, useState } from 'react';
import {
  AlertCircle,
  ArrowRight,
  ChevronRight,
  CheckCircle2,
  ClipboardList,
  GitBranch,
  Link2,
  RefreshCw,
} from 'lucide-react';

import { useApiClient } from '../../lib/api/ApiClientContext';
import type {
  ProjectPlanResponse,
  ProjectRequirementResponse,
  ProjectWorkItemResponse,
} from '../../lib/api/client/types';
import { cn } from '../../lib/utils';
import type { Project } from '../../types';
import { LazyMarkdownRenderer } from '../LazyMarkdownRenderer';

interface ProjectPlanPaneProps {
  project: Project;
  className?: string;
}

const REQUIREMENT_COLUMN_WIDTH = 320;
const MAX_REQUIREMENT_PANE_WIDTH = 860;

type DependencyMaps = {
  requirementDependents: Map<string, string[]>;
  requirementPrerequisites: Map<string, string[]>;
  workItemDependents: Map<string, string[]>;
  workItemPrerequisites: Map<string, string[]>;
};

type RequirementColumn = {
  id: string;
  items: ProjectRequirementResponse[];
  selectedId: string | null;
  title: string;
};

const readText = (value: unknown): string => (
  typeof value === 'string' ? value.trim() : ''
);

const requirementParentId = (requirement: ProjectRequirementResponse): string => (
  readText(requirement.parent_requirement_id) || readText(requirement.parentRequirementId)
);

const workItemRequirementId = (item: ProjectWorkItemResponse): string => (
  readText(item.requirement_id) || readText(item.requirementId)
);

const getUpdatedAt = (value: { updated_at?: string; updatedAt?: string }): string => (
  readText(value.updated_at) || readText(value.updatedAt)
);

const formatDateTime = (value: string): string => {
  if (!value) {
    return '-';
  }
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }
  return date.toLocaleString();
};

const statusLabel = (status?: string): string => {
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

const requirementTypeLabel = (type?: string): string => {
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

const statusClassName = (status?: string): string => {
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

const priorityLabel = (priority?: number): string => {
  if (typeof priority !== 'number' || !Number.isFinite(priority)) {
    return 'P-';
  }
  return `P${priority}`;
};

const createEmptyDependencyMaps = (): DependencyMaps => ({
  requirementDependents: new Map(),
  requirementPrerequisites: new Map(),
  workItemDependents: new Map(),
  workItemPrerequisites: new Map(),
});

const graphNodeRef = (value: string): { rawId: string; type: string } | null => {
  const [type, ...rest] = value.split(':');
  const rawId = rest.join(':').trim();
  if (!type || !rawId) {
    return null;
  }
  return { rawId, type };
};

const appendUnique = (map: Map<string, string[]>, key: string, value: string) => {
  const list = map.get(key) || [];
  if (!list.includes(value)) {
    list.push(value);
    map.set(key, list);
  }
};

const buildDependencyMaps = (plan: ProjectPlanResponse | null): DependencyMaps => {
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

const buildRequirementChildrenMap = (
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

const buildRequirementPath = (
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

const buildRequirementColumns = ({
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

const groupWorkItemsByRequirement = (
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

const countOpenItems = (items: ProjectWorkItemResponse[]): number => (
  items.filter((item) => !['done', 'cancelled', 'archived'].includes(item.status || '')).length
);

const sortWorkItemsByDependencies = (
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

const RequirementContentSection: React.FC<{
  title: string;
  content?: string | null;
}> = ({ title, content }) => {
  const text = readText(content);
  if (!text) {
    return null;
  }
  return (
    <section className="border-t border-border/70 py-3">
      <h4 className="text-xs font-semibold text-muted-foreground">{title}</h4>
      <div className="mt-2 rounded-md border border-border/70 bg-muted/10 px-3 py-2">
        <LazyMarkdownRenderer content={text} className="text-sm" />
      </div>
    </section>
  );
};

const DependencyPill: React.FC<{
  children: React.ReactNode;
  tone?: 'dependency' | 'dependent';
}> = ({ children, tone = 'dependency' }) => (
  <span className={cn(
    'inline-flex min-w-0 items-center rounded border px-1.5 py-0.5',
    tone === 'dependent'
      ? 'border-blue-200 bg-blue-50 text-blue-700 dark:border-blue-800 dark:bg-blue-950/30 dark:text-blue-300'
      : 'border-amber-200 bg-amber-50 text-amber-700 dark:border-amber-800 dark:bg-amber-950/30 dark:text-amber-300',
  )}
  >
    <span className="truncate">{children}</span>
  </span>
);

const DependencyLine: React.FC<{
  emptyLabel?: string;
  ids: string[];
  label: string;
  resolveLabel: (id: string) => string;
  tone?: 'dependency' | 'dependent';
}> = ({
  emptyLabel = '无',
  ids,
  label,
  resolveLabel,
  tone = 'dependency',
}) => (
  <div className="flex min-w-0 flex-wrap items-center gap-1.5 text-[11px]">
    <span className="shrink-0 font-medium text-muted-foreground">{label}</span>
    {ids.length === 0 ? (
      <span className="rounded border border-border/70 bg-muted/20 px-1.5 py-0.5 text-muted-foreground">
        {emptyLabel}
      </span>
    ) : (
      ids.map((id) => (
        <DependencyPill key={id} tone={tone}>
          {resolveLabel(id)}
        </DependencyPill>
      ))
    )}
  </div>
);

const WorkItemRow: React.FC<{
  dependents: string[];
  item: ProjectWorkItemResponse;
  prerequisites: string[];
  resolveWorkItemTitle: (id: string) => string;
}> = ({
  dependents,
  item,
  prerequisites,
  resolveWorkItemTitle,
}) => (
  <article className="rounded-md border border-border bg-background px-3 py-2">
    <div className="flex flex-wrap items-start justify-between gap-2">
      <div className="min-w-0">
        <div className="break-words text-sm font-medium text-foreground">
          {item.title || item.id}
        </div>
        {readText(item.description) ? (
          <div className="mt-1 line-clamp-3 text-xs leading-5 text-muted-foreground">
            {item.description}
          </div>
        ) : null}
      </div>
      <div className="flex shrink-0 items-center gap-1">
        <span className={cn(
          'rounded-full border px-2 py-0.5 text-[11px] font-medium',
          statusClassName(item.status),
        )}
        >
          {statusLabel(item.status)}
        </span>
        <span className="rounded-full border border-border bg-muted/30 px-2 py-0.5 text-[11px] text-muted-foreground">
          {priorityLabel(item.priority)}
        </span>
      </div>
    </div>
    <div className="mt-2 space-y-1.5 rounded-md border border-border/70 bg-muted/10 px-2 py-2">
      <DependencyLine
        ids={prerequisites}
        label="前置任务"
        resolveLabel={resolveWorkItemTitle}
      />
      {dependents.length > 0 ? (
        <DependencyLine
          ids={dependents}
          label="后续任务"
          resolveLabel={resolveWorkItemTitle}
          tone="dependent"
        />
      ) : null}
    </div>
    {(item.tags || []).length > 0 || item.due_at || item.dueAt ? (
      <div className="mt-2 flex flex-wrap gap-1.5 text-[11px] text-muted-foreground">
        {(item.tags || []).map((tag) => (
          <span key={tag} className="rounded border border-border/70 bg-muted/20 px-1.5 py-0.5">
            {tag}
          </span>
        ))}
        {item.due_at || item.dueAt ? (
          <span className="rounded border border-border/70 bg-muted/20 px-1.5 py-0.5">
            截止 {formatDateTime(readText(item.due_at) || readText(item.dueAt))}
          </span>
        ) : null}
      </div>
    ) : null}
  </article>
);

export const ProjectPlanPane: React.FC<ProjectPlanPaneProps> = ({ project, className }) => {
  const apiClient = useApiClient();
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [plan, setPlan] = useState<ProjectPlanResponse | null>(null);
  const [selectedRequirementId, setSelectedRequirementId] = useState<string | null>(null);

  const loadPlan = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const result = await apiClient.getProjectPlan(project.id);
      setPlan(result);
    } catch (err) {
      setError(err instanceof Error ? err.message : '加载 Plan 失败');
    } finally {
      setLoading(false);
    }
  }, [apiClient, project.id]);

  useEffect(() => {
    setPlan(null);
    setSelectedRequirementId(null);
    void loadPlan();
  }, [loadPlan]);

  const requirements = useMemo(
    () => (Array.isArray(plan?.requirements) ? plan.requirements : []),
    [plan?.requirements],
  );
  const workItems = useMemo(
    () => (Array.isArray(plan?.workItems) ? plan.workItems : (plan?.work_items || [])),
    [plan?.workItems, plan?.work_items],
  );
  const workItemsByRequirement = useMemo(
    () => groupWorkItemsByRequirement(workItems),
    [workItems],
  );
  const dependencyMaps = useMemo(() => buildDependencyMaps(plan), [plan]);
  const requirementById = useMemo(
    () => new Map(requirements.map((requirement) => [requirement.id, requirement])),
    [requirements],
  );
  const workItemById = useMemo(
    () => new Map(workItems.map((item) => [item.id, item])),
    [workItems],
  );
  const resolveRequirementTitle = useCallback(
    (id: string) => requirementById.get(id)?.title || id,
    [requirementById],
  );
  const resolveWorkItemTitle = useCallback(
    (id: string) => workItemById.get(id)?.title || id,
    [workItemById],
  );
  const requirementChildrenMap = useMemo(
    () => buildRequirementChildrenMap(requirements),
    [requirements],
  );
  const requirementPath = useMemo(
    () => buildRequirementPath(selectedRequirementId, requirementById),
    [requirementById, selectedRequirementId],
  );
  const requirementColumns = useMemo(
    () => buildRequirementColumns({
      childrenMap: requirementChildrenMap,
      path: requirementPath,
      requirementById,
    }),
    [requirementById, requirementChildrenMap, requirementPath],
  );
  const requirementPaneWidth = useMemo(() => {
    const columnCount = Math.max(requirementColumns.length, 1);
    return Math.min(columnCount * REQUIREMENT_COLUMN_WIDTH, MAX_REQUIREMENT_PANE_WIDTH);
  }, [requirementColumns.length]);

  useEffect(() => {
    if (requirements.length === 0) {
      setSelectedRequirementId(null);
      return;
    }
    if (selectedRequirementId && requirements.some((item) => item.id === selectedRequirementId)) {
      return;
    }
    setSelectedRequirementId(requirements[0]?.id || null);
  }, [requirements, selectedRequirementId]);

  const selectedRequirement = useMemo(
    () => requirements.find((requirement) => requirement.id === selectedRequirementId) || null,
    [requirements, selectedRequirementId],
  );
  const rawSelectedWorkItems = selectedRequirement
    ? workItemsByRequirement.get(selectedRequirement.id) || []
    : [];
  const selectedWorkItems = useMemo(
    () => sortWorkItemsByDependencies(rawSelectedWorkItems, dependencyMaps.workItemPrerequisites),
    [dependencyMaps.workItemPrerequisites, rawSelectedWorkItems],
  );
  const selectedRequirementPrerequisites = selectedRequirement
    ? dependencyMaps.requirementPrerequisites.get(selectedRequirement.id) || []
    : [];
  const selectedRequirementDependents = selectedRequirement
    ? dependencyMaps.requirementDependents.get(selectedRequirement.id) || []
    : [];
  const selectedRequirementChildren = selectedRequirement
    ? requirementChildrenMap.get(selectedRequirement.id) || []
    : [];
  const doneWorkItemCount = workItems.filter((item) => item.status === 'done').length;
  const blockedWorkItemCount = workItems.filter((item) => item.status === 'blocked').length;

  return (
    <div className={cn('flex h-full flex-col overflow-hidden bg-background', className)}>
      <div className="flex items-center justify-between gap-3 border-b border-border px-4 py-3">
        <div className="min-w-0">
          <h2 className="text-sm font-semibold text-foreground">Plan</h2>
          <p className="mt-0.5 truncate text-xs text-muted-foreground">
            {requirements.length} 个需求 · {workItems.length} 个任务 · {countOpenItems(workItems)} 个未完成
          </p>
        </div>
        <button
          type="button"
          className="inline-flex items-center gap-1.5 rounded-md border border-border bg-background px-2.5 py-1.5 text-xs font-medium text-muted-foreground hover:bg-accent hover:text-foreground disabled:opacity-60"
          disabled={loading}
          onClick={() => {
            void loadPlan();
          }}
        >
          <RefreshCw className={cn('h-3.5 w-3.5', loading && 'animate-spin')} />
          刷新
        </button>
      </div>

      {error ? (
        <div className="border-b border-destructive/30 bg-destructive/10 px-4 py-2 text-sm text-destructive">
          {error}
        </div>
      ) : null}

      {loading && !plan ? (
        <div className="flex flex-1 items-center justify-center text-sm text-muted-foreground">
          正在加载 Plan...
        </div>
      ) : requirements.length === 0 ? (
        <div className="flex flex-1 items-center justify-center px-4 text-center">
          <div>
            <div className="mx-auto flex h-10 w-10 items-center justify-center rounded-full bg-muted text-muted-foreground">
              <ClipboardList className="h-5 w-5" />
            </div>
            <div className="mt-3 text-sm font-medium text-foreground">暂无需求</div>
            <div className="mt-1 text-xs text-muted-foreground">
              规划任务写入 Project Management 后，需求和项目任务会显示在这里。
            </div>
          </div>
        </div>
      ) : (
        <div
          className="grid min-h-0 flex-1 overflow-hidden"
          style={{ gridTemplateColumns: `${requirementPaneWidth}px minmax(0, 1fr)` }}
        >
          <aside className="flex min-h-0 flex-col border-r border-border bg-muted/10">
            <div className="shrink-0 border-b border-border bg-background/95 px-3 py-2">
              <div className="grid grid-cols-3 gap-2">
                <div className="rounded-md border border-border bg-background px-2 py-1.5">
                  <div className="text-[10px] text-muted-foreground">需求</div>
                  <div className="text-sm font-semibold text-foreground">{requirements.length}</div>
                </div>
                <div className="rounded-md border border-border bg-background px-2 py-1.5">
                  <div className="text-[10px] text-muted-foreground">完成</div>
                  <div className="text-sm font-semibold text-foreground">{doneWorkItemCount}</div>
                </div>
                <div className="rounded-md border border-border bg-background px-2 py-1.5">
                  <div className="text-[10px] text-muted-foreground">阻塞</div>
                  <div className="text-sm font-semibold text-foreground">{blockedWorkItemCount}</div>
                </div>
              </div>
            </div>
            <div className="min-h-0 flex-1 overflow-x-auto overflow-y-hidden">
              <div className="flex h-full min-w-max">
                {requirementColumns.map((column, columnIndex) => (
                  <section
                    key={column.id}
                    className="flex h-full shrink-0 flex-col border-r border-border/80 bg-background"
                    style={{ width: REQUIREMENT_COLUMN_WIDTH }}
                  >
                    <div className="flex h-10 shrink-0 items-center gap-1.5 border-b border-border/80 px-3">
                      {columnIndex === 0 ? (
                        <GitBranch className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
                      ) : null}
                      <span className="min-w-0 truncate text-xs font-semibold text-foreground">
                        {column.title}
                      </span>
                      <span className="ml-auto rounded-full border border-border bg-muted/20 px-1.5 py-0.5 text-[10px] text-muted-foreground">
                        {column.items.length}
                      </span>
                    </div>
                    <div className="min-h-0 flex-1 space-y-1 overflow-y-auto p-2">
                      {column.items.map((requirement) => {
                        const tasks = workItemsByRequirement.get(requirement.id) || [];
                        const prerequisiteCount = dependencyMaps.requirementPrerequisites.get(requirement.id)?.length || 0;
                        const children = requirementChildrenMap.get(requirement.id) || [];
                        const active = requirement.id === selectedRequirementId;
                        const inPath = requirementPath.includes(requirement.id);
                        return (
                          <button
                            key={requirement.id}
                            type="button"
                            className={cn(
                              'w-full rounded-md border px-2.5 py-2 text-left transition-colors',
                              active
                                ? 'border-primary/40 bg-primary/10 shadow-sm'
                                : inPath
                                  ? 'border-primary/20 bg-accent/70'
                                  : 'border-transparent hover:border-border hover:bg-accent/50',
                            )}
                            onClick={() => setSelectedRequirementId(requirement.id)}
                          >
                            <div className="flex items-center gap-1.5">
                              <span className="min-w-0 flex-1 truncate text-sm font-medium text-foreground">
                                {requirement.title || requirement.id}
                              </span>
                              <span className="shrink-0 rounded-full border border-border bg-background px-1.5 py-0.5 text-[10px] text-muted-foreground">
                                {tasks.length}
                              </span>
                              {children.length > 0 ? (
                                <ChevronRight className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
                              ) : null}
                            </div>
                            <div className="mt-1.5 flex flex-wrap gap-1.5">
                              <span className={cn(
                                'rounded-full border px-1.5 py-0.5 text-[10px] font-medium',
                                statusClassName(requirement.status),
                              )}
                              >
                                {statusLabel(requirement.status)}
                              </span>
                              <span className="rounded-full border border-border bg-background px-1.5 py-0.5 text-[10px] text-muted-foreground">
                                {requirementTypeLabel(requirement.requirement_type || requirement.requirementType)}
                              </span>
                              <span className="rounded-full border border-border bg-background px-1.5 py-0.5 text-[10px] text-muted-foreground">
                                {priorityLabel(requirement.priority)}
                              </span>
                              {prerequisiteCount > 0 ? (
                                <span className="rounded-full border border-amber-200 bg-amber-50 px-1.5 py-0.5 text-[10px] font-medium text-amber-700 dark:border-amber-800 dark:bg-amber-950/30 dark:text-amber-300">
                                  前置 {prerequisiteCount}
                                </span>
                              ) : null}
                              {children.length > 0 ? (
                                <span className="rounded-full border border-blue-200 bg-blue-50 px-1.5 py-0.5 text-[10px] font-medium text-blue-700 dark:border-blue-800 dark:bg-blue-950/30 dark:text-blue-300">
                                  子需求 {children.length}
                                </span>
                              ) : null}
                            </div>
                            {readText(requirement.summary) ? (
                              <div className="mt-1 line-clamp-2 text-xs leading-4 text-muted-foreground">
                                {requirement.summary}
                              </div>
                            ) : null}
                          </button>
                        );
                      })}
                    </div>
                  </section>
                ))}
              </div>
            </div>
          </aside>

          <main className="min-h-0 overflow-y-auto px-5 py-4">
            {selectedRequirement ? (
              <div className="mx-auto max-w-5xl">
                <div className="flex flex-wrap items-start justify-between gap-3">
                  <div className="min-w-0">
                    <div className="flex flex-wrap items-center gap-2 text-xs">
                      <span className={cn(
                        'rounded-full border px-2 py-0.5 font-medium',
                        statusClassName(selectedRequirement.status),
                      )}
                      >
                        {statusLabel(selectedRequirement.status)}
                      </span>
                      <span className="rounded-full border border-border bg-muted/20 px-2 py-0.5 text-muted-foreground">
                        {requirementTypeLabel(selectedRequirement.requirement_type || selectedRequirement.requirementType)}
                      </span>
                      <span className="rounded-full border border-border bg-muted/20 px-2 py-0.5 text-muted-foreground">
                        {priorityLabel(selectedRequirement.priority)}
                      </span>
                    </div>
                    <h3 className="mt-2 break-words text-lg font-semibold leading-7 text-foreground">
                      {selectedRequirement.title || selectedRequirement.id}
                    </h3>
                    <div className="mt-1 text-xs text-muted-foreground">
                      更新于 {formatDateTime(getUpdatedAt(selectedRequirement))}
                    </div>
                  </div>
                </div>

                <div className="mt-4 rounded-md border border-border bg-muted/10 px-3 py-3">
                  <div className="mb-2 flex items-center gap-2 text-xs font-semibold text-foreground">
                    <Link2 className="h-3.5 w-3.5 text-muted-foreground" />
                    需求前置关系
                  </div>
                  <div className="space-y-1.5">
                    <DependencyLine
                      ids={selectedRequirementPrerequisites}
                      label="前置需求"
                      resolveLabel={resolveRequirementTitle}
                    />
                    {selectedRequirementDependents.length > 0 ? (
                      <DependencyLine
                        ids={selectedRequirementDependents}
                        label="后续需求"
                        resolveLabel={resolveRequirementTitle}
                        tone="dependent"
                      />
                    ) : null}
                    {selectedRequirementChildren.length > 0 ? (
                      <DependencyLine
                        ids={selectedRequirementChildren.map((requirement) => requirement.id)}
                        label="子需求"
                        resolveLabel={resolveRequirementTitle}
                        tone="dependent"
                      />
                    ) : null}
                  </div>
                </div>

                <div className="mt-4">
                  <RequirementContentSection title="摘要" content={selectedRequirement.summary} />
                  <RequirementContentSection title="详细说明" content={selectedRequirement.detail} />
                  <RequirementContentSection title="业务价值" content={selectedRequirement.business_value || selectedRequirement.businessValue} />
                  <RequirementContentSection title="验收标准" content={selectedRequirement.acceptance_criteria || selectedRequirement.acceptanceCriteria} />
                  {!readText(selectedRequirement.summary)
                    && !readText(selectedRequirement.detail)
                    && !readText(selectedRequirement.business_value || selectedRequirement.businessValue)
                    && !readText(selectedRequirement.acceptance_criteria || selectedRequirement.acceptanceCriteria) ? (
                      <div className="rounded-md border border-border bg-muted/20 px-3 py-3 text-sm text-muted-foreground">
                        这个需求还没有补充内容。
                      </div>
                    ) : null}
                </div>

                <section className="mt-5 border-t border-border pt-4">
                  <div className="mb-3 flex flex-wrap items-center justify-between gap-2">
                    <div>
                      <h4 className="text-sm font-semibold text-foreground">任务</h4>
                      <div className="mt-0.5 text-xs text-muted-foreground">
                        {selectedWorkItems.length} 个任务 · {countOpenItems(selectedWorkItems)} 个未完成
                      </div>
                    </div>
                    {selectedWorkItems.length > 0 && countOpenItems(selectedWorkItems) === 0 ? (
                      <span className="inline-flex items-center gap-1 rounded-full border border-emerald-200 bg-emerald-50 px-2 py-0.5 text-xs font-medium text-emerald-700 dark:border-emerald-800 dark:bg-emerald-950/30 dark:text-emerald-300">
                        <CheckCircle2 className="h-3.5 w-3.5" />
                        已全部完成
                      </span>
                    ) : null}
                  </div>
                  {selectedWorkItems.length > 0 ? (
                    <div className="mb-3 flex items-start gap-2 rounded-md border border-border bg-muted/10 px-3 py-2 text-xs text-muted-foreground">
                      <ArrowRight className="mt-0.5 h-3.5 w-3.5 shrink-0" />
                      <span>任务已按前置关系尽量排序；“前置任务”是当前任务开始前需要先完成的任务。</span>
                    </div>
                  ) : null}
                  {selectedWorkItems.length === 0 ? (
                    <div className="rounded-md border border-amber-200 bg-amber-50 px-3 py-3 text-sm text-amber-800 dark:border-amber-800 dark:bg-amber-950/30 dark:text-amber-200">
                      <div className="flex items-center gap-2 font-medium">
                        <AlertCircle className="h-4 w-4" />
                        这个需求下面还没有任务
                      </div>
                    </div>
                  ) : (
                    <div className="space-y-2">
                      {selectedWorkItems.map((item) => (
                        <WorkItemRow
                          key={item.id}
                          item={item}
                          prerequisites={dependencyMaps.workItemPrerequisites.get(item.id) || []}
                          dependents={dependencyMaps.workItemDependents.get(item.id) || []}
                          resolveWorkItemTitle={resolveWorkItemTitle}
                        />
                      ))}
                    </div>
                  )}
                </section>
              </div>
            ) : null}
          </main>
        </div>
      )}
    </div>
  );
};

export default ProjectPlanPane;
