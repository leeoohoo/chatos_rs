// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { PlusOutlined, RocketOutlined } from '@ant-design/icons';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { Alert, Button, Space, Table, Tag, Typography } from 'antd';
import type { ColumnsType } from 'antd/es/table';
import { useMemo, useState } from 'react';

import { api } from '../api/client';
import { CompactId, DateTimeCell } from '../components/DisplayCells';
import { EnabledTag } from '../components/Tags';
import { useI18n } from '../i18n/I18nProvider';
import type { PluginCatalogListItem, PluginRuntimeTarget } from '../pluginTypes';
import type { CurrentUser } from '../types';
import { PluginPublishWizard } from './catalogForm/PluginPublishWizard';

interface PluginCatalogAdminPageProps {
  user: CurrentUser;
  onOpenReleases: (pluginId: string) => void;
}

const RUNTIME_TARGET_ORDER: PluginRuntimeTarget[] = ['local_connector'];
const RUNTIME_TARGET_COLORS: Record<PluginRuntimeTarget, string> = {
  local_connector: 'purple',
};

function renderRuntimeTargets(
  targets: PluginRuntimeTarget[] | undefined,
  latestReleaseId: string,
  t: (key: string, values?: Record<string, string | number>) => string,
) {
  if (!latestReleaseId) {
    return <Tag>{t('pluginCatalog.runtime.unpublished')}</Tag>;
  }
  const targetSet = new Set(targets || []);
  const orderedTargets = RUNTIME_TARGET_ORDER.filter((target) => targetSet.has(target));
  if (orderedTargets.length === 0) {
    return <Tag>{t('pluginCatalog.runtime.unknown')}</Tag>;
  }
  return (
    <Space size={[4, 4]} wrap>
      {orderedTargets.map((target) => (
        <Tag key={target} color={RUNTIME_TARGET_COLORS[target]}>
          {t(`pluginCatalog.runtime.${target}`)}
        </Tag>
      ))}
    </Space>
  );
}

export function PluginCatalogAdminPage({ user, onOpenReleases }: PluginCatalogAdminPageProps) {
  const { t } = useI18n();
  const queryClient = useQueryClient();
  const [modalOpen, setModalOpen] = useState(false);
  const isAdmin = user.role === 'super_admin';
  const pluginsQuery = useQuery({
    queryKey: ['plugin-management', 'admin-plugins'],
    queryFn: () => api.listAdminPlugins({ limit: 500 }),
    enabled: isAdmin,
  });
  const marketplacesQuery = useQuery({
    queryKey: ['plugin-management', 'plugin-marketplaces'],
    queryFn: api.listPluginMarketplaces,
    enabled: isAdmin,
  });
  const publishersQuery = useQuery({
    queryKey: ['plugin-management', 'plugin-publishers', 'admin'],
    queryFn: () => api.listAdminPluginPublishers({ limit: 500 }),
    enabled: isAdmin,
  });
  const columns = useMemo<ColumnsType<PluginCatalogListItem>>(
    () => [
      {
        title: t('table.name'),
        dataIndex: 'display_name',
        render: (_, record) => (
          <Space direction="vertical" size={0}>
            <Space>
              <Typography.Text strong>{record.display_name}</Typography.Text>
              {record.featured ? <Tag color="gold">{t('pluginCatalog.featured')}</Tag> : null}
            </Space>
            <Typography.Text type="secondary">{record.name}</Typography.Text>
            <Typography.Text type="secondary" ellipsis={{ tooltip: record.description }}>
              {record.description}
            </Typography.Text>
          </Space>
        ),
      },
      { title: t('pluginCatalog.marketplace'), dataIndex: 'marketplace_id', width: 160 },
      {
        title: t('pluginCatalog.publisher'),
        dataIndex: ['publisher', 'name'],
        width: 160,
        render: (_, record) => (
          <Space direction="vertical" size={0}>
            <Typography.Text>{record.publisher.name}</Typography.Text>
            <CompactId value={record.publisher.id} />
          </Space>
        ),
      },
      { title: t('pluginCatalog.category'), dataIndex: ['interface', 'category'], width: 150 },
      {
        title: t('pluginCatalog.runtimeTargets'),
        dataIndex: 'runtime_targets',
        width: 150,
        render: (_, record) => renderRuntimeTargets(record.runtime_targets, record.latest_release_id, t),
      },
      {
        title: t('pluginCatalog.license'),
        dataIndex: ['license', 'license_id'],
        width: 190,
        render: (_, record) => (
          <Space direction="vertical" size={0}>
            <CompactId value={record.license.license_id} />
            <Typography.Text type={record.license.redistributable ? 'success' : 'warning'}>
              {t(record.license.redistributable ? 'pluginCatalog.redistributable' : 'pluginCatalog.notRedistributable')}
            </Typography.Text>
          </Space>
        ),
      },
      {
        title: t('pluginCatalog.latestRelease'),
        dataIndex: 'latest_release_id',
        width: 180,
        render: (value) => <CompactId value={value} />,
      },
      {
        title: t('table.status'),
        dataIndex: 'enabled',
        width: 100,
        render: (enabled) => <EnabledTag enabled={enabled} />,
      },
      {
        title: t('table.updated'),
        dataIndex: 'updated_at',
        width: 170,
        render: (value) => <DateTimeCell value={value} />,
      },
      {
        title: t('table.actions'),
        key: 'actions',
        width: 130,
        render: (_, record) => (
          <Button icon={<RocketOutlined />} onClick={() => onOpenReleases(record.id)}>
            {t('pluginCatalog.releases')}
          </Button>
        ),
      },
    ],
    [onOpenReleases, t],
  );

  if (!isAdmin) {
    return <Alert type="error" showIcon message={t('admin.only')} />;
  }

  return (
    <div className="page">
      <div className="page-toolbar">
        <Space direction="vertical" size={0}>
          <Typography.Title level={3}>{t('pluginCatalog.title')}</Typography.Title>
          <Typography.Text type="secondary">{t('pluginCatalog.description')}</Typography.Text>
        </Space>
        <Button
          type="primary"
          icon={<PlusOutlined />}
          onClick={() => setModalOpen(true)}
        >
          {t('pluginCatalog.add')}
        </Button>
      </div>
      <Table
        rowKey="id"
        columns={columns}
        dataSource={pluginsQuery.data?.items || []}
        loading={pluginsQuery.isLoading}
        scroll={{ x: 1450 }}
        pagination={{ pageSize: 12 }}
      />
      <PluginPublishWizard
        open={modalOpen}
        marketplaces={marketplacesQuery.data?.items || []}
        plugins={pluginsQuery.data?.items || []}
        publishers={publishersQuery.data?.items || []}
        onClose={() => setModalOpen(false)}
        onPublished={(result) => {
          queryClient.invalidateQueries({ queryKey: ['plugin-management', 'admin-plugins'] });
          queryClient.invalidateQueries({ queryKey: ['plugin-management', 'plugin-releases', result.catalog.id] });
          onOpenReleases(result.catalog.id);
        }}
      />
    </div>
  );
}
