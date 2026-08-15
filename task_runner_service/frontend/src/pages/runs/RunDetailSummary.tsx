// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import {
  Button,
  Descriptions,
  Space,
  Tag,
} from 'antd';
import dayjs from 'dayjs';

import type { TranslateFn } from '../../i18n/I18nProvider';
import type { TaskRunRecord, TaskRunStatus } from '../../types';
import { formatUserVisibleRunText, runColorMap } from './runPageUtils';

type RunStreamStats = {
  chunkCount: number;
  chunkChars: number;
  thinkingCount: number;
  thinkingChars: number;
};

type RunDetailSummaryProps = {
  t: TranslateFn;
  run: TaskRunRecord;
  taskTitle: string;
  modelName: string;
  toolCallCount: number;
  toolResultCount: number;
  modelRequestCount: number;
  streamStats: RunStreamStats;
  canceling: boolean;
  retrying: boolean;
  integrationRetrying: boolean;
  changesLoading: boolean;
  onOpenTask: (taskId: string) => void;
  onOpenModel: (modelConfigId: string) => void;
  onCancel: (runId: string) => void;
  onRetry: (runId: string) => void;
  onRetryIntegration: (runId: string) => void;
  onOpenChanges: (runId: string) => void;
};

export function RunDetailSummary({
  t,
  run,
  taskTitle,
  modelName,
  toolCallCount,
  toolResultCount,
  modelRequestCount,
  streamStats,
  canceling,
  retrying,
  integrationRetrying,
  changesLoading,
  onOpenTask,
  onOpenModel,
  onCancel,
  onRetry,
  onRetryIntegration,
  onOpenChanges,
}: RunDetailSummaryProps) {
  const runStatusLabel = (status: TaskRunStatus) => t(`runs.status.${status}`);
  const inputSnapshot =
    run.input_snapshot && typeof run.input_snapshot === 'object'
      ? (run.input_snapshot as Record<string, unknown>)
      : null;
  const agentKey = typeof inputSnapshot?.agent_key === 'string' ? inputSnapshot.agent_key : null;
  const agentLabel =
    agentKey === 'task_runner_plan_phase'
      ? t('runs.detail.cloudPlanningAgent')
      : agentKey === 'task_runner_run_phase'
        ? t('runs.detail.cloudExecutionAgent')
        : '-';
  const totalDuration = run.started_at
    ? formatDuration(run.started_at, run.finished_at || undefined)
    : '-';
  const integration = run.workspace_execution;

  return (
    <>
      <Space>
        <Button onClick={() => onOpenTask(run.task_id)}>
          {t('runs.detail.openTask')}
        </Button>
        <Button
          disabled={run.status !== 'queued' && run.status !== 'running'}
          loading={canceling}
          onClick={() => onCancel(run.id)}
        >
          {t('runs.detail.cancelRun')}
        </Button>
        <Button
          disabled={run.status === 'queued' || run.status === 'running'}
          loading={retrying}
          onClick={() => onRetry(run.id)}
        >
          {t('runs.detail.retryWithCurrentConfig')}
        </Button>
        <Button
          disabled={integration?.integration_status !== 'conflict'}
          loading={integrationRetrying}
          onClick={() => onRetryIntegration(run.id)}
        >
          {t('runs.detail.retryIntegration')}
        </Button>
        <Button
          disabled={!integration?.result_commit}
          loading={changesLoading}
          onClick={() => onOpenChanges(run.id)}
        >
          {t('runs.detail.viewChanges')}
        </Button>
      </Space>

      <Descriptions bordered column={1} size="small">
        <Descriptions.Item label={t('runs.column.runId')}>{run.id}</Descriptions.Item>
        <Descriptions.Item label={t('runs.column.task')}>
          {taskTitle}
        </Descriptions.Item>
        <Descriptions.Item label={t('common.status')}>
          <Tag color={runColorMap[run.status]}>{runStatusLabel(run.status)}</Tag>
        </Descriptions.Item>
        <Descriptions.Item label={t('runs.detail.modelPhaseStatus')}>
          {t(`runs.modelPhaseStatus.${run.model_phase_status}`)}
        </Descriptions.Item>
        <Descriptions.Item label={t('runs.detail.integrationStatus')}>
          {integration
            ? t(`runs.integrationStatus.${integration.integration_status}`)
            : t('runs.integrationStatus.not_required')}
        </Descriptions.Item>
        {integration?.execution_branch_ref ? (
          <Descriptions.Item label={t('runs.detail.executionBranch')}>
            {integration.execution_branch_ref}
          </Descriptions.Item>
        ) : null}
        {integration?.conflict_files?.length ? (
          <Descriptions.Item label={t('runs.detail.conflictFiles')}>
            {integration.conflict_files.join(', ')}
          </Descriptions.Item>
        ) : null}
        <Descriptions.Item label={t('runs.detail.agent')}>{agentLabel}</Descriptions.Item>
        <Descriptions.Item label={t('runs.column.modelConfig')}>
          <Button
            type="link"
            size="small"
            style={{ paddingInline: 0 }}
            onClick={() => onOpenModel(run.model_config_id)}
          >
            {modelName}
          </Button>
        </Descriptions.Item>
        <Descriptions.Item label={t('runs.column.startedAt')}>
          {run.started_at ? dayjs(run.started_at).format('YYYY-MM-DD HH:mm:ss') : '-'}
        </Descriptions.Item>
        <Descriptions.Item label={t('runs.column.finishedAt')}>
          {run.finished_at ? dayjs(run.finished_at).format('YYYY-MM-DD HH:mm:ss') : '-'}
        </Descriptions.Item>
        <Descriptions.Item label={t('runs.detail.totalDuration')}>
          {totalDuration}
        </Descriptions.Item>
        <Descriptions.Item label={t('runs.detail.resultSummary')}>
          {formatUserVisibleRunText(run.result_summary, t)}
        </Descriptions.Item>
        <Descriptions.Item label={t('runs.detail.errorMessage')}>
          {formatUserVisibleRunText(run.error_message, t)}
        </Descriptions.Item>
        <Descriptions.Item label={t('runs.detail.toolCallCount')}>
          {toolCallCount}
        </Descriptions.Item>
        <Descriptions.Item label={t('runs.detail.toolResultCount')}>
          {toolResultCount}
        </Descriptions.Item>
        <Descriptions.Item label={t('runs.detail.modelRequestRounds')}>
          {modelRequestCount}
        </Descriptions.Item>
        <Descriptions.Item label="Summary Job">
          {run.summary_job_run_id || '-'}
        </Descriptions.Item>
      </Descriptions>

      <Descriptions bordered column={1} size="small">
        <Descriptions.Item label={t('runs.detail.outputChunks')}>
          {t('runs.detail.chunkSummary', {
            count: streamStats.chunkCount,
            chars: streamStats.chunkChars,
          })}
        </Descriptions.Item>
        <Descriptions.Item label={t('runs.detail.thinkingChunks')}>
          {t('runs.detail.chunkSummary', {
            count: streamStats.thinkingCount,
            chars: streamStats.thinkingChars,
          })}
        </Descriptions.Item>
      </Descriptions>
    </>
  );
}

function formatDuration(startedAt: string, finishedAt?: string): string {
  const seconds = Math.max(0, dayjs(finishedAt).diff(dayjs(startedAt), 'second'));
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  const remainingSeconds = seconds % 60;
  return [hours, minutes, remainingSeconds]
    .map((value) => String(value).padStart(2, '0'))
    .join(':');
}
