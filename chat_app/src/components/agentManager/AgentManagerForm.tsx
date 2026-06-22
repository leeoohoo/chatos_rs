import type { FormEvent } from 'react';

import { useI18n } from '../../i18n/I18nProvider';
import type { AgentFormData } from './types';

interface AgentManagerFormProps {
  editingAgentId: string | null;
  formData: AgentFormData;
  pluginOptions: Array<{ value: string; label: string }>;
  skillOptions: Array<{ value: string; label: string }>;
  showTitle?: boolean;
  onSubmit: (event: FormEvent<HTMLFormElement>) => Promise<void>;
  onCancel: () => void;
  onFormDataChange: (patch: Partial<AgentFormData>) => void;
}

const AgentManagerForm = ({
  editingAgentId,
  formData,
  pluginOptions,
  skillOptions,
  showTitle = true,
  onSubmit,
  onCancel,
  onFormDataChange,
}: AgentManagerFormProps) => {
  const { t } = useI18n();

  return (
    <form onSubmit={(event) => void onSubmit(event)} className="space-y-4">
      {showTitle ? (
        <h3 className="text-sm font-semibold text-foreground">
          {editingAgentId ? t('agentManager.form.titleEdit') : t('agentManager.form.titleCreate')}
        </h3>
      ) : null}

      <div className="space-y-4 rounded-xl border border-border bg-background/40 p-4">
        <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
          <label className="space-y-1">
            <span className="text-xs text-muted-foreground">{t('agentManager.form.name')}</span>
            <input
              value={formData.name}
              onChange={(event) => onFormDataChange({ name: event.target.value })}
              className="w-full rounded-lg border border-border bg-background px-3 py-2 text-sm outline-none focus:ring-2 focus:ring-primary/30"
              placeholder={t('agentManager.form.namePlaceholder')}
              autoFocus
            />
          </label>
          <label className="space-y-1">
            <span className="text-xs text-muted-foreground">{t('agentManager.form.category')}</span>
            <input
              value={formData.category}
              onChange={(event) => onFormDataChange({ category: event.target.value })}
              className="w-full rounded-lg border border-border bg-background px-3 py-2 text-sm outline-none focus:ring-2 focus:ring-primary/30"
              placeholder={t('agentManager.form.categoryPlaceholder')}
            />
          </label>
        </div>

        <label className="block space-y-1">
          <span className="text-xs text-muted-foreground">{t('agentManager.form.description')}</span>
          <input
            value={formData.description}
            onChange={(event) => onFormDataChange({ description: event.target.value })}
            className="w-full rounded-lg border border-border bg-background px-3 py-2 text-sm outline-none focus:ring-2 focus:ring-primary/30"
            placeholder={t('agentManager.form.descriptionPlaceholder')}
          />
        </label>

        <label className="block space-y-1">
          <span className="text-xs text-muted-foreground">{t('agentManager.form.roleDefinition')}</span>
          <textarea
            value={formData.roleDefinition}
            onChange={(event) => onFormDataChange({ roleDefinition: event.target.value })}
            rows={5}
            className="w-full rounded-lg border border-border bg-background px-3 py-2 text-sm outline-none focus:ring-2 focus:ring-primary/30"
            placeholder={t('agentManager.form.roleDefinitionPlaceholder')}
          />
        </label>

        <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
          <label className="block space-y-1">
            <span className="text-xs text-muted-foreground">{t('agentManager.form.pluginSources')}</span>
            <select
              multiple
              value={formData.pluginSources}
              onChange={(event) => {
                const values = Array.from(event.currentTarget.selectedOptions).map((option) => option.value);
                onFormDataChange({ pluginSources: values });
              }}
              className="w-full min-h-36 rounded-lg border border-border bg-background px-3 py-2 text-sm outline-none focus:ring-2 focus:ring-primary/30"
            >
              {pluginOptions.map((option) => (
                <option key={option.value} value={option.value}>
                  {option.label}
                </option>
              ))}
            </select>
          </label>

          <label className="block space-y-1">
            <span className="text-xs text-muted-foreground">{t('agentManager.form.skillIds')}</span>
            <select
              multiple
              value={formData.skillIds}
              onChange={(event) => {
                const values = Array.from(event.currentTarget.selectedOptions).map((option) => option.value);
                onFormDataChange({ skillIds: values });
              }}
              className="w-full min-h-36 rounded-lg border border-border bg-background px-3 py-2 text-sm outline-none focus:ring-2 focus:ring-primary/30"
            >
              {skillOptions.map((option) => (
                <option key={option.value} value={option.value}>
                  {option.label}
                </option>
              ))}
            </select>
          </label>
        </div>

        <label className="inline-flex items-center gap-2 text-sm text-foreground">
          <input
            type="checkbox"
            checked={formData.enabled}
            onChange={(event) => onFormDataChange({ enabled: event.target.checked })}
            className="rounded border-border"
          />
          {t('agentManager.form.enabled')}
        </label>
      </div>

      <div className="flex items-center justify-end gap-2">
        <button
          type="button"
          onClick={onCancel}
          className="px-3 py-2 text-sm rounded-lg bg-muted hover:bg-accent transition-colors"
        >
          {t('common.cancel')}
        </button>
        <button
          type="submit"
          className="px-3 py-2 text-sm rounded-lg bg-primary text-primary-foreground hover:opacity-90 transition-opacity"
        >
          {editingAgentId ? t('agentManager.form.submitEdit') : t('agentManager.form.submitCreate')}
        </button>
      </div>
    </form>
  );
};

export default AgentManagerForm;
