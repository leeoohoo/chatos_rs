// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';

import { useI18n } from '../../i18n/I18nProvider';
import { thinkingOptionsForProvider } from '../../lib/modelThinkingOptions';
import type { AiModelConfig } from '../../types';
import type { TaskModelDraft, TaskModelDrafts } from './cloudAiSettingsTypes';

export function CloudTaskModelSettings({
  models,
  drafts,
  disabled,
  onChange,
}: {
  models: AiModelConfig[];
  drafts: TaskModelDrafts;
  disabled: boolean;
  onChange: (modelId: string, patch: Partial<TaskModelDraft>) => void;
}) {
  const { t } = useI18n();
  return (
    <section className="rounded-xl border border-border/60 bg-card">
      <div className="border-b border-border/60 px-4 py-3">
        <h4 className="text-sm font-semibold text-foreground">{t('cloudAi.taskModels')}</h4>
        <p className="mt-1 text-xs text-muted-foreground">
          {t('cloudAi.taskModelsDescription')}
        </p>
      </div>
      <div className="space-y-3 p-4">
        {models.length === 0 ? (
          <div className="rounded-lg bg-muted/30 p-4 text-sm text-muted-foreground">
            {t('modelSettings.emptyModels')}
          </div>
        ) : null}
        {models.map((model) => (
          <TaskModelRow
            key={model.id}
            model={model}
            draft={drafts[model.id] || fallbackDraft(model)}
            disabled={disabled}
            onChange={(patch) => onChange(model.id, patch)}
          />
        ))}
      </div>
    </section>
  );
}

function TaskModelRow({
  model,
  draft,
  disabled,
  onChange,
}: {
  model: AiModelConfig;
  draft: TaskModelDraft;
  disabled: boolean;
  onChange: (patch: Partial<TaskModelDraft>) => void;
}) {
  const { t } = useI18n();
  const thinkingOptions = React.useMemo(
    () => thinkingOptionsForProvider(model.provider, t),
    [model.provider, t],
  );
  return (
    <article className={`rounded-lg border border-border/70 p-3 ${draft.enabled ? '' : 'opacity-65'}`}>
      <div className="flex flex-wrap items-start justify-between gap-2">
        <div className="min-w-0">
          <div className="truncate text-sm font-medium text-foreground">{model.name}</div>
          <div className="mt-0.5 truncate text-xs text-muted-foreground">
            {model.provider} · {model.model_name}
          </div>
        </div>
        <button
          type="button"
          disabled={disabled}
          onClick={() => onChange({ enabled: !draft.enabled })}
          className={`rounded-md border px-2 py-1 text-xs transition-colors disabled:opacity-50 ${
            draft.enabled
              ? 'border-green-200 text-green-700 dark:border-green-900 dark:text-green-200'
              : 'border-border text-muted-foreground'
          }`}
        >
          {draft.enabled ? t('taskModelSettings.disable') : t('taskModelSettings.enable')}
        </button>
      </div>
      <div className="mt-3 grid gap-3 lg:grid-cols-[minmax(180px,1.5fr)_minmax(150px,1fr)_120px_130px]">
        <Field label={t('taskModelSettings.usage')}>
          <input
            value={draft.usage}
            disabled={disabled || !draft.enabled}
            onChange={(event) => onChange({ usage: event.target.value })}
            placeholder={t('taskModelSettings.usagePlaceholder')}
            className="cloudAiInput"
          />
        </Field>
        <Field label={t('taskModelSettings.defaultThinkingLevel')}>
          <select
            value={draft.thinking}
            disabled={disabled || !draft.enabled}
            onChange={(event) => onChange({ thinking: event.target.value })}
            className="cloudAiInput"
          >
            {thinkingOptions.map((option) => (
              <option key={option.value || 'default'} value={option.value}>
                {option.label}
              </option>
            ))}
          </select>
        </Field>
        <Field label={t('cloudAi.temperature')}>
          <input
            type="number"
            min="0"
            max="2"
            step="0.1"
            value={draft.temperature}
            disabled={disabled || !draft.enabled}
            onChange={(event) => onChange({ temperature: event.target.value })}
            className="cloudAiInput"
          />
        </Field>
        <Field label={t('cloudAi.maxTokens')}>
          <input
            type="number"
            min="1"
            step="1"
            value={draft.maxOutputTokens}
            disabled={disabled || !draft.enabled}
            onChange={(event) => onChange({ maxOutputTokens: event.target.value })}
            className="cloudAiInput"
          />
        </Field>
      </div>
    </article>
  );
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <label className="block">
      <span className="mb-1 block text-xs text-muted-foreground">{label}</span>
      {children}
    </label>
  );
}

function fallbackDraft(model: AiModelConfig): TaskModelDraft {
  return {
    usage: model.task_usage_scenario || '',
    thinking: model.task_thinking_level || '',
    temperature: model.temperature == null ? '' : String(model.temperature),
    maxOutputTokens: model.max_output_tokens == null ? '' : String(model.max_output_tokens),
    enabled: model.enabled,
  };
}
