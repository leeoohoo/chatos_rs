import React, { useEffect, useMemo, useState } from 'react';

import { useApiClient } from '../lib/api/ApiClientContext';
import { useChatStoreResolved } from '../lib/store/ChatStoreContext';
import { useI18n } from '../i18n/I18nProvider';
import { thinkingOptionsForProvider } from '../lib/modelThinkingOptions';
import ManagerFormDialog from './ui/ManagerFormDialog';

interface Props {
  onClose: () => void;
}

const MemoryModelSettingsPanel: React.FC<Props> = ({ onClose }) => {
  const { t } = useI18n();
  const client = useApiClient();
  const { aiModelConfigs, loadAiModelConfigs } = useChatStoreResolved();
  const [selectedModelId, setSelectedModelId] = useState('');
  const [thinkingLevel, setThinkingLevel] = useState('');
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    Promise.all([
      loadAiModelConfigs({ force: true }),
      client.getAiModelSettings(),
    ])
      .then(([, settings]) => {
        if (cancelled) {
          return;
        }
        setSelectedModelId(settings.memory_summary_model_config_id || '');
        setThinkingLevel(settings.memory_summary_thinking_level || '');
      })
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
  }, [client, loadAiModelConfigs, t]);

  const selectedModel = useMemo(
    () => aiModelConfigs.find((item) => item.id === selectedModelId) || null,
    [aiModelConfigs, selectedModelId],
  );
  const concreteModels = useMemo(
    () => aiModelConfigs.filter((item) => item.enabled && item.model_name.trim()),
    [aiModelConfigs],
  );
  const thinkingOptions = useMemo(
    () => thinkingOptionsForProvider(selectedModel?.provider, t),
    [selectedModel?.provider, t],
  );

  useEffect(() => {
    if (!thinkingOptions.some((item) => item.value === thinkingLevel)) {
      setThinkingLevel('');
    }
  }, [thinkingLevel, thinkingOptions]);

  const save = async () => {
    setSaving(true);
    setError(null);
    try {
      await client.updateAiModelSettings({
        memory_summary_model_config_id: selectedModelId || null,
        memory_summary_thinking_level: thinkingLevel || null,
      });
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
      title={t('memoryModelSettings.title')}
      description={t('memoryModelSettings.description')}
      widthClassName="max-w-xl"
      onClose={onClose}
    >
      <div className="space-y-4">
        {error ? (
          <div className="rounded-lg border border-red-200 bg-red-50 px-3 py-2 text-sm text-red-700 dark:border-red-900 dark:bg-red-950/40 dark:text-red-200">
            {error}
          </div>
        ) : null}

        <div>
          <label className="mb-2 block text-sm font-medium text-foreground">
            {t('memoryModelSettings.model')}
          </label>
          <select
            value={selectedModelId}
            onChange={(event) => setSelectedModelId(event.target.value)}
            disabled={loading}
            className="w-full rounded-md border border-input bg-background px-3 py-2 text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
          >
            <option value="">{t('modelSettings.none')}</option>
            {concreteModels.map((model) => (
              <option key={model.id} value={model.id}>
                {`${model.name} | ${model.provider} | ${model.model_name}`}
              </option>
            ))}
          </select>
        </div>

        <div>
          <label className="mb-2 block text-sm font-medium text-foreground">
            {t('modelSettings.thinkingLevel')}
          </label>
          <select
            value={thinkingLevel}
            onChange={(event) => setThinkingLevel(event.target.value)}
            disabled={loading || !selectedModelId}
            className="w-full rounded-md border border-input bg-background px-3 py-2 text-foreground focus:outline-none focus:ring-2 focus:ring-ring disabled:opacity-60"
          >
            {thinkingOptions.map((option) => (
              <option key={option.value || 'default'} value={option.value}>
                {option.label}
              </option>
            ))}
          </select>
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

export default MemoryModelSettingsPanel;
