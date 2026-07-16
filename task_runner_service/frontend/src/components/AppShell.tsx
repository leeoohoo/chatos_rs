// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { createElement } from 'react';
import { Outlet, useLocation, useNavigate } from 'react-router-dom';
import { Button, Layout, Menu, Segmented, Space, Typography } from 'antd';
import {
  AppstoreOutlined,
  BellOutlined,
  DatabaseOutlined,
  DeploymentUnitOutlined,
  FolderOpenOutlined,
  LogoutOutlined,
  SettingOutlined,
  TeamOutlined,
  ToolOutlined,
  PlayCircleOutlined,
  ProjectOutlined,
  UserOutlined,
} from '@ant-design/icons';

import { useI18n } from '../i18n/I18nProvider';
import type { AuthUser } from '../types';
import { createStandardAdminAppShell } from '@chatos/frontend-runtime/antd';

const StandardAdminAppShell = createStandardAdminAppShell({
  createElement,
  Layout,
  Menu,
  Space,
  Typography,
  Button,
  Outlet,
  useLocation,
  useNavigate,
  UserIcon: UserOutlined,
  LogoutIcon: LogoutOutlined,
});

type AppShellProps = {
  currentUser: AuthUser;
  logoutLoading?: boolean;
  onLogout: () => void;
};

export function AppShell({ currentUser, logoutLoading, onLogout }: AppShellProps) {
  const { locale, setLocale, t } = useI18n();
  const isAdmin = currentUser.role === 'admin';
  const navItems = [
    {
      key: '/tasks',
      label: t('nav.tasks'),
      icon: <AppstoreOutlined />,
    },
    {
      key: '/projects',
      label: t('nav.projects'),
      icon: <ProjectOutlined />,
    },
    {
      key: '/models',
      label: t('nav.models'),
      icon: <DeploymentUnitOutlined />,
    },
    {
      key: '/servers',
      label: t('nav.servers'),
      icon: <DatabaseOutlined />,
    },
    {
      key: '/runs',
      label: t('nav.runs'),
      icon: <PlayCircleOutlined />,
    },
    {
      key: '/prompts',
      label: t('nav.prompts'),
      icon: <BellOutlined />,
    },
    {
      key: '/mcp',
      label: t('nav.mcp'),
      icon: <ToolOutlined />,
    },
    {
      key: '/tooling',
      label: t('nav.tooling'),
      icon: <FolderOpenOutlined />,
    },
    {
      key: '/users',
      label: t('nav.users'),
      icon: <TeamOutlined />,
      adminOnly: true,
    },
    {
      key: '/settings',
      label: t('nav.settings'),
      icon: <SettingOutlined />,
      adminOnly: true,
    },
  ];
  const items = navItems
    .filter((item) => isAdmin || !item.adminOnly)
    .map(({ adminOnly: _adminOnly, ...item }) => item);

  return (
    <StandardAdminAppShell
      brandTitle={t('app.brand')}
      brandSubtitle={t('app.subtitle')}
      headerSummary={t('app.headerSummary')}
      navItems={items}
      currentUser={currentUser}
      logoutLabel={t('auth.logout')}
      logoutLoading={logoutLoading}
      onLogout={onLogout}
      headerBeforeUser={
        <Segmented
          size="small"
          value={locale}
          onChange={(value) => setLocale(value === 'en-US' ? 'en-US' : 'zh-CN')}
          options={[
            { label: t('language.chinese'), value: 'zh-CN' },
            { label: t('language.english'), value: 'en-US' },
          ]}
          aria-label={t('common.language')}
        />
      }
    />
  );
}
