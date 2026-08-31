// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useMemo } from 'react';
import { useQuery } from '@tanstack/react-query';

import { api } from '../../api/client';
import type { TranslateFn } from '../../i18n/I18nProvider';
import type { TaskSummaryRecord, TaskRunStatus } from '../../types';
import {
  collectToolCalls,
  collectToolResults,
  summarizeStreamEvents,
} from './runEventUtils';
import {
  type RunStatusFilter,
  runStatusFilterValues,
} from './runPageUtils';

type UseRunsPageDataParams = {
  t: TranslateFn;
  taskFilterId?: string;
  statusFilter: RunStatusFilter;
  routeModelConfigId?: string;
  runPage: number;
  runPageSize: number;
  selectedRunId: string | null;
  runPromptPage: number;
  runPromptPageSize: number;
  taskSearchTerm: string;
};

const ACTIVE_RUN_REFRESH_INTERVAL_MS = 2500;
const activeRunStatuses = new Set<TaskRunStatus>(['queued', 'running']);

function activeRefreshInterval(active: boolean) {
  return active ? ACTIVE_RUN_REFRESH_INTERVAL_MS : false;
}

function isActiveRunStatus(status?: TaskRunStatus | null) {
  return Boolean(status && activeRunStatuses.has(status));
}

function runPageHasActiveItems(
  data?: { items?: Array<{ status?: TaskRunStatus | null }> } | null,
) {
  return Boolean(data?.items?.some((run) => isActiveRunStatus(run.status)));
}

export function useRunsPageData({
  t,
  taskFilterId,
  statusFilter,
  routeModelConfigId,
  runPage,
  runPageSize,
  selectedRunId,
  runPromptPage,
  runPromptPageSize,
  taskSearchTerm,
}: UseRunsPageDataParams) {
  const runStatusOptions = useMemo(
    () => runStatusFilterValues.map((value) => ({
      label: t(`runs.status.${value}`),
      value,
    })),
    [t],
  );

  const runsQuery = useQuery({
    queryKey: ['task-runner', 'runs', taskFilterId, statusFilter, routeModelConfigId, runPage, runPageSize],
    queryFn: ({ signal }) =>
      api.listRunsPage({
        task_id: taskFilterId,
        status: statusFilter === 'all' ? undefined : statusFilter,
        model_config_id: routeModelConfigId,
        limit: runPageSize,
        offset: (runPage - 1) * runPageSize,
      }, signal),
    refetchInterval: (query) => activeRefreshInterval(runPageHasActiveItems(query.state.data)),
  });
  const modelsQuery = useQuery({
    queryKey: ['task-runner', 'model-configs'],
    queryFn: api.listModelConfigs,
  });
  const selectedRunQuery = useQuery({
    queryKey: ['task-runner', 'run', selectedRunId],
    queryFn: async ({ signal }) => {
      const run = await api.getRun(selectedRunId!, signal);
      if (run.id !== selectedRunId) {
        throw new Error(`Run response identity mismatch: expected ${selectedRunId}, received ${run.id}`);
      }
      return run;
    },
    enabled: Boolean(selectedRunId),
    refetchInterval: (query) =>
      activeRefreshInterval(isActiveRunStatus(query.state.data?.status)),
  });
  const runEventsQuery = useQuery({
    queryKey: ['task-runner', 'run-events', selectedRunId],
    queryFn: async ({ signal }) => {
      const events = await api.getRunEvents(selectedRunId!, undefined, signal);
      if (events.some((event) => event.run_id !== selectedRunId)) {
        throw new Error(`Run event response contained events outside Run ${selectedRunId}`);
      }
      return events;
    },
    enabled: Boolean(selectedRunId),
  });
  const runPromptsQuery = useQuery({
    queryKey: ['task-runner', 'run-prompts', selectedRunId, runPromptPage, runPromptPageSize],
    queryFn: ({ signal }) =>
      api.listPromptsPage({
        runId: selectedRunId!,
        limit: runPromptPageSize,
        offset: (runPromptPage - 1) * runPromptPageSize,
      }, signal),
    enabled: Boolean(selectedRunId),
  });

  const displayTaskIds = useMemo(() => {
    const ids = new Set<string>();
    if (taskFilterId) {
      ids.add(taskFilterId);
    }
    if (selectedRunQuery.data?.task_id) {
      ids.add(selectedRunQuery.data.task_id);
    }
    (runsQuery.data?.items || []).forEach((run) => ids.add(run.task_id));
    return Array.from(ids).sort();
  }, [taskFilterId, selectedRunQuery.data?.task_id, runsQuery.data?.items]);

  const taskSummariesQuery = useQuery({
    queryKey: ['task-runner', 'task-summaries', displayTaskIds.join(',')],
    queryFn: () => api.listTaskSummaries({ ids: displayTaskIds }),
    enabled: displayTaskIds.length > 0,
  });

  const taskSearchQuery = useQuery({
    queryKey: ['task-runner', 'task-summary-search', taskSearchTerm],
    queryFn: () =>
      api.listTaskSummaries({
        keyword: taskSearchTerm.trim() || undefined,
        limit: 20,
      }),
    enabled: taskSearchTerm.trim().length > 0,
  });

  const taskMap = useMemo(() => {
    const map = new Map<string, TaskSummaryRecord>();
    (taskSummariesQuery.data || []).forEach((task) => map.set(task.id, task));
    return map;
  }, [taskSummariesQuery.data]);

  const selectedRun = useMemo(() => {
    if (!selectedRunId) {
      return null;
    }
    return (
      (selectedRunQuery.data?.id === selectedRunId ? selectedRunQuery.data : undefined) ||
      (runsQuery.data?.items || []).find((run) => run.id === selectedRunId) ||
      null
    );
  }, [selectedRunId, selectedRunQuery.data, runsQuery.data]);

  const selectedRunEvents = (runEventsQuery.data || []).filter(
    (event) => event.run_id === selectedRunId,
  );
  const selectedToolCalls = useMemo(
    () => collectToolCalls(selectedRunEvents, selectedRun?.report),
    [selectedRun?.report, selectedRunEvents],
  );
  const selectedToolResults = useMemo(
    () => collectToolResults(selectedRunEvents),
    [selectedRunEvents],
  );
  const selectedModelRequests = useMemo(
    () =>
      selectedRunEvents.filter((event) => event.event_type === 'model_request'),
    [selectedRunEvents],
  );
  const selectedStreamStats = useMemo(
    () => summarizeStreamEvents(selectedRunEvents),
    [selectedRunEvents],
  );

  const taskOptions = useMemo(
    () => {
      const map = new Map<string, { label: string; value: string }>();
      [...(taskSummariesQuery.data || []), ...(taskSearchQuery.data || [])].forEach((task) => {
        map.set(task.id, {
          label: task.title,
          value: task.id,
        });
      });
      return Array.from(map.values());
    },
    [taskSearchQuery.data, taskSummariesQuery.data],
  );

  const modelOptions = useMemo(
    () =>
      (modelsQuery.data || []).map((model) => ({
        label: `${model.name} (${model.model})`,
        value: model.id,
      })),
    [modelsQuery.data],
  );

  const modelNameMap = useMemo(() => {
    const map = new Map<string, string>();
    (modelsQuery.data || []).forEach((model) => {
      map.set(model.id, model.name);
    });
    return map;
  }, [modelsQuery.data]);

  return {
    runStatusOptions,
    runsQuery,
    selectedRunQuery,
    runEventsQuery,
    runPromptsQuery,
    taskMap,
    selectedRun,
    selectedRunEvents,
    selectedToolCalls,
    selectedToolResults,
    selectedModelRequests,
    selectedStreamStats,
    taskOptions,
    modelOptions,
    modelNameMap,
  };
}
