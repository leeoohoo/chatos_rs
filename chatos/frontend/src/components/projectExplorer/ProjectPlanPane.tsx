// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import { useApiClient } from '../../lib/api/ApiClientContext';
import { useChatStore } from '../../lib/store';
import type {
  ProjectDependencyGraphResponse,
  ProjectPlanResponse,
  ProjectRequirementDocumentResponse,
  ProjectRequirementResponse,
  ProjectWorkItemResponse,
} from '../../lib/api/client/types';
import { cn } from '../../lib/utils';
import type { Project } from '../../types';
import {
  PlanBannerMessages,
  PlanEmptyState,
  PlanLoadingState,
  PlanPaneHeader,
  RequirementExecutionPreviewModal,
} from './projectPlanPane/components';
import { PlanRequirementColumns } from './projectPlanPane/PlanRequirementColumns';
import { PlanRequirementDetail, type DetailTabId } from './projectPlanPane/PlanRequirementDetail';
import {
  buildRequirementExecutionProcess,
  RequirementExecutionProcessModal,
  RequirementExecutionStartingModal,
  type RequirementExecutionProcess,
} from './projectPlanPane/RequirementExecutionProcessModal';
import {
  normalizeRequirementWorkItemsResponse,
  planWorkItemCounts,
} from './projectPlanPane/planResponse';
import {
  MAX_REQUIREMENT_PANE_WIDTH,
  REQUIREMENT_COLUMN_WIDTH,
  SELECTED_WORK_ITEM_INITIAL_RENDER_LIMIT,
  SELECTED_WORK_ITEM_RENDER_INCREMENT,
  buildDependencyMaps,
  buildDependencyMapsFromGraph,
  buildRequirementExecutionPayload,
  buildRequirementExecutionScope,
  buildRequirementChildrenMap,
  buildRequirementColumns,
  buildRequirementPath,
  buildVisiblePlanItems,
  canShowRequirementExecutionAction,
  countOpenItems,
  isCompletedStatus,
  mergeDependencyMaps,
  sortWorkItemsByDependencies,
} from './projectPlanPane/model';

interface ProjectPlanPaneProps {
  project: Project;
  className?: string;
}

export const isActiveRequirementExecutionConflict = (message: string): boolean => (
  message.includes('正在执行或待执行')
  || message.includes('已有执行中的任务')
  || message.includes('已有正在生成或等待确认的执行计划')
  || message.includes('已有执行中的 Task Runner 任务')
  || message.includes('already has active task runs')
  || message.includes('already has a task graph being generated')
  || message.includes('already has a generated task graph awaiting confirmation')
);

export const isTerminalRequirementExecutionStatus = (status?: string | null): boolean => (
  [
    'completed',
    'succeeded',
    'success',
    'failed',
    'error',
    'stopped',
    'cancelled',
    'canceled',
  ].includes((status || '').trim().toLowerCase())
);

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
  const [startingExecutionRequirement, setStartingExecutionRequirement] = useState<ProjectRequirementResponse | null>(null);
  const [executionProcess, setExecutionProcess] = useState<RequirementExecutionProcess | null>(null);
  const [executionProcessOpen, setExecutionProcessOpen] = useState(false);
  const [loadingExecutionPlanRequirementId, setLoadingExecutionPlanRequirementId] = useState<string | null>(null);
  const [activeExecutionBlockedRequirementId, setActiveExecutionBlockedRequirementId] = useState<string | null>(null);
  const [stoppingActiveExecution, setStoppingActiveExecution] = useState(false);
  const [executionMessage, setExecutionMessage] = useState<string | null>(null);
  const refreshedTerminalExecutionKeysRef = useRef(new Set<string>());
  const [visibleWorkItemLimit, setVisibleWorkItemLimit] = useState(SELECTED_WORK_ITEM_INITIAL_RENDER_LIMIT);
  const refreshSessionById = useChatStore((state) => state.refreshSessionById);
  const selectedModelId = useChatStore((state) => state.selectedModelId);

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
    refreshedTerminalExecutionKeysRef.current.clear();
    setPlan(null);
    setWorkItemsByRequirement(new Map());
    setWorkItemGraphsByRequirement(new Map());
    setDocumentsByRequirement(new Map());
    setLoadingWorkItemsRequirementId(null);
    setLoadingDocumentsRequirementId(null);
    setSelectedRequirementId(null);
    setStartingExecutionRequirement(null);
    setExecutionProcess(null);
    setExecutionProcessOpen(false);
    setLoadingExecutionPlanRequirementId(null);
    setActiveExecutionBlockedRequirementId(null);
    setStoppingActiveExecution(false);
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

  const openRequirementExecutionStarter = useCallback((
    requirement: ProjectRequirementResponse,
  ) => {
    setStartingExecutionRequirement(requirement);
    setExecutionProcess(null);
    setExecutionProcessOpen(true);
    setExecutionMessage(null);
    setError(null);
  }, []);

  const openExistingRequirementExecution = useCallback(() => {
    // A starter from a previously selected requirement must not win the modal
    // render precedence when the user opens an already generated plan.
    setStartingExecutionRequirement(null);
    setExecutionProcessOpen(true);
  }, []);

  const executeRequirement = useCallback(async (
    requirement: ProjectRequirementResponse,
    options?: {
      includePrerequisiteDependents?: boolean;
      planningFeedback?: string;
    },
  ) => {
    if (executingRequirementId) {
      return;
    }
    setExecutingRequirementId(requirement.id);
    setStartingExecutionRequirement(requirement);
    setExecutionMessage(null);
    setError(null);
    try {
      const result = await apiClient.executeProjectRequirement(
        project.id,
        requirement.id,
        buildRequirementExecutionPayload({
          includePrerequisiteDependents: options?.includePrerequisiteDependents,
          planningFeedback: options?.planningFeedback,
          selectedModelId,
        }),
      );
      const nextProcess = buildRequirementExecutionProcess({
        projectId: project.id,
        requirement,
        response: result,
      });
      if (!nextProcess) {
        throw new Error('后端已接受执行请求，但没有返回完整的规划会话和执行批次标识，无法安全展示执行过程');
      }
      setExecutionProcess(nextProcess);
      setStartingExecutionRequirement(null);
      setActiveExecutionBlockedRequirementId(null);
      setExecutionMessage('执行计划工作台已打开；点击“执行”前不会启动任务');
      void loadPlan();
      try {
        await refreshSessionById(nextProcess.conversationId);
      } catch (refreshErr) {
        setError(refreshErr instanceof Error
          ? `执行计划窗口已打开，但刷新规划会话失败：${refreshErr.message}`
          : '执行计划窗口已打开，但刷新规划会话失败');
      }
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : '生成执行计划失败';
      if (isActiveRequirementExecutionConflict(errorMessage)) {
        setStartingExecutionRequirement(null);
        setActiveExecutionBlockedRequirementId(requirement.id);
        try {
          const response = await apiClient.getProjectRequirementExecutionPlan(
            project.id,
            requirement.id,
          );
          const restored = buildRequirementExecutionProcess({
            projectId: project.id,
            requirement,
            response,
          });
          if (restored) {
            setExecutionProcess(restored);
            setExecutionProcessOpen(true);
            setExecutionMessage('检测到尚未停止的执行批次，可在工作台中查看并停止');
          } else {
            setExecutionProcessOpen(false);
          }
        } catch {
          setExecutionProcessOpen(false);
        }
      } else {
        setStartingExecutionRequirement(requirement);
        setExecutionProcessOpen(true);
      }
      setError(errorMessage);
    } finally {
      setExecutingRequirementId(null);
    }
  }, [apiClient, executingRequirementId, loadPlan, project.id, refreshSessionById, selectedModelId]);

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
  const stopActiveRequirementExecution = useCallback(async () => {
    if (!selectedRequirement || stoppingActiveExecution) return;
    setStoppingActiveExecution(true);
    setError(null);
    try {
      await apiClient.stopProjectRequirementExecution(
        project.id,
        selectedRequirement.id,
        {},
      );
      setActiveExecutionBlockedRequirementId(null);
      setExecutionProcess(null);
      setExecutionProcessOpen(false);
      setExecutionMessage('当前执行批次已停止，可以重新生成执行流程');
      await loadPlan();
    } catch (stopError) {
      setError(stopError instanceof Error ? stopError.message : '取消当前执行失败');
    } finally {
      setStoppingActiveExecution(false);
    }
  }, [
    apiClient,
    loadPlan,
    project.id,
    selectedRequirement,
    stoppingActiveExecution,
  ]);
  useEffect(() => {
    if (!selectedRequirement) {
      return;
    }
    void loadRequirementWorkItems(selectedRequirement.id);
    void loadRequirementDocuments(selectedRequirement.id);
  }, [loadRequirementDocuments, loadRequirementWorkItems, selectedRequirement]);
  useEffect(() => {
    if (!selectedRequirement) {
      return undefined;
    }
    let cancelled = false;
    setLoadingExecutionPlanRequirementId(selectedRequirement.id);
    void apiClient
      .getProjectRequirementExecutionPlan(project.id, selectedRequirement.id)
      .then((response) => {
        if (cancelled) return;
        setExecutionProcess((current) => {
          const restored = buildRequirementExecutionProcess({
            fallback: current?.requirement.id === selectedRequirement.id ? current : null,
            projectId: project.id,
            requirement: selectedRequirement,
            response,
          });
          return restored
            || (current?.requirement.id === selectedRequirement.id ? current : null);
        });
      })
      .catch((planError) => {
        if (!cancelled) {
          setError(planError instanceof Error
            ? `读取当前执行计划失败：${planError.message}`
            : '读取当前执行计划失败');
        }
      })
      .finally(() => {
        if (!cancelled) {
          setLoadingExecutionPlanRequirementId((current) => (
            current === selectedRequirement.id ? null : current
          ));
        }
      });
    return () => {
      cancelled = true;
    };
  }, [apiClient, project.id, selectedRequirement]);
  const selectedRequirementExecutionProcess = selectedRequirement
    && executionProcess?.requirement.id === selectedRequirement.id
    ? executionProcess
    : null;
  const selectedRequirementCanShowAction = Boolean(
    selectedRequirement && canShowRequirementExecutionAction(selectedRequirement.status),
  );
  const selectedRequirementActionBusy = Boolean(
    selectedRequirement && (
      executingRequirementId === selectedRequirement.id
      || loadingExecutionPlanRequirementId === selectedRequirement.id
    ),
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
    : workItems.filter((item) => isCompletedStatus(item.status)).length;
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

      <PlanBannerMessages
        error={error}
        executionMessage={executionMessage}
        onOpenExecutionProcess={selectedRequirementExecutionProcess
          ? openExistingRequirementExecution
          : undefined}
        onStopActiveExecution={selectedRequirement
          && activeExecutionBlockedRequirementId === selectedRequirement.id
          ? () => void stopActiveRequirementExecution()
          : undefined}
        stoppingActiveExecution={stoppingActiveExecution}
      />

      {loading && !plan ? (
        <PlanLoadingState />
      ) : requirements.length === 0 ? (
        <PlanEmptyState />
      ) : (
        <div
          className="grid min-h-0 flex-1 overflow-hidden"
          style={{ gridTemplateColumns: `${requirementPaneWidth}px minmax(0, 1fr)` }}
        >
          <PlanRequirementColumns
            blockedWorkItemCount={blockedWorkItemCount}
            dependencyMaps={dependencyMaps}
            doneWorkItemCount={doneWorkItemCount}
            onSelectRequirement={setSelectedRequirementId}
            requirementChildrenMap={requirementChildrenMap}
            requirementColumns={requirementColumns}
            requirementCount={requirements.length}
            requirementPath={requirementPath}
            resolveRequirementTitle={resolveRequirementTitle}
            selectedRequirementId={selectedRequirementId}
            workItemsByRequirement={workItemsByRequirement}
          />

          <PlanRequirementDetail
            actionDisabled={Boolean(executingRequirementId || loadingExecutionPlanRequirementId)}
            activeDetailTab={activeDetailTab}
            dependencyMaps={dependencyMaps}
            onActiveDetailTabChange={setActiveDetailTab}
            onLoadMoreWorkItems={() => {
              setVisibleWorkItemLimit((value) => value + SELECTED_WORK_ITEM_RENDER_INCREMENT);
            }}
            onGenerateRequirementExecution={(requirement) => {
              openRequirementExecutionStarter(requirement);
            }}
            onPreviewRequirement={setExecutionPreviewRequirement}
            onOpenRequirementExecution={openExistingRequirementExecution}
            resolveRequirementTitle={resolveRequirementTitle}
            resolveWorkItemTitle={resolveWorkItemTitle}
            selectedDocumentsLoading={selectedDocumentsLoading}
            selectedExecutionScopeRelatedIds={selectedExecutionScopeRelatedIds}
            selectedRequirement={selectedRequirement}
            selectedRequirementActionBusy={selectedRequirementActionBusy}
            selectedRequirementCanShowAction={selectedRequirementCanShowAction}
            selectedRequirementChildren={selectedRequirementChildren}
            selectedRequirementDependents={selectedRequirementDependents}
            selectedRequirementDocuments={selectedRequirementDocuments}
            selectedRequirementExecutionProcess={selectedRequirementExecutionProcess}
            selectedRequirementPrerequisites={selectedRequirementPrerequisites}
            selectedWorkItems={selectedWorkItems}
            selectedWorkItemsLoading={selectedWorkItemsLoading}
            visibleSelectedWorkItems={visibleSelectedWorkItems}
          />
        </div>
      )}
      {executionPreviewRequirement ? (
        <RequirementExecutionPreviewModal
          dependencyMaps={dependencyMaps}
          requirement={executionPreviewRequirement}
          requirements={requirements}
          running={Boolean(executingRequirementId)}
          onClose={() => setExecutionPreviewRequirement(null)}
        />
      ) : null}
      {startingExecutionRequirement && executionProcessOpen ? (
        <RequirementExecutionStartingModal
          requirement={startingExecutionRequirement}
          executionPlane={project.executionPlane}
          starting={executingRequirementId === startingExecutionRequirement.id}
          onClose={() => {
            setStartingExecutionRequirement(null);
            setExecutionProcessOpen(false);
          }}
          onStart={(planningFeedback) => {
            void executeRequirement(startingExecutionRequirement, { planningFeedback });
          }}
        />
      ) : executionProcess && executionProcessOpen ? (
        <RequirementExecutionProcessModal
          process={executionProcess}
          clientManagedRuntime={project.executionPlane === 'local_connector'
            || project.sourceType === 'local'
            || project.sourceType === 'local_connector'}
          onClose={() => setExecutionProcessOpen(false)}
          onProcessChange={(nextProcess) => {
            setExecutionProcess(nextProcess);
            setExecutionProcessOpen(true);
            if (isTerminalRequirementExecutionStatus(nextProcess.serverStatus)) {
              const refreshKey = `${nextProcess.executionGroupId}:${nextProcess.serverStatus}`;
              if (!refreshedTerminalExecutionKeysRef.current.has(refreshKey)) {
                refreshedTerminalExecutionKeysRef.current.add(refreshKey);
                void loadPlan();
              }
            }
          }}
        />
      ) : null}
    </div>
  );
};

export default ProjectPlanPane;
