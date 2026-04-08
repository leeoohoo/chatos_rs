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
  if (!showAddForm) {
    return (
      <button
        onClick={onCreate}
        className="w-full mb-6 p-4 border-2 border-dashed border-border rounded-lg hover:border-blue-500 transition-colors flex items-center justify-center space-x-2 text-muted-foreground hover:text-blue-600"
      >
        <span>+ 添加 MCP 服务器</span>
      </button>
    );
  }

  return (
    <form onSubmit={onSubmit} className="mb-6 p-4 bg-muted rounded-lg">
      <h3 className="text-lg font-medium text-foreground mb-4">
        {editingConfig ? '编辑服务器' : '添加新服务器'}
      </h3>

      <div className="space-y-4">
        <div>
          <label className="block text-sm font-medium text-foreground mb-2">服务器名称</label>
          <input
            type="text"
            value={formData.name}
            onChange={(event) => onFormDataChange({ name: event.target.value })}
            className="w-full px-3 py-2 border border-input bg-background text-foreground rounded-md focus:outline-none focus:ring-2 focus:ring-ring"
            placeholder="例如: File System"
            required
          />
        </div>

        <div>
          <label className="block text-sm font-medium text-foreground mb-2">协议类型</label>
          <select
            value={formData.type}
            onChange={(event) => onFormDataChange({ type: event.target.value as 'http' | 'stdio' })}
            className="w-full px-3 py-2 border border-input bg-background text-foreground rounded-md focus:outline-none focus:ring-2 focus:ring-ring"
          >
            <option value="stdio">Stdio (标准输入输出)</option>
            <option value="http">HTTP (网络协议)</option>
          </select>
        </div>

        <div>
          <label className="block text-sm font-medium text-foreground mb-2">
            {formData.type === 'http' ? 'URL地址' : '命令'}
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
                可执行地址（cwd）
              </label>
              <input
                type="text"
                value={formData.cwd}
                onChange={(event) => onFormDataChange({ cwd: event.target.value })}
                className="w-full px-3 py-2 border border-input bg-background text-foreground rounded-md focus:outline-none focus:ring-2 focus:ring-ring"
                placeholder="/absolute/path/to/executable"
              />
              <p className="mt-1 text-xs text-muted-foreground">为空则使用当前页面的工作目录</p>
              <div className="mt-2 flex gap-2">
                <button
                  type="button"
                  className="px-3 py-1 text-xs bg-secondary text-secondary-foreground rounded-md hover:bg-secondary/80"
                  onClick={() => void onFetchDynamicConfig()}
                >
                  {configLoading ? '获取中…' : '获取配置'}
                </button>
              </div>
            </div>

            <div>
              <label className="block text-sm font-medium text-foreground mb-2">
                启动参数（args，逗号分隔）
              </label>
              <input
                type="text"
                value={formData.argsInput}
                onChange={(event) => onFormDataChange({ argsInput: event.target.value })}
                className="w-full px-3 py-2 border border-input bg-background text-foreground rounded-md focus:outline-none focus:ring-2 focus:ring-ring"
                placeholder="--flag, value, --opt"
              />
              <p className="mt-1 text-xs text-muted-foreground">将自动转换为参数数组</p>
            </div>
          </div>
        )}

        {formData.type === 'stdio' && Object.keys(dynamicConfig).length > 0 && (
          <div className="mt-4 p-3 border border-border rounded-md bg-card">
            <div className="flex items-center justify-between mb-2">
              <span className="text-sm font-medium text-foreground">
                服务器可配置参数（动态解析）
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
          {editingConfig ? '更新' : '添加'}
        </button>
        <button
          type="button"
          onClick={onCancel}
          className="px-4 py-2 bg-secondary text-secondary-foreground rounded-md hover:bg-secondary/80 focus:outline-none focus:ring-2 focus:ring-ring"
        >
          取消
        </button>
      </div>
    </form>
  );
};

export default McpManagerForm;
