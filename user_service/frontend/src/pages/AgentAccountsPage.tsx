import { useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import {
  App,
  Button,
  Drawer,
  Empty,
  Form,
  Input,
  Modal,
  Select,
  Space,
  Switch,
  Table,
  Tag,
  Typography,
} from 'antd';
import type { ColumnsType } from 'antd/es/table';
import { EditOutlined, KeyOutlined, PlusOutlined, ReloadOutlined } from '@ant-design/icons';
import dayjs from 'dayjs';

import { api } from '../api/client';
import type {
  AgentAccountListItem,
  CreateAgentAccountPayload,
  ResetAgentPasswordPayload,
  UpdateAgentAccountPayload,
  UserSummaryRecord,
} from '../types';

type AgentFormValues = {
  username: string;
  display_name?: string;
  password?: string;
  owner_user_id?: string;
  enabled: boolean;
};

type ResetPasswordValues = {
  password: string;
};

export function AgentAccountsPage() {
  const { message } = App.useApp();
  const queryClient = useQueryClient();
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [editingAgent, setEditingAgent] = useState<AgentAccountListItem | null>(null);
  const [resetPasswordAgent, setResetPasswordAgent] = useState<AgentAccountListItem | null>(null);
  const [form] = Form.useForm<AgentFormValues>();
  const [resetPasswordForm] = Form.useForm<ResetPasswordValues>();

  const currentUserQuery = useQuery({
    queryKey: ['current-user'],
    queryFn: () => api.currentUser(),
  });
  const usersQuery = useQuery({
    queryKey: ['users'],
    queryFn: () => api.listUsers(),
  });
  const agentsQuery = useQuery({
    queryKey: ['agent-accounts'],
    queryFn: () => api.listAgentAccounts(),
  });

  const currentUser = currentUserQuery.data?.user;
  const isSuperAdmin = currentUser?.role === 'super_admin';
  const userOptions = (usersQuery.data || []).map((item: UserSummaryRecord) => ({
    label: `${item.display_name || item.username} (${item.username})`,
    value: item.id,
  }));

  const createAgentMutation = useMutation({
    mutationFn: (payload: CreateAgentAccountPayload) => api.createAgentAccount(payload),
    onSuccess: async () => {
      message.success('Agent 账号已创建');
      closeDrawer();
      await queryClient.invalidateQueries({ queryKey: ['agent-accounts'] });
      await queryClient.invalidateQueries({ queryKey: ['users'] });
    },
    onError: showError,
  });

  const updateAgentMutation = useMutation({
    mutationFn: ({ id, payload }: { id: string; payload: UpdateAgentAccountPayload }) =>
      api.updateAgentAccount(id, payload),
    onSuccess: async () => {
      message.success('Agent 账号已更新');
      closeDrawer();
      await queryClient.invalidateQueries({ queryKey: ['agent-accounts'] });
      await queryClient.invalidateQueries({ queryKey: ['users'] });
    },
    onError: showError,
  });

  const resetPasswordMutation = useMutation({
    mutationFn: ({ id, payload }: { id: string; payload: ResetAgentPasswordPayload }) =>
      api.resetAgentPassword(id, payload),
    onSuccess: async () => {
      message.success('Agent 密码已重置');
      setResetPasswordAgent(null);
      resetPasswordForm.resetFields();
      await queryClient.invalidateQueries({ queryKey: ['agent-accounts'] });
    },
    onError: showError,
  });

  const pending =
    createAgentMutation.isPending || updateAgentMutation.isPending || resetPasswordMutation.isPending;

  const columns: ColumnsType<AgentAccountListItem> = [
    {
      title: 'Agent 账号',
      dataIndex: 'username',
      render: (_, record) => (
        <Space direction="vertical" size={0}>
          <Typography.Text strong>{record.display_name || record.username}</Typography.Text>
          <Typography.Text type="secondary">{record.username}</Typography.Text>
        </Space>
      ),
    },
    {
      title: '归属用户',
      dataIndex: 'owner_display_name',
      width: 220,
      render: (_, record) => (
        <Space direction="vertical" size={0}>
          <Typography.Text>{record.owner_display_name || record.owner_username}</Typography.Text>
          <Typography.Text type="secondary">{record.owner_username}</Typography.Text>
        </Space>
      ),
    },
    {
      title: '状态',
      dataIndex: 'enabled',
      width: 120,
      render: (enabled: boolean) => (
        <Tag color={enabled ? 'success' : 'default'}>{enabled ? '启用' : '禁用'}</Tag>
      ),
    },
    {
      title: '最近登录',
      dataIndex: 'last_login_at',
      width: 180,
      render: (value?: string | null) => (value ? dayjs(value).format('YYYY-MM-DD HH:mm:ss') : '-'),
    },
    {
      title: '更新时间',
      dataIndex: 'updated_at',
      width: 180,
      render: (value: string) => dayjs(value).format('YYYY-MM-DD HH:mm:ss'),
    },
    {
      title: '操作',
      key: 'actions',
      width: 220,
      render: (_, record) => (
        <Space wrap>
          <Button size="small" icon={<EditOutlined />} onClick={() => openEditDrawer(record)}>
            编辑
          </Button>
          <Button size="small" icon={<KeyOutlined />} onClick={() => openResetPasswordModal(record)}>
            重置密码
          </Button>
        </Space>
      ),
    },
  ];

  function showError(error: unknown) {
    message.error(error instanceof Error ? error.message : '操作失败');
  }

  function openCreateDrawer() {
    setEditingAgent(null);
    form.resetFields();
    form.setFieldsValue({
      enabled: true,
      owner_user_id: currentUser?.id,
    });
    setDrawerOpen(true);
  }

  function openEditDrawer(agent: AgentAccountListItem) {
    setEditingAgent(agent);
    form.setFieldsValue({
      username: agent.username,
      display_name: agent.display_name,
      password: undefined,
      owner_user_id: agent.owner_user_id,
      enabled: agent.enabled,
    });
    setDrawerOpen(true);
  }

  function closeDrawer() {
    setDrawerOpen(false);
    setEditingAgent(null);
    form.resetFields();
  }

  function openResetPasswordModal(agent: AgentAccountListItem) {
    setResetPasswordAgent(agent);
    resetPasswordForm.resetFields();
  }

  function submitAgent(values: AgentFormValues) {
    if (editingAgent) {
      const payload: UpdateAgentAccountPayload = {
        display_name: values.display_name,
        enabled: values.enabled,
      };
      if (isSuperAdmin) {
        payload.owner_user_id = values.owner_user_id;
      }
      updateAgentMutation.mutate({ id: editingAgent.id, payload });
      return;
    }

    createAgentMutation.mutate({
      username: values.username,
      display_name: values.display_name,
      password: values.password || '',
      owner_user_id: isSuperAdmin ? values.owner_user_id : undefined,
      enabled: values.enabled,
    });
  }

  function submitResetPassword(values: ResetPasswordValues) {
    if (!resetPasswordAgent) {
      return;
    }
    resetPasswordMutation.mutate({
      id: resetPasswordAgent.id,
      payload: {
        password: values.password,
      },
    });
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
            Agent 账号管理
          </Typography.Title>
          <Typography.Text type="secondary">
            每个真实用户都可以创建和管理自己名下的 Task Runner Agent 账号。
          </Typography.Text>
        </Space>
        <Space>
          <Button
            icon={<ReloadOutlined />}
            onClick={() => agentsQuery.refetch()}
            loading={agentsQuery.isFetching}
          >
            刷新
          </Button>
          <Button type="primary" icon={<PlusOutlined />} onClick={openCreateDrawer}>
            新建 Agent
          </Button>
        </Space>
      </div>

      <Table<AgentAccountListItem>
        rowKey="id"
        columns={columns}
        dataSource={agentsQuery.data || []}
        loading={agentsQuery.isLoading}
        pagination={{ pageSize: 10, showSizeChanger: true }}
        locale={{
          emptyText: <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description="暂无 Agent 账号" />,
        }}
      />

      <Drawer
        title={editingAgent ? `编辑 Agent ${editingAgent.username}` : '新建 Agent 账号'}
        open={drawerOpen}
        width={480}
        onClose={closeDrawer}
        destroyOnClose
        extra={
          <Space>
            <Button onClick={closeDrawer}>取消</Button>
            <Button type="primary" loading={pending} onClick={() => form.submit()}>
              保存
            </Button>
          </Space>
        }
      >
        <Form<AgentFormValues>
          form={form}
          layout="vertical"
          requiredMark={false}
          initialValues={{ enabled: true }}
          onFinish={submitAgent}
        >
          <Form.Item
            name="username"
            label="Agent 用户名"
            rules={[{ required: !editingAgent, message: '请输入 Agent 用户名' }]}
          >
            <Input disabled={Boolean(editingAgent)} autoComplete="username" />
          </Form.Item>
          <Form.Item name="display_name" label="显示名">
            <Input />
          </Form.Item>
          {!editingAgent ? (
            <Form.Item
              name="password"
              label="初始密码"
              rules={[{ required: true, message: '请输入初始密码' }]}
            >
              <Input.Password autoComplete="new-password" />
            </Form.Item>
          ) : null}
          <Form.Item
            name="owner_user_id"
            label="归属用户"
            rules={[{ required: true, message: '请选择归属用户' }]}
          >
            <Select
              disabled={!isSuperAdmin}
              options={userOptions}
              placeholder="选择真实用户"
            />
          </Form.Item>
          <Form.Item name="enabled" label="状态" valuePropName="checked">
            <Switch checkedChildren="启用" unCheckedChildren="禁用" />
          </Form.Item>
        </Form>
      </Drawer>

      <Modal
        title={resetPasswordAgent ? `重置密码: ${resetPasswordAgent.username}` : '重置密码'}
        open={Boolean(resetPasswordAgent)}
        onCancel={() => setResetPasswordAgent(null)}
        onOk={() => resetPasswordForm.submit()}
        confirmLoading={resetPasswordMutation.isPending}
        destroyOnHidden
      >
        <Form<ResetPasswordValues>
          form={resetPasswordForm}
          layout="vertical"
          requiredMark={false}
          onFinish={submitResetPassword}
        >
          <Form.Item
            name="password"
            label="新密码"
            rules={[{ required: true, message: '请输入新密码' }]}
          >
            <Input.Password autoComplete="new-password" />
          </Form.Item>
        </Form>
      </Modal>
    </Space>
  );
}
