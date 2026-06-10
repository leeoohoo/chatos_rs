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
import { useI18n } from '../i18n/I18nProvider';
import type { CreateUserPayload, UpdateUserPayload, UserRole, UserSummaryRecord } from '../types';

type UserFormValues = {
  username: string;
  display_name?: string;
  password?: string;
  role: UserRole;
  enabled: boolean;
};

export function UsersPage() {
  const { t } = useI18n();
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
      message.success(t('users.created'));
      closeDrawer();
      await queryClient.invalidateQueries({ queryKey: ['users'] });
    },
    onError: showError,
  });

  const updateUserMutation = useMutation({
    mutationFn: ({ id, payload }: { id: string; payload: UpdateUserPayload }) =>
      api.updateUser(id, payload),
    onSuccess: async () => {
      message.success(t('users.updated'));
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
      message.success(t('users.deleted'));
      await queryClient.invalidateQueries({ queryKey: ['users'] });
    },
    onError: showError,
  });

  const currentUserId = currentUserQuery.data?.user.id;
  const pending =
    createUserMutation.isPending || updateUserMutation.isPending || deleteUserMutation.isPending;

  const columns: ColumnsType<UserSummaryRecord> = [
    {
      title: t('users.column.user'),
      dataIndex: 'username',
      render: (_, record) => (
        <Space direction="vertical" size={0}>
          <Space size={8}>
            <Typography.Text strong>{record.display_name || record.username}</Typography.Text>
            {record.id === currentUserId ? <Tag color="blue">{t('users.currentUser')}</Tag> : null}
          </Space>
          <Typography.Text type="secondary">{record.username}</Typography.Text>
        </Space>
      ),
    },
    {
      title: t('common.status'),
      dataIndex: 'enabled',
      width: 110,
      render: (enabled: boolean) => (
        <Tag color={enabled ? 'success' : 'default'}>
          {enabled ? t('users.enabled') : t('users.disabled')}
        </Tag>
      ),
    },
    {
      title: t('users.column.role'),
      dataIndex: 'role',
      width: 110,
      render: (role: UserRole) => (
        <Tag color={role === 'admin' ? 'blue' : 'purple'}>
          {role === 'admin' ? t('users.role.admin') : 'AI agent'}
        </Tag>
      ),
    },
    {
      title: t('users.lastLogin'),
      dataIndex: 'last_login_at',
      width: 180,
      render: (value?: string | null) =>
        value ? dayjs(value).format('YYYY-MM-DD HH:mm:ss') : '-',
    },
    {
      title: t('models.detail.createdAt'),
      dataIndex: 'created_at',
      width: 180,
      render: (value: string) => dayjs(value).format('YYYY-MM-DD HH:mm:ss'),
    },
    {
      title: t('common.updatedAt'),
      dataIndex: 'updated_at',
      width: 180,
      render: (value: string) => dayjs(value).format('YYYY-MM-DD HH:mm:ss'),
    },
    {
      title: t('common.actions'),
      key: 'actions',
      width: 300,
      render: (_, record) => {
        const isSelf = record.id === currentUserId;
        return (
          <Space wrap>
            <Button size="small" icon={<EditOutlined />} onClick={() => openEditDrawer(record)}>
              {t('common.edit')}
            </Button>
            <Button
              size="small"
              icon={record.enabled ? <StopOutlined /> : <UnlockOutlined />}
              disabled={isSelf}
              loading={updateUserMutation.isPending}
              onClick={() => toggleUserEnabled(record)}
            >
              {record.enabled ? t('users.disabled') : t('users.enabled')}
            </Button>
            <Button
              size="small"
              danger
              icon={<DeleteOutlined />}
              disabled={isSelf}
              loading={deleteUserMutation.isPending}
              onClick={() => confirmDelete(record)}
            >
              {t('common.delete')}
            </Button>
          </Space>
        );
      },
    },
  ];

  function showError(error: unknown) {
    message.error(error instanceof Error ? error.message : t('users.operationFailed'));
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
      title: t('users.deleteConfirmTitle', { username: user.username }),
      content: t('users.deleteConfirmContent'),
      okText: t('common.delete'),
      okButtonProps: { danger: true },
      cancelText: t('common.cancel'),
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
            {t('users.title')}
          </Typography.Title>
          <Typography.Text type="secondary">{t('users.subtitle')}</Typography.Text>
        </Space>
        <Space>
          <Button
            icon={<ReloadOutlined />}
            onClick={() => usersQuery.refetch()}
            loading={usersQuery.isFetching}
          >
            {t('common.refresh')}
          </Button>
          <Button type="primary" icon={<PlusOutlined />} onClick={openCreateDrawer}>
            {t('users.new')}
          </Button>
        </Space>
      </div>

      <Table<UserSummaryRecord>
        rowKey="id"
        columns={columns}
        dataSource={usersQuery.data || []}
        loading={usersQuery.isLoading}
        pagination={{ pageSize: 10, showSizeChanger: true }}
        locale={{ emptyText: <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('users.empty')} /> }}
      />

      <Drawer
        title={editingUser
          ? t('users.drawer.editWithName', { username: editingUser.username })
          : t('users.drawer.create')}
        open={drawerOpen}
        width={460}
        onClose={closeDrawer}
        destroyOnClose
        extra={
          <Space>
            <Button onClick={closeDrawer}>{t('common.cancel')}</Button>
            <Button type="primary" loading={pending} onClick={() => form.submit()}>
              {t('common.save')}
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
            label={t('users.form.username')}
            rules={[{ required: !editingUser, message: t('users.form.usernameRequired') }]}
          >
            <Input disabled={Boolean(editingUser)} autoComplete="username" />
          </Form.Item>
          <Form.Item name="display_name" label={t('users.form.displayName')}>
            <Input autoComplete="name" />
          </Form.Item>
          <Form.Item name="role" label={t('users.column.role')} rules={[{ required: true, message: t('users.form.roleRequired') }]}>
            <Select
              disabled={editingUser?.id === currentUserId}
              options={[
                { label: 'AI agent', value: 'agent' },
                { label: t('users.role.admin'), value: 'admin' },
              ]}
            />
          </Form.Item>
          <Form.Item
            name="password"
            label={editingUser ? t('users.form.resetPassword') : t('auth.password')}
            rules={[{ required: !editingUser, message: t('users.form.passwordRequired') }]}
        >
            <Input.Password autoComplete={editingUser ? 'new-password' : 'new-password'} />
          </Form.Item>
          <Form.Item name="enabled" label={t('common.status')} valuePropName="checked">
            <Switch
              checkedChildren={t('users.enabled')}
              unCheckedChildren={t('users.disabled')}
              disabled={editingUser?.id === currentUserId}
            />
          </Form.Item>
        </Form>
      </Drawer>
    </Space>
  );
}
