// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useEffect, useState } from 'react';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { useNavigate, useSearchParams } from 'react-router-dom';
import {
  Space,
  message,
} from 'antd';

import { api, buildEventSourceUrl } from '../api/client';
import { useI18n } from '../i18n/I18nProvider';
import type { RunWorkspaceChanges, TaskRunEventRecord, TaskRunStatus } from '../types';
import {
  type RunStatusFilter,
} from './runs/runPageUtils';
import { RunDetailDrawer } from './runs/RunDetailDrawer';
import { RunListTable } from './runs/RunListTable';
import { RunListToolbar } from './runs/RunListToolbar';
import { useRunsPageData } from './runs/useRunsPageData';

const RUN_EVENT_SYNC_BATCH_SIZE = 200;
const RUN_EVENT_RECONNECT_DELAY_MS = 1000;

async function loadRunEventSuffix(
  runId: string,
  cursor: TaskRunEventRecord,
  collected: TaskRunEventRecord[] = [],
  signal?: AbortSignal,
): Promise<TaskRunEventRecord[]> {
  const batch = await api.getRunEvents(runId, {
    after_created_at: cursor.created_at,
    after_id: cursor.id,
    limit: RUN_EVENT_SYNC_BATCH_SIZE,
  }, signal);
  if (batch.some((event) => event.run_id !== runId)) {
    throw new Error(`Run event response contained events outside Run ${runId}`);
  }
  const next = [...collected, ...batch];
  if (batch.length < RUN_EVENT_SYNC_BATCH_SIZE) {
    return next;
  }
  const nextCursor = batch[batch.length - 1];
  return nextCursor ? loadRunEventSuffix(runId, nextCursor, next, signal) : next;
}

function appendUniqueRunEvents(
  current: TaskRunEventRecord[],
  incoming: TaskRunEventRecord[],
): TaskRunEventRecord[] {
  if (incoming.length === 0) {
    return current;
  }
  const knownIds = new Set(current.map((event) => event.id));
  return [...current, ...incoming.filter((event) => !knownIds.has(event.id))];
}

export function RunsPage() {
  const { t } = useI18n();
  const DEFAULT_PAGE_SIZE = 10;
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const [searchParams, setSearchParams] = useSearchParams();
  const [messageApi, contextHolder] = message.useMessage();
  const [runChanges, setRunChanges] = useState<RunWorkspaceChanges | null>(null);
  const [selectedRunId, setSelectedRunId] = useState<string | null>(null);
  const [statusFilter, setStatusFilter] = useState<RunStatusFilter>('all');
  const [runPage, setRunPage] = useState(1);
  const [runPageSize, setRunPageSize] = useState(DEFAULT_PAGE_SIZE);
  const [runPromptPage, setRunPromptPage] = useState(1);
  const [runPromptPageSize, setRunPromptPageSize] = useState(10);
  const [taskSearchTerm, setTaskSearchTerm] = useState('');
  const taskFilterId = searchParams.get('task_id') || undefined;
  const routeRunId = searchParams.get('run_id') || undefined;
  const routeModelConfigId = searchParams.get('model_config_id') || undefined;
  const runStatusLabel = (status: TaskRunStatus) => t(`runs.status.${status}`);
  const {
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
  } = useRunsPageData({
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
  });

  useEffect(() => {
    setSelectedRunId(routeRunId ?? null);
  }, [routeRunId]);

  useEffect(() => {
    setRunPromptPage(1);
  }, [selectedRunId]);

  useEffect(() => {
    setRunPage(1);
  }, [taskFilterId, statusFilter, routeModelConfigId]);

  useEffect(() => {
    if (!selectedRunId) {
      return undefined;
    }

    let closed = false;
    let eventSource: EventSource | null = null;
    let reconnectTimer: number | undefined;
    let eventSync = Promise.resolve();
    let syncAbortController: AbortController | null = null;
    const syncRunEvents = async () => {
      const queryKey = ['run-events', selectedRunId] as const;
      const current = queryClient.getQueryData<TaskRunEventRecord[]>(queryKey);
      const cursor = current?.[current.length - 1];
      if (!current || !cursor) {
        await queryClient.invalidateQueries({ queryKey });
        return;
      }
      syncAbortController?.abort();
      syncAbortController = new AbortController();
      const incoming = await loadRunEventSuffix(
        selectedRunId,
        cursor,
        [],
        syncAbortController.signal,
      );
      if (closed) {
        return;
      }
      queryClient.setQueryData<TaskRunEventRecord[]>(queryKey, (existing = []) =>
        appendUniqueRunEvents(existing, incoming),
      );
    };
    const refresh = (event: MessageEvent<string>) => {
      try {
        const notification = JSON.parse(event.data) as { run_id?: string };
        if (notification.run_id !== selectedRunId) {
          return;
        }
      } catch {
        return;
      }
      eventSync = eventSync
        .then(syncRunEvents)
        .catch(() => queryClient.invalidateQueries({ queryKey: ['task-runner', 'run-events', selectedRunId] }));
      void Promise.all([
        queryClient.invalidateQueries({ queryKey: ['task-runner', 'runs'] }),
        queryClient.invalidateQueries({ queryKey: ['task-runner', 'run-index'] }),
        queryClient.invalidateQueries({ queryKey: ['task-runner', 'run', selectedRunId] }),
        queryClient.invalidateQueries({ queryKey: ['task-runner', 'run-prompts', selectedRunId] }),
      ]);
    };
    const connect = async () => {
      try {
        const queryKey = ['run-events', selectedRunId] as const;
        let current = queryClient.getQueryData<TaskRunEventRecord[]>(queryKey);
        if (current === undefined) {
          current = await api.getRunEvents(selectedRunId);
          if (current.some((event) => event.run_id !== selectedRunId)) {
            throw new Error(`Run event response contained events outside Run ${selectedRunId}`);
          }
          queryClient.setQueryData(queryKey, current);
        }
        const cursor = current[current.length - 1];
        const streamQuery = new URLSearchParams();
        if (cursor) {
          streamQuery.set('after_created_at', cursor.created_at);
          streamQuery.set('after_id', cursor.id);
        } else {
          streamQuery.set('from_start', 'true');
        }
        const { ticket } = await api.issueSseTicket();
        if (closed) {
          return;
        }
        eventSource = new EventSource(
          buildEventSourceUrl(
            `/api/runs/${selectedRunId}/stream?${streamQuery.toString()}`,
            ticket,
          ),
        );
        eventSource.addEventListener('run_event', refresh);
        eventSource.onerror = () => {
          eventSource?.removeEventListener('run_event', refresh);
          eventSource?.close();
          eventSource = null;
          if (!closed && reconnectTimer === undefined) {
            reconnectTimer = window.setTimeout(() => {
              reconnectTimer = undefined;
              void connect();
            }, RUN_EVENT_RECONNECT_DELAY_MS);
          }
        };
      } catch {
        if (!closed && reconnectTimer === undefined) {
          reconnectTimer = window.setTimeout(() => {
            reconnectTimer = undefined;
            void connect();
          }, RUN_EVENT_RECONNECT_DELAY_MS);
        }
      }
    };

    void connect();

    return () => {
      closed = true;
      if (reconnectTimer !== undefined) {
        window.clearTimeout(reconnectTimer);
      }
      eventSource?.removeEventListener('run_event', refresh);
      eventSource?.close();
      syncAbortController?.abort();
    };
  }, [queryClient, selectedRunId]);

  const cancelRunMutation = useMutation({
    mutationFn: api.cancelRun,
    onSuccess: async (_, runId) => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ['task-runner', 'runs'] }),
        queryClient.invalidateQueries({ queryKey: ['task-runner', 'run-index'] }),
        queryClient.invalidateQueries({ queryKey: ['task-runner', 'run', runId] }),
        queryClient.invalidateQueries({ queryKey: ['task-runner', 'run-events', runId] }),
      ]);
      messageApi.success(t('runs.cancelRequested'));
    },
    onError: (error: Error) => messageApi.error(error.message),
  });

  const retryRunMutation = useMutation({
    mutationFn: api.retryRun,
    onSuccess: async (run) => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ['task-runner', 'runs'] }),
        queryClient.invalidateQueries({ queryKey: ['task-runner', 'run-index'] }),
        queryClient.invalidateQueries({ queryKey: ['task-runner', 'model-config-usage'] }),
      ]);
      const next = new URLSearchParams(searchParams);
      next.set('run_id', run.id);
      next.set('task_id', run.task_id);
      setSearchParams(next);
      setSelectedRunId(run.id);
      messageApi.success(t('runs.retryCreated'));
    },
    onError: (error: Error) => messageApi.error(error.message),
  });

  const retryRunIntegrationMutation = useMutation({
    mutationFn: api.retryRunIntegration,
    onSuccess: async (run) => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ['task-runner', 'runs'] }),
        queryClient.invalidateQueries({ queryKey: ['task-runner', 'run-index'] }),
        queryClient.invalidateQueries({ queryKey: ['task-runner', 'run', run.id] }),
        queryClient.invalidateQueries({ queryKey: ['task-runner', 'run-events', run.id] }),
      ]);
      messageApi.success(t('runs.integrationRetryRequested'));
    },
    onError: (error: Error) => messageApi.error(error.message),
  });

  const waiveRunIntegrationMutation = useMutation({
    mutationFn: ({ runId, reason }: { runId: string; reason: string }) => (
      api.waiveRunIntegration(runId, reason)
    ),
    onSuccess: async (run) => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ['task-runner', 'runs'] }),
        queryClient.invalidateQueries({ queryKey: ['task-runner', 'run-index'] }),
        queryClient.invalidateQueries({ queryKey: ['task-runner', 'run', run.id] }),
        queryClient.invalidateQueries({ queryKey: ['task-runner', 'run-events', run.id] }),
      ]);
      messageApi.success(t('runs.integrationWaived'));
    },
    onError: (error: Error) => messageApi.error(error.message),
  });

  const runChangesMutation = useMutation({
    mutationFn: api.getRunChanges,
    onSuccess: setRunChanges,
    onError: (error: Error) => messageApi.error(error.message),
  });

  return (
    <>
      {contextHolder}
      <Space direction="vertical" size="large" style={{ width: '100%' }}>
        <RunListToolbar
          t={t}
          taskFilterId={taskFilterId}
          routeModelConfigId={routeModelConfigId}
          statusFilter={statusFilter}
          taskOptions={taskOptions}
          modelOptions={modelOptions}
          runStatusOptions={runStatusOptions}
          onTaskSearch={setTaskSearchTerm}
          onTaskFilterChange={(value) => {
            const next = new URLSearchParams(searchParams);
            if (value) {
              next.set('task_id', value);
            } else {
              next.delete('task_id');
            }
            setSearchParams(next);
          }}
          onModelFilterChange={(value) => {
            const next = new URLSearchParams(searchParams);
            if (value) {
              next.set('model_config_id', value);
            } else {
              next.delete('model_config_id');
            }
            setSearchParams(next);
          }}
          onStatusFilterChange={setStatusFilter}
          onClearFilters={() => {
            setStatusFilter('all');
            const next = new URLSearchParams(searchParams);
            next.delete('task_id');
            next.delete('model_config_id');
            setSearchParams(next);
          }}
          onRefresh={() => {
            void Promise.all([
              runsQuery.refetch(),
              selectedRunId ? selectedRunQuery.refetch() : Promise.resolve(),
              selectedRunId ? runEventsQuery.refetch() : Promise.resolve(),
              selectedRunId ? runPromptsQuery.refetch() : Promise.resolve(),
            ]);
          }}
        />

        <RunListTable
          t={t}
          runs={runsQuery.data?.items || []}
          loading={runsQuery.isLoading}
          currentPage={runPage}
          pageSize={runPageSize}
          total={runsQuery.data?.total || 0}
          taskMap={taskMap}
          modelNameMap={modelNameMap}
          runStatusLabel={runStatusLabel}
          onPageChange={(page, pageSize) => {
            setRunPage(page);
            setRunPageSize(pageSize);
          }}
          onOpenDetail={(runId) => {
            const next = new URLSearchParams(searchParams);
            next.set('run_id', runId);
            setSearchParams(next);
          }}
          onOpenTask={(taskId) => navigate(`/task-runner/tasks?task_id=${encodeURIComponent(taskId)}`)}
          onCancel={(runId) => cancelRunMutation.mutate(runId)}
          onRetry={(runId) => retryRunMutation.mutate(runId)}
        />
      </Space>

      <RunDetailDrawer
        t={t}
        open={Boolean(selectedRunId)}
        loading={selectedRunQuery.isLoading}
        run={selectedRun}
        taskMap={taskMap}
        modelNameMap={modelNameMap}
        toolCalls={selectedToolCalls}
        toolResults={selectedToolResults}
        modelRequests={selectedModelRequests}
        streamStats={selectedStreamStats}
        promptsPage={runPromptsQuery.data}
        promptsLoading={runPromptsQuery.isLoading}
        promptPage={runPromptPage}
        promptPageSize={runPromptPageSize}
        events={selectedRunEvents}
        eventsLoading={runEventsQuery.isLoading}
        canceling={cancelRunMutation.isPending}
        retrying={retryRunMutation.isPending}
        integrationRetrying={retryRunIntegrationMutation.isPending}
        integrationWaiving={waiveRunIntegrationMutation.isPending}
        changes={runChanges}
        changesLoading={runChangesMutation.isPending}
        onClose={() => {
          const next = new URLSearchParams(searchParams);
          next.delete('run_id');
          setSearchParams(next);
          setSelectedRunId(null);
          setRunChanges(null);
        }}
        onOpenTask={(taskId) => navigate(`/task-runner/tasks?task_id=${encodeURIComponent(taskId)}`)}
        onCancel={(runId) => cancelRunMutation.mutate(runId)}
        onRetry={(runId) => retryRunMutation.mutate(runId)}
        onRetryIntegration={(runId) => retryRunIntegrationMutation.mutate(runId)}
        onWaiveIntegration={async (runId, reason) => {
          await waiveRunIntegrationMutation.mutateAsync({ runId, reason });
        }}
        onOpenChanges={(runId) => runChangesMutation.mutate(runId)}
        onCloseChanges={() => setRunChanges(null)}
        onOpenPrompt={(promptId, runId) =>
          navigate(
            `/task-runner/prompts?prompt_id=${encodeURIComponent(promptId)}&run_id=${encodeURIComponent(runId)}`,
          )
        }
        onPromptPageChange={(page, pageSize) => {
          setRunPromptPage(page);
          setRunPromptPageSize(pageSize);
        }}
      />
    </>
  );
}
