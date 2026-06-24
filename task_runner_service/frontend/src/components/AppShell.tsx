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

const { Header, Sider, Content } = Layout;

type AppShellProps = {
  currentUser: AuthUser;
  logoutLoading?: boolean;
  onLogout: () => void;
};

export function AppShell({ currentUser, logoutLoading, onLogout }: AppShellProps) {
  const { locale, setLocale, t } = useI18n();
  const location = useLocation();
  const navigate = useNavigate();
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
    <Layout style={{ minHeight: '100vh' }}>
      <Sider width={220} theme="light" style={{ borderRight: '1px solid #f0f0f0' }}>
        <div style={{ padding: '20px 20px 8px' }}>
          <Space direction="vertical" size={0}>
            <Typography.Title level={4} style={{ margin: 0 }}>
              {t('app.brand')}
            </Typography.Title>
            <Typography.Text type="secondary">
              {t('app.subtitle')}
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
            {t('app.headerSummary')}
          </Typography.Text>
          <Space size="middle" style={{ flexShrink: 0 }}>
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
              {t('auth.logout')}
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
