import { DeleteOutlined, EditOutlined, FolderOpenOutlined, PlusOutlined } from '@ant-design/icons';
import { useEffect, useState } from 'react';
import {
  Alert,
  Button,
  Card,
  Form,
  Input,
  Modal,
  Popconfirm,
  Select,
  Space,
  Table,
  Tag,
} from 'antd';
import type { ColumnsType } from 'antd/es/table';

import { api } from '../api/client';
import { useI18n } from '../i18n';
import type { UserItem } from '../types';

interface UserManagementPageProps {
  isAdmin: boolean;
  currentUsername: string;
  onOpenConfigUser?: (username: string) => void;
}

interface CreateUserForm {
  username: string;
  password: string;
  role: 'admin' | 'user';
}

interface EditUserForm {
  role: 'admin' | 'user';
  password?: string;
}

export function UserManagementPage({
  isAdmin,
  currentUsername,
  onOpenConfigUser,
}: UserManagementPageProps) {
  const { t } = useI18n();
  const [users, setUsers] = useState<UserItem[]>([]);
  const [loading, setLoading] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [createOpen, setCreateOpen] = useState(false);
  const [editOpen, setEditOpen] = useState(false);
  const [editingUser, setEditingUser] = useState<UserItem | null>(null);

  const [createForm] = Form.useForm<CreateUserForm>();
  const [editForm] = Form.useForm<EditUserForm>();

  const load = async () => {
    setLoading(true);
    setError(null);
    try {
      const items = await api.listUsers(500);
      setUsers(items);
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    load();
  }, []);

  const openCreate = () => {
    createForm.setFieldsValue({
      username: '',
      password: '',
      role: 'user',
    });
    setCreateOpen(true);
  };

  const submitCreate = async () => {
    setError(null);
    setMessage(null);

    try {
      const values = await createForm.validateFields();
      setSubmitting(true);
      await api.createUser({
        username: values.username.trim(),
        password: values.password,
        role: values.role,
      });
      setMessage(t('users.createSuccess'));
      setCreateOpen(false);
      await load();
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setSubmitting(false);
    }
  };

  const openEdit = (user: UserItem) => {
    setEditingUser(user);
    editForm.setFieldsValue({
      role: (user.role as 'admin' | 'user') || 'user',
      password: '',
    });
    setEditOpen(true);
  };

  const submitEdit = async () => {
    if (!editingUser) {
      return;
    }

    setError(null);
    setMessage(null);

    try {
      const values = await editForm.validateFields();
      const payload: { role?: string; password?: string } = {};
      if (values.role && values.role !== editingUser.role) {
        payload.role = values.role;
      }
      if (values.password && values.password.trim()) {
        payload.password = values.password.trim();
      }

      if (!payload.password && !payload.role) {
        setError(t('users.noChanges'));
        return;
      }

      setSubmitting(true);
      await api.updateUser(editingUser.username, payload);
      setMessage(t('users.updateSuccess'));
      setEditOpen(false);
      setEditingUser(null);
      await load();
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setSubmitting(false);
    }
  };

  const removeUser = async (username: string) => {
    setError(null);
    setMessage(null);

    try {
      await api.deleteUser(username);
      setMessage(t('users.deleteSuccess'));
      await load();
    } catch (err) {
      setError((err as Error).message);
    }
  };

  const columns: ColumnsType<UserItem> = [
    {
      title: t('users.userId'),
      dataIndex: 'username',
      key: 'username',
      render: (value: string) => (
        <Space size={6}>
          <span>{value}</span>
          {currentUsername === value && <Tag>{t('users.current')}</Tag>}
        </Space>
      ),
    },
    {
      title: t('users.role'),
      dataIndex: 'role',
      key: 'role',
      width: 120,
      render: (value: string) =>
        value === 'admin' ? <Tag color="gold">admin</Tag> : <Tag color="default">user</Tag>,
    },
    {
      title: t('users.createdAt'),
      dataIndex: 'created_at',
      key: 'created_at',
      width: 180,
      render: (value: string) => new Date(value).toLocaleString(),
    },
    {
      title: t('users.updatedAt'),
      dataIndex: 'updated_at',
      key: 'updated_at',
      width: 180,
      render: (value: string) => new Date(value).toLocaleString(),
    },
    {
      title: t('common.action'),
      key: 'action',
      width: 300,
      render: (_, item) => {
        const isAdminAccount = item.username === 'admin';
        const isCurrentLoginUser = item.username === currentUsername;
        const disableDelete = isAdminAccount || isCurrentLoginUser;

        return (
          <Space>
            <Button
              size="small"
              icon={<FolderOpenOutlined />}
              type="default"
              onClick={() => onOpenConfigUser?.(item.username)}
            >
              {t('users.openConfig')}
            </Button>
            {isAdmin && (
              <Button size="small" icon={<EditOutlined />} onClick={() => openEdit(item)}>
                {t('common.edit')}
              </Button>
            )}
            {isAdmin && (
              <Popconfirm
                title={t('users.deleteConfirm')}
                onConfirm={() => removeUser(item.username)}
                okText={t('common.confirm')}
                cancelText={t('common.cancel')}
                disabled={disableDelete}
              >
                <Button size="small" danger icon={<DeleteOutlined />} disabled={disableDelete}>
                  {t('common.delete')}
                </Button>
              </Popconfirm>
            )}
          </Space>
        );
      },
    },
  ];

  return (
    <Card
      title={t('users.title')}
      extra={
        <Space>
          <Button onClick={load} loading={loading}>
            {t('common.refresh')}
          </Button>
          {isAdmin && (
            <Button type="primary" icon={<PlusOutlined />} onClick={openCreate}>
              {t('users.create')}
            </Button>
          )}
        </Space>
      }
    >
      {error && <Alert type="error" showIcon message={error} style={{ marginBottom: 12 }} />}
      {message && <Alert type="success" showIcon message={message} style={{ marginBottom: 12 }} />}

      <Table<UserItem>
        rowKey="username"
        loading={loading}
        columns={columns}
        dataSource={users}
        pagination={false}
        size="small"
        scroll={{ x: 960 }}
      />

      {isAdmin && (
        <Modal
          title={t('users.create')}
          open={createOpen}
          onCancel={() => setCreateOpen(false)}
          onOk={submitCreate}
          confirmLoading={submitting}
          okText={t('common.save')}
          cancelText={t('common.cancel')}
          destroyOnClose
        >
          <Form form={createForm} layout="vertical">
            <Form.Item
              name="username"
              label={t('users.userId')}
              rules={[{ required: true, message: t('users.userIdRequired') }]}
            >
              <Input autoComplete="off" />
            </Form.Item>
            <Form.Item
              name="password"
              label={t('users.password')}
              rules={[{ required: true, message: t('users.passwordRequired') }]}
            >
              <Input.Password autoComplete="new-password" />
            </Form.Item>
            <Form.Item name="role" label={t('users.role')} rules={[{ required: true }]}>
              <Select
                options={[
                  { label: 'user', value: 'user' },
                  { label: 'admin', value: 'admin' },
                ]}
              />
            </Form.Item>
          </Form>
        </Modal>
      )}

      {isAdmin && (
        <Modal
          title={t('users.edit')}
          open={editOpen}
          onCancel={() => {
            setEditOpen(false);
            setEditingUser(null);
          }}
          onOk={submitEdit}
          confirmLoading={submitting}
          okText={t('common.save')}
          cancelText={t('common.cancel')}
          destroyOnClose
        >
          <Form form={editForm} layout="vertical">
          <Form.Item name="role" label={t('users.role')} rules={[{ required: true }]}>
            <Select
              disabled={editingUser?.username === 'admin'}
              options={[
                { label: 'user', value: 'user' },
                { label: 'admin', value: 'admin' },
              ]}
            />
          </Form.Item>
            <Form.Item name="password" label={t('users.newPassword')}>
              <Input.Password autoComplete="new-password" />
            </Form.Item>
          </Form>
        </Modal>
      )}
    </Card>
  );
}
