import { Button, Space, Tag, Typography } from 'antd';
import type { ColumnsType } from 'antd/es/table';
import dayjs from 'dayjs';

import type { TranslateFn } from '../../i18n/I18nProvider';
import type {
  TaskMcpConfig,
  TaskRecord,
  TaskScheduleConfig,
  TaskScheduleMode,
  TaskStatus,
} from '../../types';
import {
  isSchedulerOnlyTask,
  statusColorMap,
  taskCreatorLabel,
  taskOwnerLabel,
  type TaskRemoteOperationStats,
  type TaskRemoteOperationView,
} from './taskPageUtils';

export type TaskRowRemoteActivity = TaskRemoteOperationStats & {
  latest: TaskRemoteOperationView | null;
};

type BuildTaskTableColumnsParams = {
  t: TranslateFn;
  navigate: (to: string) => void;
  modelNameMap: Map<string, string>;
  pendingPromptCountByTaskId: Map<string, number>;
  scheduleModeLabels: Record<TaskScheduleMode, string>;
  taskRowRemoteActivityByTaskId: Map<string, TaskRowRemoteActivity>;
  onOpenDetail: (task: TaskRecord) => void;
  onOpenEdit: (task: TaskRecord) => void;
  onOpenMemory: (task: TaskRecord) => void;
  onOpenRun: (task: TaskRecord) => void;
  onConfirmDelete: (task: TaskRecord) => void;
};

export function buildTaskTableColumns({
  t,
  navigate,
  modelNameMap,
  pendingPromptCountByTaskId,
  scheduleModeLabels,
  taskRowRemoteActivityByTaskId,
  onOpenDetail,
  onOpenEdit,
  onOpenMemory,
  onOpenRun,
  onConfirmDelete,
}: BuildTaskTableColumnsParams): ColumnsType<TaskRecord> {
  const taskStatusLabel = (status: TaskStatus) => t(`tasks.status.${status}`);

  return [
    {
      title: t('tasks.column.task'),
      dataIndex: 'title',
      width: 320,
      render: (_, record) => {
        const remoteActivity = taskRowRemoteActivityByTaskId.get(record.id);
        return (
          <Space direction="vertical" size={4}>
            <Space direction="vertical" size={0}>
              <Typography.Text strong>{record.title}</Typography.Text>
              <Typography.Text type="secondary">{record.objective}</Typography.Text>
            </Space>
            <Space size={[4, 4]} wrap>
              {record.parent_task_id ? (
                <Tag color="purple">{t('tasks.followUp')}</Tag>
              ) : (
                <Tag>{t('tasks.manual')}</Tag>
              )}
              {record.parent_task_id ? (
                <Typography.Text type="secondary">
                  parent: {record.parent_task_id.slice(0, 8)}
                </Typography.Text>
              ) : null}
              {record.source_run_id ? (
                <Typography.Text type="secondary">
                  run: {record.source_run_id.slice(0, 8)}
                </Typography.Text>
              ) : null}
            </Space>
            {record.tags.length || (pendingPromptCountByTaskId.get(record.id) || 0) > 0 ? (
              <Space size={[4, 4]} wrap>
                {record.tags.map((tag) => (
                  <Tag key={tag}>{tag}</Tag>
                ))}
                {(pendingPromptCountByTaskId.get(record.id) || 0) > 0 ? (
                  <Tag color="gold">
                    {t('tasks.pendingPrompts', {
                      count: pendingPromptCountByTaskId.get(record.id) || 0,
                    })}
                  </Tag>
                ) : null}
              </Space>
            ) : null}
            {remoteActivity ? (
              <Space direction="vertical" size={0}>
                <Space size={[4, 4]} wrap>
                  <Tag color={remoteActivity.failedCount > 0 ? 'error' : 'success'}>
                    {t('tasks.remoteOperations', { count: remoteActivity.total })}
                  </Tag>
                  <Tag>{t('tasks.remoteServers', { count: remoteActivity.serverCount })}</Tag>
                  {remoteActivity.latest?.connectionName ? (
                    <Tag color="blue">{remoteActivity.latest.connectionName}</Tag>
                  ) : null}
                </Space>
                <Typography.Text type="secondary">
                  {remoteActivity.latest?.command ||
                    remoteActivity.latest?.path ||
                    remoteActivity.latest?.summary ||
                    t('tasks.remoteActivityFallback')}
                </Typography.Text>
              </Space>
            ) : null}
          </Space>
        );
      },
    },
    {
      title: t('common.status'),
      dataIndex: 'status',
      width: 120,
      render: (status: TaskStatus) => (
        <Tag color={statusColorMap[status]}>{taskStatusLabel(status)}</Tag>
      ),
    },
    {
      title: t('tasks.column.creator'),
      dataIndex: 'creator_display_name',
      width: 150,
      render: (_, record) => taskCreatorLabel(record),
    },
    {
      title: t('tasks.column.owner'),
      dataIndex: 'owner_display_name',
      width: 170,
      render: (_, record) => taskOwnerLabel(record),
    },
    {
      title: t('tasks.column.model'),
      dataIndex: 'default_model_config_id',
      width: 220,
      render: (value?: string | null) => {
        if (!value) {
          return t('tasks.modelUnbound');
        }
        return (
          <Button
            type="link"
            size="small"
            style={{ paddingInline: 0 }}
            onClick={() => navigate(`/models?model_id=${encodeURIComponent(value)}`)}
          >
            {modelNameMap.get(value) || value}
          </Button>
        );
      },
    },
    {
      title: t('tasks.column.mcp'),
      dataIndex: 'mcp_config',
      width: 220,
      render: (mcpConfig: TaskMcpConfig) => (
        <Space size={[4, 4]} wrap>
          <Tag color={mcpConfig.enabled ? 'processing' : 'default'}>
            {mcpConfig.enabled ? t('common.enabled') : t('common.disabled')}
          </Tag>
          <Tag>{mcpConfig.init_mode}</Tag>
          <Tag>{t('tasks.mcpTools', { count: mcpConfig.enabled_builtin_kinds.length })}</Tag>
        </Space>
      ),
    },
    {
      title: t('tasks.column.schedule'),
      dataIndex: 'schedule',
      width: 220,
      render: (schedule: TaskScheduleConfig) => {
        if (schedule.mode === 'manual') {
          return <Tag>{scheduleModeLabels.manual}</Tag>;
        }
        return (
          <Space direction="vertical" size={2}>
            <Space size={[4, 4]} wrap>
              <Tag color="processing">{scheduleModeLabels[schedule.mode]}</Tag>
              {schedule.interval_seconds ? <Tag>{schedule.interval_seconds}s</Tag> : null}
            </Space>
            <Typography.Text type="secondary">
              {t('tasks.schedule.next')}:{' '}
              {schedule.next_run_at
                ? dayjs(schedule.next_run_at).format('YYYY-MM-DD HH:mm:ss')
                : '-'}
            </Typography.Text>
          </Space>
        );
      },
    },
    {
      title: t('tasks.column.summary'),
      dataIndex: 'result_summary',
      render: (value?: string | null) =>
        value ? (
          <Typography.Paragraph
            type="secondary"
            ellipsis={{ rows: 2 }}
            style={{ marginBottom: 0 }}
          >
            {value}
          </Typography.Paragraph>
        ) : (
          '-'
        ),
    },
    {
      title: t('tasks.column.priority'),
      dataIndex: 'priority',
      width: 90,
    },
    {
      title: t('tasks.column.updatedAt'),
      dataIndex: 'updated_at',
      width: 180,
      render: (value: string) => dayjs(value).format('YYYY-MM-DD HH:mm:ss'),
    },
    {
      title: t('common.actions'),
      key: 'actions',
      width: 430,
      render: (_, record) => (
        <Space wrap>
          <Button size="small" onClick={() => onOpenDetail(record)}>
            {t('tasks.action.detail')}
          </Button>
          <Button size="small" onClick={() => onOpenEdit(record)}>
            {t('tasks.action.edit')}
          </Button>
          <Button
            size="small"
            onClick={() => navigate(`/runs?task_id=${encodeURIComponent(record.id)}`)}
          >
            {t('tasks.action.history')}
          </Button>
          <Button
            size="small"
            onClick={() => navigate(`/prompts?task_id=${encodeURIComponent(record.id)}`)}
          >
            {t('tasks.action.prompts')}
          </Button>
          <Button size="small" onClick={() => onOpenMemory(record)}>
            Memory
          </Button>
          <Button
            size="small"
            type="primary"
            disabled={
              (record.status === 'queued' || record.status === 'running')
              || isSchedulerOnlyTask(record)
            }
            onClick={() => onOpenRun(record)}
          >
            {t('tasks.action.run')}
          </Button>
          <Button size="small" danger onClick={() => onConfirmDelete(record)}>
            {t('tasks.action.delete')}
          </Button>
        </Space>
      ),
    },
  ];
}
