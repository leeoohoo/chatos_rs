// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import {
  ApiOutlined,
  AuditOutlined,
  BranchesOutlined,
  GlobalOutlined,
  KeyOutlined,
  LogoutOutlined,
  MonitorOutlined,
  RocketOutlined,
  RobotOutlined,
  SafetyCertificateOutlined,
  ShopOutlined,
  SolutionOutlined,
} from '@ant-design/icons';
import type { ReactNode } from 'react';
import { Button, Layout, Menu, Segmented, Space, Typography } from 'antd';

import { clearAuthToken } from '../api/client';
import { useI18n } from '../i18n/I18nProvider';
import type { CurrentUser } from '../types';

export type AppSection =
  | 'mcps'
  | 'marketplaces'
  | 'publishers'
  | 'diagnostics'
  | 'cloud-oauth'
  | 'plugins'
  | 'releases'
  | 'agents'
  | 'runtime'
  | 'audit';

interface AppShellProps {
  user: CurrentUser;
  section: AppSection;
  onSectionChange: (section: AppSection) => void;
  children: ReactNode;
}

export function AppShell({ user, section, onSectionChange, children }: AppShellProps) {
  const { locale, setLocale, t } = useI18n();
  const menuItems = [
    { key: 'mcps', icon: <ApiOutlined />, label: t('nav.mcps') },
    { key: 'marketplaces', icon: <ShopOutlined />, label: t('nav.pluginMarketplaces') },
    { key: 'publishers', icon: <SolutionOutlined />, label: t('nav.pluginPublishers') },
    { key: 'diagnostics', icon: <MonitorOutlined />, label: t('nav.pluginDiagnostics') },
    { key: 'cloud-oauth', icon: <KeyOutlined />, label: t('nav.pluginCloudOAuth') },
    ...(user.role === 'super_admin'
      ? [
          { key: 'plugins', icon: <SafetyCertificateOutlined />, label: t('nav.plugins') },
          { key: 'releases', icon: <RocketOutlined />, label: t('nav.pluginReleases') },
          { key: 'agents', icon: <RobotOutlined />, label: t('nav.agents') },
          { key: 'runtime', icon: <BranchesOutlined />, label: t('nav.runtime') },
          { key: 'audit', icon: <AuditOutlined />, label: t('nav.pluginAudit') },
        ]
      : []),
  ];

  return (
    <Layout className="app-shell">
      <Layout.Sider width={248} theme="light" className="app-sider">
        <div className="brand">
          <Typography.Title level={4}>{t('app.title')}</Typography.Title>
          <Typography.Text type="secondary">{t('app.subtitle')}</Typography.Text>
        </div>
        <Menu
          mode="inline"
          selectedKeys={[section]}
          items={menuItems}
          onClick={(info) => onSectionChange(info.key as AppSection)}
        />
      </Layout.Sider>
      <Layout>
        <Layout.Header className="app-header">
          <Space size={12} className="header-actions">
            <GlobalOutlined className="header-language-icon" />
            <Segmented
              size="small"
              value={locale}
              options={[
                { label: t('language.zh'), value: 'zh-CN' },
                { label: t('language.en'), value: 'en-US' },
              ]}
              onChange={(value) => setLocale(value as 'zh-CN' | 'en-US')}
            />
          </Space>
          <Space direction="vertical" size={1} className="header-user">
            <Typography.Text strong ellipsis={{ tooltip: user.display_name || user.username }}>
              {user.display_name || user.username}
            </Typography.Text>
            <Typography.Text type="secondary" className="header-role">
              {t(`role.${user.role}`)}
            </Typography.Text>
          </Space>
          <Button
            icon={<LogoutOutlined />}
            onClick={() => {
              clearAuthToken();
              window.location.reload();
            }}
          >
            {t('common.logout')}
          </Button>
        </Layout.Header>
        <Layout.Content className="app-content">{children}</Layout.Content>
      </Layout>
    </Layout>
  );
}
