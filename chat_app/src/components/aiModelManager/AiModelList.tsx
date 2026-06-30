import { useI18n } from '../../i18n/I18nProvider';
import type { AiModelListProps } from './types';
import { BrainIcon, PencilIcon, TrashIcon } from './icons';

const AiModelList = ({
  aiModelConfigs,
  onToggleEnabled,
  onEdit,
  onDelete,
}: AiModelListProps) => {
  const { t } = useI18n();
  if (aiModelConfigs.length === 0) {
    return (
      <div className="text-center py-8 text-muted-foreground">
        <BrainIcon />
        <p className="mt-2">{t('aiModelManager.emptyTitle')}</p>
        <p className="text-sm">{t('aiModelManager.emptyDescription')}</p>
      </div>
    );
  }

  return (
    <>
      {aiModelConfigs.map((config) => (
        <div
          key={config.id}
          className="flex items-center justify-between gap-3 p-4 bg-card border border-border rounded-lg hover:shadow-md transition-shadow"
        >
          <div className="flex items-center space-x-3 flex-1 min-w-0">
            <div className={`w-3 h-3 rounded-full ${config.enabled ? 'bg-green-500' : 'bg-gray-400'}`} />
            <div className="min-w-0 flex-1">
              <h4 className="font-medium text-foreground truncate" title={config.name}>
                {config.name}
              </h4>
              <p
                className="text-xs sm:text-sm text-muted-foreground truncate"
                title={config.base_url}
              >
                {config.base_url}
              </p>
              {(config.provider
                || config.supports_images
                || config.supports_reasoning
                || config.supports_responses
                || config.last_sync_status) && (
                <div className="mt-1 flex flex-wrap gap-2 text-[11px] text-muted-foreground">
                  {config.provider && (
                    <span className="rounded bg-accent px-1.5 py-0.5">{config.provider}</span>
                  )}
                  <span className="rounded bg-accent px-1.5 py-0.5">
                    {t('aiModelManager.badge.importedModels', { count: config.imported_model_count })}
                  </span>
                  {config.last_sync_status && (
                    <span className="rounded bg-accent px-1.5 py-0.5">
                      {t('aiModelManager.badge.syncStatus', { value: config.last_sync_status })}
                    </span>
                  )}
                  {config.supports_images && (
                    <span className="rounded bg-accent px-1.5 py-0.5">{t('aiModelManager.badge.images')}</span>
                  )}
                  {config.supports_reasoning && (
                    <span className="rounded bg-accent px-1.5 py-0.5">{t('aiModelManager.badge.reasoning')}</span>
                  )}
                  {config.supports_responses && (
                    <span className="rounded bg-accent px-1.5 py-0.5">{t('aiModelManager.badge.responses')}</span>
                  )}
                </div>
              )}
              {config.last_sync_error && (
                <p
                  className="mt-1 text-xs text-red-600 dark:text-red-400 truncate"
                  title={config.last_sync_error}
                >
                  {config.last_sync_error}
                </p>
              )}
            </div>
          </div>

          <div className="flex items-center space-x-2 shrink-0">
            <button
              type="button"
              onClick={() => void onToggleEnabled(config)}
              className={`px-3 py-1 text-xs rounded-full transition-colors ${
                config.enabled
                  ? 'bg-green-100 text-green-800 hover:bg-green-200 dark:bg-green-900 dark:text-green-200'
                  : 'bg-secondary text-secondary-foreground hover:bg-secondary/80'
              }`}
            >
              {config.enabled ? t('aiModelManager.status.enabled') : t('aiModelManager.status.disabled')}
            </button>

            <button
              type="button"
              onClick={() => onEdit(config)}
              className="p-2 text-muted-foreground hover:text-blue-600 hover:bg-blue-50 dark:hover:bg-blue-900 rounded transition-colors"
              title={t('aiModelManager.action.edit')}
              aria-label={t('aiModelManager.action.edit')}
            >
              <PencilIcon />
            </button>

            <button
              type="button"
              onClick={() => void onDelete(config.id)}
              className="p-2 text-muted-foreground hover:text-red-600 hover:bg-red-50 dark:hover:bg-red-900 rounded transition-colors"
              title={t('aiModelManager.action.delete')}
              aria-label={t('aiModelManager.action.delete')}
            >
              <TrashIcon />
            </button>
          </div>
        </div>
      ))}
    </>
  );
};

export default AiModelList;
