// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useQuery } from '@tanstack/react-query';
import { Alert, Card, Descriptions, Space, Typography } from 'antd';

import { api } from '../api/client';

export function SettingsPage() {
  const currentUserQuery = useQuery({
    queryKey: ['current-user'],
    queryFn: () => api.currentUser(),
  });
  const systemConfigQuery = useQuery({
    queryKey: ['system-config'],
    queryFn: () => api.getSystemConfig(),
  });

  return (
    <Space direction="vertical" size="large" style={{ width: '100%' }}>
      <Space direction="vertical" size={0}>
        <Typography.Title level={3} style={{ margin: 0 }}>
          设置
        </Typography.Title>
        <Typography.Text type="secondary">
          查看当前用户服务实例的基础身份配置。
        </Typography.Text>
      </Space>

      <Alert
        type="info"
        showIcon
        message="当前实现说明"
        description="这版用户微服务负责统一用户、Agent 账号和共享模型配置。Task Runner 的短期访问凭证由 chatos 后端按需自动换取，用户不需要手动生成。"
      />

      <Card title="当前登录用户" loading={currentUserQuery.isLoading}>
        {currentUserQuery.data ? (
          <Descriptions bordered column={1} size="small">
            <Descriptions.Item label="ID">{currentUserQuery.data.user.id}</Descriptions.Item>
            <Descriptions.Item label="Username">
              {currentUserQuery.data.user.username}
            </Descriptions.Item>
            <Descriptions.Item label="Display Name">
              {currentUserQuery.data.user.display_name}
            </Descriptions.Item>
            <Descriptions.Item label="Role">{currentUserQuery.data.user.role}</Descriptions.Item>
            <Descriptions.Item label="Principal Type">
              {currentUserQuery.data.user.principal_type}
            </Descriptions.Item>
          </Descriptions>
        ) : null}
      </Card>

      <Card title="系统配置" loading={systemConfigQuery.isLoading}>
        {systemConfigQuery.data ? (
          <Descriptions bordered column={1} size="small">
            <Descriptions.Item label="Service">{systemConfigQuery.data.service}</Descriptions.Item>
            <Descriptions.Item label="Issuer">{systemConfigQuery.data.issuer}</Descriptions.Item>
            <Descriptions.Item label="User Audience">
              {systemConfigQuery.data.user_service_audience}
            </Descriptions.Item>
            <Descriptions.Item label="Task Runner Audience">
              {systemConfigQuery.data.task_runner_audience}
            </Descriptions.Item>
            <Descriptions.Item label="Database URL">
              {systemConfigQuery.data.database_url}
            </Descriptions.Item>
            <Descriptions.Item label="User Access TTL">
              {systemConfigQuery.data.user_access_ttl_seconds}s
            </Descriptions.Item>
            <Descriptions.Item label="Task Runner Access TTL">
              {systemConfigQuery.data.task_runner_access_ttl_seconds}s
            </Descriptions.Item>
          </Descriptions>
        ) : null}
      </Card>
    </Space>
  );
}
