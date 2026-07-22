// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';

import { useI18n } from '../../i18n/I18nProvider';
import { useApiClient } from '../../lib/api/ApiClientContext';
import { normalizeAiModelConfig } from '../../lib/domain/configs';
import { useChatStoreResolved } from '../../lib/store/ChatStoreContext';
import type { AiModelConfig } from '../../types';
import { CloudDefaultModelSettings } from './CloudDefaultModelSettings';
import { CloudTaskModelSettings } from './CloudTaskModelSettings';
import {
  buildTaskModelPatch,
  defaultModelDraftsFromSettings,
  isCloudConfiguredModel,
  isCloudRunnableModel,
  taskModelDraftsFromModels,
} from './cloudAiSettingsModel';
import {
  emptyDefaultModelDrafts,
  type DefaultModelDrafts,
  type DefaultModelSlot,
  type TaskModelDraft,
  type TaskModelDrafts,
} from './cloudAiSettingsTypes';

export function CloudAiSettingsSection({
  refreshKey,
  onManageModels,
}: {
  refreshKey: number;
  onManageModels: () => void;
}) {
  const { t } = useI18n();
  const client = useApiClient();
  const { loadAiModelConfigs } = useChatStoreResolved();
  const [models, setModels] = React.useState<AiModelConfig[]>([]);
  const [defaultDrafts, setDefaultDrafts] = React.useState<DefaultModelDrafts>(
    emptyDefaultModelDrafts,
  );
  const [taskDrafts, setTaskDrafts] = React.useState<TaskModelDrafts>({});
  const [modelRequestMaxRetries, setModelRequestMaxRetries] = React.useState('5');
  const [loading, setLoading] = React.useState(true);
  const [saving, setSaving] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  const [notice, setNotice] = React.useState<string | null>(null);

  const load = React.useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const [rawModels, settings] = await Promise.all([
        client.getAiModelConfigs(),
        client.getAiModelSettings(),
      ]);
      const cloudModels = rawModels
        .map(normalizeAiModelConfig)
        .filter(isCloudConfiguredModel);
      setModels(cloudModels);
      setDefaultDrafts(defaultModelDraftsFromSettings(settings));
      setTaskDrafts(taskModelDraftsFromModels(cloudModels));
      setModelRequestMaxRetries(String(settings.model_request_max_retries ?? 5));
      await loadAiModelConfigs({ force: true });
    } catch (value) {
      setError(errorMessage(value, t('modelSettings.error.load'), t));
    } finally {
      setLoading(false);
    }
  }, [client, loadAiModelConfigs, t]);

  React.useEffect(() => {
    void load();
  }, [load, refreshKey]);

  const updateDefaultDraft = (
    slot: DefaultModelSlot,
    draft: DefaultModelDrafts[DefaultModelSlot],
  ) => {
    setDefaultDrafts((current) => ({ ...current, [slot]: draft }));
  };

  const updateTaskDraft = (modelId: string, patch: Partial<TaskModelDraft>) => {
    setTaskDrafts((current) => ({
      ...current,
      [modelId]: { ...current[modelId], ...patch },
    }));
    if (patch.enabled === false) {
      setDefaultDrafts((current) => clearDisabledDefaultModel(current, modelId));
    }
  };

  const save = async () => {
    setSaving(true);
    setError(null);
    setNotice(null);
    try {
      const maxRetries = Number(modelRequestMaxRetries);
      if (!Number.isInteger(maxRetries) || maxRetries < 0 || maxRetries > 10) {
        throw new Error('invalid_model_request_max_retries');
      }
      const updates = models.flatMap((model) => {
        const draft = taskDrafts[model.id];
        if (!draft) return [];
        const patch = buildTaskModelPatch(model, draft);
        return Object.keys(patch).length ? [client.updateAiModelConfig(model.id, patch)] : [];
      });
      await Promise.all([
        client.updateAiModelSettings({
          model_request_max_retries: maxRetries,
          memory_summary_model_config_id: defaultDrafts.memory.modelId || null,
          memory_summary_thinking_level: defaultDrafts.memory.thinking || null,
          project_management_agent_model_config_id: defaultDrafts.project.modelId || null,
          project_management_agent_thinking_level: defaultDrafts.project.thinking || null,
          environment_initialization_model_config_id: defaultDrafts.environment.modelId || null,
          environment_initialization_thinking_level: defaultDrafts.environment.thinking || null,
        }),
        ...updates,
      ]);
      setNotice(t('cloudAi.saved'));
      await load();
    } catch (value) {
      setError(errorMessage(value, t('modelSettings.error.save'), t));
    } finally {
      setSaving(false);
    }
  };

  const defaultModels = models.filter((model) => {
    const draft = taskDrafts[model.id];
    return isCloudRunnableModel(model) && (draft?.enabled ?? model.enabled);
  });

  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <h3 className="text-base font-semibold text-foreground">{t('cloudAi.title')}</h3>
          <p className="mt-1 max-w-3xl text-xs text-muted-foreground">
            {t('cloudAi.description')}
          </p>
        </div>
        <div className="flex gap-2">
          <button
            type="button"
            onClick={onManageModels}
            className="rounded-lg border border-border bg-background px-3 py-2 text-sm text-foreground hover:bg-accent"
          >
            {t('cloudAi.manageModels')}
          </button>
          <button
            type="button"
            disabled={loading || saving}
            onClick={() => void save()}
            className="rounded-lg bg-primary px-3 py-2 text-sm text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
          >
            {saving ? t('modelSettings.saving') : t('cloudAi.save')}
          </button>
        </div>
      </div>

      {error ? (
        <div className="rounded-lg border border-destructive/20 bg-destructive/10 p-2 text-sm text-destructive">
          {error}
        </div>
      ) : null}
      {notice ? (
        <div className="rounded-lg border border-primary/20 bg-primary/10 p-2 text-sm text-primary">
          {notice}
        </div>
      ) : null}

      <CloudDefaultModelSettings
        models={defaultModels}
        drafts={defaultDrafts}
        disabled={loading || saving}
        onChange={updateDefaultDraft}
      />
      <section className="rounded-xl border border-border bg-card p-4">
        <h4 className="text-sm font-semibold text-foreground">
          {t('cloudAi.retrySettings')}
        </h4>
        <p className="mt-1 text-xs text-muted-foreground">
          {t('cloudAi.retrySettingsDescription')}
        </p>
        <label className="mt-3 block max-w-xs text-sm text-foreground">
          {t('cloudAi.maxRetries')}
          <input
            type="number"
            min={0}
            max={10}
            step={1}
            value={modelRequestMaxRetries}
            disabled={loading || saving}
            onChange={(event) => setModelRequestMaxRetries(event.target.value)}
            className="mt-1 w-full rounded-lg border border-border bg-background px-3 py-2"
          />
        </label>
      </section>
      <CloudTaskModelSettings
        models={models}
        drafts={taskDrafts}
        disabled={loading || saving}
        onChange={updateTaskDraft}
      />
    </div>
  );
}

function clearDisabledDefaultModel(
  drafts: DefaultModelDrafts,
  modelId: string,
): DefaultModelDrafts {
  return (Object.keys(drafts) as DefaultModelSlot[]).reduce((next, slot) => ({
    ...next,
    [slot]: drafts[slot].modelId === modelId
      ? { modelId: '', thinking: '' }
      : drafts[slot],
  }), drafts);
}

function errorMessage(
  value: unknown,
  fallback: string,
  t: (key: string) => string,
): string {
  if (value instanceof Error && value.message === 'invalid_temperature') {
    return t('cloudAi.error.invalidTemperature');
  }
  if (value instanceof Error && value.message === 'invalid_max_output_tokens') {
    return t('cloudAi.error.invalidMaxTokens');
  }
  if (value instanceof Error && value.message === 'invalid_model_request_max_retries') {
    return t('cloudAi.error.invalidMaxRetries');
  }
  return value instanceof Error ? value.message : fallback;
}
