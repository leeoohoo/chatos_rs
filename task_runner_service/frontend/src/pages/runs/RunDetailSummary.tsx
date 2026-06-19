import {
  Button,
  Descriptions,
  Space,
  Tag,
} from 'antd';
import dayjs from 'dayjs';

import type { TranslateFn } from '../../i18n/I18nProvider';
import type { TaskRunRecord, TaskRunStatus } from '../../types';
import { runColorMap } from './runPageUtils';

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
  onOpenTask: (taskId: string) => void;
  onOpenModel: (modelConfigId: string) => void;
  onCancel: (runId: string) => void;
  onRetry: (runId: string) => void;
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
  onOpenTask,
  onOpenModel,
  onCancel,
  onRetry,
}: RunDetailSummaryProps) {
  const runStatusLabel = (status: TaskRunStatus) => t(`runs.status.${status}`);

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
      </Space>

      <Descriptions bordered column={1} size="small">
        <Descriptions.Item label={t('runs.column.runId')}>{run.id}</Descriptions.Item>
        <Descriptions.Item label={t('runs.column.task')}>
          {taskTitle}
        </Descriptions.Item>
        <Descriptions.Item label={t('common.status')}>
          <Tag color={runColorMap[run.status]}>{runStatusLabel(run.status)}</Tag>
        </Descriptions.Item>
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
        <Descriptions.Item label={t('runs.detail.resultSummary')}>
          {run.result_summary || '-'}
        </Descriptions.Item>
        <Descriptions.Item label={t('runs.detail.errorMessage')}>
          {run.error_message || '-'}
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
