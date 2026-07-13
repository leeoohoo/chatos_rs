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
import type { TaskRunStatus } from '../types';
import {
  type RunStatusFilter,
} from './runs/runPageUtils';
import { RunDetailDrawer } from './runs/RunDetailDrawer';
import { RunListTable } from './runs/RunListTable';
import { RunListToolbar } from './runs/RunListToolbar';
import { useRunsPageData } from './runs/useRunsPageData';

export function RunsPage() {
  const { t } = useI18n();
  const DEFAULT_PAGE_SIZE = 10;
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const [searchParams, setSearchParams] = useSearchParams();
  const [messageApi, contextHolder] = message.useMessage();
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
    selectedRemoteOperations,
    selectedRemoteOperationStats,
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
    const refresh = () => {
      void Promise.all([
        queryClient.invalidateQueries({ queryKey: ['runs'] }),
        queryClient.invalidateQueries({ queryKey: ['run-index'] }),
        queryClient.invalidateQueries({ queryKey: ['run', selectedRunId] }),
        queryClient.invalidateQueries({ queryKey: ['run-events', selectedRunId] }),
        queryClient.invalidateQueries({ queryKey: ['run-prompts', selectedRunId] }),
      ]);
    };

    void api
      .issueSseTicket()
      .then(({ ticket }) => {
        if (closed) {
          return;
        }
        eventSource = new EventSource(
          buildEventSourceUrl(`/api/runs/${selectedRunId}/stream`, ticket),
        );
        eventSource.addEventListener('run_event', refresh);
        eventSource.onerror = () => {
          eventSource?.close();
        };
      })
      .catch(() => {
        // The normal query refresh path will surface auth/network errors.
      });

    return () => {
      closed = true;
      eventSource?.removeEventListener('run_event', refresh);
      eventSource?.close();
    };
  }, [queryClient, selectedRunId]);

  const cancelRunMutation = useMutation({
    mutationFn: api.cancelRun,
    onSuccess: async (_, runId) => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ['runs'] }),
        queryClient.invalidateQueries({ queryKey: ['run-index'] }),
        queryClient.invalidateQueries({ queryKey: ['run', runId] }),
        queryClient.invalidateQueries({ queryKey: ['run-events', runId] }),
      ]);
      messageApi.success(t('runs.cancelRequested'));
    },
    onError: (error: Error) => messageApi.error(error.message),
  });

  const retryRunMutation = useMutation({
    mutationFn: api.retryRun,
    onSuccess: async (run) => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ['runs'] }),
        queryClient.invalidateQueries({ queryKey: ['run-index'] }),
        queryClient.invalidateQueries({ queryKey: ['model-config-usage'] }),
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
          onRefresh={() => runsQuery.refetch()}
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
          onOpenTask={(taskId) => navigate(`/tasks?task_id=${encodeURIComponent(taskId)}`)}
          onOpenModel={(modelId) => navigate(`/models?model_id=${encodeURIComponent(modelId)}`)}
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
        remoteOperations={selectedRemoteOperations}
        remoteOperationStats={selectedRemoteOperationStats}
        promptsPage={runPromptsQuery.data}
        promptsLoading={runPromptsQuery.isLoading}
        promptPage={runPromptPage}
        promptPageSize={runPromptPageSize}
        events={selectedRunEvents}
        eventsLoading={runEventsQuery.isLoading}
        canceling={cancelRunMutation.isPending}
        retrying={retryRunMutation.isPending}
        onClose={() => {
          const next = new URLSearchParams(searchParams);
          next.delete('run_id');
          setSearchParams(next);
          setSelectedRunId(null);
        }}
        onOpenTask={(taskId) => navigate(`/tasks?task_id=${encodeURIComponent(taskId)}`)}
        onOpenModel={(modelId) => navigate(`/models?model_id=${encodeURIComponent(modelId)}`)}
        onCancel={(runId) => cancelRunMutation.mutate(runId)}
        onRetry={(runId) => retryRunMutation.mutate(runId)}
        onManageServers={() => navigate('/servers')}
        onOpenServer={(serverId) =>
          navigate(`/servers?server_id=${encodeURIComponent(serverId)}`)
        }
        onOpenPrompt={(promptId, runId) =>
          navigate(
            `/prompts?prompt_id=${encodeURIComponent(promptId)}&run_id=${encodeURIComponent(runId)}`,
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
