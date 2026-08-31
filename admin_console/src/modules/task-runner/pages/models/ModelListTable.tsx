// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

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
import type { ModelConfigRecord } from '../../types';

type ModelListTableProps = {
  t: TranslateFn;
  models: ModelConfigRecord[];
  loading: boolean;
  taskCountByModelId: Map<string, number>;
  runCountByModelId: Map<string, number>;
  testing: boolean;
  onOpenDetail: (modelId: string) => void;
  onOpenEdit: (model: ModelConfigRecord) => void;
  onDelete: (model: ModelConfigRecord) => void;
  onTest: (modelId: string) => void;
  onViewTasks: (modelId: string) => void;
  onViewRuns: (modelId: string) => void;
};

export function ModelListTable({
  t,
  models,
  loading,
  taskCountByModelId,
  runCountByModelId,
  testing,
  onOpenDetail,
  onOpenEdit,
  onDelete,
  onTest,
  onViewTasks,
  onViewRuns,
}: ModelListTableProps) {
  const columns: ColumnsType<ModelConfigRecord> = [
    {
      title: t('models.column.name'),
      dataIndex: 'name',
      render: (_, record) => (
        <Space direction="vertical" size={0}>
          <Button type="link" style={{ padding: 0 }} onClick={() => onOpenDetail(record.id)}>
            <Typography.Text strong>{record.name}</Typography.Text>
          </Button>
          <Typography.Text type="secondary">{record.model}</Typography.Text>
        </Space>
      ),
    },
    {
      title: 'Provider',
      dataIndex: 'provider',
      width: 140,
    },
    {
      title: t('models.column.owner'),
      dataIndex: 'owner_user_id',
      width: 180,
      render: (_, record) => {
        const displayName = record.owner_display_name?.trim();
        const username = record.owner_username?.trim();
        const ownerId = record.owner_user_id?.trim();
        if (!displayName && !username && !ownerId) {
          return '-';
        }
        return (
          <Space direction="vertical" size={0}>
            <Typography.Text>{displayName || username || ownerId}</Typography.Text>
            {username && username !== displayName ? (
              <Typography.Text type="secondary">{username}</Typography.Text>
            ) : null}
          </Space>
        );
      },
    },
    {
      title: t('models.column.usageScenario'),
      dataIndex: 'usage_scenario',
      width: 240,
      ellipsis: true,
      render: (value?: string | null) => value || '-',
    },
    {
      title: 'Base URL',
      dataIndex: 'base_url',
      width: 280,
      ellipsis: true,
    },
    {
      title: 'Responses',
      dataIndex: 'supports_responses',
      width: 120,
      render: (value: boolean) => (value ? t('common.yes') : t('common.no')),
    },
    {
      title: t('models.column.boundTasks'),
      key: 'task_count',
      width: 120,
      render: (_, record) => taskCountByModelId.get(record.id) || 0,
    },
    {
      title: t('models.column.runCount'),
      key: 'run_count',
      width: 120,
      render: (_, record) => runCountByModelId.get(record.id) || 0,
    },
    {
      title: t('common.status'),
      dataIndex: 'enabled',
      width: 120,
      render: (value: boolean) => (
        <Tag color={value ? 'success' : 'default'}>
          {value ? t('common.enabled') : t('common.disabled')}
        </Tag>
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
      width: 420,
      render: (_, record) => (
        <Space>
          <Button size="small" onClick={() => onOpenDetail(record.id)}>
            {t('common.detail')}
          </Button>
          <Button size="small" onClick={() => onViewTasks(record.id)}>
            {t('tasks.title')}
          </Button>
          <Button size="small" onClick={() => onViewRuns(record.id)}>
            {t('prompts.column.run')}
          </Button>
          <Button size="small" onClick={() => onOpenEdit(record)}>
            {t('common.edit')}
          </Button>
          <Button
            size="small"
            onClick={() => onTest(record.id)}
            loading={testing}
          >
            {t('common.test')}
          </Button>
          <Button size="small" danger onClick={() => onDelete(record)}>
            {t('common.delete')}
          </Button>
        </Space>
      ),
    },
  ];

  return (
    <Table<ModelConfigRecord>
      rowKey="id"
      columns={columns}
      dataSource={models}
      loading={loading}
      pagination={{ pageSize: 8 }}
      locale={{
        emptyText: (
          <Empty
            image={Empty.PRESENTED_IMAGE_SIMPLE}
            description={t('models.empty')}
          />
        ),
      }}
    />
  );
}
