import {
  Button,
  Empty,
  Space,
  Table,
  Tag,
  Typography,
} from 'antd';
import type { ColumnsType } from 'antd/es/table';
import dayjs from 'dayjs';

import type { TranslateFn } from '../../i18n/I18nProvider';
import type {
  TaskRunRecord,
  TaskRunStatus,
  TaskSummaryRecord,
} from '../../types';
import { runColorMap } from './runPageUtils';

type RunListTableProps = {
  t: TranslateFn;
  runs: TaskRunRecord[];
  loading: boolean;
  currentPage: number;
  pageSize: number;
  total: number;
  taskMap: Map<string, TaskSummaryRecord>;
  modelNameMap: Map<string, string>;
  runStatusLabel: (status: TaskRunStatus) => string;
  onPageChange: (page: number, pageSize: number) => void;
  onOpenDetail: (runId: string) => void;
  onOpenTask: (taskId: string) => void;
  onOpenModel: (modelConfigId: string) => void;
  onCancel: (runId: string) => void;
  onRetry: (runId: string) => void;
};

export function RunListTable({
  t,
  runs,
  loading,
  currentPage,
  pageSize,
  total,
  taskMap,
  modelNameMap,
  runStatusLabel,
  onPageChange,
  onOpenDetail,
  onOpenTask,
  onOpenModel,
  onCancel,
  onRetry,
}: RunListTableProps) {
  const columns: ColumnsType<TaskRunRecord> = [
    {
      title: t('runs.column.runId'),
      dataIndex: 'id',
      width: 260,
      render: (value: string) => <Typography.Text code>{value.slice(0, 12)}</Typography.Text>,
    },
    {
      title: t('runs.column.task'),
      dataIndex: 'task_id',
      render: (value: string) => (
        <Button type="link" size="small" onClick={() => onOpenTask(value)}>
          {taskMap.get(value)?.title || value}
        </Button>
      ),
    },
    {
      title: t('common.status'),
      dataIndex: 'status',
      width: 120,
      render: (status: TaskRunStatus) => (
        <Tag color={runColorMap[status]}>{runStatusLabel(status)}</Tag>
      ),
    },
    {
      title: t('runs.column.modelConfig'),
      dataIndex: 'model_config_id',
      width: 220,
      render: (value: string) => (
        <Button
          type="link"
          size="small"
          style={{ paddingInline: 0 }}
          onClick={() => onOpenModel(value)}
        >
          {modelNameMap.get(value) || value}
        </Button>
      ),
    },
    {
      title: t('runs.column.startedAt'),
      dataIndex: 'started_at',
      width: 180,
      render: (value?: string | null) =>
        value ? dayjs(value).format('YYYY-MM-DD HH:mm:ss') : '-',
    },
    {
      title: t('runs.column.finishedAt'),
      dataIndex: 'finished_at',
      width: 180,
      render: (value?: string | null) =>
        value ? dayjs(value).format('YYYY-MM-DD HH:mm:ss') : '-',
    },
    {
      title: t('common.actions'),
      key: 'actions',
      width: 220,
      render: (_, record) => (
        <Space>
          <Button size="small" onClick={() => onOpenDetail(record.id)}>
            {t('common.detail')}
          </Button>
          <Button
            size="small"
            disabled={record.status !== 'queued' && record.status !== 'running'}
            onClick={() => onCancel(record.id)}
          >
            {t('runs.action.cancel')}
          </Button>
          <Button
            size="small"
            disabled={record.status === 'queued' || record.status === 'running'}
            onClick={() => onRetry(record.id)}
          >
            {t('runs.action.retry')}
          </Button>
        </Space>
      ),
    },
  ];

  return (
    <Table<TaskRunRecord>
      rowKey="id"
      loading={loading}
      columns={columns}
      dataSource={runs}
      pagination={{
        current: currentPage,
        pageSize,
        total,
        showSizeChanger: true,
        onChange: onPageChange,
      }}
      locale={{
        emptyText: (
          <Empty
            image={Empty.PRESENTED_IMAGE_SIMPLE}
            description={t('runs.empty')}
          />
        ),
      }}
    />
  );
}
