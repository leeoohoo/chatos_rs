import {
  AppstoreOutlined,
  ApiOutlined,
  DashboardOutlined,
  PlusCircleOutlined,
  SettingOutlined,
  UnorderedListOutlined,
} from '@ant-design/icons';
import { Layout, Menu, Segmented, Space, Typography } from 'antd';
import { Link, Outlet, useLocation } from 'react-router-dom';

import { useI18n, type Language } from '../i18n';

const { Content, Sider } = Layout;

export function AppShell() {
  const location = useLocation();
  const selectedKey = selectedNavKey(location.pathname);
  const { language, setLanguage, t } = useI18n();

  return (
    <Layout className="app-layout">
      <Sider width={236}>
        <div className="app-logo">
          <Space>
            <AppstoreOutlined />
            <span>{t('app.logo')}</span>
          </Space>
        </div>
        <Menu
          theme="dark"
          mode="inline"
          selectedKeys={[selectedKey]}
          items={[
            {
              key: '/dashboard',
              icon: <DashboardOutlined />,
              label: <Link to="/dashboard">{t('nav.dashboard')}</Link>,
            },
            {
              key: '/sandboxes',
              icon: <UnorderedListOutlined />,
              label: <Link to="/sandboxes">{t('nav.sandboxes')}</Link>,
            },
            {
              key: '/mcp-test',
              icon: <ApiOutlined />,
              label: <Link to="/mcp-test">{t('nav.mcpTest')}</Link>,
            },
            {
              key: '/pool',
              icon: <AppstoreOutlined />,
              label: <Link to="/pool">{t('nav.pool')}</Link>,
            },
            {
              key: '/create',
              icon: <PlusCircleOutlined />,
              label: <Link to="/create">{t('nav.create')}</Link>,
            },
            {
              key: '/settings',
              icon: <SettingOutlined />,
              label: <Link to="/settings">{t('nav.settings')}</Link>,
            },
          ]}
        />
      </Sider>
      <Layout>
        <div className="topbar">
          <Typography.Text strong>{t('app.title')}</Typography.Text>
          <Space>
            <Typography.Text type="secondary">{t('app.stage')}</Typography.Text>
            <Segmented
              size="small"
              value={language}
              onChange={(value) => setLanguage(value as Language)}
              options={[
                { label: t('lang.zh'), value: 'zh' },
                { label: t('lang.en'), value: 'en' },
              ]}
            />
          </Space>
        </div>
        <Content className="page-content">
          <Outlet />
        </Content>
      </Layout>
    </Layout>
  );
}

function selectedNavKey(pathname: string): string {
  if (pathname.startsWith('/sandboxes')) {
    return '/sandboxes';
  }
  if (pathname.startsWith('/pool')) {
    return '/pool';
  }
  if (pathname.startsWith('/mcp-test')) {
    return '/mcp-test';
  }
  if (pathname.startsWith('/create')) {
    return '/create';
  }
  if (pathname.startsWith('/settings')) {
    return '/settings';
  }
  return '/dashboard';
}
