import {
  ApartmentOutlined,
  CloudServerOutlined,
  DatabaseOutlined,
  LogoutOutlined,
  MenuFoldOutlined,
  MenuUnfoldOutlined,
  ProjectOutlined,
  RobotOutlined,
  SettingOutlined,
  TeamOutlined,
  UserOutlined,
} from '@ant-design/icons';
import { Button, Layout, Menu, Space, Tag, Typography } from 'antd';
import { useMemo, useState } from 'react';
import { Outlet, useLocation, useNavigate } from 'react-router-dom';

import { useAdminAuth } from '../auth/AuthProvider';

const { Header, Sider, Content } = Layout;
const environmentLabel = import.meta.env.VITE_APP_ENV || 'production';

const menuItems = [
  { key: '/users', icon: <TeamOutlined />, label: '用户与模型', children: [
    { key: '/users/models', label: '模型配置' },
    { key: '/users/accounts', label: '用户账号' },
    { key: '/users/agents', label: 'Agent 账号' },
    { key: '/users/settings', label: '用户设置' },
  ] },
  { key: '/projects', icon: <ProjectOutlined />, label: '项目管理', children: [
    { key: '/projects/list', label: '项目列表' },
    { key: '/projects/config', label: '项目配置' },
  ] },
  { key: '/task-runner', icon: <RobotOutlined />, label: '任务执行', children: [
    { key: '/task-runner/tasks', label: '任务' },
    { key: '/task-runner/runs', label: '运行记录' },
    { key: '/task-runner/prompts', label: 'Prompt' },
    { key: '/task-runner/models', label: '执行模型' },
    { key: '/task-runner/projects', label: '执行项目' },
    { key: '/task-runner/mcp', label: 'MCP 与工具' },
    { key: '/task-runner/tooling', label: '工具运行时' },
    { key: '/task-runner/users', label: '执行用户' },
    { key: '/task-runner/settings', label: '执行设置' },
  ] },
  { key: '/plugins', icon: <ApartmentOutlined />, label: '插件与 MCP', children: [
    { key: '/plugins/mcp', label: 'MCP 目录' },
    { key: '/plugins/catalog', label: '插件目录' },
    { key: '/plugins/releases', label: '插件版本' },
    { key: '/plugins/marketplaces', label: '市场源' },
    { key: '/plugins/publishers', label: '发布者' },
    { key: '/plugins/agents', label: '系统 Agent' },
    { key: '/plugins/runtime', label: '运行时预览' },
    { key: '/plugins/diagnostics', label: '诊断' },
    { key: '/plugins/audit', label: '审计' },
  ] },
  { key: '/memory', icon: <DatabaseOutlined />, label: '记忆引擎', children: [
    { key: '/memory/dashboard', label: '总览' },
    { key: '/memory/data', label: '记忆数据' },
    { key: '/memory/sources', label: '来源' },
    { key: '/memory/models', label: '模型' },
    { key: '/memory/policies', label: '策略' },
    { key: '/memory/runs', label: '运行记录' },
  ] },
  { key: '/config', icon: <SettingOutlined />, label: '配置与运维', children: [
    { key: '/config/dashboard', label: '总览' },
    { key: '/config/definitions', label: '配置管理' },
    { key: '/config/releases', label: '发布历史' },
    { key: '/config/queues', label: '队列运维' },
    { key: '/config/instances', label: '服务实例' },
    { key: '/config/audit', label: '审计日志' },
  ] },
];

export function AdminLayout() {
  const { user, logout, logoutLoading } = useAdminAuth();
  const location = useLocation();
  const navigate = useNavigate();
  const [collapsed, setCollapsed] = useState(false);
  const selectedKey = useMemo(() => {
    const leafKeys = menuItems.flatMap((item) => item.children?.map((child) => child.key) || []);
    return leafKeys.sort((a, b) => b.length - a.length).find((key) => location.pathname.startsWith(key)) || location.pathname;
  }, [location.pathname]);

  return (
    <Layout className="admin-shell">
      <Sider width={248} collapsedWidth={72} collapsed={collapsed} theme="light" className="admin-sider">
        <div className="admin-brand">
          <CloudServerOutlined />
          {!collapsed ? <div><strong>ChatOS</strong><span>统一管理端</span></div> : null}
        </div>
        <Menu mode="inline" items={menuItems} selectedKeys={[selectedKey]} onClick={({ key }) => navigate(key)} className="admin-menu" />
      </Sider>
      <Layout>
        <Header className="admin-header">
          <Space>
            <Button type="text" icon={collapsed ? <MenuUnfoldOutlined /> : <MenuFoldOutlined />} onClick={() => setCollapsed((value) => !value)} />
            <Typography.Title level={4} style={{ margin: 0 }}>平台管理</Typography.Title>
            <Tag color="blue">{environmentLabel}</Tag>
          </Space>
          <Space size="middle">
            <Space size={6}><UserOutlined /><Typography.Text>{user.display_name || user.username}</Typography.Text><Typography.Text type="secondary">({user.role})</Typography.Text></Space>
            <Button size="small" icon={<LogoutOutlined />} loading={logoutLoading} onClick={() => void logout()}>退出</Button>
          </Space>
        </Header>
        <Content className="admin-content"><Outlet /></Content>
      </Layout>
    </Layout>
  );
}
