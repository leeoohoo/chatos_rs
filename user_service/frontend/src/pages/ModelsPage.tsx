import { useEffect, useMemo, useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import {
  Alert,
  App,
  Button,
  Card,
  Drawer,
  Empty,
  Form,
  Input,
  Popconfirm,
  Select,
  Space,
  Switch,
  Table,
  Tag,
  Typography,
} from 'antd';
import type { ColumnsType } from 'antd/es/table';
import { DeleteOutlined, EditOutlined, PlusOutlined, ReloadOutlined } from '@ant-design/icons';
import dayjs from 'dayjs';

import { api } from '../api/client';
import type {
  CreateUserModelProviderPayload,
  UpdateUserModelProviderPayload,
  UserModelConfigRecord,
  UserModelProviderRecord,
  UserSummaryRecord,
} from '../types';

type ProviderFormValues = {
  owner_user_id?: string;
  name: string;
  provider: string;
  api_key?: string;
  clear_api_key?: boolean;
  base_url?: string;
  enabled: boolean;
  supports_images: boolean;
  supports_reasoning: boolean;
  supports_responses: boolean;
};

const PROVIDER_OPTIONS = [
  { label: 'GPT / OpenAI', value: 'gpt' },
  { label: 'DeepSeek', value: 'deepseek' },
  { label: 'Kimi', value: 'kimi' },
  { label: 'MiniMax', value: 'minimax' },
  { label: 'OpenAI Compatible', value: 'openai_compatible' },
];

const ALL_USERS_SCOPE = '__all_users__';

function providerCatalogStatusLabel(status?: string | null) {
  switch (status) {
    case 'ok':
      return 'loaded';
    case 'error':
      return 'failed';
    case 'empty':
      return 'empty';
    default:
      return 'not fetched';
  }
}

export function ModelsPage() {
  const { message } = App.useApp();
  const queryClient = useQueryClient();
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [editingProvider, setEditingProvider] = useState<UserModelProviderRecord | null>(null);
  const [selectedUserId, setSelectedUserId] = useState<string>();
  const [form] = Form.useForm<ProviderFormValues>();

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

  useEffect(() => {
    if (!isSuperAdmin && !selectedUserId && currentUser?.id) {
      setSelectedUserId(currentUser.id);
    }
  }, [currentUser?.id, isSuperAdmin, selectedUserId]);

  const scopedUserId = selectedUserId;
  const scopedQueryKey = scopedUserId || ALL_USERS_SCOPE;
  const canLoadModelData = Boolean(currentUser) && (isSuperAdmin || Boolean(scopedUserId));

  const providersQuery = useQuery({
    queryKey: ['model-providers', scopedQueryKey],
    queryFn: () => api.listModelProviders(scopedUserId),
    enabled: canLoadModelData,
  });

  const modelConfigsQuery = useQuery({
    queryKey: ['model-configs', scopedQueryKey],
    queryFn: () => api.listModelConfigs(scopedUserId),
    enabled: canLoadModelData,
  });

  const modelSettingsQuery = useQuery({
    queryKey: ['model-settings', selectedUserId],
    queryFn: () => api.getModelSettings(selectedUserId || ''),
    enabled: Boolean(selectedUserId),
  });

  const createProviderMutation = useMutation({
    mutationFn: (payload: CreateUserModelProviderPayload) => api.createModelProvider(payload),
    onSuccess: async (result) => {
      showWarnings(result.sync_warnings);
      message.success('Provider saved');
      closeDrawer();
      await invalidateCurrentUserModelQueries();
    },
    onError: showError,
  });

  const updateProviderMutation = useMutation({
    mutationFn: ({ id, payload }: { id: string; payload: UpdateUserModelProviderPayload }) =>
      api.updateModelProvider(id, payload),
    onSuccess: async (result) => {
      showWarnings(result.sync_warnings);
      message.success('Provider updated');
      closeDrawer();
      await invalidateCurrentUserModelQueries();
    },
    onError: showError,
  });

  const refreshProviderMutation = useMutation({
    mutationFn: (provider: UserModelProviderRecord) => api.refreshModelProvider(provider.id, {}),
    onSuccess: async (result) => {
      showWarnings(result.sync_warnings);
      message.success('Provider models refreshed');
      await invalidateCurrentUserModelQueries();
    },
    onError: showError,
  });

  const deleteProviderMutation = useMutation({
    mutationFn: (id: string) => api.deleteModelProvider(id),
    onSuccess: async () => {
      message.success('Provider deleted');
      await invalidateCurrentUserModelQueries();
    },
    onError: showError,
  });

  const saveSettingsMutation = useMutation({
    mutationFn: (modelConfigId: string | null) =>
      api.updateModelSettings({
        user_id: selectedUserId,
        memory_summary_model_config_id: modelConfigId,
      }),
    onSuccess: async (result) => {
      showWarnings(result.sync_warnings);
      message.success('Memory summary model saved');
      await invalidateCurrentUserModelQueries();
    },
    onError: showError,
  });

  const userOptions = useMemo(
    () =>
      (usersQuery.data || []).map((item: UserSummaryRecord) => ({
        label: `${item.display_name || item.username} (${item.username})`,
        value: item.id,
      })),
    [usersQuery.data],
  );

  const currentProviders = providersQuery.data || [];
  const currentConfigs = modelConfigsQuery.data || [];
  const memoryEligibleConfigs = selectedUserId
    ? currentConfigs.filter((item) => item.owner_user_id === selectedUserId && item.model_name.trim())
    : [];

  const providerColumns: ColumnsType<UserModelProviderRecord> = [
    {
      title: 'Provider',
      dataIndex: 'name',
      render: (_, record) => (
        <Space direction="vertical" size={0}>
          <Space size={8} wrap>
            <Typography.Text strong>{record.name}</Typography.Text>
            {record.has_api_key ? <Tag color="blue">Key Saved</Tag> : <Tag>Missing Key</Tag>}
          </Space>
          <Typography.Text type="secondary">
            {record.provider}
            {record.base_url ? ` | ${record.base_url}` : ''}
          </Typography.Text>
        </Space>
      ),
    },
    {
      title: 'Owner',
      dataIndex: 'owner_user_id',
      width: 220,
      render: (ownerUserId: string) => {
        const owner = (usersQuery.data || []).find((item) => item.id === ownerUserId);
        return owner ? `${owner.display_name || owner.username} (${owner.username})` : ownerUserId;
      },
    },
    {
      title: 'Catalog',
      key: 'catalog',
      width: 260,
      render: (_, record) => (
        <Space direction="vertical" size={0}>
          <Space size={8} wrap>
            <Tag color={record.last_sync_status === 'ok' ? 'success' : 'default'}>
              {providerCatalogStatusLabel(record.last_sync_status)}
            </Tag>
            <Tag>{record.imported_model_count || 0} models</Tag>
          </Space>
          {record.last_sync_error ? (
            <Typography.Text type="danger" ellipsis={{ tooltip: record.last_sync_error }}>
              {record.last_sync_error}
            </Typography.Text>
          ) : record.last_synced_at ? (
            <Typography.Text type="secondary">
              {dayjs(record.last_synced_at).format('YYYY-MM-DD HH:mm:ss')}
            </Typography.Text>
          ) : null}
        </Space>
      ),
    },
    {
      title: 'Flags',
      key: 'flags',
      width: 220,
      render: (_, record) => (
        <Space wrap>
          <Tag color={record.enabled ? 'success' : 'default'}>
            {record.enabled ? 'Enabled' : 'Disabled'}
          </Tag>
          {record.supports_images ? <Tag>Image</Tag> : null}
          {record.supports_reasoning ? <Tag>Reasoning</Tag> : null}
          {record.supports_responses ? <Tag>Responses</Tag> : null}
        </Space>
      ),
    },
    {
      title: 'Updated',
      dataIndex: 'updated_at',
      width: 180,
      render: (value: string) => dayjs(value).format('YYYY-MM-DD HH:mm:ss'),
    },
    {
      title: 'Actions',
      key: 'actions',
      width: 240,
      render: (_, record) => (
        <Space>
          <Button size="small" icon={<ReloadOutlined />} onClick={() => refreshProviderMutation.mutate(record)}>
            Refresh
          </Button>
          <Button size="small" icon={<EditOutlined />} onClick={() => openEditDrawer(record)}>
            Edit
          </Button>
          <Popconfirm
            title="Delete this provider and its imported models?"
            onConfirm={() => deleteProviderMutation.mutate(record.id)}
            okButtonProps={{ loading: deleteProviderMutation.isPending }}
          >
            <Button size="small" danger icon={<DeleteOutlined />}>
              Delete
            </Button>
          </Popconfirm>
        </Space>
      ),
    },
  ];

  const modelColumns: ColumnsType<UserModelConfigRecord> = [
    {
      title: 'Concrete Model',
      dataIndex: 'name',
      render: (_, record) => (
        <Space direction="vertical" size={0}>
          <Typography.Text strong>{record.name}</Typography.Text>
          <Typography.Text type="secondary">
            {record.provider} | {record.model_name}
          </Typography.Text>
        </Space>
      ),
    },
    {
      title: 'Owner',
      dataIndex: 'owner_user_id',
      width: 220,
      render: (ownerUserId: string) => {
        const owner = (usersQuery.data || []).find((item) => item.id === ownerUserId);
        return owner ? `${owner.display_name || owner.username} (${owner.username})` : ownerUserId;
      },
    },
    {
      title: 'Task Usage',
      dataIndex: 'task_usage_scenario',
      width: 220,
      render: (value?: string | null) => value || '-',
    },
    {
      title: 'Task Thinking',
      dataIndex: 'task_thinking_level',
      width: 160,
      render: (value?: string | null) => value || '-',
    },
    {
      title: 'Flags',
      key: 'flags',
      width: 220,
      render: (_, record) => (
        <Space wrap>
          <Tag color={record.enabled ? 'success' : 'default'}>
            {record.enabled ? 'Enabled' : 'Disabled'}
          </Tag>
          {record.supports_images ? <Tag>Image</Tag> : null}
          {record.supports_reasoning ? <Tag>Reasoning</Tag> : null}
          {record.supports_responses ? <Tag>Responses</Tag> : null}
        </Space>
      ),
    },
    {
      title: 'Updated',
      dataIndex: 'updated_at',
      width: 180,
      render: (value: string) => dayjs(value).format('YYYY-MM-DD HH:mm:ss'),
    },
  ];

  function showWarnings(warnings?: string[]) {
    if (!warnings || warnings.length === 0) {
      return;
    }
    message.warning(warnings.join(' | '), 6);
  }

  function showError(error: unknown) {
    message.error(error instanceof Error ? error.message : 'Operation failed');
  }

  async function invalidateCurrentUserModelQueries() {
    await Promise.all([
      queryClient.invalidateQueries({ queryKey: ['model-providers'] }),
      queryClient.invalidateQueries({ queryKey: ['model-configs'] }),
      queryClient.invalidateQueries({ queryKey: ['model-settings'] }),
    ]);
  }

  function openCreateDrawer() {
    setEditingProvider(null);
    form.resetFields();
    form.setFieldsValue({
      owner_user_id: selectedUserId,
      provider: 'gpt',
      enabled: true,
      supports_images: false,
      supports_reasoning: false,
      supports_responses: true,
      clear_api_key: false,
    });
    setDrawerOpen(true);
  }

  function openEditDrawer(record: UserModelProviderRecord) {
    setEditingProvider(record);
    form.setFieldsValue({
      owner_user_id: record.owner_user_id,
      name: record.name,
      provider: record.provider,
      api_key: '',
      clear_api_key: false,
      base_url: record.base_url || '',
      enabled: record.enabled,
      supports_images: record.supports_images,
      supports_reasoning: record.supports_reasoning,
      supports_responses: record.supports_responses,
    });
    setDrawerOpen(true);
  }

  function closeDrawer() {
    setDrawerOpen(false);
    setEditingProvider(null);
    form.resetFields();
  }

  function submit(values: ProviderFormValues) {
    if (!selectedUserId && !values.owner_user_id) {
      message.error('Owner user is required');
      return;
    }
    const normalizedBaseUrl = values.base_url?.trim() || undefined;
    const normalizedApiKey = values.api_key?.trim() || undefined;

    if (editingProvider) {
      updateProviderMutation.mutate({
        id: editingProvider.id,
        payload: {
          name: values.name.trim(),
          provider: values.provider,
          api_key: normalizedApiKey,
          clear_api_key: values.clear_api_key === true,
          base_url: normalizedBaseUrl ?? '',
          enabled: values.enabled,
          supports_images: values.supports_images,
          supports_reasoning: values.supports_reasoning,
          supports_responses: values.supports_responses,
        },
      });
      return;
    }

    createProviderMutation.mutate({
      owner_user_id: isSuperAdmin ? values.owner_user_id : selectedUserId,
      name: values.name.trim(),
      provider: values.provider,
      api_key: normalizedApiKey,
      base_url: normalizedBaseUrl,
      enabled: values.enabled,
      supports_images: values.supports_images,
      supports_reasoning: values.supports_reasoning,
      supports_responses: values.supports_responses,
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
            AI Providers & Models
          </Typography.Title>
          <Typography.Text type="secondary">
            Save provider credentials here. User service fetches concrete models from the provider
            catalog for ChatOS, Task Runner, and Memory Engine.
          </Typography.Text>
        </Space>
        <Space wrap>
          {isSuperAdmin ? (
            <Select
              value={selectedUserId || ALL_USERS_SCOPE}
              options={[
                { label: 'All users', value: ALL_USERS_SCOPE },
                ...userOptions,
              ]}
              onChange={(value) => setSelectedUserId(value === ALL_USERS_SCOPE ? undefined : value)}
              style={{ width: 280 }}
              placeholder="Select owner user"
            />
          ) : null}
          <Button
            icon={<ReloadOutlined />}
            onClick={() => {
              void providersQuery.refetch();
              void modelConfigsQuery.refetch();
              if (selectedUserId) {
                void modelSettingsQuery.refetch();
              }
            }}
            loading={providersQuery.isFetching || modelConfigsQuery.isFetching || modelSettingsQuery.isFetching}
          >
            Refresh
          </Button>
          <Button type="primary" icon={<PlusOutlined />} onClick={openCreateDrawer}>
            New Provider
          </Button>
        </Space>
      </div>

      <Card title="Providers">
        <Table<UserModelProviderRecord>
          rowKey="id"
          columns={providerColumns}
          dataSource={currentProviders}
          loading={providersQuery.isLoading}
          pagination={{ pageSize: 10, showSizeChanger: true }}
          expandable={{
            expandedRowRender: (record) =>
              record.sync_warnings && record.sync_warnings.length > 0 ? (
                <Alert
                  type="warning"
                  showIcon
                  message="Refresh warnings"
                  description={record.sync_warnings.join(' | ')}
                />
              ) : null,
            rowExpandable: (record) => Boolean(record.sync_warnings?.length),
          }}
          locale={{
            emptyText: <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description="No provider" />,
          }}
        />
      </Card>

      <Card title="Memory Engine Summary Model">
        <Space direction="vertical" size="middle" style={{ width: '100%' }}>
          {!selectedUserId ? (
            <Alert
              type="info"
              showIcon
              message="Select one user to edit memory settings"
              description="Super admin can view all providers and imported models at once, but memory summary defaults are saved per user."
            />
          ) : (
            <>
              <Typography.Text type="secondary">
                Choose the default concrete model for this user's memory summary jobs.
              </Typography.Text>
              <Space wrap style={{ width: '100%' }}>
                <Select
                  value={modelSettingsQuery.data?.memory_summary_model_config_id ?? undefined}
                  allowClear
                  style={{ minWidth: 320 }}
                  placeholder="Select summary model"
                  options={memoryEligibleConfigs.map((item) => ({
                    label: `${item.name} | ${item.model_name}`,
                    value: item.id,
                  }))}
                  onChange={(value) => saveSettingsMutation.mutate(value ?? null)}
                  loading={modelSettingsQuery.isLoading}
                />
              </Space>
              {memoryEligibleConfigs.length === 0 ? (
                <Alert
                  type="info"
                  showIcon
                  message="No concrete model available"
                  description="Create a provider and refresh its model catalog before choosing a memory summary model."
                />
              ) : null}
            </>
          )}
        </Space>
      </Card>

      <Card title="Imported Concrete Models">
        <Table<UserModelConfigRecord>
          rowKey="id"
          columns={modelColumns}
          dataSource={currentConfigs}
          loading={modelConfigsQuery.isLoading}
          pagination={{ pageSize: 10, showSizeChanger: true }}
          locale={{
            emptyText: <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description="No imported model" />,
          }}
        />
      </Card>

      <Drawer
        title={editingProvider ? `Edit ${editingProvider.name}` : 'New Provider'}
        open={drawerOpen}
        width={520}
        onClose={closeDrawer}
        destroyOnClose
        extra={
          <Space>
            <Button onClick={closeDrawer}>Cancel</Button>
            <Button
              type="primary"
              loading={createProviderMutation.isPending || updateProviderMutation.isPending}
              onClick={() => form.submit()}
            >
              Save
            </Button>
          </Space>
        }
      >
        <Form<ProviderFormValues>
          form={form}
          layout="vertical"
          requiredMark={false}
          initialValues={{
            provider: 'gpt',
            enabled: true,
            supports_images: false,
            supports_reasoning: false,
            supports_responses: true,
            clear_api_key: false,
          }}
          onFinish={submit}
        >
          {isSuperAdmin ? (
            <Form.Item
              name="owner_user_id"
              label="Owner User"
              rules={[{ required: true, message: 'Please choose an owner user' }]}
            >
              <Select options={userOptions} />
            </Form.Item>
          ) : null}
          <Form.Item
            name="name"
            label="Name"
            rules={[{ required: true, message: 'Please enter a name' }]}
          >
            <Input />
          </Form.Item>
          <Form.Item name="provider" label="Provider" rules={[{ required: true }]}>
            <Select options={PROVIDER_OPTIONS} />
          </Form.Item>
          <Form.Item name="base_url" label="Base URL">
            <Input placeholder="https://api.openai.com/v1" />
          </Form.Item>
          <Form.Item
            name="api_key"
            label={editingProvider ? 'New API Key' : 'API Key'}
            rules={editingProvider ? undefined : [{ required: true, message: 'Please enter an API key' }]}
          >
            <Input.Password placeholder={editingProvider ? 'Leave empty to keep existing key' : ''} />
          </Form.Item>
          {editingProvider?.has_api_key ? (
            <Form.Item name="clear_api_key" valuePropName="checked">
              <Switch checkedChildren="Clear Saved Key" unCheckedChildren="Keep Saved Key" />
            </Form.Item>
          ) : null}
          <Form.Item name="enabled" label="Enabled" valuePropName="checked">
            <Switch />
          </Form.Item>
          <Form.Item name="supports_images" label="Supports Images" valuePropName="checked">
            <Switch />
          </Form.Item>
          <Form.Item name="supports_reasoning" label="Supports Reasoning" valuePropName="checked">
            <Switch />
          </Form.Item>
          <Form.Item name="supports_responses" label="Supports Responses API" valuePropName="checked">
            <Switch />
          </Form.Item>
        </Form>
      </Drawer>
    </Space>
  );
}
