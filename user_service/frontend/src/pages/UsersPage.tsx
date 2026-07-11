// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { type ReactNode, useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import {
  App,
  Button,
  Drawer,
  Empty,
  Form,
  Input,
  InputNumber,
  Modal,
  Select,
  Space,
  Switch,
  Table,
  Tag,
  Tooltip,
  Typography,
} from 'antd';
import type { ColumnsType } from 'antd/es/table';
import { EditOutlined, PlusOutlined, ReloadOutlined, SyncOutlined } from '@ant-design/icons';
import dayjs from 'dayjs';

import { api } from '../api/client';
import type {
  CreateInviteCodePayload,
  CreateUserPayload,
  InviteCodeRecord,
  ProvisionHarnessPayload,
  UpdateUserPayload,
  UserRole,
  UserSummaryRecord,
} from '../types';

type UserFormValues = {
  username: string;
  display_name?: string;
  password?: string;
  role: UserRole;
  enabled: boolean;
};

type HarnessProvisionFormValues = {
  password: string;
};

type InviteCodeFormValues = {
  label?: string;
  max_uses?: number;
  expires_in_days?: number;
};

export function UsersPage() {
  const { message } = App.useApp();
  const queryClient = useQueryClient();
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [editingUser, setEditingUser] = useState<UserSummaryRecord | null>(null);
  const [harnessProvisionUser, setHarnessProvisionUser] = useState<UserSummaryRecord | null>(null);
  const [inviteModalOpen, setInviteModalOpen] = useState(false);
  const [form] = Form.useForm<UserFormValues>();
  const [harnessForm] = Form.useForm<HarnessProvisionFormValues>();
  const [inviteForm] = Form.useForm<InviteCodeFormValues>();

  const currentUserQuery = useQuery({
    queryKey: ['current-user'],
    queryFn: () => api.currentUser(),
  });
  const usersQuery = useQuery({
    queryKey: ['users'],
    queryFn: () => api.listUsers(),
  });

  const currentUser = currentUserQuery.data?.user;
  const isSuperAdmin = currentUser?.role === 'super_admin';

  const inviteCodesQuery = useQuery({
    queryKey: ['invite-codes'],
    queryFn: () => api.listInviteCodes(),
    enabled: isSuperAdmin,
  });

  const createUserMutation = useMutation({
    mutationFn: (payload: CreateUserPayload) => api.createUser(payload),
    onSuccess: async () => {
      message.success('用户已创建');
      closeDrawer();
      await queryClient.invalidateQueries({ queryKey: ['users'] });
    },
    onError: showError,
  });

  const updateUserMutation = useMutation({
    mutationFn: ({ id, payload }: { id: string; payload: UpdateUserPayload }) =>
      api.updateUser(id, payload),
    onSuccess: async () => {
      message.success('用户已更新');
      closeDrawer();
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ['users'] }),
        queryClient.invalidateQueries({ queryKey: ['current-user'] }),
      ]);
    },
    onError: showError,
  });

  const provisionHarnessMutation = useMutation({
    mutationFn: ({ id, payload }: { id: string; payload: ProvisionHarnessPayload }) =>
      api.provisionHarnessUser(id, payload),
    onSuccess: () => {
      message.success('Harness 账号已开通');
      closeHarnessProvisionModal({ force: true });
    },
    onError: showError,
    onSettled: async () => {
      await queryClient.invalidateQueries({ queryKey: ['users'] });
    },
  });

  const createInviteCodeMutation = useMutation({
    mutationFn: (payload: CreateInviteCodePayload) => api.createInviteCode(payload),
    onSuccess: async (response) => {
      setInviteModalOpen(false);
      inviteForm.resetFields();
      Modal.info({
        title: '新邀请码',
        content: (
          <Space direction="vertical" style={{ width: '100%' }}>
            <Typography.Text copyable={{ text: response.code }} code>
              {response.code}
            </Typography.Text>
            <Typography.Text type="secondary">邀请码只在生成时显示一次，请现在复制保存。</Typography.Text>
          </Space>
        ),
      });
      await queryClient.invalidateQueries({ queryKey: ['invite-codes'] });
    },
    onError: showError,
  });

  const revokeInviteCodeMutation = useMutation({
    mutationFn: (id: string) => api.revokeInviteCode(id),
    onSuccess: async () => {
      message.success('邀请码已撤销');
      await queryClient.invalidateQueries({ queryKey: ['invite-codes'] });
    },
    onError: showError,
  });

  const pending = createUserMutation.isPending || updateUserMutation.isPending;

  const inviteColumns: ColumnsType<InviteCodeRecord> = [
    {
      title: '标签',
      dataIndex: 'label',
      render: (value?: string | null) => value || '-',
    },
    {
      title: '使用',
      width: 120,
      render: (_, record) => `${record.used_count}/${record.max_uses}`,
    },
    {
      title: '状态',
      width: 120,
      render: (_, record) => renderInviteStatus(record),
    },
    {
      title: '过期时间',
      dataIndex: 'expires_at_unix',
      width: 180,
      render: (value?: number | null) => (value ? dayjs.unix(value).format('YYYY-MM-DD HH:mm:ss') : '-'),
    },
    {
      title: '创建时间',
      dataIndex: 'created_at',
      width: 180,
      render: (value: string) => dayjs(value).format('YYYY-MM-DD HH:mm:ss'),
    },
    {
      title: '操作',
      width: 120,
      render: (_, record) => (
        <Button
          danger
          size="small"
          disabled={Boolean(record.revoked_at)}
          loading={revokeInviteCodeMutation.isPending}
          onClick={() => revokeInviteCodeMutation.mutate(record.id)}
        >
          撤销
        </Button>
      ),
    },
  ];

  const columns: ColumnsType<UserSummaryRecord> = [
    {
      title: '用户',
      dataIndex: 'username',
      render: (_, record) => (
        <Space direction="vertical" size={0}>
          <Space size={8}>
            <Typography.Text strong>{record.display_name || record.username}</Typography.Text>
            {record.id === currentUser?.id ? <Tag color="blue">当前登录</Tag> : null}
          </Space>
          <Typography.Text type="secondary">{record.username}</Typography.Text>
        </Space>
      ),
    },
    {
      title: '角色',
      dataIndex: 'role',
      width: 140,
      render: (role: UserRole) => (
        <Tag color={role === 'super_admin' ? 'blue' : 'green'}>
          {role === 'super_admin' ? 'Super Admin' : 'User'}
        </Tag>
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
      title: 'Agent 数量',
      dataIndex: 'agent_count',
      width: 120,
    },
    {
      title: 'Harness',
      dataIndex: 'harness_provisioning',
      width: 190,
      render: (_, record) => renderHarnessStatus(record),
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
      width: 120,
      render: (_, record) => (
        <Button size="small" icon={<EditOutlined />} onClick={() => openEditDrawer(record)}>
          编辑
        </Button>
      ),
    },
  ];

  function renderHarnessStatus(record: UserSummaryRecord) {
    const provisioning = record.harness_provisioning;
    if (!provisioning) {
      return (
        <Space size={4}>
          <Tag>Off</Tag>
          {renderHarnessProvisionButton(record, '开通', <PlusOutlined />)}
        </Space>
      );
    }

    const title = provisioning.last_error
      ? provisioning.last_error
      : `${provisioning.harness_uid} / ${provisioning.space_identifier}`;
    if (provisioning.status === 'provisioned') {
      return (
        <Tooltip title={title}>
          <Tag color="success">OK</Tag>
        </Tooltip>
      );
    }
    if (provisioning.status === 'pending') {
      return (
        <Tooltip title={title}>
          <Tag color="processing">Pending</Tag>
        </Tooltip>
      );
    }
    if (provisioning.status === 'failed') {
      return (
        <Space size={4}>
          <Tooltip title={title}>
            <Tag color="error">Failed</Tag>
          </Tooltip>
          {renderHarnessProvisionButton(record, '重试', <SyncOutlined />)}
        </Space>
      );
    }
    return (
      <Tooltip title={title}>
        <Tag>{provisioning.status}</Tag>
      </Tooltip>
    );
  }

  function renderInviteStatus(record: InviteCodeRecord) {
    if (record.revoked_at) {
      return <Tag color="default">Revoked</Tag>;
    }
    if (record.used_count >= record.max_uses) {
      return <Tag color="warning">Used</Tag>;
    }
    if (record.expires_at_unix && record.expires_at_unix < Math.floor(Date.now() / 1000)) {
      return <Tag color="error">Expired</Tag>;
    }
    return <Tag color="success">Active</Tag>;
  }

  function renderHarnessProvisionButton(
    record: UserSummaryRecord,
    label: string,
    icon: ReactNode,
  ) {
    if (!isSuperAdmin || !record.enabled) {
      return null;
    }
    return (
      <Button
        type="link"
        size="small"
        icon={icon}
        loading={provisionHarnessMutation.isPending && harnessProvisionUser?.id === record.id}
        onClick={() => openHarnessProvisionModal(record)}
      >
        {label}
      </Button>
    );
  }

  function showError(error: unknown) {
    message.error(error instanceof Error ? error.message : '操作失败');
  }

  function openCreateDrawer() {
    setEditingUser(null);
    form.resetFields();
    form.setFieldsValue({ enabled: true, role: 'user' });
    setDrawerOpen(true);
  }

  function openEditDrawer(user: UserSummaryRecord) {
    setEditingUser(user);
    form.setFieldsValue({
      username: user.username,
      display_name: user.display_name,
      password: undefined,
      role: user.role,
      enabled: user.enabled,
    });
    setDrawerOpen(true);
  }

  function closeDrawer() {
    setDrawerOpen(false);
    setEditingUser(null);
    form.resetFields();
  }

  function openHarnessProvisionModal(user: UserSummaryRecord) {
    setHarnessProvisionUser(user);
    harnessForm.resetFields();
  }

  function closeHarnessProvisionModal(options?: { force?: boolean }) {
    if (provisionHarnessMutation.isPending && !options?.force) {
      return;
    }
    setHarnessProvisionUser(null);
    harnessForm.resetFields();
  }

  function submitUser(values: UserFormValues) {
    if (editingUser) {
      const payload: UpdateUserPayload = {
        display_name: values.display_name,
      };
      if (values.password?.trim()) {
        payload.password = values.password;
      }
      if (isSuperAdmin) {
        payload.role = values.role;
        payload.enabled = values.enabled;
      }
      updateUserMutation.mutate({ id: editingUser.id, payload });
      return;
    }

    createUserMutation.mutate({
      username: values.username,
      display_name: values.display_name,
      password: values.password || '',
      role: values.role,
      enabled: values.enabled,
    });
  }

  function submitHarnessProvision(values: HarnessProvisionFormValues) {
    if (!harnessProvisionUser) {
      return;
    }
    provisionHarnessMutation.mutate({
      id: harnessProvisionUser.id,
      payload: { password: values.password },
    });
  }

  function submitInviteCode(values: InviteCodeFormValues) {
    createInviteCodeMutation.mutate({
      label: values.label,
      max_uses: values.max_uses,
      expires_in_days: values.expires_in_days,
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
            用户管理
          </Typography.Title>
          <Typography.Text type="secondary">
            Super Admin 可以管理所有真实用户，普通用户只能编辑自己的资料。
          </Typography.Text>
        </Space>
        <Space>
          <Button
            icon={<ReloadOutlined />}
            onClick={() => usersQuery.refetch()}
            loading={usersQuery.isFetching}
          >
            刷新
          </Button>
          {isSuperAdmin ? <Button onClick={() => setInviteModalOpen(true)}>生成邀请码</Button> : null}
          {isSuperAdmin ? (
            <Button type="primary" icon={<PlusOutlined />} onClick={openCreateDrawer}>
              新建用户
            </Button>
          ) : null}
        </Space>
      </div>

      <Table<UserSummaryRecord>
        rowKey="id"
        columns={columns}
        dataSource={usersQuery.data || []}
        loading={usersQuery.isLoading}
        pagination={{ pageSize: 10, showSizeChanger: true }}
        locale={{
          emptyText: <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description="暂无用户" />,
        }}
      />

      {isSuperAdmin ? (
        <Table<InviteCodeRecord>
          rowKey="id"
          title={() => '邀请码'}
          columns={inviteColumns}
          dataSource={inviteCodesQuery.data || []}
          loading={inviteCodesQuery.isLoading}
          pagination={{ pageSize: 5, showSizeChanger: true }}
        />
      ) : null}

      <Drawer
        title={editingUser ? `编辑用户 ${editingUser.username}` : '新建用户'}
        open={drawerOpen}
        width={460}
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
        <Form<UserFormValues>
          form={form}
          layout="vertical"
          requiredMark={false}
          initialValues={{ enabled: true, role: 'user' }}
          onFinish={submitUser}
        >
          <Form.Item
            name="username"
            label="用户名"
            rules={[{ required: !editingUser, message: '请输入用户名' }]}
          >
            <Input disabled={Boolean(editingUser)} autoComplete="username" />
          </Form.Item>
          <Form.Item name="display_name" label="显示名">
            <Input autoComplete="name" />
          </Form.Item>
          <Form.Item
            name="password"
            label={editingUser ? '重置密码' : '密码'}
            rules={[{ required: !editingUser, message: '请输入密码' }]}
          >
            <Input.Password autoComplete="new-password" />
          </Form.Item>
          <Form.Item name="role" label="角色">
            <Select
              disabled={!isSuperAdmin}
              options={[
                { label: 'User', value: 'user' },
                { label: 'Super Admin', value: 'super_admin' },
              ]}
            />
          </Form.Item>
          <Form.Item name="enabled" label="状态" valuePropName="checked">
            <Switch checkedChildren="启用" unCheckedChildren="禁用" disabled={!isSuperAdmin} />
          </Form.Item>
        </Form>
      </Drawer>

      <Modal
        title={
          harnessProvisionUser
            ? `开通 Harness 账号 ${harnessProvisionUser.username}`
            : '开通 Harness 账号'
        }
        open={Boolean(harnessProvisionUser)}
        okText={harnessProvisionUser?.harness_provisioning ? '重试开通' : '开通'}
        cancelText="取消"
        confirmLoading={provisionHarnessMutation.isPending}
        onOk={() => harnessForm.submit()}
        onCancel={() => closeHarnessProvisionModal()}
        destroyOnClose
      >
        <Form<HarnessProvisionFormValues>
          form={harnessForm}
          layout="vertical"
          requiredMark={false}
          onFinish={submitHarnessProvision}
        >
          <Typography.Paragraph type="secondary">
            这个密码会同时写入 Chatos 和 Harness，保持两个账号登录密码一致。
          </Typography.Paragraph>
          <Form.Item
            name="password"
            label="Chatos / Harness 密码"
            rules={[{ required: true, message: '请输入密码' }]}
          >
            <Input.Password autoComplete="new-password" />
          </Form.Item>
        </Form>
      </Modal>

      <Modal
        title="生成邀请码"
        open={inviteModalOpen}
        okText="生成"
        cancelText="取消"
        confirmLoading={createInviteCodeMutation.isPending}
        onOk={() => inviteForm.submit()}
        onCancel={() => setInviteModalOpen(false)}
        destroyOnClose
      >
        <Form<InviteCodeFormValues>
          form={inviteForm}
          layout="vertical"
          requiredMark={false}
          initialValues={{ max_uses: 1, expires_in_days: 30 }}
          onFinish={submitInviteCode}
        >
          <Form.Item name="label" label="标签">
            <Input placeholder="例如：内测用户 / 客户 A" />
          </Form.Item>
          <Form.Item name="max_uses" label="可使用次数" rules={[{ required: true }]}>
            <InputNumber min={1} max={10000} style={{ width: '100%' }} />
          </Form.Item>
          <Form.Item name="expires_in_days" label="有效天数">
            <InputNumber min={1} max={3650} style={{ width: '100%' }} />
          </Form.Item>
        </Form>
      </Modal>
    </Space>
  );
}
