// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import {
  Button,
  Descriptions,
  Empty,
  List,
  Space,
  Statistic,
  Tag,
  Typography,
} from 'antd';
import dayjs from 'dayjs';

import type { TranslateFn } from '../../i18n/I18nProvider';
import type {
  AskUserPromptRecord,
  PaginatedResponse,
  TaskRecord,
  TaskRunRecord,
  TaskStatus,
} from '../../types';
import {
  formatTaskRemoteEndpoint,
  isSchedulerOnlyTask,
  promptStatusColorMap,
  runStatusColorMap,
  statusColorMap,
  type TaskRemoteOperationStats,
  type TaskRemoteOperationView,
} from './taskPageUtils';

export function TextSection({ title, value }: { title: string; value: string }) {
  return (
    <div>
      <Typography.Title level={5}>{title}</Typography.Title>
      <Typography.Paragraph style={{ whiteSpace: 'pre-wrap' }}>
        {value}
      </Typography.Paragraph>
    </div>
  );
}

export function RemoteOperationsSection({
  t,
  task,
  detailLastRunId,
  operations,
  stats,
  latest,
  recent,
  loading,
  onOpenRunHistory,
  onOpenServers,
}: {
  t: TranslateFn;
  task: TaskRecord;
  detailLastRunId: string;
  operations: TaskRemoteOperationView[];
  stats: TaskRemoteOperationStats;
  latest: TaskRemoteOperationView | null;
  recent: TaskRemoteOperationView[];
  loading: boolean;
  onOpenRunHistory: (taskId: string, runId?: string) => void;
  onOpenServers: (serverId?: string) => void;
}) {
  return (
    <div>
      <Space
        style={{ justifyContent: 'space-between', width: '100%', marginBottom: 12 }}
        align="start"
      >
        <Space direction="vertical" size={0}>
          <Typography.Title level={5} style={{ margin: 0 }}>
            {t('tasks.detail.recentRemoteOperations')}
          </Typography.Title>
          <Typography.Text type="secondary">
            {t('tasks.detail.remoteDescription')}
          </Typography.Text>
        </Space>
        <Space>
          <Button
            size="small"
            onClick={() => onOpenRunHistory(task.id, detailLastRunId)}
          >
            {t('tasks.detail.openRecentRun')}
          </Button>
          <Button size="small" onClick={() => onOpenServers()}>
            {t('tasks.detail.servers')}
          </Button>
        </Space>
      </Space>

      {operations.length ? (
        <Space direction="vertical" size="middle" style={{ width: '100%' }}>
          <Space size="large" wrap>
            <Statistic title={t('tasks.detail.remoteOperationCount')} value={stats.total} />
            <Statistic title={t('tasks.detail.involvedServers')} value={stats.serverCount} />
            <Statistic title={t('tasks.detail.success')} value={stats.successCount} />
            <Statistic title={t('tasks.detail.failed')} value={stats.failedCount} />
          </Space>

          {latest ? (
            <Descriptions bordered column={1} size="small">
              <Descriptions.Item label={t('tasks.detail.latestOperation')}>
                <Space wrap>
                  <Tag color={latest.success ? 'success' : 'error'}>
                    {latest.success ? t('tasks.detail.success') : t('tasks.detail.failed')}
                  </Tag>
                  <Typography.Text strong>{latest.name}</Typography.Text>
                </Space>
              </Descriptions.Item>
              <Descriptions.Item label={t('tasks.detail.server')}>
                {latest.connectionId ? (
                  <Button
                    type="link"
                    size="small"
                    style={{ paddingInline: 0 }}
                    onClick={() => onOpenServers(latest.connectionId)}
                  >
                    {latest.connectionName || latest.connectionId}
                  </Button>
                ) : (
                  latest.connectionName || '-'
                )}
              </Descriptions.Item>
              <Descriptions.Item label={t('tasks.detail.host')}>
                {formatTaskRemoteEndpoint(latest.username, latest.host, latest.port) || '-'}
              </Descriptions.Item>
              <Descriptions.Item label={t('tasks.detail.commandPath')}>
                {latest.command || latest.path || latest.summary || '-'}
              </Descriptions.Item>
              <Descriptions.Item label={t('tasks.detail.remoteHost')}>
                {latest.remoteHost || '-'}
              </Descriptions.Item>
              <Descriptions.Item label={t('tasks.detail.resultSummary')}>
                {latest.content || '-'}
              </Descriptions.Item>
            </Descriptions>
          ) : null}

          <List
            bordered
            dataSource={recent}
            renderItem={(operation) => (
              <List.Item
                actions={[
                  <Button
                    key="run"
                    size="small"
                    onClick={() => onOpenRunHistory(task.id, detailLastRunId)}
                  >
                    {t('tasks.detail.runDetails')}
                  </Button>,
                ]}
              >
                <Space direction="vertical" size={4} style={{ width: '100%' }}>
                  <Space wrap>
                    <Tag color={operation.success ? 'success' : 'error'}>
                      {operation.success
                        ? t('tasks.detail.success')
                        : t('tasks.detail.failed')}
                    </Tag>
                    <Typography.Text strong>{operation.name}</Typography.Text>
                    {operation.connectionName ? (
                      <Typography.Text type="secondary">
                        {operation.connectionName}
                      </Typography.Text>
                    ) : null}
                  </Space>
                  <Typography.Paragraph
                    type="secondary"
                    ellipsis={{ rows: 2 }}
                    style={{ marginBottom: 0 }}
                  >
                    {operation.command ||
                      operation.path ||
                      operation.summary ||
                      operation.content ||
                      t('tasks.detail.noSummary')}
                  </Typography.Paragraph>
                </Space>
              </List.Item>
            )}
          />
        </Space>
      ) : loading ? null : (
        <Empty
          image={Empty.PRESENTED_IMAGE_SIMPLE}
          description={t('tasks.detail.noRemoteOperations')}
        />
      )}
    </div>
  );
}

export function RecentRunsSection({
  t,
  task,
  runs,
  loading,
  onOpenRunHistory,
}: {
  t: TranslateFn;
  task: TaskRecord;
  runs?: TaskRunRecord[];
  loading: boolean;
  onOpenRunHistory: (taskId: string, runId?: string) => void;
}) {
  return (
    <div>
      <Typography.Title level={5}>{t('tasks.detail.recentRuns')}</Typography.Title>
      {runs?.length ? (
        <List
          bordered
          dataSource={runs}
          renderItem={(run) => (
            <List.Item
              actions={[
                <Button
                  key="open"
                  size="small"
                  onClick={() => onOpenRunHistory(task.id, run.id)}
                >
                  {t('common.open')}
                </Button>,
              ]}
            >
              <Space direction="vertical" size={4} style={{ width: '100%' }}>
                <Space wrap>
                  <Typography.Text code>{run.id.slice(0, 12)}</Typography.Text>
                  <Tag color={runStatusColorMap[run.status]}>{run.status}</Tag>
                  <Typography.Text type="secondary">
                    {run.started_at
                      ? dayjs(run.started_at).format('YYYY-MM-DD HH:mm:ss')
                      : dayjs(run.created_at).format('YYYY-MM-DD HH:mm:ss')}
                  </Typography.Text>
                </Space>
                {run.result_summary ? (
                  <Typography.Paragraph
                    type="secondary"
                    ellipsis={{ rows: 2 }}
                    style={{ marginBottom: 0 }}
                  >
                    {run.result_summary}
                  </Typography.Paragraph>
                ) : run.error_message ? (
                  <Typography.Text type="danger">{run.error_message}</Typography.Text>
                ) : (
                  <Typography.Text type="secondary">
                    {t('tasks.detail.noSummary')}
                  </Typography.Text>
                )}
              </Space>
            </List.Item>
          )}
        />
      ) : loading ? null : (
        <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('tasks.detail.noRunRecords')} />
      )}
    </div>
  );
}

export function RelatedPromptsSection({
  t,
  task,
  prompts,
  loading,
  onOpenPrompts,
}: {
  t: TranslateFn;
  task: TaskRecord;
  prompts?: PaginatedResponse<AskUserPromptRecord>;
  loading: boolean;
  onOpenPrompts: (taskId: string, promptId?: string) => void;
}) {
  return (
    <div>
      <Typography.Title level={5}>{t('tasks.detail.relatedPrompts')}</Typography.Title>
      {prompts?.items.length ? (
        <List
          bordered
          dataSource={prompts.items}
          renderItem={(prompt) => (
            <List.Item
              actions={[
                <Button
                  key="open"
                  size="small"
                  onClick={() => onOpenPrompts(task.id, prompt.id)}
                >
                  {t('common.open')}
                </Button>,
              ]}
            >
              <Space direction="vertical" size={4} style={{ width: '100%' }}>
                <Space wrap>
                  <Typography.Text strong>
                    {prompt.title || prompt.message || prompt.kind}
                  </Typography.Text>
                  <Tag color={promptStatusColorMap[prompt.status]}>{prompt.status}</Tag>
                  {prompt.run_id ? (
                    <Typography.Text code>{prompt.run_id.slice(0, 12)}</Typography.Text>
                  ) : null}
                </Space>
                {prompt.message ? (
                  <Typography.Paragraph
                    type="secondary"
                    ellipsis={{ rows: 2 }}
                    style={{ marginBottom: 0 }}
                  >
                    {prompt.message}
                  </Typography.Paragraph>
                ) : null}
                <Typography.Text type="secondary">
                  {dayjs(prompt.updated_at).format('YYYY-MM-DD HH:mm:ss')}
                </Typography.Text>
              </Space>
            </List.Item>
          )}
        />
      ) : loading ? null : (
        <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('tasks.detail.noPrompts')} />
      )}
      {prompts?.has_more ? (
        <Space style={{ marginTop: 12 }}>
          <Typography.Text type="secondary">
            {t('tasks.detail.promptVisibleCount', {
              shown: prompts.items.length,
              total: prompts.total,
            })}
          </Typography.Text>
          <Button size="small" onClick={() => onOpenPrompts(task.id)}>
            {t('tasks.detail.viewAll')}
          </Button>
        </Space>
      ) : null}
    </div>
  );
}

export function RelatedTasksSection({
  t,
  title,
  tasks,
  loading,
  emptyDescription,
  taskStatusLabel,
  onOpenDetail,
  onOpenRunHistory,
  onRunTask,
  showRunAction = false,
  sourceLabel,
}: {
  t: TranslateFn;
  title: string;
  tasks?: TaskRecord[];
  loading: boolean;
  emptyDescription: string;
  taskStatusLabel: (status: TaskStatus) => string;
  onOpenDetail: (task: TaskRecord) => void;
  onOpenRunHistory: (taskId: string, runId?: string) => void;
  onRunTask: (task: TaskRecord) => void;
  showRunAction?: boolean;
  sourceLabel: 'source run' | 'parent';
}) {
  return (
    <div>
      <Typography.Title level={5}>{title}</Typography.Title>
      {tasks?.length ? (
        <List
          bordered
          dataSource={tasks}
          renderItem={(relatedTask) => (
            <List.Item
              actions={[
                <Button key="detail" size="small" onClick={() => onOpenDetail(relatedTask)}>
                  {t('tasks.action.detail')}
                </Button>,
                <Button
                  key="history"
                  size="small"
                  onClick={() => onOpenRunHistory(relatedTask.id)}
                >
                  {t('tasks.action.history')}
                </Button>,
                showRunAction ? (
                  <Button
                    key="run"
                    size="small"
                    type="primary"
                    disabled={
                      relatedTask.status === 'queued' ||
                      relatedTask.status === 'running' ||
                      isSchedulerOnlyTask(relatedTask)
                    }
                    onClick={() => onRunTask(relatedTask)}
                  >
                    {t('tasks.action.run')}
                  </Button>
                ) : null,
              ]}
            >
              <Space direction="vertical" size={4} style={{ width: '100%' }}>
                <Space wrap>
                  <Typography.Text strong>{relatedTask.title}</Typography.Text>
                  <Tag color={statusColorMap[relatedTask.status]}>
                    {taskStatusLabel(relatedTask.status)}
                  </Tag>
                  <RelatedTaskSourceLabel task={relatedTask} sourceLabel={sourceLabel} />
                </Space>
                <Typography.Paragraph
                  type="secondary"
                  ellipsis={{ rows: 2 }}
                  style={{ marginBottom: 0 }}
                >
                  {relatedTask.objective}
                </Typography.Paragraph>
              </Space>
            </List.Item>
          )}
        />
      ) : loading ? null : (
        <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={emptyDescription} />
      )}
    </div>
  );
}

function RelatedTaskSourceLabel({
  task,
  sourceLabel,
}: {
  task: TaskRecord;
  sourceLabel: 'source run' | 'parent';
}) {
  if (sourceLabel === 'source run' && task.source_run_id) {
    return (
      <Typography.Text type="secondary">
        source run: {task.source_run_id.slice(0, 12)}
      </Typography.Text>
    );
  }

  if (sourceLabel === 'parent' && task.parent_task_id) {
    return (
      <Typography.Text type="secondary">
        parent: {task.parent_task_id.slice(0, 12)}
      </Typography.Text>
    );
  }

  return null;
}
