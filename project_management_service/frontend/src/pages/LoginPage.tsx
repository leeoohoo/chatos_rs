import { LockOutlined, UserOutlined } from '@ant-design/icons';
import { Alert, Button, Card, Form, Input, Typography } from 'antd';
import { useMutation } from '@tanstack/react-query';

import { api, setAuthToken } from '../api/client';
import type { LoginPayload } from '../types';

interface LoginPageProps {
  onLogin: () => void;
}

export function LoginPage({ onLogin }: LoginPageProps) {
  const mutation = useMutation({
    mutationFn: (payload: LoginPayload) => api.login(payload),
    onSuccess: (response) => {
      setAuthToken(response.token);
      onLogin();
    },
  });

  return (
    <div
      style={{
        minHeight: '100vh',
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        background: '#f5f7fa',
        padding: 24,
      }}
    >
      <Card style={{ width: 380 }}>
        <Typography.Title level={3} style={{ marginTop: 0 }}>
          项目管理服务
        </Typography.Title>
        <Form<LoginPayload> layout="vertical" onFinish={(values) => mutation.mutate(values)}>
          <Form.Item name="username" label="用户名" rules={[{ required: true }]}>
            <Input prefix={<UserOutlined />} autoComplete="username" />
          </Form.Item>
          <Form.Item name="password" label="密码" rules={[{ required: true }]}>
            <Input.Password prefix={<LockOutlined />} autoComplete="current-password" />
          </Form.Item>
          {mutation.error ? (
            <Alert
              type="error"
              showIcon
              message={(mutation.error as Error).message}
              style={{ marginBottom: 16 }}
            />
          ) : null}
          <Button type="primary" htmlType="submit" block loading={mutation.isPending}>
            登录
          </Button>
        </Form>
      </Card>
    </div>
  );
}
