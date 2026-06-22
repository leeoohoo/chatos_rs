import { useRef, useState } from 'react';
import { useMutation } from '@tanstack/react-query';
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
  Space,
  Switch,
  Tag,
  Typography,
  message,
} from 'antd';
import dayjs from 'dayjs';

import { api } from '../api/client';
import { useI18n } from '../i18n/I18nProvider';
import type {
  CreateModelConfigPayload,
  ModelCatalogResponse,
  ModelConfigRecord,
  ModelConfigTestResponse,
} from '../types';
import {
  defaultBaseUrlForProvider,
  type ModelEnabledFilter,
  type ModelFormValues,
  normalizeSupportedProvider,
  SUPPORTED_PROVIDER_OPTIONS,
  type SupportedProvider,
  THINKING_LEVEL_OPTIONS,
} from './models/modelPageUtils';
import { ModelListTable } from './models/ModelListTable';
import { ModelListToolbar } from './models/ModelListToolbar';
import { ModelStatsBar } from './models/ModelStatsBar';
import { useModelMutations } from './models/useModelMutations';
import { useModelsPageData } from './models/useModelsPageData';

export function ModelsPage() {
  const { t } = useI18n();
  const navigate = useNavigate();
  const [searchParams, setSearchParams] = useSearchParams();
  const [messageApi, contextHolder] = message.useMessage();
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [editingModel, setEditingModel] = useState<ModelConfigRecord | null>(null);
  const [testResult, setTestResult] = useState<ModelConfigTestResponse | null>(null);
  const [keywordFilter, setKeywordFilter] = useState('');
  const [providerFilter, setProviderFilter] = useState<'all' | string>('all');
  const [enabledFilter, setEnabledFilter] = useState<ModelEnabledFilter>('all');
  const [modelCatalog, setModelCatalog] = useState<ModelCatalogResponse | null>(null);
  const [baseUrlDirty, setBaseUrlDirty] = useState(false);
  const [form] = Form.useForm<ModelFormValues>();
  const watchedProvider = Form.useWatch('provider', form);
  const watchedModel = Form.useWatch('model', form);
  const watchedApiKey = Form.useWatch('api_key', form);
  const watchedSupportsResponses = Form.useWatch('supports_responses', form);
  const providerRef = useRef<SupportedProvider>('openai');
  const autoUpdatingBaseUrlRef = useRef(false);
  const routeModelId = searchParams.get('model_id') || undefined;
  const normalizedProvider = normalizeSupportedProvider(watchedProvider);
  const {
    enabledFilterOptions,
    modelsQuery,
    usageQuery,
    selectedModelQuery,
    modelTasksQuery,
    modelRunsQuery,
    taskCountByModelId,
    runCountByModelId,
    selectedModel,
    modelOptions,
    thinkingLevelOptions,
    providerOptions,
    filteredModels,
    filteredEnabledCount,
    filteredTaskCount,
    filteredRunCount,
  } = useModelsPageData({
    t,
    routeModelId,
    keywordFilter,
    providerFilter,
    enabledFilter,
    normalizedProvider,
    modelCatalog,
    watchedModel,
    editingProvider: editingModel?.provider,
    watchedSupportsResponses,
  });
  const {
    createModelMutation,
    updateModelMutation,
    deleteModelMutation,
    testModelMutation,
  } = useModelMutations({
    t,
    messageApi,
    onModelSaved: resetModelDrawerState,
    onTestResult: setTestResult,
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
        messageApi.success(t('models.catalogUpdated', { count: catalog.models.length }));
      } else if (catalog.error) {
        messageApi.warning(t('models.catalogFallbackWithError', { error: catalog.error }));
      } else {
        messageApi.warning(t('models.catalogFallback'));
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
      usage_scenario: '',
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
      usage_scenario: model.usage_scenario || undefined,
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
      title: t('models.deleteConfirmTitle', { name: model.name }),
      content: t('models.deleteConfirmContent'),
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
        <ModelListToolbar
          t={t}
          keywordFilter={keywordFilter}
          providerFilter={providerFilter}
          enabledFilter={enabledFilter}
          providerOptions={providerOptions}
          enabledFilterOptions={enabledFilterOptions}
          onKeywordFilterChange={setKeywordFilter}
          onProviderFilterChange={setProviderFilter}
          onEnabledFilterChange={setEnabledFilter}
          onClearFilters={() => {
            setKeywordFilter('');
            setProviderFilter('all');
            setEnabledFilter('all');
          }}
          onRefresh={() => modelsQuery.refetch()}
          onCreate={openCreateDrawer}
        />

        <ModelStatsBar
          t={t}
          visibleCount={filteredModels.length}
          enabledCount={filteredEnabledCount}
          taskCount={filteredTaskCount}
          runCount={filteredRunCount}
        />

        <ModelListTable
          t={t}
          models={filteredModels}
          loading={modelsQuery.isLoading || usageQuery.isLoading}
          taskCountByModelId={taskCountByModelId}
          runCountByModelId={runCountByModelId}
          testing={testModelMutation.isPending}
          onOpenDetail={openDetailDrawer}
          onOpenEdit={openEditDrawer}
          onDelete={confirmDelete}
          onTest={(modelId) => testModelMutation.mutate(modelId)}
          onViewTasks={(modelId) =>
            navigate(`/tasks?model_config_id=${encodeURIComponent(modelId)}`)
          }
          onViewRuns={(modelId) =>
            navigate(`/runs?model_config_id=${encodeURIComponent(modelId)}`)
          }
        />
      </Space>

      <Drawer
        title={editingModel ? t('models.drawer.edit') : t('models.drawer.create')}
        open={drawerOpen}
        width={560}
        destroyOnClose
        onClose={resetModelDrawerState}
        extra={
          <Space>
            <Button onClick={resetModelDrawerState}>{t('common.cancel')}</Button>
            <Button
              type="primary"
              loading={createModelMutation.isPending || updateModelMutation.isPending}
              onClick={() => form.submit()}
            >
              {t('common.save')}
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
          <Form.Item name="name" label={t('models.column.name')} rules={[{ required: true }]}>
            <Input />
          </Form.Item>
          <Form.Item
            name="usage_scenario"
            label={t('models.column.usageScenario')}
            extra={t('models.form.usageScenarioHint')}
          >
            <Input.TextArea
              rows={3}
              placeholder={t('models.form.usageScenarioPlaceholder')}
            />
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
              <Form.Item name="model" noStyle rules={[{ required: true, message: t('models.form.modelRequired') }]}>
                <Select
                  showSearch
                  allowClear
                  placeholder={t('models.form.modelPlaceholder')}
                  options={modelOptions}
                  optionFilterProp="label"
                  notFoundContent={
                    previewModelCatalogMutation.isPending
                      ? t('models.form.loadingModels')
                      : t('models.form.noModels')
                  }
                />
              </Form.Item>
              <Button
                loading={previewModelCatalogMutation.isPending}
                onClick={fetchModelCatalog}
                disabled={!watchedApiKey?.trim()}
              >
                {t('models.form.fetchModels')}
              </Button>
            </Space.Compact>
            <Typography.Text type="secondary" style={{ display: 'block', marginTop: 8 }}>
              {modelCatalog
                ? modelCatalog.source === 'live'
                  ? t('models.form.catalogLoaded', {
                      baseUrl: modelCatalog.base_url,
                      count: modelCatalog.models.length,
                    })
                  : modelCatalog.error || t('models.form.catalogEmpty')
                : t('models.form.catalogHint')}
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
                placeholder={t('models.form.thinkingPlaceholder')}
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
        title={selectedModel
          ? t('models.detail.titleWithName', { name: selectedModel.name })
          : t('models.detail.title')}
        open={Boolean(routeModelId)}
        width={760}
        onClose={closeDetailDrawer}
      >
        {selectedModel ? (
          <Space direction="vertical" size="large" style={{ width: '100%' }}>
            <Space wrap>
              <Button onClick={() => navigate(`/tasks?model_config_id=${encodeURIComponent(selectedModel.id)}`)}>
                {t('models.detail.viewTasks')}
              </Button>
              <Button onClick={() => navigate(`/runs?model_config_id=${encodeURIComponent(selectedModel.id)}`)}>
                {t('models.detail.viewRuns')}
              </Button>
              <Button
                loading={testModelMutation.isPending}
                onClick={() => testModelMutation.mutate(selectedModel.id)}
              >
                {t('models.detail.testConnection')}
              </Button>
              <Button
                onClick={() => {
                  closeDetailDrawer();
                  openEditDrawer(selectedModel);
                }}
              >
                {t('models.detail.editConfig')}
              </Button>
            </Space>

            <Descriptions bordered column={1} size="small">
              <Descriptions.Item label={t('models.detail.modelId')}>{selectedModel.id}</Descriptions.Item>
              <Descriptions.Item label="Provider">{selectedModel.provider}</Descriptions.Item>
              <Descriptions.Item label="Model">{selectedModel.model}</Descriptions.Item>
              <Descriptions.Item label={t('models.column.usageScenario')}>
                {selectedModel.usage_scenario || '-'}
              </Descriptions.Item>
              <Descriptions.Item label="Base URL">{selectedModel.base_url}</Descriptions.Item>
              <Descriptions.Item label={t('common.status')}>
                <Tag color={selectedModel.enabled ? 'success' : 'default'}>
                  {selectedModel.enabled ? t('common.enabled') : t('common.disabled')}
                </Tag>
              </Descriptions.Item>
              <Descriptions.Item label="Supports Responses">
                {selectedModel.supports_responses ? t('common.yes') : t('common.no')}
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
                {selectedModel.include_prompt_cache_retention
                  ? t('common.enabled')
                  : t('common.disabled')}
              </Descriptions.Item>
              <Descriptions.Item label="Request Body Limit">
                {selectedModel.request_body_limit_bytes ?? '-'}
              </Descriptions.Item>
              <Descriptions.Item label={t('models.column.boundTasks')}>
                {taskCountByModelId.get(selectedModel.id) || 0}
              </Descriptions.Item>
              <Descriptions.Item label={t('models.column.runCount')}>
                {runCountByModelId.get(selectedModel.id) || 0}
              </Descriptions.Item>
              <Descriptions.Item label={t('models.detail.createdAt')}>
                {dayjs(selectedModel.created_at).format('YYYY-MM-DD HH:mm:ss')}
              </Descriptions.Item>
              <Descriptions.Item label={t('common.updatedAt')}>
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
              <Typography.Title level={5}>{t('models.detail.boundTasks')}</Typography.Title>
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
                          {t('common.open')}
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
                <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('models.detail.noBoundTasks')} />
              )}
            </div>

            <div>
              <Typography.Title level={5}>{t('models.detail.recentRuns')}</Typography.Title>
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
                          {t('common.open')}
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
                          <Typography.Text type="secondary">{t('models.detail.noSummary')}</Typography.Text>
                        )}
                      </Space>
                    </List.Item>
                  )}
                />
              ) : modelRunsQuery.isLoading ? null : (
                <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t('models.detail.noRuns')} />
              )}
            </div>
          </Space>
        ) : selectedModelQuery.isLoading ? null : (
          <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} />
        )}
      </Drawer>

      <Modal
        title={t('models.testResult.title')}
        open={Boolean(testResult)}
        width={680}
        footer={[
          <Button key="close" onClick={() => setTestResult(null)}>
            {t('common.close')}
          </Button>,
        ]}
        onCancel={() => setTestResult(null)}
      >
        {testResult ? (
          <Space direction="vertical" size="large" style={{ width: '100%' }}>
            <Descriptions bordered column={1} size="small">
              <Descriptions.Item label={t('models.testResult.result')}>
                <Tag color={testResult.ok ? 'success' : 'error'}>
                  {testResult.ok ? t('common.success') : t('common.failed')}
                </Tag>
              </Descriptions.Item>
              <Descriptions.Item label="Provider">{testResult.provider}</Descriptions.Item>
              <Descriptions.Item label="Model">{testResult.model}</Descriptions.Item>
              <Descriptions.Item label={t('models.testResult.testedAt')}>
                {dayjs(testResult.tested_at).format('YYYY-MM-DD HH:mm:ss')}
              </Descriptions.Item>
              <Descriptions.Item label="Response ID">
                {testResult.response_id || '-'}
              </Descriptions.Item>
              <Descriptions.Item label={t('models.testResult.output')}>
                {testResult.content || '-'}
              </Descriptions.Item>
              <Descriptions.Item label="Reasoning">
                {testResult.reasoning || '-'}
              </Descriptions.Item>
              <Descriptions.Item label={t('models.testResult.error')}>
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
