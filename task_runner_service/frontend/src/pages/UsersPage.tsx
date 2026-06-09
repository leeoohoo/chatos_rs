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
import {
  DeleteOutlined,
  EditOutlined,
  PlusOutlined,
  ReloadOutlined,
  StopOutlined,
  UnlockOutlined,
} from '@ant-design/icons';
import dayjs from 'dayjs';

import { api } from '../api/client';
import type { CreateUserPayload, UpdateUserPayload, UserRole, UserSummaryRecord } from '../types';

type UserFormValues = {
  username: string;
  display_name?: string;
  password?: string;
  role: UserRole;
  enabled: boolean;
};

export function UsersPage() {
  const { message } = App.useApp();
  const [modal, modalContext] = Modal.useModal();
  const queryClient = useQueryClient();
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [editingUser, setEditingUser] = useState<UserSummaryRecord | null>(null);
  const [form] = Form.useForm<UserFormValues>();

  const currentUserQuery = useQuery({
    queryKey: ['current-user'],
    queryFn: () => api.currentUser(),
  });
  const usersQuery = useQuery({
    queryKey: ['users'],
    queryFn: () => api.listUsers(),
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

  const deleteUserMutation = useMutation({
    mutationFn: (id: string) => api.deleteUser(id),
    onSuccess: async () => {
      message.success('用户已删除');
      await queryClient.invalidateQueries({ queryKey: ['users'] });
    },
    onError: showError,
  });

  const currentUserId = currentUserQuery.data?.user.id;
  const pending =
    createUserMutation.isPending || updateUserMutation.isPending || deleteUserMutation.isPending;

  const columns: ColumnsType<UserSummaryRecord> = [
    {
      title: '用户',
      dataIndex: 'username',
      render: (_, record) => (
        <Space direction="vertical" size={0}>
          <Space size={8}>
            <Typography.Text strong>{record.display_name || record.username}</Typography.Text>
            {record.id === currentUserId ? <Tag color="blue">当前用户</Tag> : null}
          </Space>
          <Typography.Text type="secondary">{record.username}</Typography.Text>
        </Space>
      ),
    },
    {
      title: '状态',
      dataIndex: 'enabled',
      width: 110,
      render: (enabled: boolean) => (
        <Tag color={enabled ? 'success' : 'default'}>{enabled ? '启用' : '禁用'}</Tag>
      ),
    },
    {
      title: '角色',
      dataIndex: 'role',
      width: 110,
      render: (role: UserRole) => (
        <Tag color={role === 'admin' ? 'blue' : 'purple'}>
          {role === 'admin' ? '管理员' : 'AI agent'}
        </Tag>
      ),
    },
    {
      title: '最近登录',
      dataIndex: 'last_login_at',
      width: 180,
      render: (value?: string | null) =>
        value ? dayjs(value).format('YYYY-MM-DD HH:mm:ss') : '-',
    },
    {
      title: '创建时间',
      dataIndex: 'created_at',
      width: 180,
      render: (value: string) => dayjs(value).format('YYYY-MM-DD HH:mm:ss'),
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
      width: 300,
      render: (_, record) => {
        const isSelf = record.id === currentUserId;
        return (
          <Space wrap>
            <Button size="small" icon={<EditOutlined />} onClick={() => openEditDrawer(record)}>
              编辑
            </Button>
            <Button
              size="small"
              icon={record.enabled ? <StopOutlined /> : <UnlockOutlined />}
              disabled={isSelf}
              loading={updateUserMutation.isPending}
              onClick={() => toggleUserEnabled(record)}
            >
              {record.enabled ? '禁用' : '启用'}
            </Button>
            <Button
              size="small"
              danger
              icon={<DeleteOutlined />}
              disabled={isSelf}
              loading={deleteUserMutation.isPending}
              onClick={() => confirmDelete(record)}
            >
              删除
            </Button>
          </Space>
        );
      },
    },
  ];

  function showError(error: unknown) {
    message.error(error instanceof Error ? error.message : '操作失败');
  }

  function openCreateDrawer() {
    setEditingUser(null);
    form.resetFields();
    form.setFieldsValue({ enabled: true, role: 'agent' });
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

  function submitUser(values: UserFormValues) {
    if (editingUser) {
      const payload: UpdateUserPayload = {
        display_name: values.display_name,
        role: values.role,
        enabled: values.enabled,
      };
      if (values.password?.trim()) {
        payload.password = values.password;
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

  function toggleUserEnabled(user: UserSummaryRecord) {
    updateUserMutation.mutate({
      id: user.id,
      payload: {
        enabled: !user.enabled,
      },
    });
  }

  function confirmDelete(user: UserSummaryRecord) {
    modal.confirm({
      title: `删除用户 ${user.username}`,
      content: '删除后该用户不能再登录，已有任务上的创建人显示不会被清空。',
      okText: '删除',
      okButtonProps: { danger: true },
      cancelText: '取消',
      onOk: () => deleteUserMutation.mutateAsync(user.id),
    });
  }

  return (
    <Space direction="vertical" size="large" style={{ width: '100%' }}>
      {modalContext}
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
          <Typography.Text type="secondary">管理员管理后台，AI agent 通过 MCP token 使用任务系统。</Typography.Text>
        </Space>
        <Space>
          <Button
            icon={<ReloadOutlined />}
            onClick={() => usersQuery.refetch()}
            loading={usersQuery.isFetching}
          >
            刷新
          </Button>
          <Button type="primary" icon={<PlusOutlined />} onClick={openCreateDrawer}>
            新建用户
          </Button>
        </Space>
      </div>

      <Table<UserSummaryRecord>
        rowKey="id"
        columns={columns}
        dataSource={usersQuery.data || []}
        loading={usersQuery.isLoading}
        pagination={{ pageSize: 10, showSizeChanger: true }}
        locale={{ emptyText: <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description="暂无用户" /> }}
      />

      <Drawer
        title={editingUser ? `编辑用户 - ${editingUser.username}` : '新建用户'}
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
          initialValues={{ enabled: true }}
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
          <Form.Item name="role" label="角色" rules={[{ required: true, message: '请选择角色' }]}>
            <Select
              disabled={editingUser?.id === currentUserId}
              options={[
                { label: 'AI agent', value: 'agent' },
                { label: '管理员', value: 'admin' },
              ]}
            />
          </Form.Item>
          <Form.Item
            name="password"
            label={editingUser ? '重置密码' : '密码'}
            rules={[{ required: !editingUser, message: '请输入密码' }]}
        >
            <Input.Password autoComplete={editingUser ? 'new-password' : 'new-password'} />
          </Form.Item>
          <Form.Item name="enabled" label="状态" valuePropName="checked">
            <Switch checkedChildren="启用" unCheckedChildren="禁用" disabled={editingUser?.id === currentUserId} />
          </Form.Item>
        </Form>
      </Drawer>
    </Space>
  );
}
