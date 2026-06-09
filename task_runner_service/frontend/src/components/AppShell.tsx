import { Outlet, useLocation, useNavigate } from 'react-router-dom';
import { Button, Layout, Menu, Space, Typography } from 'antd';
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
  UserOutlined,
} from '@ant-design/icons';

import type { AuthUser } from '../types';

const { Header, Sider, Content } = Layout;

const items = [
  {
    key: '/tasks',
    label: '任务',
    icon: <AppstoreOutlined />,
  },
  {
    key: '/models',
    label: '模型配置',
    icon: <DeploymentUnitOutlined />,
  },
  {
    key: '/servers',
    label: '服务器',
    icon: <DatabaseOutlined />,
  },
  {
    key: '/runs',
    label: '运行记录',
    icon: <PlayCircleOutlined />,
  },
  {
    key: '/prompts',
    label: '人工提示',
    icon: <BellOutlined />,
  },
  {
    key: '/mcp',
    label: 'MCP 目录',
    icon: <ToolOutlined />,
  },
  {
    key: '/tooling',
    label: '工具状态',
    icon: <FolderOpenOutlined />,
  },
  {
    key: '/users',
    label: '用户管理',
    icon: <TeamOutlined />,
  },
  {
    key: '/settings',
    label: '设置',
    icon: <SettingOutlined />,
  },
];

type AppShellProps = {
  currentUser: AuthUser;
  logoutLoading?: boolean;
  onLogout: () => void;
};

export function AppShell({ currentUser, logoutLoading, onLogout }: AppShellProps) {
  const location = useLocation();
  const navigate = useNavigate();

  return (
    <Layout style={{ minHeight: '100vh' }}>
      <Sider width={220} theme="light" style={{ borderRight: '1px solid #f0f0f0' }}>
        <div style={{ padding: '20px 20px 8px' }}>
          <Space direction="vertical" size={0}>
            <Typography.Title level={4} style={{ margin: 0 }}>
              Task Runner
            </Typography.Title>
            <Typography.Text type="secondary">
              独立任务执行服务
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
            任务管理、人工提示、模型配置、服务器清单、Memory 调试、共享工具状态、MCP 目录与运行回放
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
              退出
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
