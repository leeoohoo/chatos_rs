// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useEffect, useMemo, useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import {
  Alert,
  App,
  Button,
  Card,
  Col,
  Empty,
  Form,
  Input,
  InputNumber,
  Row,
  Select,
  Space,
  Switch,
  Table,
  Tag,
  Typography,
} from 'antd';
import { PlusOutlined, ReloadOutlined, RobotOutlined, SaveOutlined } from '@ant-design/icons';

import { api } from '../api/client';
import type {
  UserModelConfigRecord,
  UserModelProviderRecord,
  UserSummaryRecord,
} from '../types';
import { ModelProviderDrawer } from './models/ModelProviderDrawer';
import {
  ALL_USERS_SCOPE,
  buildCreateProviderPayload,
  buildProviderColumns,
  buildUpdateProviderPayload,
  defaultPromptVendor,
  ModelCapabilityTags,
  type ProviderFormValues,
  userLabel,
} from './models/modelPageUtils';

type ModelTaskPreferencesDraft = {
  task_enabled: boolean;
  task_usage_scenario: string;
  task_thinking_level?: string;
  temperature?: number;
  max_output_tokens?: number;
};

const TASK_THINKING_LEVEL_OPTIONS = [
  { label: '默认', value: '' },
  { label: 'none', value: 'none' },
  { label: 'auto', value: 'auto' },
  { label: 'minimal', value: 'minimal' },
  { label: 'low', value: 'low' },
  { label: 'medium', value: 'medium' },
  { label: 'high', value: 'high' },
  { label: 'xhigh', value: 'xhigh' },
  { label: 'max', value: 'max' },
];

export function ModelsPage() {
  const { message } = App.useApp();
  const queryClient = useQueryClient();
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [editingProvider, setEditingProvider] = useState<UserModelProviderRecord | null>(null);
  const [selectedUserId, setSelectedUserId] = useState<string>();
  const [modelTaskDrafts, setModelTaskDrafts] = useState<
    Record<string, ModelTaskPreferencesDraft>
  >({});
  const [form] = Form.useForm<ProviderFormValues>();

  const currentUserQuery = useQuery({
    queryKey: ['user-service', 'current-user'],
    queryFn: () => api.currentUser(),
  });
  const usersQuery = useQuery({
    queryKey: ['user-service', 'users'],
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
    queryKey: ['user-service', 'model-providers', scopedQueryKey],
    queryFn: () => api.listModelProviders(scopedUserId),
    enabled: canLoadModelData,
  });

  const modelConfigsQuery = useQuery({
    queryKey: ['user-service', 'model-configs', scopedQueryKey],
    queryFn: () => api.listModelConfigs(scopedUserId),
    enabled: canLoadModelData,
  });

  useEffect(() => {
    const nextDrafts = Object.fromEntries(
      (modelConfigsQuery.data || []).map((model) => [model.id, taskPreferencesDraft(model)]),
    );
    setModelTaskDrafts(nextDrafts);
  }, [modelConfigsQuery.data]);

  const modelSettingsQuery = useQuery({
    queryKey: ['user-service', 'model-settings', selectedUserId],
    queryFn: () => api.getModelSettings(selectedUserId || ''),
    enabled: Boolean(selectedUserId),
  });

  const createProviderMutation = useMutation({
    mutationFn: api.createModelProvider,
    onSuccess: async (result) => {
      showWarnings(result.sync_warnings);
      message.success('Provider saved');
      closeDrawer();
      await invalidateCurrentUserModelQueries();
    },
    onError: showError,
  });

  const updateProviderMutation = useMutation({
    mutationFn: ({ id, payload }: { id: string; payload: ReturnType<typeof buildUpdateProviderPayload> }) =>
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

  const updateModelTaskPreferencesMutation = useMutation({
    mutationFn: ({ id, draft }: { id: string; draft: ModelTaskPreferencesDraft }) =>
      api.updateModelConfig(id, {
        task_enabled: draft.task_enabled,
        task_usage_scenario: draft.task_usage_scenario.trim(),
        task_thinking_level: draft.task_thinking_level || '',
        temperature: draft.temperature,
        clear_temperature: draft.temperature == null,
        max_output_tokens: draft.max_output_tokens,
        clear_max_output_tokens: draft.max_output_tokens == null,
      }),
    onSuccess: async (result) => {
      showWarnings(result.sync_warnings);
      queryClient.setQueriesData<UserModelConfigRecord[]>(
        { queryKey: ['user-service', 'model-configs'] },
        (items) =>
          items?.map((item) => (item.id === result.id ? { ...item, ...result } : item)),
      );
      setModelTaskDrafts((current) => ({
        ...current,
        [result.id]: taskPreferencesDraft(result),
      }));
      message.success('任务模型偏好已保存');
      await queryClient.invalidateQueries({ queryKey: ['user-service', 'model-configs'] });
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
  const providerColumns = useMemo(
    () =>
      buildProviderColumns({
        users: usersQuery.data,
        onRefresh: (record) => refreshProviderMutation.mutate(record),
        onEdit: openEditDrawer,
        onDelete: (id) => deleteProviderMutation.mutate(id),
        deleteLoading: deleteProviderMutation.isPending,
      }),
    [deleteProviderMutation, refreshProviderMutation, usersQuery.data],
  );
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
      queryClient.invalidateQueries({ queryKey: ['user-service', 'model-providers'] }),
      queryClient.invalidateQueries({ queryKey: ['user-service', 'model-configs'] }),
      queryClient.invalidateQueries({ queryKey: ['user-service', 'model-settings'] }),
    ]);
  }

  function openCreateDrawer() {
    setEditingProvider(null);
    form.resetFields();
    form.setFieldsValue({
      owner_user_id: selectedUserId,
      provider: 'gpt',
      prompt_vendor: 'gpt',
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
      prompt_vendor: record.prompt_vendor || defaultPromptVendor(record.provider),
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

    if (editingProvider) {
      updateProviderMutation.mutate({
        id: editingProvider.id,
        payload: buildUpdateProviderPayload(values),
      });
      return;
    }

    createProviderMutation.mutate(
      buildCreateProviderPayload({
        values,
        isSuperAdmin,
        selectedUserId,
      }),
    );
  }

  function updateModelTaskDraft(
    modelId: string,
    patch: Partial<ModelTaskPreferencesDraft>,
  ) {
    setModelTaskDrafts((current) => {
      const model = currentConfigs.find((item) => item.id === modelId);
      if (!model) {
        return current;
      }
      return {
        ...current,
        [modelId]: {
          ...(current[modelId] || taskPreferencesDraft(model)),
          ...patch,
        },
      };
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
            catalog for Chat OS, Task Runner, and Memory Engine.
          </Typography.Text>
        </Space>
        <Space wrap>
          {isSuperAdmin ? (
            <Select
              value={selectedUserId || ALL_USERS_SCOPE}
              options={[{ label: 'All users', value: ALL_USERS_SCOPE }, ...userOptions]}
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
            loading={
              providersQuery.isFetching ||
              modelConfigsQuery.isFetching ||
              modelSettingsQuery.isFetching
            }
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

      <Card
        title="任务模型偏好"
        extra={
          <Typography.Text type="secondary">
            用途可不填；ChatOS AI 会结合模型能力和自己的经验选择。
          </Typography.Text>
        }
      >
        {modelConfigsQuery.isLoading ? (
          <Typography.Text type="secondary">正在加载模型…</Typography.Text>
        ) : currentConfigs.length === 0 ? (
          <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description="暂无已导入模型" />
        ) : (
          <Space direction="vertical" size="middle" style={{ width: '100%' }}>
            {currentConfigs.map((model) => {
              const draft = modelTaskDrafts[model.id] || taskPreferencesDraft(model);
              const saving =
                updateModelTaskPreferencesMutation.isPending &&
                updateModelTaskPreferencesMutation.variables?.id === model.id;
              return (
                <Card
                  key={model.id}
                  size="small"
                  title={
                    <Space size="middle">
                      <RobotOutlined style={{ color: '#1677ff', fontSize: 20 }} />
                      <Space direction="vertical" size={0}>
                        <Typography.Text strong>{model.name}</Typography.Text>
                        <Typography.Text type="secondary">
                          {model.provider} · {model.model_name}
                        </Typography.Text>
                      </Space>
                    </Space>
                  }
                  extra={
                    <Space size="middle" wrap>
                      {isSuperAdmin && !selectedUserId ? (
                        <Tag>{userLabel(usersQuery.data, model.owner_user_id)}</Tag>
                      ) : null}
                      {!model.enabled ? <Tag color="warning">模型已停用</Tag> : null}
                      <ModelCapabilityTags record={model} showEnabled={false} />
                      <Typography.Text>用于任务</Typography.Text>
                      <Switch
                        checked={draft.task_enabled}
                        onChange={(taskEnabled) =>
                          updateModelTaskDraft(model.id, { task_enabled: taskEnabled })
                        }
                      />
                      <Button
                        type="primary"
                        size="small"
                        icon={<SaveOutlined />}
                        loading={saving}
                        onClick={() =>
                          updateModelTaskPreferencesMutation.mutate({ id: model.id, draft })
                        }
                      >
                        保存
                      </Button>
                    </Space>
                  }
                >
                  <Row gutter={[16, 12]} align="bottom">
                    <Col xs={24} lg={8}>
                      <Typography.Text type="secondary">任务用途</Typography.Text>
                      <Input
                        value={draft.task_usage_scenario}
                        placeholder="例如：代码实现、分析、视觉理解"
                        maxLength={500}
                        onChange={(event) =>
                          updateModelTaskDraft(model.id, {
                            task_usage_scenario: event.target.value,
                          })
                        }
                      />
                    </Col>
                    <Col xs={24} sm={8} lg={5}>
                      <Typography.Text type="secondary">默认 Thinking</Typography.Text>
                      <Select
                        value={draft.task_thinking_level || ''}
                        options={TASK_THINKING_LEVEL_OPTIONS}
                        style={{ width: '100%' }}
                        onChange={(taskThinkingLevel) =>
                          updateModelTaskDraft(model.id, {
                            task_thinking_level: taskThinkingLevel || undefined,
                          })
                        }
                      />
                    </Col>
                    <Col xs={24} sm={8} lg={4}>
                      <Typography.Text type="secondary">Temperature</Typography.Text>
                      <InputNumber
                        value={draft.temperature}
                        min={0}
                        max={2}
                        step={0.1}
                        precision={2}
                        placeholder="默认"
                        style={{ width: '100%' }}
                        onChange={(temperature) =>
                          updateModelTaskDraft(model.id, {
                            temperature: temperature ?? undefined,
                          })
                        }
                      />
                    </Col>
                    <Col xs={24} sm={8} lg={4}>
                      <Typography.Text type="secondary">Max Tokens</Typography.Text>
                      <InputNumber
                        value={draft.max_output_tokens}
                        min={1}
                        precision={0}
                        placeholder="默认"
                        style={{ width: '100%' }}
                        onChange={(maxOutputTokens) =>
                          updateModelTaskDraft(model.id, {
                            max_output_tokens: maxOutputTokens ?? undefined,
                          })
                        }
                      />
                    </Col>
                  </Row>
                </Card>
              );
            })}
          </Space>
        )}
      </Card>

      <ModelProviderDrawer
        open={drawerOpen}
        editingProvider={editingProvider}
        isSuperAdmin={isSuperAdmin}
        userOptions={userOptions}
        form={form}
        saveLoading={createProviderMutation.isPending || updateProviderMutation.isPending}
        onClose={closeDrawer}
        onSubmit={submit}
      />
    </Space>
  );
}

function taskPreferencesDraft(model: UserModelConfigRecord): ModelTaskPreferencesDraft {
  return {
    task_enabled: model.task_enabled,
    task_usage_scenario: model.task_usage_scenario || '',
    task_thinking_level: model.task_thinking_level || undefined,
    temperature: model.temperature ?? undefined,
    max_output_tokens: model.max_output_tokens ?? undefined,
  };
}
