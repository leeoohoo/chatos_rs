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
  CreateUserPayload,
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

export function UsersPage() {
  const { message } = App.useApp();
  const queryClient = useQueryClient();
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [editingUser, setEditingUser] = useState<UserSummaryRecord | null>(null);
  const [harnessProvisionUser, setHarnessProvisionUser] = useState<UserSummaryRecord | null>(null);
  const [form] = Form.useForm<UserFormValues>();
  const [harnessForm] = Form.useForm<HarnessProvisionFormValues>();

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

  const pending = createUserMutation.isPending || updateUserMutation.isPending;

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
    </Space>
  );
}
