// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';

import { useI18n } from '../../i18n/I18nProvider';
import { thinkingOptionsForProvider } from '../../lib/modelThinkingOptions';
import type { AiModelConfig } from '../../types';
import type {
  DefaultModelDraft,
  DefaultModelDrafts,
  DefaultModelSlot,
} from './cloudAiSettingsTypes';

const SLOT_LABEL_KEYS: Record<DefaultModelSlot, string> = {
  memory: 'cloudAi.memoryModel',
  project: 'cloudAi.projectModel',
  environment: 'cloudAi.environmentModel',
};

export function CloudDefaultModelSettings({
  models,
  drafts,
  disabled,
  onChange,
}: {
  models: AiModelConfig[];
  drafts: DefaultModelDrafts;
  disabled: boolean;
  onChange: (slot: DefaultModelSlot, draft: DefaultModelDraft) => void;
}) {
  const { t } = useI18n();
  return (
    <section className="rounded-xl border border-border/60 bg-card">
      <div className="border-b border-border/60 px-4 py-3">
        <h4 className="text-sm font-semibold text-foreground">{t('cloudAi.defaultModels')}</h4>
        <p className="mt-1 text-xs text-muted-foreground">
          {t('cloudAi.defaultModelsDescription')}
        </p>
      </div>
      <div className="grid gap-4 p-4 md:grid-cols-3">
        {(Object.keys(SLOT_LABEL_KEYS) as DefaultModelSlot[]).map((slot) => (
          <DefaultModelField
            key={slot}
            label={t(SLOT_LABEL_KEYS[slot])}
            models={models}
            draft={drafts[slot]}
            disabled={disabled}
            onChange={(draft) => onChange(slot, draft)}
          />
        ))}
      </div>
    </section>
  );
}

function DefaultModelField({
  label,
  models,
  draft,
  disabled,
  onChange,
}: {
  label: string;
  models: AiModelConfig[];
  draft: DefaultModelDraft;
  disabled: boolean;
  onChange: (draft: DefaultModelDraft) => void;
}) {
  const { t } = useI18n();
  const selected = models.find((model) => model.id === draft.modelId) || null;
  const thinkingOptions = React.useMemo(
    () => thinkingOptionsForProvider(selected?.provider, t),
    [selected?.provider, t],
  );
  const selectedIsMissing = Boolean(draft.modelId) && !selected;

  return (
    <div className="space-y-3 rounded-lg bg-muted/25 p-3">
      <label className="block">
        <span className="mb-1 block text-xs font-medium text-foreground">{label}</span>
        <select
          value={draft.modelId}
          disabled={disabled}
          onChange={(event) => onChange({ modelId: event.target.value, thinking: '' })}
          className="w-full rounded-md border border-input bg-background px-3 py-2 text-sm text-foreground focus:outline-none focus:ring-2 focus:ring-ring disabled:opacity-60"
        >
          <option value="">{t('modelSettings.none')}</option>
          {selectedIsMissing ? <option value={draft.modelId}>{draft.modelId}</option> : null}
          {models.map((model) => (
            <option key={model.id} value={model.id}>
              {`${model.name} · ${model.model_name}`}
            </option>
          ))}
        </select>
      </label>
      <label className="block">
        <span className="mb-1 block text-xs text-muted-foreground">
          {t('modelSettings.thinkingLevel')}
        </span>
        <select
          value={draft.thinking}
          disabled={disabled || !draft.modelId}
          onChange={(event) => onChange({ ...draft, thinking: event.target.value })}
          className="w-full rounded-md border border-input bg-background px-3 py-2 text-sm text-foreground focus:outline-none focus:ring-2 focus:ring-ring disabled:opacity-60"
        >
          {thinkingOptions.map((option) => (
            <option key={option.value || 'default'} value={option.value}>
              {option.label}
            </option>
          ))}
        </select>
      </label>
    </div>
  );
}
