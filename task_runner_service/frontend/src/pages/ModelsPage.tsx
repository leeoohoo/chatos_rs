import { useMemo, useRef, useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useNavigate, useSearchParams } from 'react-router-dom';
import {
  Button,
  Descriptions,
  Drawer,
  Empty,
  Form,
  Input,
  InputNumber,
  List,
  Modal,
  Select,
  Segmented,
  Space,
  Statistic,
  Switch,
  Table,
  Tag,
  Typography,
  message,
} from 'antd';
import type { ColumnsType } from 'antd/es/table';
import dayjs from 'dayjs';

import { api } from '../api/client';
import type {
  CreateModelConfigPayload,
  ModelCatalogResponse,
  ModelConfigRecord,
  ModelConfigTestResponse,
  ProviderModelRecord,
} from '../types';

type ModelFormValues = {
  name: string;
  provider: string;
  base_url: string;
  api_key: string;
  model: string;
  temperature?: number;
  max_output_tokens?: number;
  thinking_level?: string;
  supports_responses: boolean;
  instructions?: string;
  request_cwd?: string;
  include_prompt_cache_retention: boolean;
  request_body_limit_bytes?: number;
  enabled: boolean;
};

type SupportedProvider = 'openai' | 'deepseek' | 'kimik2';

const SUPPORTED_PROVIDER_OPTIONS: Array<{ label: SupportedProvider; value: SupportedProvider }> = [
  { label: 'openai', value: 'openai' },
  { label: 'deepseek', value: 'deepseek' },
  { label: 'kimik2', value: 'kimik2' },
];

const THINKING_LEVEL_OPTIONS: Record<SupportedProvider, Array<{ label: string; value: string }>> = {
  openai: [
    { label: 'none', value: 'none' },
    { label: 'minimal', value: 'minimal' },
    { label: 'low', value: 'low' },
    { label: 'medium', value: 'medium' },
    { label: 'high', value: 'high' },
    { label: 'xhigh', value: 'xhigh' },
  ],
  deepseek: [
    { label: 'none', value: 'none' },
    { label: 'low', value: 'low' },
    { label: 'medium', value: 'medium' },
    { label: 'high', value: 'high' },
    { label: 'max', value: 'max' },
  ],
  kimik2: [
    { label: 'none', value: 'none' },
    { label: 'auto', value: 'auto' },
    { label: 'low', value: 'low' },
    { label: 'medium', value: 'medium' },
    { label: 'high', value: 'high' },
    { label: 'xhigh', value: 'xhigh' },
  ],
};

function defaultBaseUrlForProvider(provider?: string): string {
  switch (provider) {
    case 'deepseek':
      return 'https://api.deepseek.com';
    case 'kimik2':
      return 'https://api.moonshot.ai/v1';
    case 'openai':
    default:
      return 'https://api.openai.com/v1';
  }
}

function normalizeSupportedProvider(provider?: string): SupportedProvider {
  const value = (provider || '').trim().toLowerCase();
  if (value === 'deepseek') {
    return 'deepseek';
  }
  if (value === 'kimi' || value === 'kimik2' || value === 'kiminik2' || value === 'moonshot') {
    return 'kimik2';
  }
  return 'openai';
}

export function ModelsPage() {
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const [searchParams, setSearchParams] = useSearchParams();
  const [messageApi, contextHolder] = message.useMessage();
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [editingModel, setEditingModel] = useState<ModelConfigRecord | null>(null);
  const [testResult, setTestResult] = useState<ModelConfigTestResponse | null>(null);
  const [keywordFilter, setKeywordFilter] = useState('');
  const [providerFilter, setProviderFilter] = useState<'all' | string>('all');
  const [enabledFilter, setEnabledFilter] = useState<'all' | 'enabled' | 'disabled'>('all');
  const [modelCatalog, setModelCatalog] = useState<ModelCatalogResponse | null>(null);
  const [baseUrlDirty, setBaseUrlDirty] = useState(false);
  const [form] = Form.useForm<ModelFormValues>();
  const watchedProvider = Form.useWatch('provider', form);
  const watchedModel = Form.useWatch('model', form);
  const watchedApiKey = Form.useWatch('api_key', form);
  const providerRef = useRef<SupportedProvider>('openai');
  const autoUpdatingBaseUrlRef = useRef(false);
  const routeModelId = searchParams.get('model_id') || undefined;
  const normalizedProvider = normalizeSupportedProvider(watchedProvider);

  const modelsQuery = useQuery({
    queryKey: ['model-configs'],
    queryFn: api.listModelConfigs,
  });
  const usageQuery = useQuery({
    queryKey: ['model-config-usage'],
    queryFn: api.listModelConfigUsage,
  });
  const selectedModelQuery = useQuery({
    queryKey: ['model-config', routeModelId],
    queryFn: () => api.getModelConfig(routeModelId!),
    enabled: Boolean(routeModelId),
  });
  const modelTasksQuery = useQuery({
    queryKey: ['model-tasks', routeModelId],
    queryFn: () => api.listTasks({ model_config_id: routeModelId!, limit: 20 }),
    enabled: Boolean(routeModelId),
  });
  const modelRunsQuery = useQuery({
    queryKey: ['model-runs', routeModelId],
    queryFn: () => api.listRuns({ model_config_id: routeModelId!, limit: 10 }),
    enabled: Boolean(routeModelId),
  });

  const createModelMutation = useMutation({
    mutationFn: api.createModelConfig,
    onSuccess: async () => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ['model-configs'] }),
        queryClient.invalidateQueries({ queryKey: ['model-config-usage'] }),
      ]);
      messageApi.success('模型配置已创建');
      resetModelDrawerState();
    },
    onError: (error: Error) => messageApi.error(error.message),
  });

  const updateModelMutation = useMutation({
    mutationFn: ({ id, payload }: { id: string; payload: Partial<CreateModelConfigPayload> }) =>
      api.updateModelConfig(id, payload),
    onSuccess: async () => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ['model-configs'] }),
        queryClient.invalidateQueries({ queryKey: ['model-config-usage'] }),
      ]);
      messageApi.success('模型配置已更新');
      resetModelDrawerState();
    },
    onError: (error: Error) => messageApi.error(error.message),
  });

  const deleteModelMutation = useMutation({
    mutationFn: api.deleteModelConfig,
    onSuccess: async () => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ['model-configs'] }),
        queryClient.invalidateQueries({ queryKey: ['model-config-usage'] }),
        queryClient.invalidateQueries({ queryKey: ['tasks'] }),
        queryClient.invalidateQueries({ queryKey: ['task-index'] }),
      ]);
      messageApi.success('模型配置已删除');
    },
    onError: (error: Error) => messageApi.error(error.message),
  });

  const testModelMutation = useMutation({
    mutationFn: (id: string) => api.testModelConfig(id, {}),
    onSuccess: (result) => {
      setTestResult(result);
      if (result.ok) {
        messageApi.success('模型连通性测试成功');
      } else {
        messageApi.warning('模型连通性测试失败');
      }
    },
    onError: (error: Error) => messageApi.error(error.message),
  });
  const previewModelCatalogMutation = useMutation({
    mutationFn: api.previewModelCatalog,
    onSuccess: (catalog) => {
      setModelCatalog(catalog);
      syncNormalizedBaseUrl(catalog.base_url);
      const currentModel = form.getFieldValue('model');
      const matched = catalog.models.find((item) => item.id === currentModel);
      if (matched) {
        form.setFieldValue('supports_responses', matched.supports_responses);
      }
      if (catalog.source === 'live') {
        messageApi.success(`模型列表已更新，共 ${catalog.models.length} 个`);
      } else if (catalog.error) {
        messageApi.warning(`模型列表拉取失败，已回退到当前配置: ${catalog.error}`);
      } else {
        messageApi.warning('未能获取在线模型列表，已回退到当前配置');
      }
    },
    onError: (error: Error) => messageApi.error(error.message),
  });

  function applyAutoBaseUrl(nextBaseUrl: string) {
    autoUpdatingBaseUrlRef.current = true;
    form.setFieldValue('base_url', nextBaseUrl);
    setBaseUrlDirty(false);
    queueMicrotask(() => {
      autoUpdatingBaseUrlRef.current = false;
    });
  }

  function syncNormalizedBaseUrl(nextBaseUrl: string) {
    autoUpdatingBaseUrlRef.current = true;
    form.setFieldValue('base_url', nextBaseUrl);
    queueMicrotask(() => {
      autoUpdatingBaseUrlRef.current = false;
    });
  }

  function resetModelDrawerState() {
    setDrawerOpen(false);
    setEditingModel(null);
    setModelCatalog(null);
    setBaseUrlDirty(false);
    form.resetFields();
  }

  function fetchModelCatalog() {
    const values = form.getFieldsValue([
      'provider',
      'base_url',
      'api_key',
      'model',
      'supports_responses',
    ]);
    previewModelCatalogMutation.mutate({
      provider: values.provider,
      base_url: values.base_url,
      api_key: values.api_key,
      model: values.model,
      supports_responses: values.supports_responses,
    });
  }

  const taskCountByModelId = useMemo(() => {
    const map = new Map<string, number>();
    (usageQuery.data || []).forEach((usage) => {
      map.set(usage.model_config_id, usage.task_count);
    });
    return map;
  }, [usageQuery.data]);
  const runCountByModelId = useMemo(() => {
    const map = new Map<string, number>();
    (usageQuery.data || []).forEach((usage) => {
      map.set(usage.model_config_id, usage.run_count);
    });
    return map;
  }, [usageQuery.data]);
  const selectedModel = useMemo(() => {
    if (!routeModelId) {
      return null;
    }
    return (
      selectedModelQuery.data ||
      (modelsQuery.data || []).find((model) => model.id === routeModelId) ||
      null
    );
  }, [modelsQuery.data, routeModelId, selectedModelQuery.data]);
  const modelOptions = useMemo(() => {
    const options = new Map<string, ProviderModelRecord>();
    (modelCatalog?.models || []).forEach((item) => {
      options.set(item.id, item);
    });
    const currentModel = watchedModel;
    if (currentModel && !options.has(currentModel)) {
      options.set(currentModel, {
        id: currentModel,
        owned_by: editingModel?.provider || null,
        context_length: null,
        supports_images: false,
        supports_video: false,
        supports_reasoning: false,
        supports_responses: form.getFieldValue('supports_responses') ?? false,
        raw: undefined,
      });
    }
    return Array.from(options.values()).map((item) => ({
      label: item.context_length
        ? `${item.id} (${item.context_length.toLocaleString()})`
        : item.id,
      value: item.id,
    }));
  }, [editingModel?.provider, form, modelCatalog?.models, watchedModel]);
  const thinkingLevelOptions = useMemo(
    () => THINKING_LEVEL_OPTIONS[normalizedProvider],
    [normalizedProvider],
  );
  const providerOptions = useMemo(
    () =>
      ['all', ...Array.from(new Set((modelsQuery.data || []).map((model) => model.provider))).sort()]
        .map((provider) => ({
          label: provider === 'all' ? '全部 Provider' : provider,
          value: provider,
        })),
    [modelsQuery.data],
  );
  const filteredModels = useMemo(() => {
    const keyword = keywordFilter.trim().toLowerCase();
    return (modelsQuery.data || []).filter((model) => {
      if (providerFilter !== 'all' && model.provider !== providerFilter) {
        return false;
      }
      if (enabledFilter === 'enabled' && !model.enabled) {
        return false;
      }
      if (enabledFilter === 'disabled' && model.enabled) {
        return false;
      }
      if (!keyword) {
        return true;
      }
      return [model.name, model.model, model.provider, model.base_url]
        .filter(Boolean)
        .some((value) => value.toLowerCase().includes(keyword));
    });
  }, [enabledFilter, keywordFilter, modelsQuery.data, providerFilter]);
  const filteredTaskCount = useMemo(
    () =>
      filteredModels.reduce((total, model) => total + (taskCountByModelId.get(model.id) || 0), 0),
    [filteredModels, taskCountByModelId],
  );
  const filteredRunCount = useMemo(
    () =>
      filteredModels.reduce((total, model) => total + (runCountByModelId.get(model.id) || 0), 0),
    [filteredModels, runCountByModelId],
  );

  const columns: ColumnsType<ModelConfigRecord> = [
    {
      title: '配置名称',
      dataIndex: 'name',
      render: (_, record) => (
        <Space direction="vertical" size={0}>
          <Button type="link" style={{ padding: 0 }} onClick={() => openDetailDrawer(record.id)}>
            <Typography.Text strong>{record.name}</Typography.Text>
          </Button>
          <Typography.Text type="secondary">{record.model}</Typography.Text>
        </Space>
      ),
    },
    {
      title: 'Provider',
      dataIndex: 'provider',
      width: 140,
    },
    {
      title: 'Base URL',
      dataIndex: 'base_url',
      width: 280,
      ellipsis: true,
    },
    {
      title: 'Responses',
      dataIndex: 'supports_responses',
      width: 120,
      render: (value: boolean) => (value ? 'yes' : 'no'),
    },
    {
      title: '绑定任务',
      key: 'task_count',
      width: 120,
      render: (_, record) => taskCountByModelId.get(record.id) || 0,
    },
    {
      title: '运行次数',
      key: 'run_count',
      width: 120,
      render: (_, record) => runCountByModelId.get(record.id) || 0,
    },
    {
      title: '状态',
      dataIndex: 'enabled',
      width: 120,
      render: (value: boolean) => <Tag color={value ? 'success' : 'default'}>{value ? 'enabled' : 'disabled'}</Tag>,
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
      width: 420,
      render: (_, record) => (
        <Space>
          <Button size="small" onClick={() => openDetailDrawer(record.id)}>
            详情
          </Button>
          <Button
            size="small"
            onClick={() => navigate(`/tasks?model_config_id=${encodeURIComponent(record.id)}`)}
          >
            任务
          </Button>
          <Button
            size="small"
            onClick={() => navigate(`/runs?model_config_id=${encodeURIComponent(record.id)}`)}
          >
            运行
          </Button>
          <Button size="small" onClick={() => openEditDrawer(record)}>
            编辑
          </Button>
          <Button
            size="small"
            onClick={() => testModelMutation.mutate(record.id)}
            loading={testModelMutation.isPending}
          >
            测试
          </Button>
          <Button size="small" danger onClick={() => confirmDelete(record)}>
            删除
          </Button>
        </Space>
      ),
    },
  ];

  function openCreateDrawer() {
    const provider: SupportedProvider = 'openai';
    setEditingModel(null);
    setModelCatalog(null);
    setBaseUrlDirty(false);
    providerRef.current = provider;
    form.setFieldsValue({
      name: '',
      provider,
      base_url: defaultBaseUrlForProvider(provider),
      api_key: '',
      model: '',
      supports_responses: true,
      include_prompt_cache_retention: false,
      enabled: true,
    });
    setDrawerOpen(true);
  }

  function openDetailDrawer(modelId: string) {
    const next = new URLSearchParams(searchParams);
    next.set('model_id', modelId);
    setSearchParams(next);
  }

  function closeDetailDrawer() {
    const next = new URLSearchParams(searchParams);
    next.delete('model_id');
    setSearchParams(next);
  }

  function openEditDrawer(model: ModelConfigRecord) {
    const provider = normalizeSupportedProvider(model.provider);
    setEditingModel(model);
    setModelCatalog(null);
    setBaseUrlDirty(false);
    providerRef.current = provider;
    form.setFieldsValue({
      name: model.name,
      provider,
      base_url: model.base_url,
      api_key: model.api_key,
      model: model.model,
      temperature: model.temperature || undefined,
      max_output_tokens: model.max_output_tokens || undefined,
      thinking_level: model.thinking_level || undefined,
      supports_responses: model.supports_responses,
      instructions: model.instructions || undefined,
      request_cwd: model.request_cwd || undefined,
      include_prompt_cache_retention: model.include_prompt_cache_retention,
      request_body_limit_bytes: model.request_body_limit_bytes || undefined,
      enabled: model.enabled,
    });
    setDrawerOpen(true);
  }

  function confirmDelete(model: ModelConfigRecord) {
    Modal.confirm({
      title: `删除模型配置: ${model.name}`,
      content: '删除后，引用它的任务会失去默认模型绑定。',
      okButtonProps: { danger: true },
      onOk: () => deleteModelMutation.mutate(model.id),
    });
  }

  function handleSubmit(values: ModelFormValues) {
    const payload: CreateModelConfigPayload = values;
    if (editingModel) {
      updateModelMutation.mutate({ id: editingModel.id, payload });
    } else {
      createModelMutation.mutate(payload);
    }
  }

  function handleModelFormChange(changedValues: Partial<ModelFormValues>) {
    if (Object.prototype.hasOwnProperty.call(changedValues, 'provider')) {
      const nextProvider = normalizeSupportedProvider(changedValues.provider);
      const currentBaseUrl = (form.getFieldValue('base_url') || '').trim();
      const previousDefault = defaultBaseUrlForProvider(providerRef.current);
      if (!baseUrlDirty || !currentBaseUrl || currentBaseUrl === previousDefault) {
        applyAutoBaseUrl(defaultBaseUrlForProvider(nextProvider));
      }
      providerRef.current = nextProvider;
      setModelCatalog(null);
      if (form.getFieldValue('model')) {
        form.setFieldValue('model', '');
      }
      form.setFieldValue('supports_responses', nextProvider === 'openai');
      const currentThinkingLevel = form.getFieldValue('thinking_level');
      const nextThinkingLevelOptions = THINKING_LEVEL_OPTIONS[nextProvider].map((item) => item.value);
      if (currentThinkingLevel && !nextThinkingLevelOptions.includes(currentThinkingLevel)) {
        form.setFieldValue('thinking_level', undefined);
      }
    }
    if (
      Object.prototype.hasOwnProperty.call(changedValues, 'base_url') &&
      !autoUpdatingBaseUrlRef.current
    ) {
      setBaseUrlDirty(true);
      setModelCatalog(null);
    }
    if (Object.prototype.hasOwnProperty.call(changedValues, 'api_key')) {
      setModelCatalog(null);
    }
    if (Object.prototype.hasOwnProperty.call(changedValues, 'model')) {
      const selected = modelCatalog?.models.find((item) => item.id === changedValues.model);
      if (selected) {
        form.setFieldValue('supports_responses', selected.supports_responses);
      }
    }
  }

  return (
    <>
      {contextHolder}
      <Space direction="vertical" size="large" style={{ width: '100%' }}>
        <Space style={{ justifyContent: 'space-between', width: '100%' }}>
          <Space direction="vertical" size={0}>
            <Typography.Title level={3} style={{ margin: 0 }}>
              模型配置
            </Typography.Title>
            <Typography.Text type="secondary">
              保存 Task Runner 可直接调用的模型连接信息与运行参数。
            </Typography.Text>
          </Space>
          <Space>
            <Input
              allowClear
              placeholder="搜索名称 / Model / URL"
              style={{ width: 240 }}
              value={keywordFilter}
              onChange={(event) => setKeywordFilter(event.target.value)}
            />
            <Select
              style={{ width: 220 }}
              value={providerFilter}
              options={providerOptions}
              onChange={(value) => setProviderFilter(value)}
            />
            <Segmented
              value={enabledFilter}
              onChange={(value) =>
                setEnabledFilter(value as 'all' | 'enabled' | 'disabled')
              }
              options={[
                { label: '全部', value: 'all' },
                { label: '启用中', value: 'enabled' },
                { label: '已停用', value: 'disabled' },
              ]}
            />
            <Button
              onClick={() => {
                setKeywordFilter('');
                setProviderFilter('all');
                setEnabledFilter('all');
              }}
            >
              清空筛选
            </Button>
            <Button onClick={() => modelsQuery.refetch()}>刷新</Button>
            <Button type="primary" onClick={openCreateDrawer}>
              新建模型配置
            </Button>
          </Space>
        </Space>

        <Space size="large" wrap>
          <Statistic title="当前可见模型" value={filteredModels.length} />
          <Statistic
            title="启用中"
            value={filteredModels.filter((model) => model.enabled).length}
          />
          <Statistic title="绑定任务" value={filteredTaskCount} />
          <Statistic title="运行记录" value={filteredRunCount} />
        </Space>

        <Table<ModelConfigRecord>
          rowKey="id"
          columns={columns}
          dataSource={filteredModels}
          loading={modelsQuery.isLoading || usageQuery.isLoading}
          pagination={{ pageSize: 8 }}
          locale={{
            emptyText: (
              <Empty
                image={Empty.PRESENTED_IMAGE_SIMPLE}
                description="暂无模型配置，请先创建或导入模型配置"
              />
            ),
          }}
        />
      </Space>

      <Drawer
        title={editingModel ? '编辑模型配置' : '新建模型配置'}
        open={drawerOpen}
        width={560}
        destroyOnClose
        onClose={resetModelDrawerState}
        extra={
          <Space>
            <Button onClick={resetModelDrawerState}>取消</Button>
            <Button
              type="primary"
              loading={createModelMutation.isPending || updateModelMutation.isPending}
              onClick={() => form.submit()}
            >
              保存
            </Button>
          </Space>
        }
      >
        <Form<ModelFormValues>
          layout="vertical"
          form={form}
          onFinish={handleSubmit}
          onValuesChange={handleModelFormChange}
        >
          <Form.Item name="name" label="配置名称" rules={[{ required: true }]}>
            <Input />
          </Form.Item>
          <Space size="middle" style={{ width: '100%' }} align="start">
            <Form.Item name="provider" label="Provider" style={{ flex: 1 }} rules={[{ required: true }]}>
              <Select options={SUPPORTED_PROVIDER_OPTIONS} />
            </Form.Item>
          </Space>
          <Form.Item name="base_url" label="Base URL" rules={[{ required: true }]}>
            <Input />
          </Form.Item>
          <Form.Item name="api_key" label="API Key">
            <Input.Password />
          </Form.Item>
          <Form.Item label="Model" required>
            <Space.Compact style={{ width: '100%' }}>
              <Form.Item name="model" noStyle rules={[{ required: true, message: '请选择模型' }]}>
                <Select
                  showSearch
                  allowClear
                  placeholder="先拉取模型列表"
                  options={modelOptions}
                  optionFilterProp="label"
                  notFoundContent={
                    previewModelCatalogMutation.isPending ? '正在拉取模型...' : '暂无模型列表'
                  }
                />
              </Form.Item>
              <Button
                loading={previewModelCatalogMutation.isPending}
                onClick={fetchModelCatalog}
                disabled={!watchedApiKey?.trim()}
              >
                拉取模型
              </Button>
            </Space.Compact>
            <Typography.Text type="secondary" style={{ display: 'block', marginTop: 8 }}>
              {modelCatalog
                ? modelCatalog.source === 'live'
                  ? `已通过 ${modelCatalog.base_url}/models 获取 ${modelCatalog.models.length} 个模型`
                  : modelCatalog.error || '未获取到在线模型列表'
                : '供应商固定为 openai / deepseek / kimik2；配置好 Base URL 和 API Key 后再拉取模型。'}
            </Typography.Text>
          </Form.Item>
          <Space size="middle" style={{ width: '100%' }} align="start">
            <Form.Item name="temperature" label="Temperature" style={{ width: 160 }}>
              <InputNumber min={0} max={2} step={0.1} style={{ width: '100%' }} />
            </Form.Item>
            <Form.Item name="max_output_tokens" label="Max Output Tokens" style={{ width: 180 }}>
              <InputNumber min={1} style={{ width: '100%' }} />
            </Form.Item>
            <Form.Item name="thinking_level" label="Thinking Level" style={{ flex: 1 }}>
              <Select
                allowClear
                placeholder="按供应商选择"
                options={thinkingLevelOptions}
              />
            </Form.Item>
          </Space>
          <Form.Item name="instructions" label="Instructions">
            <Input.TextArea rows={4} />
          </Form.Item>
          <Form.Item name="request_cwd" label="Request CWD">
            <Input />
          </Form.Item>
          <Space size="middle" style={{ width: '100%' }} align="start">
            <Form.Item name="request_body_limit_bytes" label="Request Body Limit" style={{ flex: 1 }}>
              <InputNumber min={1} style={{ width: '100%' }} />
            </Form.Item>
            <Form.Item name="supports_responses" label="Supports Responses" valuePropName="checked" style={{ marginBottom: 0 }}>
              <Switch />
            </Form.Item>
            <Form.Item
              name="include_prompt_cache_retention"
              label="Prompt Cache Retention"
              valuePropName="checked"
              style={{ marginBottom: 0 }}
            >
              <Switch />
            </Form.Item>
            <Form.Item name="enabled" label="Enabled" valuePropName="checked" style={{ marginBottom: 0 }}>
              <Switch />
            </Form.Item>
          </Space>
        </Form>
      </Drawer>

      <Drawer
        title={selectedModel ? `模型详情 - ${selectedModel.name}` : '模型详情'}
        open={Boolean(routeModelId)}
        width={760}
        onClose={closeDetailDrawer}
      >
        {selectedModel ? (
          <Space direction="vertical" size="large" style={{ width: '100%' }}>
            <Space wrap>
              <Button onClick={() => navigate(`/tasks?model_config_id=${encodeURIComponent(selectedModel.id)}`)}>
                查看绑定任务
              </Button>
              <Button onClick={() => navigate(`/runs?model_config_id=${encodeURIComponent(selectedModel.id)}`)}>
                查看运行记录
              </Button>
              <Button
                loading={testModelMutation.isPending}
                onClick={() => testModelMutation.mutate(selectedModel.id)}
              >
                测试连通性
              </Button>
              <Button
                onClick={() => {
                  closeDetailDrawer();
                  openEditDrawer(selectedModel);
                }}
              >
                编辑配置
              </Button>
            </Space>

            <Descriptions bordered column={1} size="small">
              <Descriptions.Item label="模型配置 ID">{selectedModel.id}</Descriptions.Item>
              <Descriptions.Item label="Provider">{selectedModel.provider}</Descriptions.Item>
              <Descriptions.Item label="Model">{selectedModel.model}</Descriptions.Item>
              <Descriptions.Item label="Base URL">{selectedModel.base_url}</Descriptions.Item>
              <Descriptions.Item label="状态">
                <Tag color={selectedModel.enabled ? 'success' : 'default'}>
                  {selectedModel.enabled ? 'enabled' : 'disabled'}
                </Tag>
              </Descriptions.Item>
              <Descriptions.Item label="Supports Responses">
                {selectedModel.supports_responses ? 'yes' : 'no'}
              </Descriptions.Item>
              <Descriptions.Item label="Temperature">
                {selectedModel.temperature ?? '-'}
              </Descriptions.Item>
              <Descriptions.Item label="Max Output Tokens">
                {selectedModel.max_output_tokens ?? '-'}
              </Descriptions.Item>
              <Descriptions.Item label="Thinking Level">
                {selectedModel.thinking_level || '-'}
              </Descriptions.Item>
              <Descriptions.Item label="Request CWD">
                {selectedModel.request_cwd || '-'}
              </Descriptions.Item>
              <Descriptions.Item label="Prompt Cache Retention">
                {selectedModel.include_prompt_cache_retention ? 'enabled' : 'disabled'}
              </Descriptions.Item>
              <Descriptions.Item label="Request Body Limit">
                {selectedModel.request_body_limit_bytes ?? '-'}
              </Descriptions.Item>
              <Descriptions.Item label="绑定任务数">
                {taskCountByModelId.get(selectedModel.id) || 0}
              </Descriptions.Item>
              <Descriptions.Item label="运行次数">
                {runCountByModelId.get(selectedModel.id) || 0}
              </Descriptions.Item>
              <Descriptions.Item label="创建时间">
                {dayjs(selectedModel.created_at).format('YYYY-MM-DD HH:mm:ss')}
              </Descriptions.Item>
              <Descriptions.Item label="更新时间">
                {dayjs(selectedModel.updated_at).format('YYYY-MM-DD HH:mm:ss')}
              </Descriptions.Item>
            </Descriptions>

            {selectedModel.instructions ? (
              <div>
                <Typography.Title level={5}>Instructions</Typography.Title>
                <Typography.Paragraph style={{ whiteSpace: 'pre-wrap' }}>
                  {selectedModel.instructions}
                </Typography.Paragraph>
              </div>
            ) : null}

            <div>
              <Typography.Title level={5}>绑定任务</Typography.Title>
              {modelTasksQuery.data?.length ? (
                <List
                  bordered
                  dataSource={modelTasksQuery.data}
                  renderItem={(task) => (
                    <List.Item
                      actions={[
                        <Button
                          key="task"
                          size="small"
                          onClick={() => navigate(`/tasks?task_id=${encodeURIComponent(task.id)}`)}
                        >
                          打开
                        </Button>,
                      ]}
                    >
                      <Space direction="vertical" size={4} style={{ width: '100%' }}>
                        <Space wrap>
                          <Typography.Text strong>{task.title}</Typography.Text>
                          <Tag>{task.status}</Tag>
                        </Space>
                        <Typography.Paragraph
                          type="secondary"
                          ellipsis={{ rows: 2 }}
                          style={{ marginBottom: 0 }}
                        >
                          {task.objective}
                        </Typography.Paragraph>
                      </Space>
                    </List.Item>
                  )}
                />
              ) : modelTasksQuery.isLoading ? null : (
                <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description="当前没有绑定任务" />
              )}
            </div>

            <div>
              <Typography.Title level={5}>最近运行</Typography.Title>
              {modelRunsQuery.data?.length ? (
                <List
                  bordered
                  dataSource={modelRunsQuery.data}
                  renderItem={(run) => (
                    <List.Item
                      actions={[
                        <Button
                          key="run"
                          size="small"
                          onClick={() => navigate(`/runs?run_id=${encodeURIComponent(run.id)}`)}
                        >
                          打开
                        </Button>,
                      ]}
                    >
                      <Space direction="vertical" size={4} style={{ width: '100%' }}>
                        <Space wrap>
                          <Typography.Text code>{run.id.slice(0, 12)}</Typography.Text>
                          <Tag>{run.status}</Tag>
                          <Typography.Text type="secondary">
                            {dayjs(run.updated_at).format('YYYY-MM-DD HH:mm:ss')}
                          </Typography.Text>
                        </Space>
                        {run.result_summary ? (
                          <Typography.Paragraph
                            type="secondary"
                            ellipsis={{ rows: 2 }}
                            style={{ marginBottom: 0 }}
                          >
                            {run.result_summary}
                          </Typography.Paragraph>
                        ) : (
                          <Typography.Text type="secondary">暂无摘要</Typography.Text>
                        )}
                      </Space>
                    </List.Item>
                  )}
                />
              ) : modelRunsQuery.isLoading ? null : (
                <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description="当前还没有运行记录" />
              )}
            </div>
          </Space>
        ) : selectedModelQuery.isLoading ? null : (
          <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} />
        )}
      </Drawer>

      <Modal
        title="模型测试结果"
        open={Boolean(testResult)}
        width={680}
        footer={[
          <Button key="close" onClick={() => setTestResult(null)}>
            关闭
          </Button>,
        ]}
        onCancel={() => setTestResult(null)}
      >
        {testResult ? (
          <Space direction="vertical" size="large" style={{ width: '100%' }}>
            <Descriptions bordered column={1} size="small">
              <Descriptions.Item label="结果">
                <Tag color={testResult.ok ? 'success' : 'error'}>
                  {testResult.ok ? 'success' : 'failed'}
                </Tag>
              </Descriptions.Item>
              <Descriptions.Item label="Provider">{testResult.provider}</Descriptions.Item>
              <Descriptions.Item label="Model">{testResult.model}</Descriptions.Item>
              <Descriptions.Item label="测试时间">
                {dayjs(testResult.tested_at).format('YYYY-MM-DD HH:mm:ss')}
              </Descriptions.Item>
              <Descriptions.Item label="Response ID">
                {testResult.response_id || '-'}
              </Descriptions.Item>
              <Descriptions.Item label="输出">
                {testResult.content || '-'}
              </Descriptions.Item>
              <Descriptions.Item label="Reasoning">
                {testResult.reasoning || '-'}
              </Descriptions.Item>
              <Descriptions.Item label="错误信息">
                {testResult.error || '-'}
              </Descriptions.Item>
            </Descriptions>
            {testResult.usage ? (
              <Typography.Paragraph
                style={{
                  background: '#fafafa',
                  padding: 12,
                  borderRadius: 6,
                  marginBottom: 0,
                  whiteSpace: 'pre-wrap',
                  fontFamily: 'monospace',
                }}
              >
                {JSON.stringify(testResult.usage, null, 2)}
              </Typography.Paragraph>
            ) : null}
          </Space>
        ) : null}
      </Modal>
    </>
  );
}
