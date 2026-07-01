// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { Suspense, lazy, useEffect, useState } from 'react';
import { Navigate, Route, Routes } from 'react-router-dom';
import { useQueryClient } from '@tanstack/react-query';
import {
  App as AntdApp,
  Button,
  ConfigProvider,
  Flex,
  Form,
  Input,
  Space,
  Spin,
  Typography,
  theme,
} from 'antd';
import antdEnUS from 'antd/locale/en_US';
import antdZhCN from 'antd/locale/zh_CN';
import { LockOutlined, LoginOutlined, UserOutlined } from '@ant-design/icons';

import { api, clearAuthToken, getAuthToken, setAuthToken } from './api/client';
import { AppShell } from './components/AppShell';
import { I18nProvider, useI18n } from './i18n/I18nProvider';
import type { AuthUser, LoginPayload } from './types';

const TasksPage = lazy(async () => ({
  default: (await import('./pages/TasksPage')).TasksPage,
}));
const ModelsPage = lazy(async () => ({
  default: (await import('./pages/ModelsPage')).ModelsPage,
}));
const ProjectsPage = lazy(async () => ({
  default: (await import('./pages/ProjectsPage')).ProjectsPage,
}));
const ServersPage = lazy(async () => ({
  default: (await import('./pages/ServersPage')).ServersPage,
}));
const RunsPage = lazy(async () => ({
  default: (await import('./pages/RunsPage')).RunsPage,
}));
const PromptsPage = lazy(async () => ({
  default: (await import('./pages/PromptsPage')).PromptsPage,
}));
const McpCatalogPage = lazy(async () => ({
  default: (await import('./pages/McpCatalogPage')).McpCatalogPage,
}));
const SkillsPage = lazy(async () => ({
  default: (await import('./pages/SkillsPage')).SkillsPage,
}));
const SettingsPage = lazy(async () => ({
  default: (await import('./pages/SettingsPage')).SettingsPage,
}));
const ToolingPage = lazy(async () => ({
  default: (await import('./pages/ToolingPage')).ToolingPage,
}));
const UsersPage = lazy(async () => ({
  default: (await import('./pages/UsersPage')).UsersPage,
}));

export default function App() {
  return (
    <I18nProvider>
      <TaskRunnerApp />
    </I18nProvider>
  );
}

function TaskRunnerApp() {
  const { locale } = useI18n();

  return (
    <ConfigProvider
      locale={locale === 'en-US' ? antdEnUS : antdZhCN}
      theme={{
        algorithm: theme.defaultAlgorithm,
        token: {
          borderRadius: 6,
          colorPrimary: '#1677ff',
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
  const { t } = useI18n();
  const queryClient = useQueryClient();
  const [currentUser, setCurrentUser] = useState<AuthUser | null>(null);
  const [checking, setChecking] = useState(true);
  const [loginLoading, setLoginLoading] = useState(false);
  const [logoutLoading, setLogoutLoading] = useState(false);

  useEffect(() => {
    let alive = true;

    async function loadCurrentUser() {
      if (!getAuthToken()) {
        setChecking(false);
        return;
      }
      try {
        const response = await api.currentUser();
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

    loadCurrentUser();

    function handleAuthChanged() {
      if (!getAuthToken()) {
        queryClient.clear();
        setCurrentUser(null);
      }
    }

    window.addEventListener('task-runner-auth-changed', handleAuthChanged);
    return () => {
      alive = false;
      window.removeEventListener('task-runner-auth-changed', handleAuthChanged);
    };
  }, [queryClient]);

  async function handleLogin(values: LoginPayload) {
    setLoginLoading(true);
    try {
      const response = await api.login(values);
      setAuthToken(response.token);
      queryClient.clear();
      setCurrentUser(response.user);
      message.success(t('auth.loginSuccess'));
    } catch (error) {
      message.error(error instanceof Error ? error.message : t('auth.loginFailed'));
    } finally {
      setLoginLoading(false);
    }
  }

  async function handleLogout() {
    setLogoutLoading(true);
    try {
      await api.logout();
    } catch {
      // Continue with local logout even if the server-side logout fails.
    } finally {
      clearAuthToken();
      queryClient.clear();
      setCurrentUser(null);
      setLogoutLoading(false);
      message.success(t('auth.logoutSuccess'));
    }
  }

  if (checking) {
    return (
      <Flex align="center" justify="center" style={{ minHeight: '100vh' }}>
        <Spin size="large" />
      </Flex>
    );
  }

  if (!currentUser) {
    return <LoginPage loading={loginLoading} onLogin={handleLogin} />;
  }

  const requireAdmin = (element: React.ReactElement) =>
    currentUser.role === 'admin' ? element : <Navigate to="/tasks" replace />;

  return (
    <Suspense
      fallback={
        <Flex align="center" justify="center" style={{ minHeight: '100vh' }}>
          <Spin size="large" />
        </Flex>
      }
    >
      <Routes>
        <Route
          element={
            <AppShell
              currentUser={currentUser}
              logoutLoading={logoutLoading}
              onLogout={handleLogout}
            />
          }
        >
          <Route path="/" element={<Navigate to="/tasks" replace />} />
          <Route path="/tasks" element={<TasksPage />} />
          <Route path="/projects" element={<ProjectsPage />} />
          <Route path="/models" element={<ModelsPage />} />
          <Route path="/servers" element={<ServersPage />} />
          <Route path="/runs" element={<RunsPage />} />
          <Route path="/prompts" element={<PromptsPage />} />
          <Route path="/mcp" element={<McpCatalogPage />} />
          <Route path="/skills" element={<SkillsPage />} />
          <Route path="/tooling" element={<ToolingPage />} />
          <Route path="/users" element={requireAdmin(<UsersPage />)} />
          <Route path="/settings" element={requireAdmin(<SettingsPage />)} />
        </Route>
      </Routes>
    </Suspense>
  );
}

type LoginPageProps = {
  loading: boolean;
  onLogin: (values: LoginPayload) => void;
};

function LoginPage({ loading, onLogin }: LoginPageProps) {
  const { locale, setLocale, t } = useI18n();

  return (
    <Flex
      align="center"
      justify="center"
      style={{
        minHeight: '100vh',
        padding: 24,
        background: '#f5f7fa',
      }}
    >
      <div
        style={{
          width: '100%',
          maxWidth: 380,
          padding: 28,
          background: '#fff',
          border: '1px solid #f0f0f0',
          borderRadius: 8,
          boxShadow: '0 8px 24px rgba(15, 23, 42, 0.08)',
        }}
      >
        <Space direction="vertical" size={12} style={{ width: '100%', marginBottom: 24 }}>
          <Space align="start" style={{ width: '100%', justifyContent: 'space-between' }}>
            <Space direction="vertical" size={6}>
              <Typography.Title level={3} style={{ margin: 0 }}>
                {t('app.brand')}
              </Typography.Title>
              <Typography.Text type="secondary">{t('auth.loginSubtitle')}</Typography.Text>
            </Space>
            <Button.Group size="small">
              <Button
                type={locale === 'zh-CN' ? 'primary' : 'default'}
                onClick={() => setLocale('zh-CN')}
              >
                {t('language.chinese')}
              </Button>
              <Button
                type={locale === 'en-US' ? 'primary' : 'default'}
                onClick={() => setLocale('en-US')}
              >
                EN
              </Button>
            </Button.Group>
          </Space>
        </Space>
        <Form<LoginPayload>
          layout="vertical"
          initialValues={{ username: 'admin' }}
          onFinish={onLogin}
          requiredMark={false}
        >
          <Form.Item
            label={t('auth.username')}
            name="username"
            rules={[{ required: true, message: t('auth.usernameRequired') }]}
          >
            <Input prefix={<UserOutlined />} autoComplete="username" />
          </Form.Item>
          <Form.Item
            label={t('auth.password')}
            name="password"
            rules={[{ required: true, message: t('auth.passwordRequired') }]}
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
            {t('auth.login')}
          </Button>
        </Form>
      </div>
    </Flex>
  );
}
