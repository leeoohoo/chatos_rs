// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useQuery } from '@tanstack/react-query';
import {
  Button,
  Empty,
  Space,
  Table,
  Tag,
  Typography,
} from 'antd';
import type { ColumnsType } from 'antd/es/table';
import { ReloadOutlined } from '@ant-design/icons';
import dayjs from 'dayjs';

import { api } from '../api/client';
import { useI18n } from '../i18n/I18nProvider';
import type { UserSummaryRecord } from '../types';

export function UsersPage() {
  const { t } = useI18n();
  const usersQuery = useQuery({
    queryKey: ['users'],
    queryFn: () => api.listUsers(),
  });

  const rows = (usersQuery.data || []).filter(
    (row) => row.principal_type === 'agent_account',
  );

  const columns: ColumnsType<UserSummaryRecord> = [
    {
      title: t('users.column.agent'),
      dataIndex: 'username',
      render: (_, record) => (
        <Space direction="vertical" size={0}>
          <Typography.Text strong>{record.display_name || record.username}</Typography.Text>
          <Typography.Text type="secondary">{record.username}</Typography.Text>
        </Space>
      ),
    },
    {
      title: t('users.column.owner'),
      dataIndex: 'owner_user_id',
      width: 220,
      render: (_, record) => (
        <Space direction="vertical" size={0}>
          <Typography.Text>
            {record.owner_display_name || record.owner_username || record.owner_user_id || '-'}
          </Typography.Text>
          {record.owner_username ? (
            <Typography.Text type="secondary">{record.owner_username}</Typography.Text>
          ) : null}
        </Space>
      ),
    },
    {
      title: t('common.status'),
      dataIndex: 'enabled',
      width: 110,
      render: (enabled: boolean) => (
        <Tag color={enabled ? 'success' : 'default'}>
          {enabled ? t('users.enabled') : t('users.disabled')}
        </Tag>
      ),
    },
    {
      title: t('users.lastLogin'),
      dataIndex: 'last_login_at',
      width: 180,
      render: (value?: string | null) =>
        value ? dayjs(value).format('YYYY-MM-DD HH:mm:ss') : '-',
    },
    {
      title: t('models.detail.createdAt'),
      dataIndex: 'created_at',
      width: 180,
      render: (value: string) => dayjs(value).format('YYYY-MM-DD HH:mm:ss'),
    },
    {
      title: t('common.updatedAt'),
      dataIndex: 'updated_at',
      width: 180,
      render: (value: string) => dayjs(value).format('YYYY-MM-DD HH:mm:ss'),
    },
  ];

  return (
    <Space direction="vertical" size="large" style={{ width: '100%' }}>
      <div
        style={{
          display: 'flex',
          alignItems: 'flex-start',
          justifyContent: 'space-between',
          gap: 16,
          width: '100%',
        }}
      >
        <Space direction="vertical" size={0}>
          <Typography.Title level={3} style={{ margin: 0 }}>
            {t('users.title')}
          </Typography.Title>
          <Typography.Text type="secondary">{t('users.subtitle')}</Typography.Text>
        </Space>
        <Button
          icon={<ReloadOutlined />}
          onClick={() => usersQuery.refetch()}
          loading={usersQuery.isFetching}
        >
          {t('common.refresh')}
        </Button>
      </div>

      <Table<UserSummaryRecord>
        rowKey="id"
        columns={columns}
        dataSource={rows}
        loading={usersQuery.isLoading}
        pagination={{ pageSize: 10, showSizeChanger: true }}
        locale={{
          emptyText: (
            <Empty
              image={Empty.PRESENTED_IMAGE_SIMPLE}
              description={t('users.empty')}
            />
          ),
        }}
      />
    </Space>
  );
}
