import { useRef, useState } from 'react';
import { useMutation } from '@tanstack/react-query';
import { useNavigate, useSearchParams } from 'react-router-dom';
import {
  Form,
  Modal,
  Space,
  message,
} from 'antd';

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
  type SupportedProvider,
  THINKING_LEVEL_OPTIONS,
} from './models/modelPageUtils';
import { ModelDetailDrawer } from './models/ModelDetailDrawer';
import { ModelEditorDrawer } from './models/ModelEditorDrawer';
import { ModelListTable } from './models/ModelListTable';
import { ModelListToolbar } from './models/ModelListToolbar';
import { ModelStatsBar } from './models/ModelStatsBar';
import { ModelTestResultModal } from './models/ModelTestResultModal';
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

      <ModelEditorDrawer
        t={t}
        open={drawerOpen}
        editingModel={editingModel}
        form={form}
        saving={createModelMutation.isPending || updateModelMutation.isPending}
        modelOptions={modelOptions}
        thinkingLevelOptions={thinkingLevelOptions}
        modelCatalog={modelCatalog}
        watchedApiKey={watchedApiKey}
        catalogLoading={previewModelCatalogMutation.isPending}
        onClose={resetModelDrawerState}
        onSubmit={handleSubmit}
        onValuesChange={handleModelFormChange}
        onFetchCatalog={fetchModelCatalog}
      />

      <ModelDetailDrawer
        t={t}
        open={Boolean(routeModelId)}
        selectedModel={selectedModel}
        loading={selectedModelQuery.isLoading}
        taskCount={selectedModel ? taskCountByModelId.get(selectedModel.id) || 0 : 0}
        runCount={selectedModel ? runCountByModelId.get(selectedModel.id) || 0 : 0}
        boundTasks={modelTasksQuery.data}
        boundTasksLoading={modelTasksQuery.isLoading}
        recentRuns={modelRunsQuery.data}
        recentRunsLoading={modelRunsQuery.isLoading}
        testing={testModelMutation.isPending}
        onClose={closeDetailDrawer}
        onViewTasks={(modelId) =>
          navigate(`/tasks?model_config_id=${encodeURIComponent(modelId)}`)
        }
        onViewRuns={(modelId) =>
          navigate(`/runs?model_config_id=${encodeURIComponent(modelId)}`)
        }
        onTest={(modelId) => testModelMutation.mutate(modelId)}
        onEdit={(model) => {
          closeDetailDrawer();
          openEditDrawer(model);
        }}
        onOpenTask={(taskId) =>
          navigate(`/tasks?task_id=${encodeURIComponent(taskId)}`)
        }
        onOpenRun={(runId) =>
          navigate(`/runs?run_id=${encodeURIComponent(runId)}`)
        }
      />

      <ModelTestResultModal
        t={t}
        result={testResult}
        onClose={() => setTestResult(null)}
      />
    </>
  );
}
