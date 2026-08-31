// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { EyeOutlined, StopOutlined } from '@ant-design/icons';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { Alert, Button, Modal, Popconfirm, Select, Space, Table, Tag, Typography, message } from 'antd';
import type { ColumnsType } from 'antd/es/table';
import { useEffect, useMemo, useState } from 'react';

import { api } from '../api/client';
import { CompactId, DateTimeCell } from '../components/DisplayCells';
import { useI18n } from '../i18n/I18nProvider';
import type { PluginReleaseRecord } from '../pluginTypes';
import type { CurrentUser } from '../types';
import { jsonText } from './formUtils';

interface PluginReleasesPageProps {
  user: CurrentUser;
  initialPluginId?: string | null;
}

export function PluginReleasesPage({ user, initialPluginId }: PluginReleasesPageProps) {
  const { t } = useI18n();
  const queryClient = useQueryClient();
  const [pluginId, setPluginId] = useState(initialPluginId || '');
  const [detail, setDetail] = useState<PluginReleaseRecord | null>(null);
  const isAdmin = user.role === 'super_admin';
  useEffect(() => {
    if (initialPluginId) {
      setPluginId(initialPluginId);
    }
  }, [initialPluginId]);
  const pluginsQuery = useQuery({
    queryKey: ['plugin-management', 'admin-plugins'],
    queryFn: () => api.listAdminPlugins({ limit: 500 }),
    enabled: isAdmin,
  });
  useEffect(() => {
    if (!pluginId && pluginsQuery.data?.items[0]?.id) {
      setPluginId(pluginsQuery.data.items[0].id);
    }
  }, [pluginId, pluginsQuery.data]);
  const releasesQuery = useQuery({
    queryKey: ['plugin-management', 'plugin-releases', pluginId],
    queryFn: () => api.listPluginReleases(pluginId),
    enabled: isAdmin && Boolean(pluginId),
  });
  const revokeMutation = useMutation({
    mutationFn: api.revokePluginRelease,
    onSuccess: () => {
      message.success(t('pluginRelease.revoked'));
      queryClient.invalidateQueries({ queryKey: ['plugin-management', 'plugin-releases', pluginId] });
      queryClient.invalidateQueries({ queryKey: ['plugin-management', 'admin-plugins'] });
    },
    onError: (error) => message.error((error as Error).message),
  });
  const columns = useMemo<ColumnsType<PluginReleaseRecord>>(
    () => [
      {
        title: t('pluginRelease.version'),
        dataIndex: 'version',
        width: 120,
        render: (value, record) => (
          <Space>
            <Typography.Text strong>{value}</Typography.Text>
            <Tag color={record.release_channel === 'stable' ? 'green' : record.release_channel === 'beta' ? 'gold' : 'purple'}>
              {record.release_channel}
            </Tag>
          </Space>
        ),
      },
      {
        title: t('pluginRelease.artifactHash'),
        dataIndex: 'artifact_sha256',
        width: 220,
        render: (value) => <CompactId value={value} />,
      },
      {
        title: t('pluginRelease.signingKey'),
        dataIndex: ['signature', 'key_id'],
        width: 180,
        render: (value) => <CompactId value={value} />,
      },
      {
        title: t('pluginRelease.components'),
        dataIndex: 'components',
        width: 110,
        render: (components) => components?.length || 0,
      },
      {
        title: t('pluginRelease.publishedAt'),
        dataIndex: 'published_at',
        width: 180,
        render: (value) => <DateTimeCell value={value} />,
      },
      {
        title: t('table.status'),
        dataIndex: 'revoked_at',
        width: 110,
        render: (value) => (
          <Tag color={value ? 'red' : 'green'}>
            {t(value ? 'pluginRelease.statusRevoked' : 'pluginRelease.statusActive')}
          </Tag>
        ),
      },
      {
        title: t('table.actions'),
        key: 'actions',
        width: 190,
        render: (_, record) => (
          <Space>
            <Button size="small" icon={<EyeOutlined />} onClick={() => setDetail(record)}>
              {t('common.view')}
            </Button>
            {!record.revoked_at ? (
              <Popconfirm
                title={t('pluginRelease.revokeConfirm')}
                onConfirm={() => revokeMutation.mutate(record.id)}
              >
                <Button danger size="small" icon={<StopOutlined />}>
                  {t('pluginRelease.revoke')}
                </Button>
              </Popconfirm>
            ) : null}
          </Space>
        ),
      },
    ],
    [revokeMutation, t],
  );

  if (!isAdmin) {
    return <Alert type="error" showIcon message={t('admin.only')} />;
  }

  return (
    <div className="page">
      <div className="page-toolbar">
        <Space direction="vertical" size={4}>
          <Typography.Title level={3}>{t('pluginRelease.title')}</Typography.Title>
          <Typography.Text type="secondary">{t('pluginRelease.description')}</Typography.Text>
          <Select
            showSearch
            optionFilterProp="label"
            value={pluginId || undefined}
            placeholder={t('pluginRelease.selectPlugin')}
            style={{ width: 360 }}
            options={(pluginsQuery.data?.items || []).map((plugin) => ({
              value: plugin.id,
              label: `${plugin.display_name} (${plugin.name})`,
            }))}
            onChange={setPluginId}
          />
        </Space>
      </div>
      <Table
        rowKey="id"
        columns={columns}
        dataSource={releasesQuery.data?.items || []}
        loading={releasesQuery.isLoading}
        scroll={{ x: 1250 }}
        pagination={false}
      />
      <Modal
        title={detail ? `${detail.version} · ${t('pluginRelease.details')}` : t('pluginRelease.details')}
        open={Boolean(detail)}
        onCancel={() => setDetail(null)}
        footer={null}
        width={960}
        destroyOnClose
      >
        {detail ? <pre className="json-preview">{jsonText(detail)}</pre> : null}
      </Modal>
    </div>
  );
}
