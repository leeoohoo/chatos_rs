import { useState } from 'react';
import { useMutation, useQuery } from '@tanstack/react-query';
import { App, Button, Card, Descriptions, Empty, Form, Input, Select, Space, Typography } from 'antd';
import { KeyOutlined, ReloadOutlined } from '@ant-design/icons';

import { api } from '../api/client';
import type { TaskRunnerTokenExchangePayload, TaskRunnerTokenExchangeResponse } from '../types';

type ExchangeFormValues = {
  task_runner_agent_account_id: string;
  contact_id?: string;
};

export function TokenExchangePage() {
  const { message } = App.useApp();
  const [result, setResult] = useState<TaskRunnerTokenExchangeResponse | null>(null);
  const [form] = Form.useForm<ExchangeFormValues>();

  const agentsQuery = useQuery({
    queryKey: ['agent-accounts'],
    queryFn: () => api.listAgentAccounts(),
  });

  const exchangeMutation = useMutation({
    mutationFn: (payload: TaskRunnerTokenExchangePayload) => api.exchangeTaskRunnerToken(payload),
    onSuccess: (response) => {
      setResult(response);
      message.success('Task Runner token 已签发');
    },
    onError: (error) => {
      message.error(error instanceof Error ? error.message : '签发失败');
    },
  });

  const agentOptions = (agentsQuery.data || []).map((item) => ({
    label: `${item.display_name || item.username} (${item.username})`,
    value: item.id,
  }));

  function submit(values: ExchangeFormValues) {
    exchangeMutation.mutate(values);
  }

  return (
    <Space direction="vertical" size="large" style={{ width: '100%' }}>
      <div
        style={{
          display: 'flex',
          alignItems: 'flex-start',
          justifyContent: 'space-between',
          gap: 16,
          width: '100%',
        }}
      >
        <Space direction="vertical" size={0}>
          <Typography.Title level={3} style={{ margin: 0 }}>
            Task Runner Token Exchange
          </Typography.Title>
          <Typography.Text type="secondary">
            用当前真实用户的登录态，为自己名下的 Agent 签发面向 Task Runner 的短期 Bearer token。
          </Typography.Text>
        </Space>
        <Button
          icon={<ReloadOutlined />}
          onClick={() => agentsQuery.refetch()}
          loading={agentsQuery.isFetching}
        >
          刷新 Agent 列表
        </Button>
      </div>

      <Card>
        <Form<ExchangeFormValues>
          form={form}
          layout="vertical"
          requiredMark={false}
          onFinish={submit}
        >
          <Form.Item
            name="task_runner_agent_account_id"
            label="Agent 账号"
            rules={[{ required: true, message: '请选择 Agent 账号' }]}
          >
            <Select
              options={agentOptions}
              placeholder="选择要签发 token 的 Agent"
              notFoundContent={<Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description="暂无可用 Agent" />}
            />
          </Form.Item>
          <Form.Item name="contact_id" label="Contact ID（可选）">
            <Input placeholder="用于保留上游调用上下文，不参与权限判断" />
          </Form.Item>
          <Button
            type="primary"
            htmlType="submit"
            icon={<KeyOutlined />}
            loading={exchangeMutation.isPending}
          >
            签发 Token
          </Button>
        </Form>
      </Card>

      {result ? (
        <Card title="签发结果">
          <Descriptions bordered column={1} size="small">
            <Descriptions.Item label="Principal Type">
              {result.principal.principal_type}
            </Descriptions.Item>
            <Descriptions.Item label="Agent Account ID">
              {result.principal.agent_account_id}
            </Descriptions.Item>
            <Descriptions.Item label="Owner User ID">
              {result.principal.owner_user_id}
            </Descriptions.Item>
            <Descriptions.Item label="Owner Username">
              {result.principal.owner_username}
            </Descriptions.Item>
            <Descriptions.Item label="Expires In">
              {result.expires_in}s
            </Descriptions.Item>
            <Descriptions.Item label="Access Token">
              <Input.TextArea autoSize={{ minRows: 6 }} value={result.access_token} readOnly />
            </Descriptions.Item>
          </Descriptions>
        </Card>
      ) : null}
    </Space>
  );
}
