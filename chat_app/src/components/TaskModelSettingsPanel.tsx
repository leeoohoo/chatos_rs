// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React, { useEffect, useMemo, useState } from 'react';

import { useChatStoreResolved } from '../lib/store/ChatStoreContext';
import { useI18n } from '../i18n/I18nProvider';
import { thinkingOptionsForProvider } from '../lib/modelThinkingOptions';
import type { AiModelConfig } from '../types';
import ManagerFormDialog from './ui/ManagerFormDialog';

interface Props {
  onClose: () => void;
}

type Draft = {
  usage: string;
  thinking: string;
  enabled: boolean;
};

const TaskModelRow = ({
  model,
  draft,
  onChange,
}: {
  model: AiModelConfig;
  draft: Draft;
  onChange: (patch: Partial<Draft>) => void;
}) => {
  const { t } = useI18n();
  const thinkingOptions = useMemo(
    () => thinkingOptionsForProvider(model.provider, t),
    [model.provider, t],
  );

  return (
    <div className={`rounded-lg border border-border bg-card p-3 ${draft.enabled ? '' : 'opacity-75'}`}>
      <div className="flex flex-wrap items-start justify-between gap-2">
        <div className="min-w-0">
          <div className="truncate text-sm font-medium text-foreground">{model.name}</div>
          <div className="mt-0.5 truncate text-xs text-muted-foreground">
            {model.provider} | {model.model_name}
          </div>
        </div>
        <div className="flex items-center gap-2">
          <span className={`rounded-full px-2 py-1 text-[11px] ${draft.enabled ? 'bg-green-100 text-green-800 dark:bg-green-900 dark:text-green-200' : 'bg-secondary text-secondary-foreground'}`}>
            {draft.enabled ? t('aiModelManager.status.enabled') : t('aiModelManager.status.disabled')}
          </span>
          <button
            type="button"
            onClick={() => onChange({ enabled: !draft.enabled })}
            className={`rounded-md border px-2 py-1 text-xs transition-colors ${
              draft.enabled
                ? 'border-red-200 text-red-700 hover:bg-red-50 dark:border-red-900 dark:text-red-200 dark:hover:bg-red-950/40'
                : 'border-green-200 text-green-700 hover:bg-green-50 dark:border-green-900 dark:text-green-200 dark:hover:bg-green-950/40'
            }`}
          >
            {draft.enabled ? t('taskModelSettings.disable') : t('taskModelSettings.enable')}
          </button>
        </div>
      </div>

      <div className="mt-3 grid gap-3 md:grid-cols-[1fr_180px]">
        <label className="block">
          <span className="mb-1 block text-xs text-muted-foreground">{t('taskModelSettings.usage')}</span>
          <input
            value={draft.usage}
            onChange={(event) => onChange({ usage: event.target.value })}
            placeholder={t('taskModelSettings.usagePlaceholder')}
            className="w-full rounded-md border border-input bg-background px-3 py-2 text-sm text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
          />
        </label>
        <label className="block">
          <span className="mb-1 block text-xs text-muted-foreground">{t('modelSettings.thinkingLevel')}</span>
          <select
            value={draft.thinking}
            onChange={(event) => onChange({ thinking: event.target.value })}
            className="w-full rounded-md border border-input bg-background px-3 py-2 text-sm text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
          >
            {thinkingOptions.map((option) => (
              <option key={option.value || 'default'} value={option.value}>
                {option.label}
              </option>
            ))}
          </select>
        </label>
      </div>
    </div>
  );
};

const TaskModelSettingsPanel: React.FC<Props> = ({ onClose }) => {
  const { t } = useI18n();
  const { aiModelConfigs, loadAiModelConfigs, updateAiModelConfig } = useChatStoreResolved();
  const [drafts, setDrafts] = useState<Record<string, Draft>>({});
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const concreteModels = useMemo(
    () => aiModelConfigs.filter((item) => item.model_name.trim()),
    [aiModelConfigs],
  );

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    loadAiModelConfigs({ force: true })
      .catch((err) => {
        if (!cancelled) {
          setError(err instanceof Error ? err.message : t('modelSettings.error.load'));
        }
      })
      .finally(() => {
        if (!cancelled) {
          setLoading(false);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [loadAiModelConfigs, t]);

  useEffect(() => {
    setDrafts((current) => {
      const next: Record<string, Draft> = {};
      concreteModels.forEach((model) => {
        next[model.id] = current[model.id] || {
          usage: model.task_usage_scenario || '',
          thinking: model.task_thinking_level || '',
          enabled: model.enabled,
        };
      });
      return next;
    });
  }, [concreteModels]);

  const save = async () => {
    setSaving(true);
    setError(null);
    try {
      for (const model of concreteModels) {
        const draft = drafts[model.id];
        if (!draft) {
          continue;
        }
        const usage = draft.usage.trim();
        const thinking = draft.thinking.trim();
        if ((model.task_usage_scenario || '') === usage
          && (model.task_thinking_level || '') === thinking
          && model.enabled === draft.enabled) {
          continue;
        }
        await updateAiModelConfig({
          ...model,
          enabled: draft.enabled,
          task_usage_scenario: usage || null,
          task_thinking_level: thinking || null,
          updatedAt: new Date(),
        });
      }
      onClose();
    } catch (err) {
      setError(err instanceof Error ? err.message : t('modelSettings.error.save'));
    } finally {
      setSaving(false);
    }
  };

  return (
    <ManagerFormDialog
      open
      title={t('taskModelSettings.title')}
      description={t('taskModelSettings.description')}
      widthClassName="max-w-3xl"
      onClose={onClose}
    >
      <div className="space-y-4">
        {error ? (
          <div className="rounded-lg border border-red-200 bg-red-50 px-3 py-2 text-sm text-red-700 dark:border-red-900 dark:bg-red-950/40 dark:text-red-200">
            {error}
          </div>
        ) : null}

        <div className="space-y-3">
          {concreteModels.length === 0 && !loading ? (
            <div className="rounded-lg border border-border bg-muted/40 p-4 text-sm text-muted-foreground">
              {t('modelSettings.emptyModels')}
            </div>
          ) : null}
          {concreteModels.map((model) => (
            <TaskModelRow
              key={model.id}
              model={model}
              draft={drafts[model.id] || { usage: '', thinking: '', enabled: model.enabled }}
              onChange={(patch) => {
                setDrafts((current) => ({
                  ...current,
                  [model.id]: {
                    usage: current[model.id]?.usage || '',
                    thinking: current[model.id]?.thinking || '',
                    enabled: current[model.id]?.enabled ?? model.enabled,
                    ...patch,
                  },
                }));
              }}
            />
          ))}
        </div>

        <div className="flex justify-end gap-2">
          <button
            type="button"
            onClick={onClose}
            className="rounded-lg bg-muted px-3 py-2 text-sm transition-colors hover:bg-accent"
          >
            {t('common.cancel')}
          </button>
          <button
            type="button"
            onClick={() => void save()}
            disabled={loading || saving}
            className="rounded-lg bg-primary px-3 py-2 text-sm text-primary-foreground transition-opacity hover:opacity-90 disabled:opacity-50"
          >
            {saving ? t('modelSettings.saving') : t('common.save')}
          </button>
        </div>
      </div>
    </ManagerFormDialog>
  );
};

export default TaskModelSettingsPanel;
