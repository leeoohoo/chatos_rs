import {
  ApartmentOutlined,
  CloudServerOutlined,
  DatabaseOutlined,
  GlobalOutlined,
  LogoutOutlined,
  MenuFoldOutlined,
  MenuUnfoldOutlined,
  ProjectOutlined,
  RobotOutlined,
  SettingOutlined,
  TeamOutlined,
  UserOutlined,
} from '@ant-design/icons';
import { Button, Layout, Menu, Segmented, Space, Tag, Typography, type MenuProps } from 'antd';
import { useMemo, useState } from 'react';
import { Outlet, useLocation, useNavigate } from 'react-router-dom';

import { useAdminAuth } from '../auth/AuthProvider';
import { isSuperAdmin } from '../auth/access';
import { useAdminI18n } from '../i18n/AdminI18nProvider';

const { Header, Sider, Content } = Layout;
const environmentLabel = import.meta.env.VITE_APP_ENV || 'production';

type MenuItem = Required<MenuProps>['items'][number] & {
  key: string;
  label: React.ReactNode;
  children?: MenuItem[];
};

export function AdminLayout() {
  const { user, logout, logoutLoading } = useAdminAuth();
  const { locale, setLocale, t } = useAdminI18n();
  const location = useLocation();
  const navigate = useNavigate();
  const [collapsed, setCollapsed] = useState(false);
  const menuItems = useMemo(() => buildMenuItems(isSuperAdmin(user.role), t), [t, user.role]);
  const leafItems = useMemo(() => flattenLeafItems(menuItems), [menuItems]);
  const selectedItem = useMemo(
    () => [...leafItems].sort((left, right) => right.key.length - left.key.length)
      .find((item) => location.pathname.startsWith(item.key)),
    [leafItems, location.pathname],
  );

  return (
    <Layout className="admin-shell">
      <Sider width={248} collapsedWidth={72} collapsed={collapsed} theme="light" className="admin-sider">
        <div className="admin-brand">
          <CloudServerOutlined />
          {!collapsed ? <div><strong>{t('app.title')}</strong><span>{t('app.subtitle')}</span></div> : null}
        </div>
        <Menu
          mode="inline"
          items={menuItems}
          selectedKeys={selectedItem ? [selectedItem.key] : []}
          defaultOpenKeys={menuItems.map((item) => item.key)}
          onClick={({ key }) => navigate(key)}
          className="admin-menu"
        />
      </Sider>
      <Layout>
        <Header className="admin-header">
          <Space>
            <Button type="text" icon={collapsed ? <MenuUnfoldOutlined /> : <MenuFoldOutlined />} onClick={() => setCollapsed((value) => !value)} />
            <Typography.Title level={4} style={{ margin: 0 }}>{selectedItem?.label || t('app.header')}</Typography.Title>
            <Tag color="blue">{environmentLabel}</Tag>
          </Space>
          <Space size="middle">
            <Space size={6}>
              <GlobalOutlined />
              <Segmented
                size="small"
                value={locale}
                onChange={(value) => setLocale(value === 'en-US' ? 'en-US' : 'zh-CN')}
                options={[
                  { label: t('language.zh'), value: 'zh-CN' },
                  { label: t('language.en'), value: 'en-US' },
                ]}
                aria-label={t('language.label')}
              />
            </Space>
            <Space size={6}>
              <UserOutlined />
              <Typography.Text>{user.display_name || user.username}</Typography.Text>
              <Typography.Text type="secondary">({user.role})</Typography.Text>
            </Space>
            <Button size="small" icon={<LogoutOutlined />} loading={logoutLoading} onClick={() => void logout()}>{t('auth.logout')}</Button>
          </Space>
        </Header>
        <Content className="admin-content"><Outlet /></Content>
      </Layout>
    </Layout>
  );
}

function buildMenuItems(superAdmin: boolean, t: ReturnType<typeof useAdminI18n>['t']): MenuItem[] {
  const items: MenuItem[] = [];
  if (superAdmin) {
    items.push({ key: '/users', icon: <TeamOutlined />, label: t('nav.users'), children: [
      { key: '/users/models', label: t('nav.userModels') },
      { key: '/users/accounts', label: t('nav.userAccounts') },
      { key: '/users/agents', label: t('nav.agentAccounts') },
      { key: '/users/settings', label: t('nav.userSettings') },
    ] });
  }
  items.push(
    { key: '/projects', icon: <ProjectOutlined />, label: t('nav.projects'), children: [
      { key: '/projects/list', label: t('nav.projectList') },
      { key: '/projects/config', label: t('nav.projectConfig') },
    ] },
    { key: '/task-runner', icon: <RobotOutlined />, label: t('nav.taskRunner'), children: [
      { key: '/task-runner/tasks', label: t('nav.tasks') },
      { key: '/task-runner/runs', label: t('nav.runs') },
      { key: '/task-runner/prompts', label: t('nav.prompts') },
      { key: '/task-runner/projects', label: t('nav.executionProjects') },
      { key: '/task-runner/mcp', label: t('nav.taskMcp') },
      { key: '/task-runner/tooling', label: t('nav.tooling') },
      { key: '/task-runner/users', label: t('nav.executionUsers') },
      { key: '/task-runner/settings', label: t('nav.executionSettings') },
    ] },
    { key: '/plugins', icon: <ApartmentOutlined />, label: t('nav.plugins'), children: [
      { key: '/plugins/mcp', label: t('nav.mcpCatalog') },
      ...(superAdmin ? [
        { key: '/plugins/catalog', label: t('nav.pluginCatalog') },
        { key: '/plugins/releases', label: t('nav.pluginReleases') },
      ] : []),
      { key: '/plugins/marketplaces', label: t('nav.marketplaces') },
      { key: '/plugins/publishers', label: t('nav.publishers') },
      ...(superAdmin ? [
        { key: '/plugins/agents', label: t('nav.systemAgents') },
        { key: '/plugins/runtime', label: t('nav.runtime') },
      ] : []),
      { key: '/plugins/diagnostics', label: t('nav.diagnostics') },
      ...(superAdmin ? [{ key: '/plugins/audit', label: t('nav.audit') }] : []),
    ] },
    { key: '/memory', icon: <DatabaseOutlined />, label: t('nav.memory'), children: [
      ...(superAdmin ? [{ key: '/memory/dashboard', label: t('nav.overview') }] : []),
      { key: '/memory/data', label: t('nav.memoryData') },
      ...(superAdmin ? [{ key: '/memory/sources', label: t('nav.sources') }] : []),
      ...(superAdmin ? [{ key: '/memory/policies', label: t('nav.policies') }] : []),
      { key: '/memory/runs', label: t('nav.runs') },
    ] },
  );
  if (superAdmin) {
    items.push({ key: '/config', icon: <SettingOutlined />, label: t('nav.config'), children: [
      { key: '/config/dashboard', label: t('nav.overview') },
      { key: '/config/definitions', label: t('nav.configManagement') },
      { key: '/config/releases', label: t('nav.releaseHistory') },
      { key: '/config/queues', label: t('nav.queueOperations') },
      { key: '/config/instances', label: t('nav.instances') },
      { key: '/config/audit', label: t('nav.auditLog') },
    ] });
  }
  return items;
}

function flattenLeafItems(items: MenuItem[]): MenuItem[] {
  return items.flatMap((item) => item.children?.length ? flattenLeafItems(item.children) : [item]);
}
