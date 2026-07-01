// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useMemo } from 'react';
import { useQuery } from '@tanstack/react-query';

import { api } from '../../api/client';
import type { TranslateFn } from '../../i18n/I18nProvider';
import type {
  RunSummaryRecord,
  TaskSummaryRecord,
} from '../../types';
import {
  type PromptStatusFilter,
  promptStatusFilterValues,
} from './promptPageUtils';

type UsePromptsPageDataParams = {
  t: TranslateFn;
  routePromptId?: string;
  taskFilterId?: string;
  runFilterId?: string;
  statusFilter: PromptStatusFilter;
  promptPage: number;
  promptPageSize: number;
  taskSearchTerm: string;
  runSearchTerm: string;
};

export function usePromptsPageData({
  t,
  routePromptId,
  taskFilterId,
  runFilterId,
  statusFilter,
  promptPage,
  promptPageSize,
  taskSearchTerm,
  runSearchTerm,
}: UsePromptsPageDataParams) {
  const promptStatusOptions = useMemo(
    () => promptStatusFilterValues.map((value) => ({
      label: t(`prompts.status.${value}`),
      value,
    })),
    [t],
  );

  const promptsQuery = useQuery({
    queryKey: ['prompts', taskFilterId, runFilterId, statusFilter, promptPage, promptPageSize],
    queryFn: () =>
      api.listPromptsPage({
        taskId: taskFilterId,
        runId: runFilterId,
        status: statusFilter === 'all' ? undefined : statusFilter,
        limit: promptPageSize,
        offset: (promptPage - 1) * promptPageSize,
      }),
  });
  const modelsQuery = useQuery({
    queryKey: ['model-configs'],
    queryFn: api.listModelConfigs,
  });
  const selectedPromptQuery = useQuery({
    queryKey: ['prompt', routePromptId],
    queryFn: () => api.getPrompt(routePromptId!),
    enabled: Boolean(routePromptId),
  });
  const selectedPrompt = useMemo(() => {
    if (!routePromptId) {
      return null;
    }
    return (
      selectedPromptQuery.data ||
      (promptsQuery.data?.items || []).find((prompt) => prompt.id === routePromptId) ||
      null
    );
  }, [promptsQuery.data, routePromptId, selectedPromptQuery.data]);

  const displayRunIds = useMemo(() => {
    const ids = new Set<string>();
    if (runFilterId) {
      ids.add(runFilterId);
    }
    if (selectedPromptQuery.data?.run_id) {
      ids.add(selectedPromptQuery.data.run_id);
    }
    (promptsQuery.data?.items || []).forEach((prompt) => {
      if (prompt.run_id) {
        ids.add(prompt.run_id);
      }
    });
    return Array.from(ids).sort();
  }, [promptsQuery.data?.items, runFilterId, selectedPromptQuery.data?.run_id]);

  const runSummariesQuery = useQuery({
    queryKey: ['run-summaries', displayRunIds.join(',')],
    queryFn: () => api.listRunSummaries({ ids: displayRunIds }),
    enabled: displayRunIds.length > 0,
  });

  const runSearchQuery = useQuery({
    queryKey: ['run-summary-search', taskFilterId, runSearchTerm],
    queryFn: () =>
      api.listRunSummaries({
        task_id: taskFilterId,
        keyword: runSearchTerm.trim() || undefined,
        limit: 20,
      }),
    enabled: Boolean(taskFilterId || runSearchTerm.trim()),
  });

  const displayTaskIds = useMemo(() => {
    const ids = new Set<string>();
    if (taskFilterId) {
      ids.add(taskFilterId);
    }
    if (selectedPromptQuery.data?.task_id) {
      ids.add(selectedPromptQuery.data.task_id);
    }
    (promptsQuery.data?.items || []).forEach((prompt) => {
      if (prompt.task_id) {
        ids.add(prompt.task_id);
      }
    });
    (runSearchQuery.data || []).forEach((run) => ids.add(run.task_id));
    (runSummariesQuery.data || []).forEach((run) => ids.add(run.task_id));
    return Array.from(ids).sort();
  }, [
    promptsQuery.data?.items,
    runSearchQuery.data,
    runSummariesQuery.data,
    selectedPromptQuery.data?.task_id,
    taskFilterId,
  ]);

  const taskSummariesQuery = useQuery({
    queryKey: ['task-summaries', displayTaskIds.join(',')],
    queryFn: () => api.listTaskSummaries({ ids: displayTaskIds }),
    enabled: displayTaskIds.length > 0,
  });

  const taskSearchQuery = useQuery({
    queryKey: ['task-summary-search', taskSearchTerm],
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

  const runMap = useMemo(() => {
    const map = new Map<string, RunSummaryRecord>();
    (runSummariesQuery.data || []).forEach((run) => map.set(run.id, run));
    return map;
  }, [runSummariesQuery.data]);

  const modelMap = useMemo(() => {
    const map = new Map<string, string>();
    (modelsQuery.data || []).forEach((model) => map.set(model.id, model.name));
    return map;
  }, [modelsQuery.data]);

  const taskOptions = useMemo(
    () =>
      Array.from(
        new Map(
          [...(taskSummariesQuery.data || []), ...(taskSearchQuery.data || [])].map((task) => [
            task.id,
            {
              label: task.title,
              value: task.id,
            },
          ]),
        ).values(),
      ),
    [taskSearchQuery.data, taskSummariesQuery.data],
  );

  const runOptions = useMemo(
    () =>
      Array.from(
        new Map(
          [...(runSummariesQuery.data || []), ...(runSearchQuery.data || [])].map((run) => [
            run.id,
            {
              label: `${run.id.slice(0, 12)} / ${
                taskMap.get(run.task_id)?.title || run.task_id
              }`,
              value: run.id,
            },
          ]),
        ).values(),
      ),
    [runSearchQuery.data, runSummariesQuery.data, taskMap],
  );

  return {
    promptStatusOptions,
    promptsQuery,
    selectedPromptQuery,
    selectedPrompt,
    taskMap,
    runMap,
    modelMap,
    taskOptions,
    runOptions,
  };
}
