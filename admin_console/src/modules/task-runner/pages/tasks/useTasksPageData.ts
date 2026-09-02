// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useMemo } from 'react';
import { useQuery } from '@tanstack/react-query';

import { api } from '../../api/client';
import type { TranslateFn } from '../../i18n/I18nProvider';
import type {
  TaskProjectRecord,
  TaskRecord,
  TaskRunStatus,
  TaskScheduleMode,
  TaskStatus,
} from '../../types';
import type {
  TaskMemoryRoleFilter,
  TaskMemorySummaryFilter,
} from './TaskMemoryDrawer';
import {
  scheduleModeLabelKeys,
  statusFilterValues,
  taskModelOptionLabel,
  taskRunReportContent,
} from './taskPageUtils';

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
  return trimmed || null;
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
    queryKey: ['task-runner',
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
    queryKey: ['task-runner', 'task-stats'],
    queryFn: api.getTaskStats,
    refetchInterval: (query) =>
      activeRefreshInterval(
        Boolean((query.state.data?.queued || 0) + (query.state.data?.running || 0)),
      ),
  });
  const taskIndexQuery = useQuery({
    queryKey: ['task-runner', 'task-index'],
    queryFn: api.getTaskIndex,
  });
  const selectedTaskQuery = useQuery({
    queryKey: ['task-runner', 'task', detailTaskId],
    queryFn: () => api.getTask(detailTaskId!),
    enabled: Boolean(detailTaskId),
    refetchInterval: (query) => activeRefreshInterval(isActiveTaskStatus(query.state.data?.status)),
  });
  const taskRecentRunsQuery = useQuery({
    queryKey: ['task-runner', 'task-recent-runs', detailTaskId],
    queryFn: () => api.listTaskRuns(detailTaskId!, { limit: 5 }),
    enabled: Boolean(detailTaskId),
    refetchInterval: activeRefreshInterval(isActiveTaskStatus(selectedTaskQuery.data?.status)),
  });
  const detailLastRunId = selectedTaskQuery.data?.last_run_id ?? detailTaskPreview?.last_run_id;
  const detailLastRunQuery = useQuery({
    queryKey: ['task-runner', 'task-detail-last-run', detailLastRunId],
    queryFn: () => api.getRun(detailLastRunId!),
    enabled: Boolean(detailLastRunId),
    refetchInterval: (query) => activeRefreshInterval(isActiveRunStatus(query.state.data?.status)),
  });
  const taskFollowUpQuery = useQuery({
    queryKey: ['task-runner', 'task-follow-ups', detailTaskId],
    queryFn: () =>
      api.listTasks({
        parent_task_id: detailTaskId!,
        limit: 50,
      }),
    enabled: Boolean(detailTaskId),
  });
  const taskRunDerivedQuery = useQuery({
    queryKey: ['task-runner', 'task-run-derived', detailLastRunId],
    queryFn: () =>
      api.listTasks({
        source_run_id: detailLastRunId!,
        include_subtasks: true,
        limit: 50,
      }),
    enabled: Boolean(detailLastRunId),
  });
  const taskPromptsQuery = useQuery({
    queryKey: ['task-runner', 'task-prompts', detailTaskId],
    queryFn: () =>
      api.listPromptsPage({
        taskId: detailTaskId!,
        limit: 6,
        offset: 0,
      }),
    enabled: Boolean(detailTaskId),
  });
  const modelsQuery = useQuery({
    queryKey: ['task-runner', 'model-configs'],
    queryFn: api.listModelConfigs,
  });
  const projectsQuery = useQuery({
    queryKey: ['task-runner', 'task-projects', 'active'],
    queryFn: () => api.listProjects('active'),
  });
  const pendingPromptTaskCountsQuery = useQuery({
    queryKey: ['task-runner', 'prompt-task-counts', 'pending'],
    queryFn: () => api.listPromptTaskCounts({ status: 'pending' }),
  });
  const taskMemoryContextQuery = useQuery({
    queryKey: ['task-runner', 'task-memory-context', memoryTask?.id],
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
    queryKey: ['task-runner',
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
    queryKey: ['task-runner', 'task-mcp-prompt-preview', mcpPreviewTask?.id],
    queryFn: () => api.previewTaskMcpPrompt(mcpPreviewTask!.id),
    enabled: Boolean(mcpPreviewTask),
  });
  const taskMcpResolutionQuery = useQuery({
    queryKey: ['task-runner', 'task-mcp-resolution', detailTaskId],
    queryFn: () => api.getTaskMcpResolution(detailTaskId!),
    enabled: Boolean(detailTaskId),
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
        label: project.name,
        value: project.id,
      })),
    [projectsQuery.data],
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
    return normalizeProjectId(editingTask?.project_id ?? routeProjectId);
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
  const selectedTask = useMemo(
    () => selectedTaskQuery.data || detailTaskPreview,
    [detailTaskPreview, selectedTaskQuery.data],
  );
  const detailResultSummary = useMemo(
    () => taskRunReportContent(detailLastRunQuery.data) || selectedTask?.result_summary || null,
    [detailLastRunQuery.data, selectedTask?.result_summary],
  );
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
    taskFollowUpQuery,
    taskRunDerivedQuery,
    taskPromptsQuery,
    modelsQuery,
    projectsQuery,
    taskMemoryContextQuery,
    taskMemoryRecordsQuery,
    taskMcpPromptPreviewQuery,
    taskMcpResolutionQuery,
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
    selectedTask,
    detailResultSummary,
    pendingPromptCountByTaskId,
    batchRunTasks,
  };
}
