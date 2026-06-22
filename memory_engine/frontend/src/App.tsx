import {
  ApiOutlined,
  AppstoreOutlined,
  BarsOutlined,
  FileTextOutlined,
  HistoryOutlined,
  LockOutlined,
  LoginOutlined,
  LogoutOutlined,
  ReloadOutlined,
  SafetyCertificateOutlined,
  SettingOutlined,
  UserOutlined,
} from '@ant-design/icons';
import {
  App as AntdApp,
  Button,
  ConfigProvider,
  Flex,
  Form,
  Input,
  Layout,
  Menu,
  Space,
  Spin,
  Tag,
  Typography,
} from 'antd';
import zhCN from 'antd/locale/zh_CN';
import { lazy, Suspense, useEffect, useMemo, useState } from 'react';

import {
  clearAuthToken,
  getAuthToken,
  setAuthToken,
  userServiceApi,
} from './api/userService';
import { useConsoleResources } from './app/hooks/useConsoleResources';
import type { TabKey } from './app/types';
import type { AuthUser, LoginPayload } from './api/userService';

const DashboardSection = lazy(() =>
  import('./app/sections/DashboardSection').then((module) => ({ default: module.DashboardSection })),
);
const DataSectionContainer = lazy(() =>
  import('./app/sections/DataSectionContainer').then((module) => ({
    default: module.DataSectionContainer,
  })),
);
const SourcesSectionContainer = lazy(() =>
  import('./app/sections/SourcesSectionContainer').then((module) => ({
    default: module.SourcesSectionContainer,
  })),
);
const ModelsSectionContainer = lazy(() =>
  import('./app/sections/ModelsSectionContainer').then((module) => ({
    default: module.ModelsSectionContainer,
  })),
);
const PoliciesSectionContainer = lazy(() =>
  import('./app/sections/PoliciesSectionContainer').then((module) => ({
    default: module.PoliciesSectionContainer,
  })),
);
const RunsSectionContainer = lazy(() =>
  import('./app/sections/RunsSectionContainer').then((module) => ({
    default: module.RunsSectionContainer,
  })),
);

const { Header, Sider, Content } = Layout;
const { Title, Text } = Typography;
const ADMIN_TABS: TabKey[] = ['dashboard', 'data', 'sources', 'models', 'policies', 'runs'];
const USER_TABS: TabKey[] = ['data', 'models', 'runs'];

export default function MemoryEngineApp() {
  return (
    <ConfigProvider
      locale={zhCN}
      theme={{
        token: {
          colorPrimary: '#136f63',
          borderRadius: 8,
          fontFamily: '"IBM Plex Sans","Noto Sans SC","Source Han Sans SC",sans-serif',
        },
      }}
    >
      <AntdApp>
        <AuthGate />
      </AntdApp>
    </ConfigProvider>
  );
}

function AuthGate() {
  const { message } = AntdApp.useApp();
  const [currentUser, setCurrentUser] = useState<AuthUser | null>(null);
  const [checking, setChecking] = useState(true);
  const [loginLoading, setLoginLoading] = useState(false);
  const [logoutLoading, setLogoutLoading] = useState(false);

  useEffect(() => {
    let alive = true;

    async function loadCurrentUser() {
      if (!getAuthToken()) {
        if (alive) {
          setCurrentUser(null);
          setChecking(false);
        }
        return;
      }
      try {
        const response = await userServiceApi.currentUser();
        if (alive) {
          setCurrentUser(response.user);
        }
      } catch {
        if (alive) {
          setCurrentUser(null);
        }
      } finally {
        if (alive) {
          setChecking(false);
        }
      }
    }

    void loadCurrentUser();

    function handleAuthChanged() {
      if (!alive) {
        return;
      }
      if (!getAuthToken()) {
        setCurrentUser(null);
        setChecking(false);
        return;
      }
      setChecking(true);
      void loadCurrentUser();
    }

    window.addEventListener('user-service-auth-changed', handleAuthChanged);
    return () => {
      alive = false;
      window.removeEventListener('user-service-auth-changed', handleAuthChanged);
    };
  }, []);

  async function handleLogin(values: LoginPayload) {
    setLoginLoading(true);
    try {
      const response = await userServiceApi.login(values);
      setAuthToken(response.token);
      setCurrentUser(response.user);
      message.success('Login succeeded');
    } catch (error) {
      message.error(error instanceof Error ? error.message : 'Login failed');
    } finally {
      setLoginLoading(false);
    }
  }

  async function handleLogout() {
    setLogoutLoading(true);
    try {
      await userServiceApi.logout();
    } catch {
      // noop
    } finally {
      clearAuthToken();
      setCurrentUser(null);
      setLogoutLoading(false);
      message.success('Signed out');
    }
  }

  if (checking) {
    return (
      <Flex align="center" justify="center" className="engine-loading">
        <Spin size="large" />
      </Flex>
    );
  }

  if (!currentUser) {
    return <LoginPage loading={loginLoading} onLogin={handleLogin} />;
  }

  return (
    <ConsoleShell
      currentUser={currentUser}
      logoutLoading={logoutLoading}
      onLogout={handleLogout}
    />
  );
}

type ConsoleShellProps = {
  currentUser: AuthUser;
  logoutLoading: boolean;
  onLogout: () => void;
};

function ConsoleShell({ currentUser, logoutLoading, onLogout }: ConsoleShellProps) {
  const canAccessAdminConsole = currentUser.role === 'super_admin';
  const availableTabs = canAccessAdminConsole ? ADMIN_TABS : USER_TABS;
  const defaultTab = canAccessAdminConsole ? 'dashboard' : 'data';
  const [tab, setTab] = useState<TabKey>(defaultTab);
  const [dataRefreshNonce, setDataRefreshNonce] = useState(0);
  const [sourcesRefreshNonce, setSourcesRefreshNonce] = useState(0);
  const [modelsRefreshNonce, setModelsRefreshNonce] = useState(0);
  const [policiesRefreshNonce, setPoliciesRefreshNonce] = useState(0);
  const [runsRefreshNonce, setRunsRefreshNonce] = useState(0);
  const resources = useConsoleResources(canAccessAdminConsole);

  useEffect(() => {
    if (!availableTabs.includes(tab)) {
      setTab(defaultTab);
    }
  }, [availableTabs, defaultTab, tab]);

  const menuItems = useMemo(
    () =>
      availableTabs.map((key) => {
        switch (key) {
          case 'dashboard':
            return { key, icon: <AppstoreOutlined />, label: 'Overview' };
          case 'data':
            return { key, icon: <FileTextOutlined />, label: 'Data' };
          case 'sources':
            return { key, icon: <ApiOutlined />, label: 'Sources' };
          case 'models':
            return { key, icon: <BarsOutlined />, label: 'Models' };
          case 'policies':
            return { key, icon: <SettingOutlined />, label: 'Policies' };
          case 'runs':
            return { key, icon: <HistoryOutlined />, label: 'Runs' };
          default:
            return { key, icon: <BarsOutlined />, label: key };
        }
      }),
    [availableTabs],
  );

  const refreshDashboard = () => {
    if (!canAccessAdminConsole) {
      return;
    }
    void resources.loadDashboardOverview();
  };

  const handleRefreshCurrentTab = () => {
    if (tab === 'dashboard') {
      refreshDashboard();
      return;
    }
    if (tab === 'data') {
      setDataRefreshNonce((value) => value + 1);
      return;
    }
    if (tab === 'sources') {
      setSourcesRefreshNonce((value) => value + 1);
      return;
    }
    if (tab === 'models') {
      setModelsRefreshNonce((value) => value + 1);
      return;
    }
    if (tab === 'policies') {
      setPoliciesRefreshNonce((value) => value + 1);
      return;
    }
    setRunsRefreshNonce((value) => value + 1);
  };

  return (
    <Layout className="engine-shell">
      <Sider className="engine-sider" theme="light" width={240}>
        <div className="engine-brand">
          <Title level={4} style={{ margin: 0 }}>
            Memory Engine
          </Title>
          <Text type="secondary">
            {canAccessAdminConsole ? 'Admin console' : 'User-scoped console'}
          </Text>
        </div>
        <Menu
          mode="inline"
          selectedKeys={[tab]}
          items={menuItems}
          onSelect={(event) => setTab(event.key as TabKey)}
        />
      </Sider>
      <Layout className="engine-main-layout">
        <Header className="engine-topbar">
          <Space wrap size={[12, 12]} style={{ width: '100%', justifyContent: 'space-between' }}>
            <Space wrap>
              <Tag color="blue">memory_engine</Tag>
              <Tag color={currentUser.role === 'super_admin' ? 'gold' : 'green'}>
                {currentUser.role}
              </Tag>
              <Tag color={canAccessAdminConsole ? 'processing' : 'default'}>
                {canAccessAdminConsole ? 'admin-console' : 'user-scope'}
              </Tag>
              <Text type="secondary">
                {canAccessAdminConsole
                  ? 'All tenants, sources, models, policies, and job runs are available.'
                  : 'You are viewing only your own tenant data and model profiles.'}
              </Text>
            </Space>
            <Space wrap>
              <Space size={4}>
                <SafetyCertificateOutlined />
                <Text strong>{currentUser.display_name || currentUser.username}</Text>
                <Text type="secondary">({currentUser.username})</Text>
              </Space>
              <Button icon={<ReloadOutlined />} onClick={handleRefreshCurrentTab}>
                Refresh
              </Button>
              <Button
                icon={<LogoutOutlined />}
                loading={logoutLoading}
                onClick={onLogout}
              >
                Sign Out
              </Button>
            </Space>
          </Space>
        </Header>
        <Content className={tab === 'data' ? 'engine-page engine-page--data' : 'engine-page'}>
          <Suspense
            fallback={
              <div className="engine-page engine-page--loading">
                <Spin size="large" />
              </div>
            }
          >
            {tab === 'dashboard' ? (
              <DashboardSection
                loading={!resources.initialized || resources.loading}
                dashboardStats={resources.dashboardStats}
                jobStats={resources.dashboardJobStats}
              />
            ) : tab === 'data' ? (
              <DataSectionContainer refreshNonce={dataRefreshNonce} />
            ) : tab === 'sources' ? (
              <SourcesSectionContainer
                refreshNonce={sourcesRefreshNonce}
                onCatalogMutated={refreshDashboard}
              />
            ) : tab === 'models' ? (
              <ModelsSectionContainer
                refreshNonce={modelsRefreshNonce}
                onCatalogMutated={refreshDashboard}
              />
            ) : tab === 'policies' ? (
              <PoliciesSectionContainer refreshNonce={policiesRefreshNonce} />
            ) : (
              <RunsSectionContainer refreshNonce={runsRefreshNonce} />
            )}
          </Suspense>
        </Content>
      </Layout>
    </Layout>
  );
}

type LoginPageProps = {
  loading: boolean;
  onLogin: (values: LoginPayload) => void;
};

function LoginPage({ loading, onLogin }: LoginPageProps) {
  return (
    <div className="engine-auth">
      <div className="engine-auth-card">
        <Space direction="vertical" size={18} style={{ width: '100%' }}>
          <Space direction="vertical" size={8}>
            <Tag className="engine-auth-badge" color="geekblue">
              unified-user-service
            </Tag>
            <Title level={2} style={{ margin: 0 }}>
              Memory Engine Sign In
            </Title>
            <Text type="secondary">
              Sign in with your ChatOS user account. Regular users manage only their own model
              profiles and tenant data. Super admins can view all tenants.
            </Text>
          </Space>
          <Form<LoginPayload>
            layout="vertical"
            initialValues={{ username: 'admin' }}
            onFinish={onLogin}
            requiredMark={false}
          >
            <Form.Item
              label="Username"
              name="username"
              rules={[{ required: true, message: 'Username is required' }]}
            >
              <Input prefix={<UserOutlined />} autoComplete="username" />
            </Form.Item>
            <Form.Item
              label="Password"
              name="password"
              rules={[{ required: true, message: 'Password is required' }]}
            >
              <Input.Password prefix={<LockOutlined />} autoComplete="current-password" />
            </Form.Item>
            <Button
              block
              type="primary"
              htmlType="submit"
              icon={<LoginOutlined />}
              loading={loading}
            >
              Sign In
            </Button>
          </Form>
        </Space>
      </div>
    </div>
  );
}
