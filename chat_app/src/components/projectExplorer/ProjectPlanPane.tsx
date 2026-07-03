// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React, { useCallback, useEffect, useMemo, useState } from 'react';
import {
  AlertCircle,
  ArrowRight,
  ChevronRight,
  CheckCircle2,
  ClipboardList,
  Eye,
  FileText,
  GitBranch,
  Link2,
  ListChecks,
  Play,
  Square,
} from 'lucide-react';

import { useApiClient } from '../../lib/api/ApiClientContext';
import { useChatStore } from '../../lib/store';
import { normalizeRawMessages } from '../../lib/domain/messages';
import type {
  ProjectDependencyGraphResponse,
  ProjectPlanResponse,
  ProjectRequirementDocumentResponse,
  ProjectRequirementWorkItemsResponse,
  ProjectRequirementResponse,
  ProjectWorkItemCountsResponse,
  ProjectWorkItemResponse,
} from '../../lib/api/client/types';
import { cn } from '../../lib/utils';
import type { Project } from '../../types';
import {
  DependencyLine,
  PlanBannerMessages,
  PlanEmptyState,
  PlanLoadingState,
  PlanPaneHeader,
  PlanStatsBar,
  RequirementExecutionPreviewModal,
  RequirementContentSection,
  TechnicalDocumentsSection,
  WorkItemRow,
} from './projectPlanPane/components';
import {
  MAX_REQUIREMENT_PANE_WIDTH,
  REQUIREMENT_COLUMN_WIDTH,
  SELECTED_WORK_ITEM_INITIAL_RENDER_LIMIT,
  SELECTED_WORK_ITEM_RENDER_INCREMENT,
  buildDependencyMaps,
  buildDependencyMapsFromGraph,
  buildRequirementExecutionScope,
  buildRequirementChildrenMap,
  buildRequirementColumns,
  buildRequirementPath,
  buildVisiblePlanItems,
  canShowRequirementExecutionAction,
  countOpenItems,
  formatDateTime,
  getUpdatedAt,
  mergeDependencyMaps,
  priorityLabel,
  readText,
  requirementTypeLabel,
  sortWorkItemsByDependencies,
  statusClassName,
  statusLabel,
} from './projectPlanPane/model';

interface ProjectPlanPaneProps {
  project: Project;
  className?: string;
}

type DetailTabId = 'requirement' | 'documents' | 'tasks';

const normalizeRequirementWorkItemsResponse = (
  response: ProjectRequirementWorkItemsResponse | ProjectWorkItemResponse[],
): {
  dependencyGraph: ProjectDependencyGraphResponse | null;
  workItems: ProjectWorkItemResponse[];
} => {
  if (Array.isArray(response)) {
    return { dependencyGraph: null, workItems: response };
  }
  return {
    dependencyGraph: response.dependencyGraph || response.dependency_graph || null,
    workItems: Array.isArray(response.workItems) ? response.workItems : (response.work_items || []),
  };
};

const planWorkItemCounts = (plan: ProjectPlanResponse | null): ProjectWorkItemCountsResponse | null => (
  plan?.workItemCounts || plan?.work_item_counts || null
);

const relationPreviewText = (
  ids: string[],
  resolveLabel: (id: string) => string,
  limit = 2,
): string => {
  const visible = ids.slice(0, limit).map(resolveLabel);
  const hiddenCount = ids.length - visible.length;
  return hiddenCount > 0
    ? `${visible.join('、')} 等 ${ids.length} 个`
    : visible.join('、');
};

export const ProjectPlanPane: React.FC<ProjectPlanPaneProps> = ({ project, className }) => {
  const apiClient = useApiClient();
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [plan, setPlan] = useState<ProjectPlanResponse | null>(null);
  const [workItemsByRequirement, setWorkItemsByRequirement] = useState<Map<string, ProjectWorkItemResponse[]>>(() => new Map());
  const [workItemGraphsByRequirement, setWorkItemGraphsByRequirement] = useState<Map<string, ProjectDependencyGraphResponse>>(() => new Map());
  const [documentsByRequirement, setDocumentsByRequirement] = useState<Map<string, ProjectRequirementDocumentResponse[]>>(() => new Map());
  const [loadingWorkItemsRequirementId, setLoadingWorkItemsRequirementId] = useState<string | null>(null);
  const [loadingDocumentsRequirementId, setLoadingDocumentsRequirementId] = useState<string | null>(null);
  const [selectedRequirementId, setSelectedRequirementId] = useState<string | null>(null);
  const [activeDetailTab, setActiveDetailTab] = useState<DetailTabId>('requirement');
  const [executingRequirementId, setExecutingRequirementId] = useState<string | null>(null);
  const [executionPreviewRequirement, setExecutionPreviewRequirement] = useState<ProjectRequirementResponse | null>(null);
  const [executionPreviewCanConfirm, setExecutionPreviewCanConfirm] = useState(false);
  const [executionMessage, setExecutionMessage] = useState<string | null>(null);
  const [visibleWorkItemLimit, setVisibleWorkItemLimit] = useState(SELECTED_WORK_ITEM_INITIAL_RENDER_LIMIT);
  const refreshSessionById = useChatStore((state) => state.refreshSessionById);
  const selectSession = useChatStore((state) => state.selectSession);
  const upsertSessionMessage = useChatStore((state) => state.upsertSessionMessage);

  const loadPlan = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const result = await apiClient.getProjectPlan(project.id, { includeWorkItems: false });
      setPlan(result);
      setWorkItemsByRequirement(new Map());
      setWorkItemGraphsByRequirement(new Map());
      setDocumentsByRequirement(new Map());
    } catch (err) {
      setError(err instanceof Error ? err.message : '加载 Plan 失败');
    } finally {
      setLoading(false);
    }
  }, [apiClient, project.id]);

  useEffect(() => {
    setPlan(null);
    setWorkItemsByRequirement(new Map());
    setWorkItemGraphsByRequirement(new Map());
    setDocumentsByRequirement(new Map());
    setLoadingWorkItemsRequirementId(null);
    setLoadingDocumentsRequirementId(null);
    setSelectedRequirementId(null);
    void loadPlan();
  }, [loadPlan]);

  const loadRequirementWorkItems = useCallback(async (requirementId: string, force = false) => {
    if (!force && workItemsByRequirement.has(requirementId)) {
      return;
    }

    setLoadingWorkItemsRequirementId(requirementId);
    setError(null);
    try {
      const response = await apiClient.listProjectRequirementWorkItems(project.id, requirementId, {
        includeDependencyGraph: true,
      });
      const normalized = normalizeRequirementWorkItemsResponse(response);
      setWorkItemsByRequirement((current) => {
        const next = new Map(current);
        next.set(requirementId, normalized.workItems);
        return next;
      });
      setWorkItemGraphsByRequirement((current) => {
        const next = new Map(current);
        if (normalized.dependencyGraph) {
          next.set(requirementId, normalized.dependencyGraph);
        } else {
          next.delete(requirementId);
        }
        return next;
      });
    } catch (err) {
      setError(err instanceof Error ? err.message : '加载项目任务失败');
    } finally {
      setLoadingWorkItemsRequirementId((current) => (current === requirementId ? null : current));
    }
  }, [apiClient, project.id, workItemsByRequirement]);

  const loadRequirementDocuments = useCallback(async (requirementId: string, force = false) => {
    if (!force && documentsByRequirement.has(requirementId)) {
      return;
    }

    setLoadingDocumentsRequirementId(requirementId);
    setError(null);
    try {
      const documents = await apiClient.listProjectRequirementDocuments(project.id, requirementId);
      setDocumentsByRequirement((current) => {
        const next = new Map(current);
        next.set(requirementId, Array.isArray(documents) ? documents : []);
        return next;
      });
    } catch (err) {
      setError(err instanceof Error ? err.message : '加载技术文档失败');
    } finally {
      setLoadingDocumentsRequirementId((current) => (current === requirementId ? null : current));
    }
  }, [apiClient, documentsByRequirement, project.id]);

  const executeRequirement = useCallback(async (
    requirement: ProjectRequirementResponse,
    options?: { includePrerequisiteDependents?: boolean },
  ) => {
    if (executingRequirementId) {
      return;
    }
    setExecutingRequirementId(requirement.id);
    setExecutionMessage(null);
    setError(null);
    try {
      const result = await apiClient.executeProjectRequirement(project.id, requirement.id, {
        include_prerequisite_dependents: Boolean(options?.includePrerequisiteDependents),
      });
      const createdTasks = result.created_tasks || result.createdTasks || [];
      await loadPlan();
      const conversationId = readText(result.conversation_id) || readText(result.conversationId);
      if (!conversationId) {
        setExecutionMessage(`已创建 ${createdTasks.length} 个执行任务，但后端没有返回执行会话`);
        return;
      }
      try {
        await refreshSessionById(conversationId);
        const [executionMessage] = result.message
          ? normalizeRawMessages([result.message], conversationId)
          : [];
        await selectSession(conversationId, {
          initialPageSize: 25,
          forceRefreshMessages: true,
        });
        if (result.message) {
          if (executionMessage) {
            upsertSessionMessage(executionMessage);
          }
        }
        setExecutionMessage(`已创建 ${createdTasks.length} 个执行任务，已打开执行会话`);
      } catch (navigationErr) {
        setExecutionMessage(`已创建 ${createdTasks.length} 个执行任务`);
        setError(navigationErr instanceof Error
          ? `执行任务已创建，但打开执行会话失败：${navigationErr.message}`
          : '执行任务已创建，但打开执行会话失败');
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : '执行需求失败');
    } finally {
      setExecutingRequirementId(null);
    }
  }, [apiClient, executingRequirementId, loadPlan, project.id, refreshSessionById, selectSession, upsertSessionMessage]);

  const stopRequirementExecution = useCallback(async (requirement: ProjectRequirementResponse) => {
    if (executingRequirementId) {
      return;
    }
    setExecutingRequirementId(requirement.id);
    setExecutionMessage(null);
    setError(null);
    try {
      const result = await apiClient.stopProjectRequirementExecution(project.id, requirement.id);
      const cancelledTasks = result.cancelled_tasks || result.cancelledTasks || [];
      setExecutionMessage(`已停止 ${cancelledTasks.length} 个执行任务`);
      await loadPlan();
    } catch (err) {
      setError(err instanceof Error ? err.message : '停止需求执行失败');
    } finally {
      setExecutingRequirementId(null);
    }
  }, [apiClient, executingRequirementId, loadPlan, project.id]);

  const requirements = useMemo(
    () => (Array.isArray(plan?.requirements) ? plan.requirements : []),
    [plan?.requirements],
  );
  const planWorkItems = useMemo(
    () => (Array.isArray(plan?.workItems) ? plan.workItems : (plan?.work_items || [])),
    [plan?.workItems, plan?.work_items],
  );
  const loadedWorkItems = useMemo(() => {
    const items: ProjectWorkItemResponse[] = [];
    workItemsByRequirement.forEach((requirementItems) => {
      items.push(...requirementItems);
    });
    return items;
  }, [workItemsByRequirement]);
  const workItems = planWorkItems.length > 0 ? planWorkItems : loadedWorkItems;
  const selectedWorkItemGraph = selectedRequirementId
    ? workItemGraphsByRequirement.get(selectedRequirementId) || null
    : null;
  const planDependencyMaps = useMemo(() => buildDependencyMaps(plan), [plan]);
  const selectedWorkItemDependencyMaps = useMemo(
    () => buildDependencyMapsFromGraph(selectedWorkItemGraph),
    [selectedWorkItemGraph],
  );
  const dependencyMaps = useMemo(
    () => mergeDependencyMaps(planDependencyMaps, selectedWorkItemDependencyMaps),
    [planDependencyMaps, selectedWorkItemDependencyMaps],
  );
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
  useEffect(() => {
    if (!selectedRequirement) {
      return;
    }
    void loadRequirementWorkItems(selectedRequirement.id);
    void loadRequirementDocuments(selectedRequirement.id);
  }, [loadRequirementDocuments, loadRequirementWorkItems, selectedRequirement]);
  const selectedRequirementIsExecuting = selectedRequirement?.status === 'in_progress';
  const selectedRequirementCanShowAction = Boolean(
    selectedRequirement && canShowRequirementExecutionAction(selectedRequirement.status),
  );
  const selectedRequirementActionBusy = Boolean(
    selectedRequirement && executingRequirementId === selectedRequirement.id,
  );
  const selectedRequirementWorkItemsLoaded = selectedRequirement
    ? workItemsByRequirement.has(selectedRequirement.id)
    : false;
  const selectedWorkItemsLoading = Boolean(
    selectedRequirement
      && loadingWorkItemsRequirementId === selectedRequirement.id
      && !selectedRequirementWorkItemsLoaded,
  );
  const rawSelectedWorkItems = selectedRequirement
    ? workItemsByRequirement.get(selectedRequirement.id) || []
    : [];
  const selectedRequirementDocumentsLoaded = selectedRequirement
    ? documentsByRequirement.has(selectedRequirement.id)
    : false;
  const selectedDocumentsLoading = Boolean(
    selectedRequirement
      && loadingDocumentsRequirementId === selectedRequirement.id
      && !selectedRequirementDocumentsLoaded,
  );
  const selectedRequirementDocuments = selectedRequirement
    ? documentsByRequirement.get(selectedRequirement.id) || []
    : [];
  const selectedWorkItems = useMemo(
    () => sortWorkItemsByDependencies(rawSelectedWorkItems, dependencyMaps.workItemPrerequisites),
    [dependencyMaps.workItemPrerequisites, rawSelectedWorkItems],
  );
  useEffect(() => {
    setVisibleWorkItemLimit(SELECTED_WORK_ITEM_INITIAL_RENDER_LIMIT);
  }, [selectedRequirementId]);
  const visibleSelectedWorkItems = useMemo(
    () => buildVisiblePlanItems(selectedWorkItems, visibleWorkItemLimit),
    [selectedWorkItems, visibleWorkItemLimit],
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
  const selectedExecutionScopeIds = useMemo(
    () => buildRequirementExecutionScope({
      dependencyMaps,
      requirements,
      rootId: selectedRequirementId,
    }),
    [dependencyMaps, requirements, selectedRequirementId],
  );
  const selectedExecutionScopeRelatedIds = selectedRequirement
    ? selectedExecutionScopeIds.filter((id) => id !== selectedRequirement.id)
    : [];
  const workItemCounts = planWorkItemCounts(plan);
  const totalWorkItemCount = typeof workItemCounts?.total === 'number'
    ? workItemCounts.total
    : workItems.length;
  const openWorkItemCount = typeof workItemCounts?.open === 'number'
    ? workItemCounts.open
    : countOpenItems(workItems);
  const doneWorkItemCount = typeof workItemCounts?.done === 'number'
    ? workItemCounts.done
    : workItems.filter((item) => item.status === 'done').length;
  const blockedWorkItemCount = typeof workItemCounts?.blocked === 'number'
    ? workItemCounts.blocked
    : workItems.filter((item) => item.status === 'blocked').length;

  return (
    <div className={cn('flex h-full flex-col overflow-hidden bg-background', className)}>
      <PlanPaneHeader
        loading={loading}
        onRefresh={() => {
          void loadPlan();
        }}
        openItemCount={openWorkItemCount}
        requirementCount={requirements.length}
        workItemCount={totalWorkItemCount}
      />

      <PlanBannerMessages error={error} executionMessage={executionMessage} />

      {loading && !plan ? (
        <PlanLoadingState />
      ) : requirements.length === 0 ? (
        <PlanEmptyState />
      ) : (
        <div
          className="grid min-h-0 flex-1 overflow-hidden"
          style={{ gridTemplateColumns: `${requirementPaneWidth}px minmax(0, 1fr)` }}
        >
          <aside className="flex min-h-0 flex-col border-r border-border bg-muted/10">
            <PlanStatsBar
              blockedWorkItemCount={blockedWorkItemCount}
              doneWorkItemCount={doneWorkItemCount}
              requirementCount={requirements.length}
            />
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
                        const tasks = workItemsByRequirement.get(requirement.id);
                        const prerequisiteIds = dependencyMaps.requirementPrerequisites.get(requirement.id) || [];
                        const dependentIds = dependencyMaps.requirementDependents.get(requirement.id) || [];
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
                                {tasks ? tasks.length : '-'}
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
                              {prerequisiteIds.length > 0 ? (
                                <span className="rounded-full border border-amber-200 bg-amber-50 px-1.5 py-0.5 text-[10px] font-medium text-amber-700 dark:border-amber-800 dark:bg-amber-950/30 dark:text-amber-300">
                                  前置 {prerequisiteIds.length}
                                </span>
                              ) : null}
                              {dependentIds.length > 0 ? (
                                <span className="rounded-full border border-blue-200 bg-blue-50 px-1.5 py-0.5 text-[10px] font-medium text-blue-700 dark:border-blue-800 dark:bg-blue-950/30 dark:text-blue-300">
                                  后续 {dependentIds.length}
                                </span>
                              ) : null}
                              {children.length > 0 ? (
                                <span className="rounded-full border border-blue-200 bg-blue-50 px-1.5 py-0.5 text-[10px] font-medium text-blue-700 dark:border-blue-800 dark:bg-blue-950/30 dark:text-blue-300">
                                  子需求 {children.length}
                                </span>
                              ) : null}
                            </div>
                            {prerequisiteIds.length > 0 ? (
                              <div className="mt-1.5 min-w-0 text-[11px] leading-4 text-amber-700 dark:text-amber-300">
                                <span className="font-medium">前置：</span>
                                <span className="break-words">
                                  {relationPreviewText(prerequisiteIds, resolveRequirementTitle)}
                                </span>
                              </div>
                            ) : null}
                            {dependentIds.length > 0 ? (
                              <div className="mt-1 min-w-0 text-[11px] leading-4 text-blue-700 dark:text-blue-300">
                                <span className="font-medium">后续：</span>
                                <span className="break-words">
                                  {relationPreviewText(dependentIds, resolveRequirementTitle)}
                                </span>
                              </div>
                            ) : null}
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
                  <div className="flex shrink-0 flex-wrap items-center gap-2">
                    <button
                      type="button"
                      className="inline-flex items-center gap-1.5 rounded-md border border-border bg-background px-3 py-1.5 text-xs font-medium text-muted-foreground shadow-sm hover:bg-accent hover:text-foreground"
                      onClick={() => {
                        setExecutionPreviewCanConfirm(false);
                        setExecutionPreviewRequirement(selectedRequirement);
                      }}
                    >
                      <Eye className="h-3.5 w-3.5" />
                      预览流程
                    </button>
                    {selectedRequirementCanShowAction ? (
                      <button
                        type="button"
                        className={cn(
                          'inline-flex items-center gap-1.5 rounded-md border px-3 py-1.5 text-xs font-medium shadow-sm disabled:cursor-not-allowed disabled:border-border disabled:bg-muted disabled:text-muted-foreground disabled:shadow-none',
                          selectedRequirementIsExecuting
                            ? 'border-destructive/40 bg-destructive text-destructive-foreground hover:bg-destructive/90'
                            : 'border-primary/40 bg-primary text-primary-foreground hover:bg-primary/90',
                        )}
                        disabled={!!executingRequirementId}
                        onClick={() => {
                          if (selectedRequirementIsExecuting) {
                            void stopRequirementExecution(selectedRequirement);
                          } else {
                            setExecutionPreviewCanConfirm(true);
                            setExecutionPreviewRequirement(selectedRequirement);
                          }
                        }}
                      >
                        {selectedRequirementIsExecuting ? <Square className="h-3.5 w-3.5" /> : <Play className="h-3.5 w-3.5" />}
                        {selectedRequirementActionBusy
                          ? (selectedRequirementIsExecuting ? '停止中' : '执行中')
                          : (selectedRequirementIsExecuting ? '停止' : '执行关联任务')}
                      </button>
                    ) : null}
                  </div>
                </div>

                <div className="mt-4 border-b border-border">
                  <div className="flex gap-1 overflow-x-auto" role="tablist" aria-label="需求详情">
                    <button
                      type="button"
                      role="tab"
                      aria-selected={activeDetailTab === 'requirement'}
                      className={cn(
                        'inline-flex h-9 shrink-0 items-center gap-1.5 border-b-2 px-3 text-xs font-medium transition-colors',
                        activeDetailTab === 'requirement'
                          ? 'border-primary text-foreground'
                          : 'border-transparent text-muted-foreground hover:text-foreground',
                      )}
                      onClick={() => setActiveDetailTab('requirement')}
                    >
                      <ClipboardList className="h-3.5 w-3.5" />
                      需求
                    </button>
                    <button
                      type="button"
                      role="tab"
                      aria-selected={activeDetailTab === 'documents'}
                      className={cn(
                        'inline-flex h-9 shrink-0 items-center gap-1.5 border-b-2 px-3 text-xs font-medium transition-colors',
                        activeDetailTab === 'documents'
                          ? 'border-primary text-foreground'
                          : 'border-transparent text-muted-foreground hover:text-foreground',
                      )}
                      onClick={() => setActiveDetailTab('documents')}
                    >
                      <FileText className="h-3.5 w-3.5" />
                      技术文档
                      <span className="rounded-full border border-border bg-muted/20 px-1.5 py-0.5 text-[10px] text-muted-foreground">
                        {selectedDocumentsLoading ? '...' : selectedRequirementDocuments.length}
                      </span>
                    </button>
                    <button
                      type="button"
                      role="tab"
                      aria-selected={activeDetailTab === 'tasks'}
                      className={cn(
                        'inline-flex h-9 shrink-0 items-center gap-1.5 border-b-2 px-3 text-xs font-medium transition-colors',
                        activeDetailTab === 'tasks'
                          ? 'border-primary text-foreground'
                          : 'border-transparent text-muted-foreground hover:text-foreground',
                      )}
                      onClick={() => setActiveDetailTab('tasks')}
                    >
                      <ListChecks className="h-3.5 w-3.5" />
                      任务
                      <span className="rounded-full border border-border bg-muted/20 px-1.5 py-0.5 text-[10px] text-muted-foreground">
                        {selectedWorkItemsLoading ? '...' : selectedWorkItems.length}
                      </span>
                    </button>
                  </div>
                </div>

                <div className="pt-4">
                  {activeDetailTab === 'requirement' ? (
                    <div role="tabpanel">
                      <div className="rounded-md border border-border bg-muted/10 px-3 py-3">
                        <div className="mb-2 flex items-center gap-2 text-xs font-semibold text-foreground">
                          <Link2 className="h-3.5 w-3.5 text-muted-foreground" />
                          需求关系
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
                          <DependencyLine
                            emptyLabel="仅当前需求"
                            ids={selectedExecutionScopeRelatedIds}
                            label="执行会包含"
                            resolveLabel={resolveRequirementTitle}
                            tone="dependent"
                          />
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
                    </div>
                  ) : null}

                  {activeDetailTab === 'documents' ? (
                    <div role="tabpanel">
                      <TechnicalDocumentsSection
                        className="mt-0 border-t-0 pt-0"
                        documents={selectedRequirementDocuments}
                        loading={selectedDocumentsLoading}
                      />
                    </div>
                  ) : null}

                  {activeDetailTab === 'tasks' ? (
                    <section role="tabpanel">
                      <div className="mb-3 flex flex-wrap items-center justify-between gap-2">
                        <div>
                          <h4 className="text-sm font-semibold text-foreground">项目任务</h4>
                          <div className="mt-0.5 text-xs text-muted-foreground">
                            {selectedWorkItemsLoading
                              ? '正在加载项目任务...'
                              : `${selectedWorkItems.length} 个项目任务 · ${countOpenItems(selectedWorkItems)} 个未完成`}
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
                          <span>项目任务已按前置关系尽量排序；“前置项目任务”是当前项目任务开始前需要先完成的任务。</span>
                        </div>
                      ) : null}
                      {selectedWorkItemsLoading ? (
                        <div className="rounded-md border border-border bg-muted/20 px-3 py-3 text-sm text-muted-foreground">
                          正在加载项目任务...
                        </div>
                      ) : selectedWorkItems.length === 0 ? (
                        <div className="rounded-md border border-amber-200 bg-amber-50 px-3 py-3 text-sm text-amber-800 dark:border-amber-800 dark:bg-amber-950/30 dark:text-amber-200">
                          <div className="flex items-center gap-2 font-medium">
                            <AlertCircle className="h-4 w-4" />
                            这个需求下面还没有任务
                          </div>
                        </div>
                      ) : (
                        <div className="space-y-2">
                          {visibleSelectedWorkItems.items.map((item) => (
                            <WorkItemRow
                              key={item.id}
                              item={item}
                              prerequisites={dependencyMaps.workItemPrerequisites.get(item.id) || []}
                              dependents={dependencyMaps.workItemDependents.get(item.id) || []}
                              resolveWorkItemTitle={resolveWorkItemTitle}
                            />
                          ))}
                          {visibleSelectedWorkItems.hasMore ? (
                            <button
                              type="button"
                              className="w-full rounded-md border border-border bg-background px-3 py-2 text-xs font-medium text-muted-foreground hover:bg-accent hover:text-foreground"
                              onClick={() => {
                                setVisibleWorkItemLimit((value) => value + SELECTED_WORK_ITEM_RENDER_INCREMENT);
                              }}
                            >
                              加载更多 {Math.min(
                                SELECTED_WORK_ITEM_RENDER_INCREMENT,
                                visibleSelectedWorkItems.hiddenCount,
                              )} / {visibleSelectedWorkItems.hiddenCount}
                            </button>
                          ) : null}
                        </div>
                      )}
                    </section>
                  ) : null}
                </div>
              </div>
            ) : null}
          </main>
        </div>
      )}
      {executionPreviewRequirement ? (
        <RequirementExecutionPreviewModal
          dependencyMaps={dependencyMaps}
          requirement={executionPreviewRequirement}
          requirements={requirements}
          running={Boolean(executingRequirementId)}
          onClose={() => {
            setExecutionPreviewRequirement(null);
            setExecutionPreviewCanConfirm(false);
          }}
          onConfirm={executionPreviewCanConfirm
            ? (includePrerequisiteDependents) => {
              const requirementToExecute = executionPreviewRequirement;
              setExecutionPreviewRequirement(null);
              setExecutionPreviewCanConfirm(false);
              void executeRequirement(requirementToExecute, { includePrerequisiteDependents });
            }
            : undefined}
        />
      ) : null}
    </div>
  );
};

export default ProjectPlanPane;
