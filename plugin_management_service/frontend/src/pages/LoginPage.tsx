// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { LockOutlined, UserOutlined } from '@ant-design/icons';
import { useMutation } from '@tanstack/react-query';
import { Alert, Button, Form, Input, Segmented, Typography } from 'antd';

import { api, setAuthToken } from '../api/client';
import { useI18n } from '../i18n/I18nProvider';

interface LoginPageProps {
  onLogin: () => void;
}

export function LoginPage({ onLogin }: LoginPageProps) {
  const { locale, setLocale, t } = useI18n();
  const loginMutation = useMutation({
    mutationFn: api.login,
    onSuccess: (data) => {
      setAuthToken(data.token);
      onLogin();
    },
  });

  return (
    <div className="login-page">
      <div className="login-panel">
        <div className="login-language">
          <Segmented
            size="small"
            value={locale}
            options={[
              { label: t('language.zh'), value: 'zh-CN' },
              { label: t('language.en'), value: 'en-US' },
            ]}
            onChange={(value) => setLocale(value as 'zh-CN' | 'en-US')}
          />
        </div>
        <Typography.Title level={2}>{t('app.title')}</Typography.Title>
        <Typography.Text type="secondary">{t('login.subtitle')}</Typography.Text>
        <Form
          layout="vertical"
          className="login-form"
          initialValues={{ username: 'admin' }}
          onFinish={(values) => loginMutation.mutate(values)}
        >
          {loginMutation.error ? (
            <Alert type="error" showIcon message={(loginMutation.error as Error).message} />
          ) : null}
          <Form.Item name="username" label={t('login.username')} rules={[{ required: true }]}>
            <Input prefix={<UserOutlined />} autoComplete="username" />
          </Form.Item>
          <Form.Item name="password" label={t('login.password')} rules={[{ required: true }]}>
            <Input.Password prefix={<LockOutlined />} autoComplete="current-password" />
          </Form.Item>
          <Button
            type="primary"
            htmlType="submit"
            block
            loading={loginMutation.isPending}
          >
            {t('login.submit')}
          </Button>
        </Form>
      </div>
    </div>
  );
}
