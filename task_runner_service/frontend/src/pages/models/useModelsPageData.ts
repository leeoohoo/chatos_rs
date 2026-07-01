// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useMemo } from 'react';
import { useQuery } from '@tanstack/react-query';

import { api } from '../../api/client';
import type { TranslateFn } from '../../i18n/I18nProvider';
import type { ModelCatalogResponse } from '../../types';
import {
  buildModelOptions,
  type ModelEnabledFilter,
  type SupportedProvider,
  THINKING_LEVEL_OPTIONS,
} from './modelPageUtils';

type UseModelsPageDataParams = {
  t: TranslateFn;
  routeModelId?: string;
  keywordFilter: string;
  providerFilter: 'all' | string;
  enabledFilter: ModelEnabledFilter;
  normalizedProvider: SupportedProvider;
  modelCatalog: ModelCatalogResponse | null;
  watchedModel?: string;
  editingProvider?: string | null;
  watchedSupportsResponses?: boolean;
};

export function useModelsPageData({
  t,
  routeModelId,
  keywordFilter,
  providerFilter,
  enabledFilter,
  normalizedProvider,
  modelCatalog,
  watchedModel,
  editingProvider,
  watchedSupportsResponses,
}: UseModelsPageDataParams) {
  const enabledFilterOptions = useMemo(
    () => [
      { label: t('models.filter.all'), value: 'all' },
      { label: t('models.filter.enabled'), value: 'enabled' },
      { label: t('models.filter.disabled'), value: 'disabled' },
    ],
    [t],
  );

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

  const modelOptions = useMemo(
    () =>
      buildModelOptions({
        modelCatalog,
        currentModel: watchedModel,
        ownerProvider: editingProvider,
        supportsResponses: watchedSupportsResponses,
      }),
    [editingProvider, modelCatalog, watchedModel, watchedSupportsResponses],
  );

  const thinkingLevelOptions = useMemo(
    () => THINKING_LEVEL_OPTIONS[normalizedProvider],
    [normalizedProvider],
  );

  const providerOptions = useMemo(
    () =>
      ['all', ...Array.from(new Set((modelsQuery.data || []).map((model) => model.provider))).sort()]
        .map((provider) => ({
          label: provider === 'all' ? t('models.providerAll') : provider,
          value: provider,
        })),
    [modelsQuery.data, t],
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
      return [model.name, model.model, model.provider, model.base_url, model.usage_scenario]
        .some((value) => value?.toLowerCase().includes(keyword));
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

  const filteredEnabledCount = useMemo(
    () => filteredModels.filter((model) => model.enabled).length,
    [filteredModels],
  );

  return {
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
  };
}
