// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { createElement } from 'react';
import { Outlet, useLocation, useNavigate } from 'react-router-dom';
import { Button, Layout, Menu, Space, Typography } from 'antd';
import {
  CloudServerOutlined,
  LogoutOutlined,
  RobotOutlined,
  SettingOutlined,
  TeamOutlined,
  UserOutlined,
} from '@ant-design/icons';

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
  const items = [
    {
      key: '/models',
      label: 'Model Configs',
      icon: <CloudServerOutlined />,
    },
    {
      key: '/users',
      label: 'Users',
      icon: <TeamOutlined />,
    },
    {
      key: '/agents',
      label: 'Agent Accounts',
      icon: <RobotOutlined />,
    },
    {
      key: '/settings',
      label: 'Settings',
      icon: <SettingOutlined />,
    },
  ];

  return (
    <StandardAdminAppShell
      brandTitle="User Service"
      brandSubtitle="Unified users, agents, and model configs"
      headerSummary="Central entry for human users, agent accounts, and shared model config governance"
      navItems={items}
      currentUser={currentUser}
      logoutLabel="Logout"
      logoutLoading={logoutLoading}
      onLogout={onLogout}
    />
  );
}
