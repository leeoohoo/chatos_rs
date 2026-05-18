import { useI18n } from '../../i18n/I18nProvider';
import DynamicConfigFields from './DynamicConfigFields';
import type { McpManagerFormProps } from './types';

const McpManagerForm = ({
  showAddForm,
  editingConfig,
  formData,
  dynamicConfig,
  configLoading,
  configError,
  onCreate,
  onSubmit,
  onCancel,
  onFormDataChange,
  onFetchDynamicConfig,
  onDynamicConfigChange,
}: McpManagerFormProps) => {
  const { t } = useI18n();
  if (!showAddForm) {
    return (
      <button
        onClick={onCreate}
        className="w-full mb-6 p-4 border-2 border-dashed border-border rounded-lg hover:border-blue-500 transition-colors flex items-center justify-center space-x-2 text-muted-foreground hover:text-blue-600"
      >
        <span>+ {t('mcpManager.form.createButton')}</span>
      </button>
    );
  }

  return (
    <form onSubmit={onSubmit} className="mb-6 p-4 bg-muted rounded-lg">
      <h3 className="text-lg font-medium text-foreground mb-4">
        {editingConfig ? t('mcpManager.form.title.edit') : t('mcpManager.form.title.create')}
      </h3>

      <div className="space-y-4">
        <div>
          <label className="block text-sm font-medium text-foreground mb-2">{t('mcpManager.form.name')}</label>
          <input
            type="text"
            value={formData.name}
            onChange={(event) => onFormDataChange({ name: event.target.value })}
            className="w-full px-3 py-2 border border-input bg-background text-foreground rounded-md focus:outline-none focus:ring-2 focus:ring-ring"
            placeholder={t('mcpManager.form.namePlaceholder')}
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

      <div className="flex space-x-3 mt-6">
        <button
          type="submit"
          className="px-4 py-2 bg-blue-600 text-white rounded-md hover:bg-blue-700 focus:outline-none focus:ring-2 focus:ring-blue-500"
        >
          {editingConfig ? t('mcpManager.form.submitEdit') : t('mcpManager.form.submitCreate')}
        </button>
        <button
          type="button"
          onClick={onCancel}
          className="px-4 py-2 bg-secondary text-secondary-foreground rounded-md hover:bg-secondary/80 focus:outline-none focus:ring-2 focus:ring-ring"
        >
          {t('common.cancel')}
        </button>
      </div>
    </form>
  );
};

export default McpManagerForm;
