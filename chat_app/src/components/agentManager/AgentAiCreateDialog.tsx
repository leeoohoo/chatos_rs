import { useI18n } from '../../i18n/I18nProvider';
import type { AiModelConfig } from '../../types';
import type { AgentAiCreateFormData } from './types';

interface AgentAiCreateDialogProps {
  open: boolean;
  formData: AgentAiCreateFormData;
  modelOptions: AiModelConfig[];
  onChange: (patch: Partial<AgentAiCreateFormData>) => void;
  onCancel: () => void;
  onSubmit: () => Promise<void>;
}

const AgentAiCreateDialog = ({
  open,
  formData,
  modelOptions,
  onChange,
  onCancel,
  onSubmit,
}: AgentAiCreateDialogProps) => {
  const { t } = useI18n();
  if (!open) {
    return null;
  }

  return (
    <>
      <div className="fixed inset-0 bg-black/40 backdrop-blur-sm z-[60]" onClick={onCancel} />
      <div className="fixed inset-0 z-[61] flex items-center justify-center p-4">
        <div className="w-full max-w-2xl rounded-xl border border-border bg-card shadow-2xl">
          <div className="flex items-center justify-between p-4 border-b border-border">
            <h3 className="text-base font-semibold text-foreground">{t('agentManager.aiDialog.title')}</h3>
            <button
              onClick={onCancel}
              className="p-2 text-muted-foreground hover:text-foreground hover:bg-accent rounded-lg transition-colors"
            >
              {t('common.close')}
            </button>
          </div>

          <div className="p-4 space-y-4">
            <label className="space-y-1 block">
              <span className="text-xs text-muted-foreground">{t('agentManager.form.name')}</span>
              <input
                value={formData.name}
                onChange={(event) => onChange({ name: event.target.value })}
                className="w-full rounded-lg border border-border bg-background px-3 py-2 text-sm outline-none focus:ring-2 focus:ring-primary/30"
                placeholder={t('agentManager.form.namePlaceholder')}
                required
              />
            </label>

            <label className="space-y-1 block">
              <span className="text-xs text-muted-foreground">{t('agentManager.aiDialog.requirement')}</span>
              <textarea
                value={formData.requirement}
                onChange={(event) => onChange({ requirement: event.target.value })}
                rows={6}
                className="w-full rounded-lg border border-border bg-background px-3 py-2 text-sm outline-none focus:ring-2 focus:ring-primary/30"
                placeholder={t('agentManager.aiDialog.requirementPlaceholder')}
              />
            </label>

            <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
              <label className="space-y-1 block">
                <span className="text-xs text-muted-foreground">{t('agentManager.aiDialog.model')}</span>
                <select
                  value={formData.modelConfigId}
                  onChange={(event) => onChange({ modelConfigId: event.target.value })}
                  className="w-full rounded-lg border border-border bg-background px-3 py-2 text-sm outline-none focus:ring-2 focus:ring-primary/30"
                >
                  <option value="">{t('agentManager.aiDialog.modelAuto')}</option>
                  {modelOptions.map((item) => (
                    <option key={item.id} value={item.id}>
                      {[item.name, item.provider, item.model_name].filter(Boolean).join(' | ')}
                    </option>
                  ))}
                </select>
              </label>

              <label className="space-y-1 block">
                <span className="text-xs text-muted-foreground">{t('agentManager.form.category')}</span>
                <input
                  value={formData.category}
                  onChange={(event) => onChange({ category: event.target.value })}
                  className="w-full rounded-lg border border-border bg-background px-3 py-2 text-sm outline-none focus:ring-2 focus:ring-primary/30"
                  placeholder="e.g. coding / research / ops"
                />
              </label>
            </div>

            <label className="inline-flex items-center gap-2 text-sm text-foreground">
              <input
                type="checkbox"
                checked={formData.enabled}
                onChange={(event) => onChange({ enabled: event.target.checked })}
                className="rounded border-border"
              />
              {t('agentManager.aiDialog.enableAfterCreate')}
            </label>
          </div>

          <div className="flex items-center justify-end gap-2 p-4 border-t border-border">
            <button
              onClick={onCancel}
              className="px-3 py-2 text-sm rounded-lg bg-muted hover:bg-accent transition-colors"
            >
              {t('common.cancel')}
            </button>
            <button
              onClick={() => {
                void onSubmit();
              }}
              className="px-3 py-2 text-sm rounded-lg bg-primary text-primary-foreground hover:opacity-90 transition-opacity"
            >
              {t('agentManager.aiDialog.submit')}
            </button>
          </div>
        </div>
      </div>
    </>
  );
};

export default AgentAiCreateDialog;
