// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useMemo } from 'react';
import { useQueries, useQuery } from '@tanstack/react-query';

import { api } from '../../api/client';
import type { TranslateFn } from '../../i18n/I18nProvider';
import type {
  ExternalMcpConfigRecord,
  RemoteServerRecord,
  TaskProjectRecord,
  TaskRecord,
  TaskRunEventRecord,
  TaskRunStatus,
  TaskScheduleMode,
  TaskStatus,
} from '../../types';
import type {
  TaskMemoryRoleFilter,
  TaskMemorySummaryFilter,
} from './TaskMemoryDrawer';
import {
  collectTaskRemoteOperations,
  scheduleModeLabelKeys,
  statusFilterValues,
  summarizeTaskRemoteOperations,
  taskModelOptionLabel,
  taskRunReportContent,
} from './taskPageUtils';
import type { TaskRowRemoteActivity } from './taskTableColumns';

type UseTasksPageDataParams = {
  t: TranslateFn;
  statusFilter: 'all' | TaskStatus;
  keywordFilter: string;
  tagFilter?: string;
  routeModelConfigId?: string;
  routeProjectId?: string;
  scheduledOnly: boolean;
  taskPage: number;
  taskPageSize: number;
  detailTaskId: string | null;
  detailTaskPreview: TaskRecord | null;
  memoryTask: TaskRecord | null;
  memoryRoleFilter: TaskMemoryRoleFilter;
  memorySummaryFilter: TaskMemorySummaryFilter;
  memoryLimit: number;
  mcpPreviewTask: TaskRecord | null;
  batchRunTaskIds: string[];
  editingTaskId?: string;
};

function normalizeProjectId(value?: string | null) {
  const trimmed = value?.trim();
  return trimmed && trimmed !== '0' ? trimmed : '-1';
}

const ACTIVE_TASK_REFRESH_INTERVAL_MS = 2500;
const activeTaskStatuses = new Set<TaskStatus>(['queued', 'running']);
const activeRunStatuses = new Set<TaskRunStatus>(['queued', 'running']);

function activeRefreshInterval(active: boolean) {
  return active ? ACTIVE_TASK_REFRESH_INTERVAL_MS : false;
}

function isActiveTaskStatus(status?: TaskStatus | null) {
  return Boolean(status && activeTaskStatuses.has(status));
}

function isActiveRunStatus(status?: TaskRunStatus | null) {
  return Boolean(status && activeRunStatuses.has(status));
}

function taskPageHasActiveItems(data?: { items?: TaskRecord[] } | null) {
  return Boolean(data?.items?.some((task) => isActiveTaskStatus(task.status)));
}

export function useTasksPageData({
  t,
  statusFilter,
  keywordFilter,
  tagFilter,
  routeModelConfigId,
  routeProjectId,
  scheduledOnly,
  taskPage,
  taskPageSize,
  detailTaskId,
  detailTaskPreview,
  memoryTask,
  memoryRoleFilter,
  memorySummaryFilter,
  memoryLimit,
  mcpPreviewTask,
  batchRunTaskIds,
  editingTaskId,
}: UseTasksPageDataParams) {
  const scheduleModeLabels = useMemo(
    () =>
      Object.fromEntries(
        (['manual', 'once', 'interval', 'contact_async'] as TaskScheduleMode[]).map((value) => [
          value,
          t(scheduleModeLabelKeys[value]),
        ]),
      ) as Record<TaskScheduleMode, string>,
    [t],
  );
  const statusFilterOptions = useMemo(
    () =>
      statusFilterValues.map((value) => ({
        label: t(`tasks.status.${value}`),
        value,
      })),
    [t],
  );
  const taskStatusLabel = (status: TaskStatus) => t(`tasks.status.${status}`);

  const tasksQuery = useQuery({
    queryKey: [
      'tasks',
      statusFilter,
      keywordFilter,
      tagFilter,
      routeModelConfigId,
      routeProjectId,
      scheduledOnly,
      taskPage,
      taskPageSize,
    ],
    queryFn: () =>
      api.listTasksPage({
        status: statusFilter === 'all' ? undefined : statusFilter,
        keyword: keywordFilter.trim() || undefined,
        tag: tagFilter,
        model_config_id: routeModelConfigId,
        project_id: routeProjectId,
        scheduled_only: scheduledOnly || undefined,
        limit: taskPageSize,
        offset: (taskPage - 1) * taskPageSize,
      }),
    refetchInterval: (query) => activeRefreshInterval(taskPageHasActiveItems(query.state.data)),
  });
  const taskStatsQuery = useQuery({
    queryKey: ['task-stats'],
    queryFn: api.getTaskStats,
    refetchInterval: (query) =>
      activeRefreshInterval(
        Boolean((query.state.data?.queued || 0) + (query.state.data?.running || 0)),
      ),
  });
  const taskIndexQuery = useQuery({
    queryKey: ['task-index'],
    queryFn: api.getTaskIndex,
  });
  const selectedTaskQuery = useQuery({
    queryKey: ['task', detailTaskId],
    queryFn: () => api.getTask(detailTaskId!),
    enabled: Boolean(detailTaskId),
    refetchInterval: (query) => activeRefreshInterval(isActiveTaskStatus(query.state.data?.status)),
  });
  const taskRecentRunsQuery = useQuery({
    queryKey: ['task-recent-runs', detailTaskId],
    queryFn: () => api.listTaskRuns(detailTaskId!, { limit: 5 }),
    enabled: Boolean(detailTaskId),
    refetchInterval: activeRefreshInterval(isActiveTaskStatus(selectedTaskQuery.data?.status)),
  });
  const detailLastRunId = selectedTaskQuery.data?.last_run_id ?? detailTaskPreview?.last_run_id;
  const detailLastRunQuery = useQuery({
    queryKey: ['task-detail-last-run', detailLastRunId],
    queryFn: () => api.getRun(detailLastRunId!),
    enabled: Boolean(detailLastRunId),
    refetchInterval: (query) => activeRefreshInterval(isActiveRunStatus(query.state.data?.status)),
  });
  const detailLastRunEventsQuery = useQuery({
    queryKey: ['task-detail-last-run-events', detailLastRunId],
    queryFn: () => api.getRunEvents(detailLastRunId!),
    enabled: Boolean(detailLastRunId),
    refetchInterval: activeRefreshInterval(isActiveRunStatus(detailLastRunQuery.data?.status)),
  });
  const taskFollowUpQuery = useQuery({
    queryKey: ['task-follow-ups', detailTaskId],
    queryFn: () =>
      api.listTasks({
        parent_task_id: detailTaskId!,
        limit: 50,
      }),
    enabled: Boolean(detailTaskId),
  });
  const taskRunDerivedQuery = useQuery({
    queryKey: ['task-run-derived', detailLastRunId],
    queryFn: () =>
      api.listTasks({
        source_run_id: detailLastRunId!,
        include_subtasks: true,
        limit: 50,
      }),
    enabled: Boolean(detailLastRunId),
  });
  const taskPromptsQuery = useQuery({
    queryKey: ['task-prompts', detailTaskId],
    queryFn: () =>
      api.listPromptsPage({
        taskId: detailTaskId!,
        limit: 6,
        offset: 0,
      }),
    enabled: Boolean(detailTaskId),
  });
  const modelsQuery = useQuery({
    queryKey: ['model-configs'],
    queryFn: api.listModelConfigs,
  });
  const projectsQuery = useQuery({
    queryKey: ['task-projects', 'active'],
    queryFn: () => api.listProjects('active'),
  });
  const mcpCatalogQuery = useQuery({
    queryKey: ['mcp-catalog'],
    queryFn: api.listMcpCatalog,
  });
  const taskCapabilityCatalogQuery = useQuery({
    queryKey: ['task-capability-catalog'],
    queryFn: api.listTaskCapabilityCatalog,
  });
  const remoteServersQuery = useQuery({
    queryKey: ['remote-servers'],
    queryFn: api.listRemoteServers,
  });
  const externalMcpConfigsQuery = useQuery({
    queryKey: ['external-mcp-configs'],
    queryFn: api.listExternalMcpConfigs,
  });
  const pendingPromptTaskCountsQuery = useQuery({
    queryKey: ['prompt-task-counts', 'pending'],
    queryFn: () => api.listPromptTaskCounts({ status: 'pending' }),
  });
  const taskMemoryContextQuery = useQuery({
    queryKey: ['task-memory-context', memoryTask?.id],
    queryFn: () =>
      api.getTaskMemoryContext(memoryTask!.id, {
        include_recent_records: true,
        include_thread_summary: true,
        include_subject_memory: false,
        recent_record_limit: 12,
        summary_limit: 6,
      }),
    enabled: Boolean(memoryTask),
  });
  const taskMemoryRecordsQuery = useQuery({
    queryKey: [
      'task-memory-records',
      memoryTask?.id,
      memoryRoleFilter,
      memorySummaryFilter,
      memoryLimit,
    ],
    queryFn: () =>
      api.getTaskMemoryRecords(memoryTask!.id, {
        role: memoryRoleFilter === 'all' ? undefined : memoryRoleFilter,
        summary_status: memorySummaryFilter === 'all' ? undefined : memorySummaryFilter,
        limit: memoryLimit,
        offset: 0,
        order: 'desc',
      }),
    enabled: Boolean(memoryTask),
  });
  const taskMcpPromptPreviewQuery = useQuery({
    queryKey: ['task-mcp-prompt-preview', mcpPreviewTask?.id],
    queryFn: () => api.previewTaskMcpPrompt(mcpPreviewTask!.id),
    enabled: Boolean(mcpPreviewTask),
  });
  const taskMcpResolutionQuery = useQuery({
    queryKey: ['task-mcp-resolution', detailTaskId],
    queryFn: () => api.getTaskMcpResolution(detailTaskId!),
    enabled: Boolean(detailTaskId),
  });
  const taskEditorMcpResolutionQuery = useQuery({
    queryKey: ['task-mcp-resolution', editingTaskId],
    queryFn: () => api.getTaskMcpResolution(editingTaskId!),
    enabled: Boolean(editingTaskId),
  });
  const visibleTaskLastRunIds = useMemo(
    () =>
      Array.from(
        new Set(
          (tasksQuery.data?.items || [])
            .map((task) => task.last_run_id)
            .filter((value): value is string => Boolean(value)),
        ),
      ),
    [tasksQuery.data?.items],
  );
  const activeVisibleTaskLastRunIds = useMemo(
    () =>
      new Set(
        (tasksQuery.data?.items || [])
          .filter((task) => isActiveTaskStatus(task.status))
          .map((task) => task.last_run_id)
          .filter((value): value is string => Boolean(value)),
      ),
    [tasksQuery.data?.items],
  );
  const taskListLastRunEventQueries = useQueries({
    queries: visibleTaskLastRunIds.map((runId) => ({
      queryKey: ['task-list-last-run-events', runId],
      queryFn: () => api.getRunEvents(runId),
      enabled: Boolean(runId),
      refetchInterval: activeRefreshInterval(activeVisibleTaskLastRunIds.has(runId)),
    })),
  });

  const modelOptions = useMemo(
    () =>
      (modelsQuery.data || [])
        .filter((model) => model.enabled)
        .map((model) => ({
          label: taskModelOptionLabel(model, t),
          value: model.id,
        })),
    [modelsQuery.data, t],
  );

  const modelNameMap = useMemo(() => {
    const map = new Map<string, string>();
    (modelsQuery.data || []).forEach((model) => {
      map.set(model.id, model.name);
    });
    return map;
  }, [modelsQuery.data]);

  const modelLabelMap = useMemo(() => {
    const map = new Map<string, string>();
    (modelsQuery.data || []).forEach((model) => {
      map.set(model.id, taskModelOptionLabel(model, t));
    });
    return map;
  }, [modelsQuery.data, t]);

  const projectNameMap = useMemo(() => {
    const map = new Map<string, string>();
    (projectsQuery.data || []).forEach((project) => {
      map.set(project.id, project.name);
    });
    return map;
  }, [projectsQuery.data]);

  const projectOptions = useMemo(
    () =>
      (projectsQuery.data || []).map((project: TaskProjectRecord) => ({
        label: project.id === '-1' ? t('projects.public') : project.name,
        value: project.id,
      })),
    [projectsQuery.data, t],
  );

  const taskSummaryMap = useMemo(() => {
    const map = new Map<string, string>();
    (taskIndexQuery.data?.tasks || []).forEach((task) => {
      map.set(task.id, task.title);
    });
    return map;
  }, [taskIndexQuery.data?.tasks]);

  const prerequisiteProjectId = useMemo(() => {
    const editingTask = (taskIndexQuery.data?.tasks || []).find(
      (task) => task.id === editingTaskId,
    );
    return normalizeProjectId(editingTask?.project_id || routeProjectId);
  }, [editingTaskId, routeProjectId, taskIndexQuery.data?.tasks]);

  const prerequisiteTaskOptions = useMemo(
    () =>
      (taskIndexQuery.data?.tasks || [])
        .filter((task) => task.id !== editingTaskId)
        .filter((task) => normalizeProjectId(task.project_id) === prerequisiteProjectId)
        .map((task) => ({
          label: `${task.title} (${task.status})`,
          value: task.id,
        })),
    [editingTaskId, prerequisiteProjectId, taskIndexQuery.data?.tasks],
  );

  const tagOptions = useMemo(
    () =>
      (taskIndexQuery.data?.tags || []).map((tag) => ({
        label: tag,
        value: tag,
      })),
    [taskIndexQuery.data?.tags],
  );
  const remoteServerMap = useMemo(() => {
    const map = new Map<string, RemoteServerRecord>();
    (remoteServersQuery.data || []).forEach((server) => {
      map.set(server.id, server);
    });
    return map;
  }, [remoteServersQuery.data]);
  const externalMcpConfigMap = useMemo(() => {
    const map = new Map<string, ExternalMcpConfigRecord>();
    (externalMcpConfigsQuery.data || []).forEach((config) => {
      map.set(config.id, config);
    });
    return map;
  }, [externalMcpConfigsQuery.data]);
  const selectedTask = useMemo(
    () => selectedTaskQuery.data || detailTaskPreview,
    [detailTaskPreview, selectedTaskQuery.data],
  );
  const detailResultSummary = useMemo(
    () => taskRunReportContent(detailLastRunQuery.data) || selectedTask?.result_summary || null,
    [detailLastRunQuery.data, selectedTask?.result_summary],
  );
  const detailRemoteOperations = useMemo(
    () => collectTaskRemoteOperations(detailLastRunEventsQuery.data || [], remoteServerMap),
    [detailLastRunEventsQuery.data, remoteServerMap],
  );
  const detailRemoteOperationStats = useMemo(
    () => summarizeTaskRemoteOperations(detailRemoteOperations),
    [detailRemoteOperations],
  );
  const latestRemoteOperation = detailRemoteOperations.length
    ? detailRemoteOperations[detailRemoteOperations.length - 1]
    : null;
  const recentRemoteOperations = useMemo(
    () => [...detailRemoteOperations].slice(-3).reverse(),
    [detailRemoteOperations],
  );
  const taskRowRemoteActivityByTaskId = useMemo(() => {
    const runEventsByRunId = new Map<string, TaskRunEventRecord[]>();
    visibleTaskLastRunIds.forEach((runId, index) => {
      const query = taskListLastRunEventQueries[index];
      if (query?.data) {
        runEventsByRunId.set(runId, query.data);
      }
    });

    const taskActivityMap = new Map<string, TaskRowRemoteActivity>();
    (tasksQuery.data?.items || []).forEach((task) => {
      if (!task.last_run_id) {
        return;
      }
      const events = runEventsByRunId.get(task.last_run_id) || [];
      const operations = collectTaskRemoteOperations(events, remoteServerMap);
      if (!operations.length) {
        return;
      }
      taskActivityMap.set(task.id, {
        ...summarizeTaskRemoteOperations(operations),
        latest: operations[operations.length - 1] || null,
      });
    });
    return taskActivityMap;
  }, [remoteServerMap, taskListLastRunEventQueries, tasksQuery.data?.items, visibleTaskLastRunIds]);
  const pendingPromptCountByTaskId = useMemo(() => {
    const map = new Map<string, number>();
    (pendingPromptTaskCountsQuery.data || []).forEach((item) => {
      map.set(item.task_id, item.count);
    });
    return map;
  }, [pendingPromptTaskCountsQuery.data]);
  const batchRunTasks = useMemo(() => {
    const taskMap = new Map((tasksQuery.data?.items || []).map((task) => [task.id, task]));
    return batchRunTaskIds
      .map((taskId) => taskMap.get(taskId))
      .filter((task): task is TaskRecord => Boolean(task));
  }, [batchRunTaskIds, tasksQuery.data]);

  return {
    tasksQuery,
    taskStatsQuery,
    taskIndexQuery,
    selectedTaskQuery,
    taskRecentRunsQuery,
    detailLastRunId,
    detailLastRunQuery,
    detailLastRunEventsQuery,
    taskFollowUpQuery,
    taskRunDerivedQuery,
    taskPromptsQuery,
    modelsQuery,
    projectsQuery,
    mcpCatalogQuery,
    taskCapabilityCatalogQuery,
    remoteServersQuery,
    externalMcpConfigsQuery,
    taskMemoryContextQuery,
    taskMemoryRecordsQuery,
    taskMcpPromptPreviewQuery,
    taskMcpResolutionQuery,
    taskEditorMcpResolutionQuery,
    scheduleModeLabels,
    statusFilterOptions,
    taskStatusLabel,
    modelOptions,
    modelNameMap,
    modelLabelMap,
    projectNameMap,
    projectOptions,
    taskSummaryMap,
    prerequisiteTaskOptions,
    tagOptions,
    remoteServerMap,
    externalMcpConfigMap,
    selectedTask,
    detailResultSummary,
    detailRemoteOperations,
    detailRemoteOperationStats,
    latestRemoteOperation,
    recentRemoteOperations,
    taskRowRemoteActivityByTaskId,
    pendingPromptCountByTaskId,
    batchRunTasks,
  };
}
