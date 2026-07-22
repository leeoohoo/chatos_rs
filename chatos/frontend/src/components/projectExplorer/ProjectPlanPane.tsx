// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React, { useCallback, useEffect, useMemo, useState } from 'react';

import { useApiClient } from '../../lib/api/ApiClientContext';
import { useChatStore } from '../../lib/store';
import { normalizeRawMessages } from '../../lib/domain/messages';
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
  readText,
  sortWorkItemsByDependencies,
} from './projectPlanPane/model';

interface ProjectPlanPaneProps {
  project: Project;
  className?: string;
}

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
  const selectedModelId = useChatStore((state) => state.selectedModelId);
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
      const result = await apiClient.executeProjectRequirement(
        project.id,
        requirement.id,
        buildRequirementExecutionPayload({
          includePrerequisiteDependents: options?.includePrerequisiteDependents,
          selectedModelId,
        }),
      );
      await loadPlan();
      const conversationId = readText(result.conversation_id) || readText(result.conversationId);
      if (!conversationId) {
        setExecutionMessage('需求执行规划已启动，但后端没有返回执行会话');
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
        setExecutionMessage('需求执行规划已启动，已打开执行会话');
      } catch (navigationErr) {
        setExecutionMessage('需求执行规划已启动');
        setError(navigationErr instanceof Error
          ? `需求执行规划已启动，但打开执行会话失败：${navigationErr.message}`
          : '需求执行规划已启动，但打开执行会话失败');
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : '执行需求失败');
    } finally {
      setExecutingRequirementId(null);
    }
  }, [apiClient, executingRequirementId, loadPlan, project.id, refreshSessionById, selectSession, selectedModelId, upsertSessionMessage]);

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
            actionDisabled={Boolean(executingRequirementId)}
            activeDetailTab={activeDetailTab}
            dependencyMaps={dependencyMaps}
            onActiveDetailTabChange={setActiveDetailTab}
            onLoadMoreWorkItems={() => {
              setVisibleWorkItemLimit((value) => value + SELECTED_WORK_ITEM_RENDER_INCREMENT);
            }}
            onPreviewRequirement={(requirement, canConfirm) => {
              setExecutionPreviewCanConfirm(canConfirm);
              setExecutionPreviewRequirement(requirement);
            }}
            onStopRequirementExecution={(requirement) => {
              void stopRequirementExecution(requirement);
            }}
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
            selectedRequirementIsExecuting={selectedRequirementIsExecuting}
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
