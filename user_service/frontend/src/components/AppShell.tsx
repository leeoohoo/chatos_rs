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

const { Header, Sider, Content } = Layout;

type AppShellProps = {
  currentUser: AuthUser;
  logoutLoading?: boolean;
  onLogout: () => void;
};

export function AppShell({ currentUser, logoutLoading, onLogout }: AppShellProps) {
  const location = useLocation();
  const navigate = useNavigate();
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
    <Layout style={{ minHeight: '100vh' }}>
      <Sider width={220} theme="light" style={{ borderRight: '1px solid #f0f0f0' }}>
        <div style={{ padding: '20px 20px 8px' }}>
          <Space direction="vertical" size={0}>
            <Typography.Title level={4} style={{ margin: 0 }}>
              User Service
            </Typography.Title>
            <Typography.Text type="secondary">
              Unified users, agents, and model configs
            </Typography.Text>
          </Space>
        </div>
        <Menu
          mode="inline"
          selectedKeys={[location.pathname]}
          items={items}
          onClick={({ key }) => navigate(key)}
          style={{ borderInlineEnd: 0 }}
        />
      </Sider>
      <Layout>
        <Header
          style={{
            background: '#fff',
            borderBottom: '1px solid #f0f0f0',
            padding: '0 24px',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
            gap: 16,
          }}
        >
          <Typography.Text type="secondary">
            Central entry for human users, agent accounts, and shared model config governance
          </Typography.Text>
          <Space size="middle" style={{ flexShrink: 0 }}>
            <Space size={6}>
              <UserOutlined />
              <Typography.Text>{currentUser.display_name || currentUser.username}</Typography.Text>
              <Typography.Text type="secondary">({currentUser.username})</Typography.Text>
            </Space>
            <Button
              size="small"
              icon={<LogoutOutlined />}
              loading={logoutLoading}
              onClick={onLogout}
            >
              Logout
            </Button>
          </Space>
        </Header>
        <Content style={{ padding: 24, background: '#f5f7fa' }}>
          <Outlet />
        </Content>
      </Layout>
    </Layout>
  );
}
