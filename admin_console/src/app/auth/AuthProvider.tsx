import { App as AntdApp, Button, Card, Flex, Form, Input, Segmented, Space, Spin, Typography } from 'antd';
import { LockOutlined, LoginOutlined, UserOutlined } from '@ant-design/icons';
import { useQueryClient } from '@tanstack/react-query';
import { createContext, useContext, useEffect, useMemo, useState } from 'react';

import { adminApi, type AdminUser, type LoginPayload } from '../../shared/api/adminApi';
import { canAccessAdminConsole } from './access';
import { useAdminI18n } from '../i18n/AdminI18nProvider';
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
  const { t } = useAdminI18n();
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
        if (!canAccessAdminConsole(response.user.role)) {
          throw new Error(t('auth.noAccess'));
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
  }, [queryClient, t]);

  async function login(payload: LoginPayload) {
    setLoginLoading(true);
    try {
      const response = await adminApi.login(payload);
      if (!canAccessAdminConsole(response.user.role)) {
        throw new Error(t('auth.noAccess'));
      }
      setAuthToken(response.token);
      queryClient.clear();
      setUser(response.user);
      message.success(t('auth.loginSuccess'));
    } catch (error) {
      message.error(error instanceof Error ? error.message : t('auth.loginFailed'));
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
      message.success(t('auth.logoutSuccess'));
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
  const { locale, setLocale, t } = useAdminI18n();
  return (
    <Flex align="center" justify="center" className="admin-login-shell">
      <Card className="admin-login-card">
        <Flex justify="flex-end" style={{ marginBottom: 16 }}>
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
        </Flex>
        <Space direction="vertical" size={6} style={{ marginBottom: 24 }}>
          <Typography.Title level={2} style={{ margin: 0 }}>{t('login.title')}</Typography.Title>
          <Typography.Text type="secondary">{t('login.subtitle')}</Typography.Text>
        </Space>
        <Form<LoginPayload> layout="vertical" initialValues={{ username: 'admin' }} onFinish={onLogin} requiredMark={false}>
          <Form.Item name="username" label={t('auth.username')} rules={[{ required: true, message: t('auth.usernameRequired') }]}>
            <Input prefix={<UserOutlined />} autoComplete="username" autoFocus />
          </Form.Item>
          <Form.Item name="password" label={t('auth.password')} rules={[{ required: true, message: t('auth.passwordRequired') }]}>
            <Input.Password prefix={<LockOutlined />} autoComplete="current-password" />
          </Form.Item>
          <Button block type="primary" htmlType="submit" icon={<LoginOutlined />} loading={loading}>{t('auth.login')}</Button>
        </Form>
      </Card>
    </Flex>
  );
}
