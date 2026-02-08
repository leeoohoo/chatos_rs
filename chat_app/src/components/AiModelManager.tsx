import React, { useState, useEffect } from 'react';
import { useChatStoreFromContext } from '../lib/store/ChatStoreContext';
import { useChatStore } from '../lib/store';
import type { AiModelConfig } from '../types';
import ConfirmDialog from './ui/ConfirmDialog';
import { useConfirmDialog } from '../hooks/useConfirmDialog';

// 图标组件
const BrainIcon = () => (
  <svg className="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9.663 17h4.673M12 3v1m6.364 1.636l-.707.707M21 12h-1M4 12H3m3.343-5.657l-.707-.707m2.828 9.9a5 5 0 117.072 0l-.548.547A3.374 3.374 0 0014 18.469V19a2 2 0 11-4 0v-.531c0-.895-.356-1.754-.988-2.386l-.548-.547z" />
  </svg>
);

const XMarkIcon = () => (
  <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
  </svg>
);

const PlusIcon = () => (
  <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 4v16m8-8H4" />
  </svg>
);

const PencilIcon = () => (
  <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z" />
  </svg>
);

const TrashIcon = () => (
  <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
  </svg>
);

interface AiModelManagerProps {
  onClose: () => void;
  store?: any; // 可选的store参数，用于在没有Context Provider的情况下使用
}

interface AiModelFormData {
  name: string;
  provider: string;
  base_url: string;
  api_key: string;
  model_name: string;
  thinking_level: string;
  enabled: boolean;
  supports_images: boolean;
  supports_reasoning: boolean;
  supports_responses: boolean;
}

const AiModelManager: React.FC<AiModelManagerProps> = ({ onClose, store: externalStore }) => {
  // 尝试使用外部传入的store，如果没有则尝试使用Context，最后回退到默认store
  let storeData;
  
  if (externalStore) {
    // 使用外部传入的store
    storeData = externalStore();
  } else {
    // 尝试使用Context store，如果失败则使用默认store
    try {
      storeData = useChatStoreFromContext();
    } catch (error) {
      // 如果Context不可用，使用默认store
      storeData = useChatStore();
    }
  }

  const { aiModelConfigs, loadAiModelConfigs, updateAiModelConfig, deleteAiModelConfig } = storeData;
  const [showAddForm, setShowAddForm] = useState(false);
  const [editingConfig, setEditingConfig] = useState<AiModelConfig | null>(null);
  const [formData, setFormData] = useState<AiModelFormData>({
    name: '',
    provider: 'gpt',
    base_url: '',
    api_key: '',
    model_name: '',
    thinking_level: '',
    enabled: true,
    supports_images: false,
    supports_reasoning: false,
    supports_responses: false
  });
  
  const { dialogState, showConfirmDialog, handleConfirm, handleCancel } = useConfirmDialog();

  // 加载AI模型配置（StrictMode 下防止重复触发）
  useEffect(() => {
    const key = '__aiModelManagerInitAt__';
    const last = (window as any)[key] || 0;
    const now = Date.now();
    if (typeof last === 'number' && now - last < 1000) {
      return;
    }
    (window as any)[key] = now;
    loadAiModelConfigs();
  }, [loadAiModelConfigs]);

  // 重置表单
  const resetForm = () => {
    setFormData({
      name: '',
      provider: 'gpt',
      base_url: '',
      api_key: '',
      model_name: '',
      thinking_level: '',
      enabled: true,
      supports_images: false,
      supports_reasoning: false,
      supports_responses: false
    });
    setEditingConfig(null);
    setShowAddForm(false);
  };

  // 处理添加服务器
  const handleAddServer = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!formData.name.trim() || !formData.base_url.trim() || !formData.api_key.trim() || !formData.model_name.trim()) {
      return;
    }

    const normalizedThinking = formData.provider === 'gpt' && formData.thinking_level.trim()
      ? formData.thinking_level.trim()
      : undefined;
    const newConfig: AiModelConfig = {
      id: Math.random().toString(36).substr(2, 9),
      name: formData.name.trim(),
      provider: formData.provider,
      base_url: formData.base_url.trim(),
      api_key: formData.api_key.trim(),
      model_name: formData.model_name.trim(),
      thinking_level: normalizedThinking,
      enabled: formData.enabled,
      supports_images: formData.supports_images,
      supports_reasoning: formData.supports_reasoning,
      supports_responses: formData.supports_responses,
      createdAt: new Date(),
      updatedAt: new Date()
    };

    await updateAiModelConfig(newConfig);
    resetForm();
  };

  // 处理编辑服务器
  const handleEditServer = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!editingConfig || !formData.name.trim() || !formData.base_url.trim() || !formData.api_key.trim() || !formData.model_name.trim()) {
      return;
    }

    const normalizedThinking = formData.provider === 'gpt' && formData.thinking_level.trim()
      ? formData.thinking_level.trim()
      : undefined;
    const updatedConfig: AiModelConfig = {
      ...editingConfig,
      name: formData.name.trim(),
      provider: formData.provider,
      base_url: formData.base_url.trim(),
      api_key: formData.api_key.trim(),
      model_name: formData.model_name.trim(),
      thinking_level: normalizedThinking,
      enabled: formData.enabled,
      supports_images: formData.supports_images,
      supports_reasoning: formData.supports_reasoning,
      supports_responses: formData.supports_responses,
      updatedAt: new Date()
    };

    await updateAiModelConfig(updatedConfig);
    resetForm();
  };

  // 开始编辑
  const startEdit = (config: AiModelConfig) => {
    setEditingConfig(config);
    setFormData({
      name: config.name,
      provider: config.provider || 'gpt',
      base_url: config.base_url,
      api_key: config.api_key,
      model_name: config.model_name,
      thinking_level: config.thinking_level || '',
      enabled: config.enabled,
      supports_images: config.supports_images ?? false,
      supports_reasoning: config.supports_reasoning ?? false,
      supports_responses: config.supports_responses ?? false
    });
    setShowAddForm(true);
  };

  // 删除服务器
  const handleDeleteServer = async (id: string) => {
    const config = aiModelConfigs.find((c: AiModelConfig) => c.id === id);
    showConfirmDialog({
      title: '删除确认',
      message: `确定要删除AI模型配置 "${config?.name || 'Unknown'}" 吗？此操作无法撤销。`,
      confirmText: '删除',
      cancelText: '取消',
      type: 'danger',
      onConfirm: async () => {
        await deleteAiModelConfig(id);
      }
    });
  };

  // 切换服务器启用状态
  const toggleServerEnabled = async (config: AiModelConfig) => {
    const updatedConfig: AiModelConfig = {
      ...config,
      enabled: !config.enabled,
      updatedAt: new Date()
    };
    await updateAiModelConfig(updatedConfig);
  };

  return (
    <div className="modal-container">
      <div className="modal-content w-full max-w-2xl max-h-[80vh] overflow-hidden">
        {/* 头部 */}
        <div className="flex items-center justify-between p-6 border-b border-border">
          <div className="flex items-center space-x-3">
            <BrainIcon />
            <h2 className="text-xl font-semibold text-foreground">
              AI 模型管理
            </h2>
          </div>
          <button
            onClick={onClose}
            className="p-2 text-muted-foreground hover:text-foreground hover:bg-accent rounded-lg transition-colors"
          >
            <XMarkIcon />
          </button>
        </div>

        {/* 内容区域：禁用横向滚动，避免长 URL 撑破布局 */}
        <div className="p-6 overflow-y-auto overflow-x-hidden max-h-[calc(80vh-120px)]">
          {/* 添加按钮 */}
          {!showAddForm && (
            <button
              onClick={() => setShowAddForm(true)}
              className="w-full mb-6 p-4 border-2 border-dashed border-border rounded-lg hover:border-blue-500 transition-colors flex items-center justify-center space-x-2 text-muted-foreground hover:text-blue-600"
            >
              <PlusIcon />
              <span>添加 AI 模型</span>
            </button>
          )}

          {/* 添加/编辑表单 */}
          {showAddForm && (
            <form onSubmit={editingConfig ? handleEditServer : handleAddServer} className="mb-6 p-4 bg-muted rounded-lg">
              <h3 className="text-lg font-medium text-foreground mb-4">
                {editingConfig ? '编辑 AI 模型' : '添加 AI 模型'}
              </h3>
              
              <div className="space-y-4">
                <div>
                  <label className="block text-sm font-medium text-foreground mb-2">
                    配置名称
                  </label>
                  <input
                    type="text"
                    value={formData.name}
                    onChange={(e) => setFormData({ ...formData, name: e.target.value })}
                    className="w-full px-3 py-2 border border-input bg-background text-foreground rounded-md focus:outline-none focus:ring-2 focus:ring-ring"
                    placeholder="例如: OpenAI GPT-4"
                    required
                  />
                </div>

                <div>
                  <label className="block text-sm font-medium text-foreground mb-2">
                    供应商
                  </label>
                  <select
                    value={formData.provider}
                    onChange={(e) => {
                      const nextProvider = e.target.value;
                      setFormData({
                        ...formData,
                        provider: nextProvider,
                        thinking_level: nextProvider === 'gpt' ? formData.thinking_level : ''
                      });
                    }}
                    className="w-full px-3 py-2 border border-input bg-background text-foreground rounded-md focus:outline-none focus:ring-2 focus:ring-ring"
                  >
                    <option value="gpt">gpt</option>
                    <option value="deepseek">deepseek</option>
                    <option value="kimik2">kimik2</option>
                  </select>
                </div>
                
                <div>
                  <label className="block text-sm font-medium text-foreground mb-2">
                    Base URL
                  </label>
                  <input
                    type="url"
                    value={formData.base_url}
                    onChange={(e) => setFormData({ ...formData, base_url: e.target.value })}
                    className="w-full px-3 py-2 border border-input bg-background text-foreground rounded-md focus:outline-none focus:ring-2 focus:ring-ring"
                    placeholder="例如: https://api.openai.com/v1"
                    required
                  />
                </div>
                
                <div>
                  <label className="block text-sm font-medium text-foreground mb-2">
                    API Key
                  </label>
                  <input
                    type="password"
                    value={formData.api_key}
                    onChange={(e) => setFormData({ ...formData, api_key: e.target.value })}
                    className="w-full px-3 py-2 border border-input bg-background text-foreground rounded-md focus:outline-none focus:ring-2 focus:ring-ring"
                    placeholder="输入API密钥"
                    required
                  />
                </div>
                
                <div>
                  <label className="block text-sm font-medium text-foreground mb-2">
                    模型名称
                  </label>
                  <input
                    type="text"
                    value={formData.model_name}
                    onChange={(e) => setFormData({ ...formData, model_name: e.target.value })}
                    className="w-full px-3 py-2 border border-input bg-background text-foreground rounded-md focus:outline-none focus:ring-2 focus:ring-ring"
                    placeholder="例如: gpt-4"
                    required
                  />
                </div>

                <div>
                  <label className="block text-sm font-medium text-foreground mb-2">
                    思考等级
                  </label>
                  <select
                    value={formData.thinking_level}
                    onChange={(e) => setFormData({ ...formData, thinking_level: e.target.value })}
                    disabled={formData.provider !== 'gpt'}
                    className="w-full px-3 py-2 border border-input bg-background text-foreground rounded-md focus:outline-none focus:ring-2 focus:ring-ring disabled:opacity-60"
                  >
                    <option value="">不指定</option>
                    <option value="none">none</option>
                    <option value="minimal">minimal</option>
                    <option value="low">low</option>
                    <option value="medium">medium</option>
                    <option value="high">high</option>
                    <option value="xhigh">xhigh</option>
                  </select>
                </div>
                
                <div className="flex items-center">
                  <input
                    type="checkbox"
                    id="enabled"
                    checked={formData.enabled}
                    onChange={(e) => setFormData({ ...formData, enabled: e.target.checked })}
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
                    onChange={(e) => setFormData({ ...formData, supports_images: e.target.checked })}
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
                    onChange={(e) => setFormData({ ...formData, supports_reasoning: e.target.checked })}
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
                    onChange={(e) => setFormData({ ...formData, supports_responses: e.target.checked })}
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
                  onClick={resetForm}
                  className="px-4 py-2 bg-secondary text-secondary-foreground rounded-md hover:bg-secondary/80 focus:outline-none focus:ring-2 focus:ring-ring"
                >
                  取消
                </button>
              </div>
            </form>
          )}

          {/* 服务器列表 */}
          <div className="space-y-3">
            {aiModelConfigs.length === 0 ? (
              <div className="text-center py-8 text-muted-foreground">
                <BrainIcon />
                <p className="mt-2">暂无 AI 模型配置</p>
                <p className="text-sm">点击上方按钮添加第一个模型</p>
              </div>
            ) : (
              aiModelConfigs.map((config: AiModelConfig) => (
                <div
                  key={config.id}
                  className="flex items-center justify-between gap-3 p-4 bg-card border border-border rounded-lg hover:shadow-md transition-shadow"
                >
                  {/* 左侧信息区：可截断 */}
                  <div className="flex items-center space-x-3 flex-1 min-w-0">
                    <div className={`w-3 h-3 rounded-full ${
                      config.enabled ? 'bg-green-500' : 'bg-gray-400'
                    }`} />
                    <div className="min-w-0 flex-1">
                      <h4 className="font-medium text-foreground truncate" title={config.name}>
                        {config.name}
                      </h4>
                      <p className="text-xs sm:text-sm text-muted-foreground truncate" title={`${config.base_url} - ${config.model_name}`}>
                        {config.base_url} - {config.model_name}
                      </p>
                      {(config.provider || config.thinking_level || config.supports_images || config.supports_reasoning || config.supports_responses) && (
                        <div className="mt-1 flex flex-wrap gap-2 text-[11px] text-muted-foreground">
                          {config.provider && <span className="rounded bg-accent px-1.5 py-0.5">{config.provider}</span>}
                          {config.provider === 'gpt' && config.thinking_level && <span className="rounded bg-accent px-1.5 py-0.5">thinking:{config.thinking_level}</span>}
                          {config.supports_images && <span className="rounded bg-accent px-1.5 py-0.5">图片</span>}
                          {config.supports_reasoning && <span className="rounded bg-accent px-1.5 py-0.5">推理</span>}
                          {config.supports_responses && <span className="rounded bg-accent px-1.5 py-0.5">Responses</span>}
                        </div>
                      )}
                    </div>
                  </div>
                  
                  {/* 右侧操作区：不收缩 */}
                  <div className="flex items-center space-x-2 shrink-0">
                    <button
                      onClick={() => toggleServerEnabled(config)}
                      className={`px-3 py-1 text-xs rounded-full transition-colors ${
                        config.enabled
                          ? 'bg-green-100 text-green-800 hover:bg-green-200 dark:bg-green-900 dark:text-green-200'
                          : 'bg-secondary text-secondary-foreground hover:bg-secondary/80'
                      }`}
                    >
                      {config.enabled ? '已启用' : '已禁用'}
                    </button>
                    
                    <button
                      onClick={() => startEdit(config)}
                      className="p-2 text-muted-foreground hover:text-blue-600 hover:bg-blue-50 dark:hover:bg-blue-900 rounded transition-colors"
                      title="编辑"
                    >
                      <PencilIcon />
                    </button>
                    
                    <button
                      onClick={() => handleDeleteServer(config.id)}
                      className="p-2 text-muted-foreground hover:text-red-600 hover:bg-red-50 dark:hover:bg-red-900 rounded transition-colors"
                      title="删除"
                    >
                      <TrashIcon />
                    </button>
                  </div>
                </div>
              ))
            )}
          </div>
        </div>
      </div>

      {/* 确认对话框 */}
      <ConfirmDialog
        isOpen={dialogState.isOpen}
        title={dialogState.title}
        message={dialogState.message}
        confirmText={dialogState.confirmText}
        cancelText={dialogState.cancelText}
        type={dialogState.type}
        onConfirm={handleConfirm}
        onCancel={handleCancel}
      />
    </div>
  );
};

export default AiModelManager;
