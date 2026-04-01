import {
  AI_MODEL_PROVIDERS,
  AI_MODEL_THINKING_LEVELS,
  applyProviderChange,
} from './helpers';
import { PlusIcon } from './icons';
import type { AiModelManagerFormProps } from './types';

const AiModelManagerForm = ({
  showAddForm,
  editingConfig,
  formData,
  onCreate,
  onSubmit,
  onCancel,
  onFormDataChange,
}: AiModelManagerFormProps) => {
  if (!showAddForm) {
    return (
      <button
        onClick={onCreate}
        className="w-full mb-6 p-4 border-2 border-dashed border-border rounded-lg hover:border-blue-500 transition-colors flex items-center justify-center space-x-2 text-muted-foreground hover:text-blue-600"
      >
        <PlusIcon />
        <span>添加 AI 模型</span>
      </button>
    );
  }

  return (
    <form onSubmit={onSubmit} className="mb-6 p-4 bg-muted rounded-lg">
      <h3 className="text-lg font-medium text-foreground mb-4">
        {editingConfig ? '编辑 AI 模型' : '添加 AI 模型'}
      </h3>

      <div className="space-y-4">
        <div>
          <label className="block text-sm font-medium text-foreground mb-2">配置名称</label>
          <input
            type="text"
            value={formData.name}
            onChange={(event) => onFormDataChange({ name: event.target.value })}
            className="w-full px-3 py-2 border border-input bg-background text-foreground rounded-md focus:outline-none focus:ring-2 focus:ring-ring"
            placeholder="例如: OpenAI GPT-4"
            required
          />
        </div>

        <div>
          <label className="block text-sm font-medium text-foreground mb-2">供应商</label>
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
          <label className="block text-sm font-medium text-foreground mb-2">Base URL</label>
          <input
            type="url"
            value={formData.base_url}
            onChange={(event) => onFormDataChange({ base_url: event.target.value })}
            className="w-full px-3 py-2 border border-input bg-background text-foreground rounded-md focus:outline-none focus:ring-2 focus:ring-ring"
            placeholder="例如: https://api.openai.com/v1"
            required
          />
        </div>

        <div>
          <label className="block text-sm font-medium text-foreground mb-2">API Key</label>
          <input
            type="password"
            value={formData.api_key}
            onChange={(event) => onFormDataChange({ api_key: event.target.value })}
            className="w-full px-3 py-2 border border-input bg-background text-foreground rounded-md focus:outline-none focus:ring-2 focus:ring-ring"
            placeholder="输入API密钥"
            required
          />
        </div>

        <div>
          <label className="block text-sm font-medium text-foreground mb-2">模型名称</label>
          <input
            type="text"
            value={formData.model_name}
            onChange={(event) => onFormDataChange({ model_name: event.target.value })}
            className="w-full px-3 py-2 border border-input bg-background text-foreground rounded-md focus:outline-none focus:ring-2 focus:ring-ring"
            placeholder="例如: gpt-4"
            required
          />
        </div>

        <div>
          <label className="block text-sm font-medium text-foreground mb-2">思考等级</label>
          <select
            value={formData.thinking_level}
            onChange={(event) => onFormDataChange({ thinking_level: event.target.value })}
            disabled={formData.provider !== 'gpt'}
            className="w-full px-3 py-2 border border-input bg-background text-foreground rounded-md focus:outline-none focus:ring-2 focus:ring-ring disabled:opacity-60"
          >
            {AI_MODEL_THINKING_LEVELS.map((level) => (
              <option key={level || 'empty'} value={level}>
                {level || '不指定'}
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
            启用此模型
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
            支持图片输入
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
            支持推理输出
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
            支持 Responses API
          </label>
        </div>
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

export default AiModelManagerForm;
