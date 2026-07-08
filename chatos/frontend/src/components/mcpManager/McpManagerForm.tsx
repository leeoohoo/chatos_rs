// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useI18n } from '../../i18n/I18nProvider';
import DynamicConfigFields from './DynamicConfigFields';
import type { McpManagerFormProps } from './types';

const McpManagerForm = ({
  editingConfig,
  formData,
  dynamicConfig,
  configLoading,
  configError,
  showTitle = true,
  onSubmit,
  onCancel,
  onFormDataChange,
  onFetchDynamicConfig,
  onDynamicConfigChange,
}: McpManagerFormProps) => {
  const { t } = useI18n();

  return (
    <form onSubmit={onSubmit} className="space-y-4">
      {showTitle ? (
        <h3 className="text-lg font-medium text-foreground">
          {editingConfig ? t('mcpManager.form.title.edit') : t('mcpManager.form.title.create')}
        </h3>
      ) : null}

      <div className="space-y-4 rounded-xl border border-border bg-muted/40 p-4">
        <div>
          <label className="block text-sm font-medium text-foreground mb-2">{t('mcpManager.form.name')}</label>
          <input
            type="text"
            value={formData.name}
            onChange={(event) => onFormDataChange({ name: event.target.value })}
            className="w-full px-3 py-2 border border-input bg-background text-foreground rounded-md focus:outline-none focus:ring-2 focus:ring-ring"
            placeholder={t('mcpManager.form.namePlaceholder')}
            autoFocus
            required
          />
        </div>

        <div>
          <label className="block text-sm font-medium text-foreground mb-2">{t('mcpManager.form.protocol')}</label>
          <select
            value={formData.type}
            onChange={(event) => onFormDataChange({ type: event.target.value as 'http' | 'stdio' })}
            className="w-full px-3 py-2 border border-input bg-background text-foreground rounded-md focus:outline-none focus:ring-2 focus:ring-ring"
          >
            <option value="stdio">{t('mcpManager.form.stdioLabel')}</option>
            <option value="http">{t('mcpManager.form.httpLabel')}</option>
          </select>
        </div>

        <div>
          <label className="block text-sm font-medium text-foreground mb-2">
            {formData.type === 'http' ? t('mcpManager.form.url') : t('mcpManager.form.command')}
          </label>
          <input
            type="text"
            value={formData.command}
            onChange={(event) => onFormDataChange({ command: event.target.value })}
            className="w-full px-3 py-2 border border-input bg-background text-foreground rounded-md focus:outline-none focus:ring-2 focus:ring-ring"
            placeholder={
              formData.type === 'http'
                ? 'https://api.example.com/mcp'
                : 'npx @modelcontextprotocol/server-filesystem /path/to/allowed/files'
            }
            required
          />
        </div>

        {formData.type === 'stdio' && (
          <div className="space-y-3">
            <div>
              <label className="block text-sm font-medium text-foreground mb-2">
                {t('mcpManager.form.cwd')}
              </label>
              <input
                type="text"
                value={formData.cwd}
                onChange={(event) => onFormDataChange({ cwd: event.target.value })}
                className="w-full px-3 py-2 border border-input bg-background text-foreground rounded-md focus:outline-none focus:ring-2 focus:ring-ring"
                placeholder="/absolute/path/to/executable"
              />
              <p className="mt-1 text-xs text-muted-foreground">{t('mcpManager.form.cwdHelp')}</p>
              <div className="mt-2 flex gap-2">
                <button
                  type="button"
                  className="px-3 py-1 text-xs bg-secondary text-secondary-foreground rounded-md hover:bg-secondary/80"
                  onClick={() => void onFetchDynamicConfig()}
                >
                  {configLoading ? t('mcpManager.form.fetchingConfig') : t('mcpManager.form.fetchConfig')}
                </button>
              </div>
            </div>

            <div>
              <label className="block text-sm font-medium text-foreground mb-2">
                {t('mcpManager.form.args')}
              </label>
              <input
                type="text"
                value={formData.argsInput}
                onChange={(event) => onFormDataChange({ argsInput: event.target.value })}
                className="w-full px-3 py-2 border border-input bg-background text-foreground rounded-md focus:outline-none focus:ring-2 focus:ring-ring"
                placeholder="--flag, value, --opt"
              />
              <p className="mt-1 text-xs text-muted-foreground">{t('mcpManager.form.argsHelp')}</p>
            </div>
          </div>
        )}

        {formData.type === 'stdio' && Object.keys(dynamicConfig).length > 0 && (
          <div className="mt-4 p-3 border border-border rounded-md bg-card">
            <div className="flex items-center justify-between mb-2">
              <span className="text-sm font-medium text-foreground">
                {t('mcpManager.form.dynamicConfig')}
              </span>
            </div>
            {configError && <p className="text-xs text-red-600 mb-2">{configError}</p>}
            <DynamicConfigFields config={dynamicConfig} onChange={onDynamicConfigChange} />
          </div>
        )}
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
          {editingConfig ? t('mcpManager.form.submitEdit') : t('mcpManager.form.submitCreate')}
        </button>
      </div>
    </form>
  );
};

export default McpManagerForm;
