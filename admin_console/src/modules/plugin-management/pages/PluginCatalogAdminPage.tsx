// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { PlusOutlined, RocketOutlined } from '@ant-design/icons';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { Alert, Button, Space, Table, Tag, Typography } from 'antd';
import type { ColumnsType } from 'antd/es/table';
import { lazy, Suspense, useMemo, useState } from 'react';

import { api } from '../api/client';
import { CompactId, DateTimeCell } from '../components/DisplayCells';
import { EnabledTag } from '../components/Tags';
import { useI18n } from '../i18n/I18nProvider';
import type { PluginCatalogListItem, PluginRuntimeTarget } from '../pluginTypes';
import type { CurrentUser } from '../types';

const PluginPublishWizard = lazy(() => import('./catalogForm/PluginPublishWizard').then((module) => ({ default: module.PluginPublishWizard })));

interface PluginCatalogAdminPageProps {
  user: CurrentUser;
  onOpenReleases: (pluginId: string) => void;
}

const RUNTIME_TARGET_ORDER: PluginRuntimeTarget[] = ['local_connector'];
const RUNTIME_TARGET_COLORS: Record<PluginRuntimeTarget, string> = {
  local_connector: 'purple',
};

type PluginCatalogCursor = {
  featured: boolean;
  category: string;
  displayName: string;
  id: string;
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
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState(12);
  const [pageCursors, setPageCursors] = useState<Array<PluginCatalogCursor | null>>([null]);
  const isAdmin = user.role === 'super_admin';
  const cursor = pageCursors[page - 1] || null;
  const pluginsQuery = useQuery({
    queryKey: ['plugin-management', 'admin-plugins', 'catalog-page', pageSize, cursor],
    queryFn: () => api.listAdminPlugins({
      limit: pageSize + 1,
      ...(cursor ? {
        after_featured: cursor.featured,
        after_category: cursor.category,
        after_display_name: cursor.displayName,
        after_id: cursor.id,
      } : {}),
    }),
    enabled: isAdmin,
  });
  const visiblePlugins = (pluginsQuery.data?.items || []).slice(0, pageSize);
  const canAdvance = (pluginsQuery.data?.items.length || 0) > pageSize;
  const maxReachablePage = Math.max(
    pageCursors.length,
    page === pageCursors.length && canAdvance ? page + 1 : page,
  );
  const resetPagination = (nextPageSize = pageSize) => {
    setPage(1);
    setPageSize(nextPageSize);
    setPageCursors([null]);
  };
  const changePage = (nextPage: number, nextPageSize: number) => {
    if (nextPageSize !== pageSize) {
      resetPagination(nextPageSize);
      return;
    }
    if (nextPage < 1 || nextPage === page) return;
    if (nextPage <= pageCursors.length) {
      setPage(nextPage);
      return;
    }
    if (nextPage !== page + 1 || page !== pageCursors.length || !canAdvance) return;
    const lastPlugin = visiblePlugins[visiblePlugins.length - 1];
    if (!lastPlugin) return;
    setPageCursors((current) => [
      ...current.slice(0, page),
      {
        featured: lastPlugin.featured,
        category: lastPlugin.interface.category,
        displayName: lastPlugin.display_name,
        id: lastPlugin.id,
      },
    ]);
    setPage(nextPage);
  };
  const marketplacesQuery = useQuery({
    queryKey: ['plugin-management', 'plugin-marketplaces'],
    queryFn: api.listPluginMarketplaces,
    enabled: isAdmin && modalOpen,
  });
  const publishersQuery = useQuery({
    queryKey: ['plugin-management', 'plugin-publishers', 'admin'],
    queryFn: () => api.listAdminPluginPublishers({ limit: 500 }),
    enabled: isAdmin && modalOpen,
  });
  const wizardPluginsQuery = useQuery({
    queryKey: ['plugin-management', 'admin-plugins', 'publish-wizard'],
    queryFn: () => api.listAdminPlugins({ limit: 500 }),
    enabled: isAdmin && modalOpen,
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
        dataSource={visiblePlugins}
        loading={pluginsQuery.isLoading || pluginsQuery.isFetching}
        scroll={{ x: 1450 }}
        pagination={{
          current: page,
          pageSize,
          total: pluginsQuery.data?.total || 0,
          showSizeChanger: true,
          showLessItems: true,
          onChange: changePage,
          itemRender: (pageNumber, type, originalElement) => {
            if (type === 'page' && pageNumber > maxReachablePage) {
              return <span aria-disabled="true">{pageNumber}</span>;
            }
            if (type === 'jump-prev' || type === 'jump-next') {
              return <span aria-disabled="true">•••</span>;
            }
            return originalElement;
          },
        }}
      />
      {modalOpen ? <Suspense fallback={null}><PluginPublishWizard
        open={modalOpen}
        marketplaces={marketplacesQuery.data?.items || []}
        plugins={wizardPluginsQuery.data?.items || []}
        publishers={publishersQuery.data?.items || []}
        onClose={() => setModalOpen(false)}
        onPublished={(result) => {
          queryClient.invalidateQueries({ queryKey: ['plugin-management', 'admin-plugins'] });
          queryClient.invalidateQueries({ queryKey: ['plugin-management', 'plugin-releases', result.catalog.id] });
          onOpenReleases(result.catalog.id);
        }}
      /></Suspense> : null}
    </div>
  );
}
