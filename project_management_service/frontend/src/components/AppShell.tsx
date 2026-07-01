// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { FolderOutlined, ProjectOutlined, SettingOutlined } from '@ant-design/icons';
import { Layout, Menu, Space, Typography, Button } from 'antd';
import { Link, Outlet, useLocation, useNavigate } from 'react-router-dom';

import { clearAuthToken } from '../api/client';
import type { AuthUser } from '../types';

const { Content, Sider } = Layout;

interface AppShellProps {
  user: AuthUser;
}

export function AppShell({ user }: AppShellProps) {
  const navigate = useNavigate();
  const location = useLocation();
  const selectedKey = location.pathname.startsWith('/config') ? '/config' : '/projects';

  return (
    <Layout style={{ minHeight: '100vh' }}>
      <Sider width={232}>
        <div className="app-logo">
          <Space>
            <ProjectOutlined />
            <span>项目管理</span>
          </Space>
        </div>
        <Menu
          theme="dark"
          mode="inline"
          selectedKeys={[selectedKey]}
          items={[
            {
              key: '/projects',
              icon: <FolderOutlined />,
              label: <Link to="/projects">项目</Link>,
            },
            {
              key: '/config',
              icon: <SettingOutlined />,
              label: <Link to="/config">配置</Link>,
            },
          ]}
        />
      </Sider>
      <Layout>
        <Content>
          <div
            style={{
              height: 56,
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'space-between',
              padding: '0 24px',
              background: '#fff',
              borderBottom: '1px solid #edf0f5',
            }}
          >
            <Typography.Text strong>{user.display_name || user.username}</Typography.Text>
            <Button
              size="small"
              onClick={() => {
                clearAuthToken();
                navigate('/projects', { replace: true });
              }}
            >
              退出
            </Button>
          </div>
          <Outlet />
        </Content>
      </Layout>
    </Layout>
  );
}
