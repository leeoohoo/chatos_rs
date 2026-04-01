import type { McpServerListProps } from './types';
import { EditIcon, ServerIcon, TrashIcon } from './icons';
import { getMcpDisplayName, isReadonlyMcpConfig } from './helpers';

const McpServerList = ({ mcpConfigs, onEdit, onDelete }: McpServerListProps) => {
  if (mcpConfigs.length === 0) {
    return (
      <div className="text-center py-8 text-muted-foreground">
        <ServerIcon />
        <p className="mt-2">暂无 MCP 服务器配置</p>
        <p className="text-sm">点击上方按钮添加第一个服务器</p>
      </div>
    );
  }

  return (
    <>
      {mcpConfigs.map((config) => {
        const displayName = getMcpDisplayName(config);
        const isReadonly = isReadonlyMcpConfig(config);

        return (
          <div
            key={config.id}
            className="flex items-center justify-between gap-3 p-4 bg-card border border-border rounded-lg"
          >
            <div className="flex items-center space-x-3 flex-1 min-w-0">
              <div className="w-3 h-3 rounded-full bg-green-500" />
              <div className="min-w-0 flex-1">
                <div className="flex items-center space-x-2 min-w-0">
                  <h4 className="font-medium text-foreground truncate" title={displayName}>
                    {displayName}
                  </h4>
                  <span
                    className={`px-2 py-1 text-xs rounded-full ${
                      config.type === 'http'
                        ? 'bg-blue-100 text-blue-800 dark:bg-blue-900 dark:text-blue-200'
                        : 'bg-purple-100 text-purple-800 dark:bg-purple-900 dark:text-purple-200'
                    }`}
                  >
                    {config.type === 'http' ? 'HTTP' : 'Stdio'}
                  </span>
                  {isReadonly && (
                    <span className="px-2 py-1 text-xs rounded-full bg-muted text-muted-foreground">
                      内置
                    </span>
                  )}
                </div>
                <p
                  className="text-xs sm:text-sm text-muted-foreground truncate break-all font-mono"
                  title={config.command}
                >
                  {config.command}
                </p>
              </div>
            </div>

            <div className="flex items-center space-x-2 shrink-0">
              <button
                onClick={() => onEdit(config)}
                disabled={isReadonly}
                className={`p-2 text-muted-foreground transition-colors ${
                  isReadonly ? 'opacity-50 cursor-not-allowed' : 'hover:text-blue-600'
                }`}
                title="编辑"
              >
                <EditIcon />
              </button>
              <button
                onClick={() => onDelete(config.id)}
                disabled={isReadonly}
                className={`p-2 text-muted-foreground transition-colors ${
                  isReadonly ? 'opacity-50 cursor-not-allowed' : 'hover:text-red-600'
                }`}
                title="删除"
              >
                <TrashIcon />
              </button>
            </div>
          </div>
        );
      })}
    </>
  );
};

export default McpServerList;
