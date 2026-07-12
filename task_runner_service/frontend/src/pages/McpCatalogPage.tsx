// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useQuery } from '@tanstack/react-query';
import {
  Empty,
  Space,
  Tabs,
  Typography,
} from 'antd';

import { api } from '../api/client';
import { useI18n } from '../i18n/I18nProvider';
import { BuiltinMcpCatalogTab } from './mcpCatalog/BuiltinMcpCatalogTab';
import { ExternalMcpConfigTab } from './mcpCatalog/ExternalMcpConfigTab';
import { ExternalMcpServerCard } from './mcpCatalog/ExternalMcpServerCard';
import type { AuthUser } from '../types';

interface McpCatalogPageProps {
  currentUser: AuthUser;
}

export function McpCatalogPage({ currentUser }: McpCatalogPageProps) {
  const { t } = useI18n();
  const serverInfoQuery = useQuery({
    queryKey: ['mcp-server-info'],
    queryFn: api.getMcpServerInfo,
  });

  return (
    <Space direction="vertical" size="large" style={{ width: '100%' }}>
      <Space direction="vertical" size={0}>
        <Typography.Title level={3} style={{ margin: 0 }}>
          {t('mcpCatalog.title')}
        </Typography.Title>
        <Typography.Text type="secondary">
          {t('mcpCatalog.subtitle')}
        </Typography.Text>
      </Space>

      <Tabs
        items={[
          {
            key: 'external-server',
            label: t('mcpCatalog.tab.externalServer'),
            children: serverInfoQuery.data ? (
              <ExternalMcpServerCard
                info={serverInfoQuery.data}
                onRefresh={() => serverInfoQuery.refetch()}
              />
            ) : (
              <Empty
                image={Empty.PRESENTED_IMAGE_SIMPLE}
                description={t('common.noData')}
              />
            ),
          },
          {
            key: 'builtin',
            label: t('mcpCatalog.tab.builtin'),
            children: <BuiltinMcpCatalogTab />,
          },
          ...(currentUser.role === 'admin'
            ? [
                {
                  key: 'external-configs',
                  label: t('mcpCatalog.tab.externalConfigs'),
                  children: <ExternalMcpConfigTab />,
                },
              ]
            : []),
        ]}
      />
    </Space>
  );
}
