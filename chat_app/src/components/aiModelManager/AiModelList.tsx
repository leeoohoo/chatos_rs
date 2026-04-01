import type { AiModelListProps } from './types';
import { BrainIcon, PencilIcon, TrashIcon } from './icons';

const AiModelList = ({
  aiModelConfigs,
  onToggleEnabled,
  onEdit,
  onDelete,
}: AiModelListProps) => {
  if (aiModelConfigs.length === 0) {
    return (
      <div className="text-center py-8 text-muted-foreground">
        <BrainIcon />
        <p className="mt-2">暂无 AI 模型配置</p>
        <p className="text-sm">点击上方按钮添加第一个模型</p>
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
                title={`${config.base_url} - ${config.model_name}`}
              >
                {config.base_url} - {config.model_name}
              </p>
              {(config.provider
                || config.thinking_level
                || config.supports_images
                || config.supports_reasoning
                || config.supports_responses) && (
                <div className="mt-1 flex flex-wrap gap-2 text-[11px] text-muted-foreground">
                  {config.provider && (
                    <span className="rounded bg-accent px-1.5 py-0.5">{config.provider}</span>
                  )}
                  {config.provider === 'gpt' && config.thinking_level && (
                    <span className="rounded bg-accent px-1.5 py-0.5">
                      thinking:{config.thinking_level}
                    </span>
                  )}
                  {config.supports_images && (
                    <span className="rounded bg-accent px-1.5 py-0.5">图片</span>
                  )}
                  {config.supports_reasoning && (
                    <span className="rounded bg-accent px-1.5 py-0.5">推理</span>
                  )}
                  {config.supports_responses && (
                    <span className="rounded bg-accent px-1.5 py-0.5">Responses</span>
                  )}
                </div>
              )}
            </div>
          </div>

          <div className="flex items-center space-x-2 shrink-0">
            <button
              onClick={() => void onToggleEnabled(config)}
              className={`px-3 py-1 text-xs rounded-full transition-colors ${
                config.enabled
                  ? 'bg-green-100 text-green-800 hover:bg-green-200 dark:bg-green-900 dark:text-green-200'
                  : 'bg-secondary text-secondary-foreground hover:bg-secondary/80'
              }`}
            >
              {config.enabled ? '已启用' : '已禁用'}
            </button>

            <button
              onClick={() => onEdit(config)}
              className="p-2 text-muted-foreground hover:text-blue-600 hover:bg-blue-50 dark:hover:bg-blue-900 rounded transition-colors"
              title="编辑"
            >
              <PencilIcon />
            </button>

            <button
              onClick={() => void onDelete(config.id)}
              className="p-2 text-muted-foreground hover:text-red-600 hover:bg-red-50 dark:hover:bg-red-900 rounded transition-colors"
              title="删除"
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
