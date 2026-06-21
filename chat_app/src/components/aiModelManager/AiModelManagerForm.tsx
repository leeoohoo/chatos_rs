import { useI18n } from '../../i18n/I18nProvider';
import {
  AI_MODEL_PROVIDERS,
  AI_MODEL_THINKING_LEVELS,
  applyProviderChange,
} from './helpers';
import type { AiModelManagerFormProps } from './types';

const AiModelManagerForm = ({
  editingConfig,
  formData,
  showTitle = true,
  onSubmit,
  onCancel,
  onFormDataChange,
}: AiModelManagerFormProps) => {
  const { t } = useI18n();

  return (
    <form onSubmit={onSubmit} className="space-y-4">
      {showTitle ? (
        <h3 className="text-lg font-medium text-foreground">
          {editingConfig ? t('aiModelManager.form.title.edit') : t('aiModelManager.form.title.create')}
        </h3>
      ) : null}

      <div className="space-y-4 rounded-xl border border-border bg-muted/40 p-4">
        <div>
          <label className="block text-sm font-medium text-foreground mb-2">{t('aiModelManager.form.name')}</label>
          <input
            type="text"
            value={formData.name}
            onChange={(event) => onFormDataChange({ name: event.target.value })}
            className="w-full px-3 py-2 border border-input bg-background text-foreground rounded-md focus:outline-none focus:ring-2 focus:ring-ring"
            placeholder={t('aiModelManager.form.namePlaceholder')}
            autoFocus
            required
          />
        </div>

        <div>
          <label className="block text-sm font-medium text-foreground mb-2">{t('aiModelManager.form.provider')}</label>
          <select
            value={formData.provider}
            onChange={(event) =>
              onFormDataChange(applyProviderChange(formData, event.target.value))
            }
            className="w-full px-3 py-2 border border-input bg-background text-foreground rounded-md focus:outline-none focus:ring-2 focus:ring-ring"
          >
            {AI_MODEL_PROVIDERS.map((provider) => (
              <option key={provider} value={provider}>
                {provider}
              </option>
            ))}
          </select>
        </div>

        <div>
          <label className="block text-sm font-medium text-foreground mb-2">{t('aiModelManager.form.baseUrl')}</label>
          <input
            type="url"
            value={formData.base_url}
            onChange={(event) => onFormDataChange({ base_url: event.target.value })}
            className="w-full px-3 py-2 border border-input bg-background text-foreground rounded-md focus:outline-none focus:ring-2 focus:ring-ring"
            placeholder={t('aiModelManager.form.baseUrlPlaceholder')}
            required
          />
        </div>

        <div>
          <label className="block text-sm font-medium text-foreground mb-2">{t('aiModelManager.form.apiKey')}</label>
          <input
            type="password"
            value={formData.api_key}
            onChange={(event) => onFormDataChange({
              api_key: event.target.value,
              clear_api_key: false,
            })}
            disabled={editingConfig !== null && formData.clear_api_key}
            className="w-full px-3 py-2 border border-input bg-background text-foreground rounded-md focus:outline-none focus:ring-2 focus:ring-ring"
            placeholder={editingConfig
              ? t('aiModelManager.form.apiKeyPlaceholderEdit')
              : t('aiModelManager.form.apiKeyPlaceholder')}
            required={editingConfig === null}
          />
          {editingConfig && formData.has_stored_api_key ? (
            <div className="mt-2 space-y-2">
              <p className="text-xs text-muted-foreground">
                {t('aiModelManager.form.apiKeyHintKeep')}
              </p>
              <label className="flex items-center text-sm text-foreground">
                <input
                  type="checkbox"
                  checked={formData.clear_api_key}
                  onChange={(event) => onFormDataChange({
                    clear_api_key: event.target.checked,
                    api_key: event.target.checked ? '' : formData.api_key,
                  })}
                  className="h-4 w-4 text-blue-600 focus:ring-blue-500 border-gray-300 rounded"
                />
                <span className="ml-2">{t('aiModelManager.form.clearSavedApiKey')}</span>
              </label>
            </div>
          ) : null}
        </div>

        <div>
          <label className="block text-sm font-medium text-foreground mb-2">{t('aiModelManager.form.modelName')}</label>
          <input
            type="text"
            value={formData.model_name}
            onChange={(event) => onFormDataChange({ model_name: event.target.value })}
            className="w-full px-3 py-2 border border-input bg-background text-foreground rounded-md focus:outline-none focus:ring-2 focus:ring-ring"
            placeholder={t('aiModelManager.form.modelNamePlaceholder')}
          />
          <p className="mt-2 text-xs text-muted-foreground">
            Optional. Leave blank to pick the concrete runtime model later in the chat composer.
          </p>
        </div>

        <div>
          <label className="block text-sm font-medium text-foreground mb-2">{t('aiModelManager.form.thinkingLevel')}</label>
          <select
            value={formData.thinking_level}
            onChange={(event) => onFormDataChange({ thinking_level: event.target.value })}
            disabled={formData.provider !== 'gpt'}
            className="w-full px-3 py-2 border border-input bg-background text-foreground rounded-md focus:outline-none focus:ring-2 focus:ring-ring disabled:opacity-60"
          >
            {AI_MODEL_THINKING_LEVELS.map((level) => (
              <option key={level || 'empty'} value={level}>
                {level || t('aiModelManager.form.thinkingLevelAuto')}
              </option>
            ))}
          </select>
        </div>

        <div className="flex items-center">
          <input
            type="checkbox"
            id="enabled"
            checked={formData.enabled}
            onChange={(event) => onFormDataChange({ enabled: event.target.checked })}
            className="h-4 w-4 text-blue-600 focus:ring-blue-500 border-gray-300 rounded"
          />
          <label htmlFor="enabled" className="ml-2 block text-sm text-foreground">
            {t('aiModelManager.form.enabled')}
          </label>
        </div>

        <div className="flex items-center">
          <input
            type="checkbox"
            id="supports_images"
            checked={formData.supports_images}
            onChange={(event) => onFormDataChange({ supports_images: event.target.checked })}
            className="h-4 w-4 text-blue-600 focus:ring-blue-500 border-gray-300 rounded"
          />
          <label htmlFor="supports_images" className="ml-2 block text-sm text-foreground">
            {t('aiModelManager.form.supportsImages')}
          </label>
        </div>

        <div className="flex items-center">
          <input
            type="checkbox"
            id="supports_reasoning"
            checked={formData.supports_reasoning}
            onChange={(event) => onFormDataChange({ supports_reasoning: event.target.checked })}
            className="h-4 w-4 text-blue-600 focus:ring-blue-500 border-gray-300 rounded"
          />
          <label htmlFor="supports_reasoning" className="ml-2 block text-sm text-foreground">
            {t('aiModelManager.form.supportsReasoning')}
          </label>
        </div>

        <div className="flex items-center">
          <input
            type="checkbox"
            id="supports_responses"
            checked={formData.supports_responses}
            onChange={(event) => onFormDataChange({ supports_responses: event.target.checked })}
            className="h-4 w-4 text-blue-600 focus:ring-blue-500 border-gray-300 rounded"
          />
          <label htmlFor="supports_responses" className="ml-2 block text-sm text-foreground">
            {t('aiModelManager.form.supportsResponses')}
          </label>
        </div>
      </div>

      <div className="flex items-center justify-end gap-2">
        <button
          type="button"
          onClick={onCancel}
          className="rounded-lg bg-muted px-3 py-2 text-sm transition-colors hover:bg-accent"
        >
          {t('common.cancel')}
        </button>
        <button
          type="submit"
          className="rounded-lg bg-primary px-3 py-2 text-sm text-primary-foreground transition-opacity hover:opacity-90"
        >
          {editingConfig ? t('aiModelManager.form.submitEdit') : t('aiModelManager.form.submitCreate')}
        </button>
      </div>
    </form>
  );
};

export default AiModelManagerForm;
