import { App as AntdApp, Button, Card, Flex, Form, Input, Space, Spin, Typography } from 'antd';
import { LockOutlined, LoginOutlined, UserOutlined } from '@ant-design/icons';
import { useQueryClient } from '@tanstack/react-query';
import { createContext, useContext, useEffect, useMemo, useState } from 'react';

import { adminApi, type AdminUser, type LoginPayload } from '../../shared/api/adminApi';
import {
  ADMIN_AUTH_CHANGED_EVENT,
  clearAuthToken,
  getAuthToken,
  migrateLegacyAuthToken,
  setAuthToken,
} from '../../shared/auth/tokenStore';

type AuthContextValue = {
  user: AdminUser;
  logout: () => Promise<void>;
  logoutLoading: boolean;
};

const AuthContext = createContext<AuthContextValue | null>(null);

export function useAdminAuth() {
  const value = useContext(AuthContext);
  if (!value) {
    throw new Error('useAdminAuth must be used inside AdminAuthProvider');
  }
  return value;
}

export function AdminAuthProvider({ children }: { children: React.ReactNode }) {
  const { message } = AntdApp.useApp();
  const queryClient = useQueryClient();
  const [user, setUser] = useState<AdminUser | null>(null);
  const [checking, setChecking] = useState(true);
  const [loginLoading, setLoginLoading] = useState(false);
  const [logoutLoading, setLogoutLoading] = useState(false);

  useEffect(() => {
    let alive = true;
    async function refresh() {
      const token = migrateLegacyAuthToken();
      if (!token) {
        if (alive) setChecking(false);
        return;
      }
      try {
        const response = await adminApi.me();
        if (!['super_admin', 'admin'].includes(response.user.role)) {
          throw new Error('当前账号没有管理端访问权限');
        }
        if (alive) setUser(response.user);
      } catch {
        clearAuthToken();
        if (alive) setUser(null);
      } finally {
        if (alive) setChecking(false);
      }
    }
    void refresh();
    const onAuthChanged = () => {
      if (!getAuthToken()) {
        queryClient.clear();
        setUser(null);
      }
    };
    window.addEventListener(ADMIN_AUTH_CHANGED_EVENT, onAuthChanged);
    return () => {
      alive = false;
      window.removeEventListener(ADMIN_AUTH_CHANGED_EVENT, onAuthChanged);
    };
  }, [queryClient]);

  async function login(payload: LoginPayload) {
    setLoginLoading(true);
    try {
      const response = await adminApi.login(payload);
      if (!['super_admin', 'admin'].includes(response.user.role)) {
        throw new Error('当前账号没有管理端访问权限');
      }
      setAuthToken(response.token);
      queryClient.clear();
      setUser(response.user);
      message.success('登录成功');
    } catch (error) {
      message.error(error instanceof Error ? error.message : '登录失败');
    } finally {
      setLoginLoading(false);
    }
  }

  async function logout() {
    setLogoutLoading(true);
    try {
      await adminApi.logout();
    } catch {
      // Local logout remains authoritative when the remote session is unavailable.
    } finally {
      clearAuthToken();
      queryClient.clear();
      setUser(null);
      setLogoutLoading(false);
      message.success('已退出登录');
    }
  }

  const value = useMemo(
    () => (user ? { user, logout, logoutLoading } : null),
    [logoutLoading, user],
  );

  if (checking) {
    return <Flex align="center" justify="center" className="admin-fullscreen"><Spin size="large" /></Flex>;
  }
  if (!user) {
    return <LoginPage loading={loginLoading} onLogin={login} />;
  }
  if (!value) return null;
  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>;
}

function LoginPage({ loading, onLogin }: { loading: boolean; onLogin: (payload: LoginPayload) => void }) {
  return (
    <Flex align="center" justify="center" className="admin-login-shell">
      <Card className="admin-login-card">
        <Space direction="vertical" size={6} style={{ marginBottom: 24 }}>
          <Typography.Title level={2} style={{ margin: 0 }}>ChatOS 统一管理端</Typography.Title>
          <Typography.Text type="secondary">一次登录，管理全部平台服务</Typography.Text>
        </Space>
        <Form<LoginPayload> layout="vertical" initialValues={{ username: 'admin' }} onFinish={onLogin} requiredMark={false}>
          <Form.Item name="username" label="用户名" rules={[{ required: true, message: '请输入用户名' }]}>
            <Input prefix={<UserOutlined />} autoComplete="username" autoFocus />
          </Form.Item>
          <Form.Item name="password" label="密码" rules={[{ required: true, message: '请输入密码' }]}>
            <Input.Password prefix={<LockOutlined />} autoComplete="current-password" />
          </Form.Item>
          <Button block type="primary" htmlType="submit" icon={<LoginOutlined />} loading={loading}>登录</Button>
        </Form>
      </Card>
    </Flex>
  );
}
