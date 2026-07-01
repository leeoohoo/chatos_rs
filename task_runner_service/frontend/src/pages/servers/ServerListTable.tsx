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
import type { RemoteServerRecord } from '../../types';
import {
  getAuthTypeLabel,
  renderTestStatus,
  serverCreatorLabel,
  serverOwnerLabel,
} from './serverPageUtils';

type ServerListTableProps = {
  t: TranslateFn;
  servers: RemoteServerRecord[];
  loading: boolean;
  testing: boolean;
  testingServerId: string | null;
  onOpenDetail: (serverId: string) => void;
  onOpenEdit: (server: RemoteServerRecord) => void;
  onTest: (serverId: string) => void;
  onDelete: (server: RemoteServerRecord) => void;
};

export function ServerListTable({
  t,
  servers,
  loading,
  testing,
  testingServerId,
  onOpenDetail,
  onOpenEdit,
  onTest,
  onDelete,
}: ServerListTableProps) {
  const columns: ColumnsType<RemoteServerRecord> = [
    {
      title: t('servers.column.server'),
      dataIndex: 'name',
      width: 260,
      render: (_, record) => (
        <Space direction="vertical" size={0}>
          <Button type="link" style={{ padding: 0 }} onClick={() => onOpenDetail(record.id)}>
            <Typography.Text strong>{record.name}</Typography.Text>
          </Button>
          <Typography.Text type="secondary">
            {record.username}@{record.host}:{record.port}
          </Typography.Text>
        </Space>
      ),
    },
    {
      title: t('servers.column.authType'),
      dataIndex: 'auth_type',
      width: 140,
      render: (value: string) => getAuthTypeLabel(value, t),
    },
    {
      title: t('servers.column.defaultDir'),
      dataIndex: 'default_remote_path',
      width: 220,
      render: (value?: string | null) => value || '-',
      ellipsis: true,
    },
    {
      title: t('servers.column.source'),
      key: 'source',
      width: 170,
      render: (_, record) => (
        <Space direction="vertical" size={0}>
          <Typography.Text>{serverCreatorLabel(record)}</Typography.Text>
          {record.task_id ? (
            <Typography.Text type="secondary" code>
              {record.task_id.slice(0, 8)}
            </Typography.Text>
          ) : null}
        </Space>
      ),
    },
    {
      title: t('servers.column.owner'),
      key: 'owner',
      width: 170,
      render: (_, record) => serverOwnerLabel(record),
    },
    {
      title: t('servers.column.hostKeyPolicy'),
      dataIndex: 'host_key_policy',
      width: 120,
      render: (value: string) => (
        <Tag color={value === 'strict' ? 'blue' : 'default'}>{value}</Tag>
      ),
    },
    {
      title: t('servers.column.lastTest'),
      key: 'last_test',
      width: 240,
      render: (_, record) => (
        <Space direction="vertical" size={0}>
          {renderTestStatus(record.last_test_status, t)}
          <Typography.Text type="secondary">
            {record.last_tested_at
              ? dayjs(record.last_tested_at).format('YYYY-MM-DD HH:mm:ss')
              : t('servers.untested')}
          </Typography.Text>
        </Space>
      ),
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
      width: 300,
      render: (_, record) => (
        <Space>
          <Button size="small" onClick={() => onOpenDetail(record.id)}>
            {t('common.detail')}
          </Button>
          <Button size="small" onClick={() => onOpenEdit(record)}>
            {t('common.edit')}
          </Button>
          <Button
            size="small"
            loading={testing && testingServerId === record.id}
            onClick={() => onTest(record.id)}
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
    <Table<RemoteServerRecord>
      rowKey="id"
      columns={columns}
      dataSource={servers}
      loading={loading}
      pagination={{ pageSize: 8 }}
      scroll={{ x: 1400 }}
      locale={{
        emptyText: (
          <Empty
            image={Empty.PRESENTED_IMAGE_SIMPLE}
            description={t('servers.empty')}
          />
        ),
      }}
    />
  );
}
