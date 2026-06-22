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
  CreateUserModelConfigPayload,
  UpdateUserModelConfigPayload,
  UserModelConfigRecord,
  UserSummaryRecord,
} from '../types';

type ModelFormValues = {
  owner_user_id?: string;
  name: string;
  provider: string;
  model?: string;
  thinking_level?: string;
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

const THINKING_OPTIONS = [
  { label: 'Default', value: '' },
  { label: 'none', value: 'none' },
  { label: 'auto', value: 'auto' },
  { label: 'minimal', value: 'minimal' },
  { label: 'low', value: 'low' },
  { label: 'medium', value: 'medium' },
  { label: 'high', value: 'high' },
  { label: 'xhigh', value: 'xhigh' },
];

export function ModelsPage() {
  const { message } = App.useApp();
  const queryClient = useQueryClient();
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [editingConfig, setEditingConfig] = useState<UserModelConfigRecord | null>(null);
  const [selectedUserId, setSelectedUserId] = useState<string>();
  const [form] = Form.useForm<ModelFormValues>();

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
    if (!selectedUserId && currentUser?.id) {
      setSelectedUserId(currentUser.id);
    }
  }, [currentUser?.id, selectedUserId]);

  const modelConfigsQuery = useQuery({
    queryKey: ['model-configs', selectedUserId],
    queryFn: () => api.listModelConfigs(selectedUserId),
    enabled: Boolean(selectedUserId),
  });

  const modelSettingsQuery = useQuery({
    queryKey: ['model-settings', selectedUserId],
    queryFn: () => api.getModelSettings(selectedUserId || ''),
    enabled: Boolean(selectedUserId),
  });

  const createMutation = useMutation({
    mutationFn: (payload: CreateUserModelConfigPayload) => api.createModelConfig(payload),
    onSuccess: async (result) => {
      showWarnings(result.sync_warnings);
      message.success('Model config created');
      closeDrawer();
      await invalidateCurrentUserModelQueries();
    },
    onError: showError,
  });

  const updateMutation = useMutation({
    mutationFn: ({ id, payload }: { id: string; payload: UpdateUserModelConfigPayload }) =>
      api.updateModelConfig(id, payload),
    onSuccess: async (result) => {
      showWarnings(result.sync_warnings);
      message.success('Model config updated');
      closeDrawer();
      await invalidateCurrentUserModelQueries();
    },
    onError: showError,
  });

  const deleteMutation = useMutation({
    mutationFn: (id: string) => api.deleteModelConfig(id),
    onSuccess: async () => {
      message.success('Model config deleted');
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

  const currentConfigs = modelConfigsQuery.data || [];
  const memoryEligibleConfigs = currentConfigs.filter((item) => item.model_name.trim());

  const columns: ColumnsType<UserModelConfigRecord> = [
    {
      title: 'Config',
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
      title: 'Model',
      dataIndex: 'model_name',
      width: 220,
      render: (value: string, record) => (
        <Space direction="vertical" size={0}>
          <Typography.Text>{value || '-'}</Typography.Text>
          {record.thinking_level ? (
            <Typography.Text type="secondary">{record.thinking_level}</Typography.Text>
          ) : null}
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
      width: 160,
      render: (_, record) => (
        <Space>
          <Button size="small" icon={<EditOutlined />} onClick={() => openEditDrawer(record)}>
            Edit
          </Button>
          <Popconfirm
            title="Delete this model config?"
            onConfirm={() => deleteMutation.mutate(record.id)}
            okButtonProps={{ loading: deleteMutation.isPending }}
          >
            <Button size="small" danger icon={<DeleteOutlined />}>
              Delete
            </Button>
          </Popconfirm>
        </Space>
      ),
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
      queryClient.invalidateQueries({ queryKey: ['model-configs', selectedUserId] }),
      queryClient.invalidateQueries({ queryKey: ['model-settings', selectedUserId] }),
    ]);
  }

  function openCreateDrawer() {
    setEditingConfig(null);
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

  function openEditDrawer(record: UserModelConfigRecord) {
    setEditingConfig(record);
    form.setFieldsValue({
      owner_user_id: record.owner_user_id,
      name: record.name,
      provider: record.provider,
      model: record.model_name,
      thinking_level: record.thinking_level || '',
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
    setEditingConfig(null);
    form.resetFields();
  }

  function submit(values: ModelFormValues) {
    if (!selectedUserId && !values.owner_user_id) {
      message.error('Owner user is required');
      return;
    }
    const normalizedModel = values.model?.trim() || undefined;
    const normalizedThinkingLevel = values.thinking_level?.trim() || undefined;
    const normalizedBaseUrl = values.base_url?.trim() || undefined;
    const normalizedApiKey = values.api_key?.trim() || undefined;

    if (editingConfig) {
      updateMutation.mutate({
        id: editingConfig.id,
        payload: {
          name: values.name.trim(),
          provider: values.provider,
          model: normalizedModel,
          thinking_level: normalizedThinkingLevel,
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

    createMutation.mutate({
      owner_user_id: isSuperAdmin ? values.owner_user_id : selectedUserId,
      name: values.name.trim(),
      provider: values.provider,
      model: normalizedModel,
      thinking_level: normalizedThinkingLevel,
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
            Model Configs
          </Typography.Title>
          <Typography.Text type="secondary">
            User-scoped provider credentials live here. ChatOS can keep model blank, while
            task and memory use configs with a concrete model name.
          </Typography.Text>
        </Space>
        <Space wrap>
          {isSuperAdmin ? (
            <Select
              value={selectedUserId}
              options={userOptions}
              onChange={setSelectedUserId}
              style={{ width: 280 }}
              placeholder="Select owner user"
            />
          ) : null}
          <Button
            icon={<ReloadOutlined />}
            onClick={() => {
              void modelConfigsQuery.refetch();
              void modelSettingsQuery.refetch();
            }}
            loading={modelConfigsQuery.isFetching || modelSettingsQuery.isFetching}
          >
            Refresh
          </Button>
          <Button type="primary" icon={<PlusOutlined />} onClick={openCreateDrawer}>
            New Config
          </Button>
        </Space>
      </div>

      <Card title="Memory Engine Summary Model">
        <Space direction="vertical" size="middle" style={{ width: '100%' }}>
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
              description="Configs without a model name still work for ChatOS provider setup, but they cannot be used as the memory summary model."
            />
          ) : null}
        </Space>
      </Card>

      <Table<UserModelConfigRecord>
        rowKey="id"
        columns={columns}
        dataSource={currentConfigs}
        loading={modelConfigsQuery.isLoading}
        pagination={{ pageSize: 10, showSizeChanger: true }}
        expandable={{
          expandedRowRender: (record) =>
            record.sync_warnings && record.sync_warnings.length > 0 ? (
              <Alert
                type="warning"
                showIcon
                message="Sync warnings"
                description={record.sync_warnings.join(' | ')}
              />
            ) : null,
          rowExpandable: (record) => Boolean(record.sync_warnings?.length),
        }}
        locale={{
          emptyText: <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description="No model config" />,
        }}
      />

      <Drawer
        title={editingConfig ? `Edit ${editingConfig.name}` : 'New Model Config'}
        open={drawerOpen}
        width={520}
        onClose={closeDrawer}
        destroyOnClose
        extra={
          <Space>
            <Button onClick={closeDrawer}>Cancel</Button>
            <Button
              type="primary"
              loading={createMutation.isPending || updateMutation.isPending}
              onClick={() => form.submit()}
            >
              Save
            </Button>
          </Space>
        }
      >
        <Form<ModelFormValues>
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
            label={editingConfig ? 'New API Key' : 'API Key'}
            rules={editingConfig ? undefined : [{ required: true, message: 'Please enter an API key' }]}
          >
            <Input.Password placeholder={editingConfig ? 'Leave empty to keep existing key' : ''} />
          </Form.Item>
          {editingConfig?.has_api_key ? (
            <Form.Item name="clear_api_key" valuePropName="checked">
              <Switch checkedChildren="Clear Saved Key" unCheckedChildren="Keep Saved Key" />
            </Form.Item>
          ) : null}
          <Form.Item
            name="model"
            label="Concrete Model"
            extra="Optional for ChatOS provider setup. Required if this config should sync into task or memory as a runnable model."
          >
            <Input placeholder="gpt-4.1 / deepseek-chat / kimi-k2..." />
          </Form.Item>
          <Form.Item name="thinking_level" label="Thinking Level">
            <Select options={THINKING_OPTIONS} />
          </Form.Item>
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
