// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useEffect, useState } from 'react';
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
import { LockOutlined, LoginOutlined, UserOutlined } from '@ant-design/icons';

import { api, clearAuthToken, getAuthToken, setAuthToken } from './api/client';
import { AppShell } from './components/AppShell';
import { AgentAccountsPage } from './pages/AgentAccountsPage';
import { ModelsPage } from './pages/ModelsPage';
import { SettingsPage } from './pages/SettingsPage';
import { UsersPage } from './pages/UsersPage';
import type { AuthUser, LoginPayload } from './types';

export default function App() {
  return (
    <ConfigProvider
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

    window.addEventListener('user-service-auth-changed', handleAuthChanged);
    return () => {
      alive = false;
      window.removeEventListener('user-service-auth-changed', handleAuthChanged);
    };
  }, [queryClient]);

  async function handleLogin(values: LoginPayload) {
    setLoginLoading(true);
    try {
      const response = await api.login(values);
      setAuthToken(response.token);
      queryClient.clear();
      setCurrentUser(response.user);
      message.success('登录成功');
    } catch (error) {
      message.error(error instanceof Error ? error.message : '登录失败');
    } finally {
      setLoginLoading(false);
    }
  }

  async function handleLogout() {
    setLogoutLoading(true);
    try {
      await api.logout();
    } catch {
      // noop
    } finally {
      clearAuthToken();
      queryClient.clear();
      setCurrentUser(null);
      setLogoutLoading(false);
      message.success('已退出登录');
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

  return (
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
        <Route path="/" element={<Navigate to="/models" replace />} />
        <Route path="/models" element={<ModelsPage />} />
        <Route path="/users" element={<UsersPage />} />
        <Route path="/agents" element={<AgentAccountsPage />} />
        <Route path="/settings" element={<SettingsPage />} />
        <Route path="*" element={<Navigate to="/models" replace />} />
      </Route>
    </Routes>
  );
}

type LoginPageProps = {
  loading: boolean;
  onLogin: (values: LoginPayload) => void;
};

function LoginPage({ loading, onLogin }: LoginPageProps) {
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
          <Space direction="vertical" size={6}>
            <Typography.Title level={3} style={{ margin: 0 }}>
              User Service
            </Typography.Title>
            <Typography.Text type="secondary">
              统一用户、Agent 账号与模型配置
            </Typography.Text>
          </Space>
        </Space>
        <Form<LoginPayload>
          layout="vertical"
          initialValues={{ username: 'admin' }}
          onFinish={onLogin}
          requiredMark={false}
        >
          <Form.Item
            label="用户名"
            name="username"
            rules={[{ required: true, message: '请输入用户名' }]}
          >
            <Input prefix={<UserOutlined />} autoComplete="username" />
          </Form.Item>
          <Form.Item
            label="密码"
            name="password"
            rules={[{ required: true, message: '请输入密码' }]}
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
            登录
          </Button>
        </Form>
      </div>
    </Flex>
  );
}
