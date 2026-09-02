// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import {
  Button,
  Empty,
  Table,
  Tag,
  Typography,
} from 'antd';
import type { ColumnsType } from 'antd/es/table';
import dayjs from 'dayjs';

import type { TranslateFn } from '../../i18n/I18nProvider';
import type {
  TaskSummaryRecord,
  AskUserPromptRecord,
  AskUserPromptStatus,
} from '../../types';
import { promptColorMap } from './promptPageUtils';

type PromptListTableProps = {
  t: TranslateFn;
  prompts: AskUserPromptRecord[];
  loading: boolean;
  currentPage: number;
  pageSize: number;
  total: number;
  taskMap: Map<string, TaskSummaryRecord>;
  promptStatusLabel: (status: AskUserPromptStatus) => string;
  onOpenTask: (taskId: string) => void;
  onOpenRun: (runId: string) => void;
  onOpenPrompt: (promptId: string) => void;
  onPageChange: (page: number, pageSize: number) => void;
};

export function PromptListTable({
  t,
  prompts,
  loading,
  currentPage,
  pageSize,
  total,
  taskMap,
  promptStatusLabel,
  onOpenTask,
  onOpenRun,
  onOpenPrompt,
  onPageChange,
}: PromptListTableProps) {
  function promptOwnerLabel(taskId?: string | null): string {
    if (!taskId) {
      return '-';
    }
    const task = taskMap.get(taskId);
    return task?.creator_display_name || task?.creator_username || task?.creator_user_id || '-';
  }

  const columns: ColumnsType<AskUserPromptRecord> = [
    {
      title: t('prompts.column.promptId'),
      dataIndex: 'id',
      width: 180,
      render: (value: string) => <Typography.Text code>{value.slice(0, 12)}</Typography.Text>,
    },
    {
      title: t('prompts.column.title'),
      dataIndex: 'title',
      render: (_, record) => record.title || record.message || record.kind,
    },
    {
      title: t('prompts.column.task'),
      dataIndex: 'task_id',
      render: (value?: string | null) =>
        value ? (
          <Button
            type="link"
            size="small"
            onClick={() => onOpenTask(value)}
          >
            {taskMap.get(value)?.title || value}
          </Button>
        ) : (
          '-'
        ),
    },
    {
      title: t('runs.column.owner'),
      dataIndex: 'task_id',
      width: 160,
      render: (value?: string | null) => promptOwnerLabel(value),
    },
    {
      title: t('prompts.column.run'),
      dataIndex: 'run_id',
      width: 180,
      render: (value?: string | null) =>
        value ? (
          <Button
            type="link"
            size="small"
            onClick={() => onOpenRun(value)}
          >
            <Typography.Text code>{value.slice(0, 12)}</Typography.Text>
          </Button>
        ) : (
          '-'
        ),
    },
    {
      title: t('common.status'),
      dataIndex: 'status',
      width: 120,
      render: (status: AskUserPromptStatus) => (
        <Tag color={promptColorMap[status]}>{promptStatusLabel(status)}</Tag>
      ),
    },
    {
      title: t('common.updatedAt'),
      dataIndex: 'updated_at',
      width: 180,
      render: (value: string) => dayjs(value).format('YYYY-MM-DD HH:mm:ss'),
    },
    {
      title: t('common.actions'),
      key: 'actions',
      width: 120,
      render: (_, record) => (
        <Button size="small" onClick={() => onOpenPrompt(record.id)}>
          {record.status === 'pending' ? t('prompts.action.handle') : t('common.view')}
        </Button>
      ),
    },
  ];

  return (
    <Table<AskUserPromptRecord>
      rowKey="id"
      loading={loading}
      columns={columns}
      dataSource={prompts}
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
            description={t('prompts.empty')}
          />
        ),
      }}
    />
  );
}
