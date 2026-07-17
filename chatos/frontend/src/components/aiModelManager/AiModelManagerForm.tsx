// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { Eye, EyeOff, Loader2, RefreshCw } from 'lucide-react';

import { useI18n } from '../../i18n/I18nProvider';
import {
  AI_MODEL_PROVIDERS,
  AGENT_PROMPT_VENDORS,
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
  apiKeyVisible = false,
  apiKeyLoading = false,
  refreshingModels = false,
  onToggleApiKeyVisible,
  onRefreshModels,
}: AiModelManagerFormProps) => {
  const { t } = useI18n();
  const canRefreshModels = Boolean(
    editingConfig
      && formData.name.trim()
      && formData.base_url.trim()
      && !formData.clear_api_key
      && (formData.has_stored_api_key || formData.api_key.trim()),
  );

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
          <label className="block text-sm font-medium text-foreground mb-2">
            {t('aiModelManager.form.promptVendor')}
          </label>
          <select
            value={formData.prompt_vendor}
            onChange={(event) => onFormDataChange({
              prompt_vendor: event.target.value as typeof formData.prompt_vendor,
            })}
            className="w-full px-3 py-2 border border-input bg-background text-foreground rounded-md focus:outline-none focus:ring-2 focus:ring-ring"
          >
            {AGENT_PROMPT_VENDORS.map((vendor) => (
              <option key={vendor} value={vendor}>{vendor}</option>
            ))}
          </select>
          <p className="mt-1 text-xs text-muted-foreground">
            {t('aiModelManager.form.promptVendorHint')}
          </p>
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
          <div className="relative">
            <input
              type={apiKeyVisible ? 'text' : 'password'}
              value={formData.api_key}
              onChange={(event) => onFormDataChange({
                api_key: event.target.value,
                clear_api_key: false,
              })}
              disabled={editingConfig !== null && formData.clear_api_key}
              className="w-full rounded-md border border-input bg-background py-2 pl-3 pr-11 text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
              placeholder={editingConfig
                ? t('aiModelManager.form.apiKeyPlaceholderEdit')
                : t('aiModelManager.form.apiKeyPlaceholder')}
              required={editingConfig === null}
            />
            <button
              type="button"
              onClick={onToggleApiKeyVisible}
              disabled={apiKeyLoading || (editingConfig !== null && formData.clear_api_key)}
              className="absolute right-2 top-1/2 -translate-y-1/2 rounded-md p-1.5 text-muted-foreground transition-colors hover:bg-accent hover:text-foreground disabled:opacity-50"
              title={apiKeyVisible ? t('aiModelManager.form.hideApiKey') : t('aiModelManager.form.showApiKey')}
              aria-label={apiKeyVisible ? t('aiModelManager.form.hideApiKey') : t('aiModelManager.form.showApiKey')}
            >
              {apiKeyLoading ? (
                <Loader2 className="h-4 w-4 animate-spin" />
              ) : apiKeyVisible ? (
                <EyeOff className="h-4 w-4" />
              ) : (
                <Eye className="h-4 w-4" />
              )}
            </button>
          </div>
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
        {editingConfig ? (
          <button
            type="button"
            onClick={onRefreshModels}
            disabled={refreshingModels || !canRefreshModels}
            className="mr-auto inline-flex items-center gap-2 rounded-lg border border-border bg-background px-3 py-2 text-sm transition-colors hover:bg-accent disabled:opacity-50"
          >
            {refreshingModels ? (
              <Loader2 className="h-4 w-4 animate-spin" />
            ) : (
              <RefreshCw className="h-4 w-4" />
            )}
            {refreshingModels ? t('aiModelManager.form.refreshingModels') : t('aiModelManager.form.refreshModels')}
          </button>
        ) : null}
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
