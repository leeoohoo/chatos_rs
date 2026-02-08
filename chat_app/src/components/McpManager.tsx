import React, { useState } from 'react';
import { useChatStoreFromContext } from '../lib/store/ChatStoreContext';
import { useChatStore } from '../lib/store';
import { McpConfig } from '../types';
import ConfirmDialog from './ui/ConfirmDialog';
import { useConfirmDialog } from '../hooks/useConfirmDialog';
import { apiClient } from '../lib/api/client';

// 服务器图标组件
const ServerIcon = () => (
  <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 12h14M5 12a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v4a2 2 0 01-2 2M5 12a2 2 0 00-2 2v4a2 2 0 002 2h14a2 2 0 002-2v-4a2 2 0 00-2-2m-2-4h.01M17 16h.01" />
  </svg>
);

// 关闭图标组件
const XMarkIcon = () => (
  <svg className="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
  </svg>
);

// 加号图标组件
const PlusIcon = () => (
  <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 4v16m8-8H4" />
  </svg>
);

// 删除图标组件
const TrashIcon = () => (
  <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
  </svg>
);

// 编辑图标组件
const EditIcon = () => (
  <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z" />
  </svg>
);

interface McpManagerProps {
  onClose?: () => void;
  store?: any; // 可选的store参数，用于在没有Context Provider的情况下使用
}

interface McpFormData {
  name: string;
  command: string;
  type: 'http' | 'stdio';
  cwd?: string;
  argsInput?: string; // 逗号分隔的参数输入
}

const McpManager: React.FC<McpManagerProps> = ({ onClose, store: externalStore }) => {
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

  const { mcpConfigs, updateMcpConfig, deleteMcpConfig, loadMcpConfigs } = storeData;
  const { dialogState, showConfirmDialog, handleConfirm, handleCancel } = useConfirmDialog();

  const [showAddForm, setShowAddForm] = useState(false);
  const [editingConfig, setEditingConfig] = useState<McpConfig | null>(null);
  const [formData, setFormData] = useState<McpFormData>({
    name: '',
    command: '',
    type: 'stdio',
    cwd: '',
    argsInput: ''
  });

  // stdio 资源配置相关状态
  const [configLoading, setConfigLoading] = useState<boolean>(false);
  const [configError, setConfigError] = useState<string | null>(null);
  const [dynamicConfig, setDynamicConfig] = useState<Record<string, any>>({});
  

  // 组件初始化时加载MCP配置（StrictMode 下防止重复触发）
  React.useEffect(() => {
    const key = '__mcpManagerInitAt__';
    const last = (window as any)[key] || 0;
    const now = Date.now();
    if (typeof last === 'number' && now - last < 1000) {
      return;
    }
    (window as any)[key] = now;
    loadMcpConfigs();
  }, [loadMcpConfigs]);

  // 重置表单
  const resetForm = () => {
    setFormData({
      name: '',
      command: '',
      type: 'stdio',
      cwd: '',
      argsInput: ''
    });
    setEditingConfig(null);
    setShowAddForm(false);
    setDynamicConfig({});
  };

  // 处理添加服务器
  const handleAddServer = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!formData.name.trim() || !formData.command.trim()) {
      return;
    }

    const newConfig: Partial<McpConfig> = {
      name: formData.name.trim(),
      command: formData.command.trim(),
      type: formData.type,
      enabled: true,
      cwd: formData.cwd?.trim() ? formData.cwd.trim() : undefined,
      args: formData.argsInput?.trim() ? formData.argsInput.split(',').map(s => s.trim()).filter(Boolean) : undefined,
      createdAt: new Date(),
      updatedAt: new Date()
    };

    await updateMcpConfig(newConfig as McpConfig);
    resetForm();
  };

  // 处理编辑服务器
  const handleEditServer = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!editingConfig || !formData.name.trim() || !formData.command.trim()) {
      return;
    }

    const updatedConfig: McpConfig = {
      ...editingConfig,
      name: formData.name.trim(),
      command: formData.command.trim(),
      type: formData.type,
      enabled: true,
      cwd: formData.cwd?.trim() ? formData.cwd.trim() : undefined,
      args: formData.argsInput?.trim() ? formData.argsInput.split(',').map(s => s.trim()).filter(Boolean) : undefined,
      updatedAt: new Date()
    };
    await updateMcpConfig(updatedConfig);
    resetForm();
  };

  // 开始编辑
  const startEdit = (config: McpConfig) => {
    if ((config as any)?.readonly || (config as any)?.builtin) {
      return;
    }
    setEditingConfig(config);
    // 解析 args：兼容数组或JSON字符串
    let argsInput = '';
    const rawArgs: any = (config as any).args;
    if (Array.isArray(rawArgs)) {
      argsInput = rawArgs.map((x: any) => String(x)).join(', ');
    } else if (typeof rawArgs === 'string' && rawArgs.trim() !== '') {
      try {
        const parsed = JSON.parse(rawArgs);
        if (Array.isArray(parsed)) argsInput = parsed.map((x: any) => String(x)).join(', ');
        else argsInput = rawArgs;
      } catch {
        // 非JSON字符串，直接回显
        argsInput = rawArgs;
      }
    }

    setFormData({
      name: (config as any).name,
      command: (config as any).command,
      type: (config as any).type || 'stdio',
      cwd: (config as any).cwd || '',
      argsInput
    });
    setShowAddForm(true);
    // 重置配置状态
    setDynamicConfig({});
    setConfigError(null);
    setConfigLoading(false);
  };

  // 删除服务器
  const handleDeleteServer = async (id: string) => {
    const config = mcpConfigs.find((c: McpConfig) => c.id === id);
    if ((config as any)?.readonly || (config as any)?.builtin) {
      return;
    }
    showConfirmDialog({
      title: '删除 MCP 服务器',
      message: `确定要删除服务器 "${config?.name || 'Unknown'}" 吗？此操作无法撤销。`,
      confirmText: '删除',
      cancelText: '取消',
      type: 'danger',
      onConfirm: () => deleteMcpConfig(id)
    });
  };

  // 切换服务器启用状态
  return (
    <>
      {/* 背景遮罩 */}
      <div 
        className="fixed inset-0 bg-black/50 backdrop-blur-sm z-40"
        onClick={onClose}
      />

      {/* 抽屉面板（右侧） */}
      <div className="fixed right-0 top-0 h-full w-[520px] sm:w-[560px] bg-card z-50 shadow-xl breathing-border flex flex-col">
        {/* 头部 */}
        <div className="flex items-center justify-between p-4 border-b border-border">
          <div className="flex items-center space-x-3">
            <ServerIcon />
            <h2 className="text-lg font-semibold text-foreground">MCP 服务器管理</h2>
          </div>
          <button
            onClick={onClose}
            className="p-2 text-muted-foreground hover:text-foreground hover:bg-accent rounded-lg transition-colors"
            title="关闭"
          >
            <XMarkIcon />
          </button>
        </div>

        {/* 内容区域：禁用横向滚动，避免长文本撑破布局 */}
        <div className="p-4 flex-1 overflow-y-auto overflow-x-hidden">
          {/* 添加按钮 */}
          {!showAddForm && (
            <button
              onClick={() => setShowAddForm(true)}
              className="w-full mb-6 p-4 border-2 border-dashed border-border rounded-lg hover:border-blue-500 transition-colors flex items-center justify-center space-x-2 text-muted-foreground hover:text-blue-600"
            >
              <PlusIcon />
              <span>添加 MCP 服务器</span>
            </button>
          )}

          {/* 添加/编辑表单 */}
          {showAddForm && (
            <form onSubmit={editingConfig ? handleEditServer : handleAddServer} className="mb-6 p-4 bg-muted rounded-lg">
              <h3 className="text-lg font-medium text-foreground mb-4">
                {editingConfig ? '编辑服务器' : '添加新服务器'}
              </h3>
              
              <div className="space-y-4">
                <div>
                  <label className="block text-sm font-medium text-foreground mb-2">
                    服务器名称
                  </label>
                  <input
                    type="text"
                    value={formData.name}
                    onChange={(e) => setFormData({ ...formData, name: e.target.value })}
                    className="w-full px-3 py-2 border border-input bg-background text-foreground rounded-md focus:outline-none focus:ring-2 focus:ring-ring"
                    placeholder="例如: File System"
                    required
                  />
                </div>
                
                <div>
                  <label className="block text-sm font-medium text-foreground mb-2">
                    协议类型
                  </label>
                  <select
                    value={formData.type}
                    onChange={(e) => setFormData({ ...formData, type: e.target.value as 'http' | 'stdio' })}
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
                    onChange={(e) => setFormData({ ...formData, command: e.target.value })}
                    className="w-full px-3 py-2 border border-input bg-background text-foreground rounded-md focus:outline-none focus:ring-2 focus:ring-ring"
                    placeholder={formData.type === 'http' ? 'https://api.example.com/mcp' : 'npx @modelcontextprotocol/server-filesystem /path/to/allowed/files'}
                    required
                  />
                  {formData.type === 'stdio' && (
                    <></>
                  )}
                </div>

                {/* stdio 专有设置：工作目录和启动参数 */}
                {formData.type === 'stdio' && (
                  <div className="space-y-3">
                    <div>
                      <label className="block text-sm font-medium text-foreground mb-2">
                        可执行地址（cwd）
                      </label>
                      <input
                        type="text"
                        value={formData.cwd}
                        onChange={(e) => setFormData({ ...formData, cwd: e.target.value })}
                        className="w-full px-3 py-2 border border-input bg-background text-foreground rounded-md focus:outline-none focus:ring-2 focus:ring-ring"
                        placeholder="/absolute/path/to/executable"
                      />
                      <p className="mt-1 text-xs text-muted-foreground">为空则使用当前页面的工作目录</p>
                      <div className="mt-2 flex gap-2">
                        <button
                          type="button"
                          className="px-3 py-1 text-xs bg-secondary text-secondary-foreground rounded-md hover:bg-secondary/80"
                          onClick={async () => {
                            if (!formData.command.trim()) return;
                            setConfigLoading(true);
                            setConfigError(null);
                            try {
                              const res = await apiClient.getMcpConfigResourceByCommand({
                                type: formData.type,
                                command: formData.command.trim(),
                                args: formData.argsInput?.trim()
                                  ? formData.argsInput.split(',').map(s => s.trim()).filter(Boolean)
                                  : undefined,
                                env: undefined,
                                cwd: formData.cwd?.trim() ? formData.cwd.trim() : undefined,
                                alias: null,
                              });
                              if (res && (res as any).success && (res as any).config) {
                                const cfg = (res as any).config;
                                setDynamicConfig(cfg);
                              } else {
                                setConfigError('无法获取服务器可配置参数');
                              }
                            } catch (err: any) {
                              setConfigError(err?.message || '获取配置失败');
                            } finally {
                              setConfigLoading(false);
                            }
                          }}
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
                        onChange={(e) => setFormData({ ...formData, argsInput: e.target.value })}
                        className="w-full px-3 py-2 border border-input bg-background text-foreground rounded-md focus:outline-none focus:ring-2 focus:ring-ring"
                        placeholder="--flag, value, --opt"
                      />
                      <p className="mt-1 text-xs text-muted-foreground">将自动转换为参数数组</p>
                    </div>
                  </div>
                )}

                {/* 动态配置表格：基于服务器定义的参数 */}
                {formData.type === 'stdio' && Object.keys(dynamicConfig).length > 0 && (
                  <div className="mt-4 p-3 border border-border rounded-md bg-card">
                    <div className="flex items-center justify-between mb-2">
                      <span className="text-sm font-medium text-foreground">服务器可配置参数（动态解析）</span>
                      {configLoading && (
                        <span className="text-xs text-muted-foreground">解析中…</span>
                      )}
                    </div>

                    {configError && (
                      <p className="text-xs text-red-600 mb-2">{configError}</p>
                    )}

                    <div className="grid grid-cols-1 gap-3">
                      {Object.keys(dynamicConfig).map((key) => {
                        const val = dynamicConfig[key];
                        const type = typeof val;
                        const isArray = Array.isArray(val);
                        return (
                          <div key={key}>
                            <label className="block text-xs text-muted-foreground mb-1">{key}</label>
                            {type === 'boolean' ? (
                              <div className="flex items-center">
                                <input
                                  type="checkbox"
                                  checked={!!val}
                                  onChange={(e) => setDynamicConfig({ ...dynamicConfig, [key]: e.target.checked })}
                                  className="h-4 w-4 text-blue-600 focus:ring-blue-500 border-gray-300 rounded"
                                />
                                <span className="ml-2 text-xs">{String(val)}</span>
                              </div>
                            ) : isArray ? (
                              <input
                                type="text"
                                value={(val as any[]).join(', ')}
                                onChange={(e) => setDynamicConfig({
                                  ...dynamicConfig,
                                  [key]: e.target.value
                                    .split(',')
                                    .map(s => s.trim())
                                    .filter(Boolean)
                                })}
                                className="w-full px-2 py-1 border border-input bg-background text-foreground rounded-md"
                              />
                            ) : (
                              <input
                                type={type === 'number' ? 'number' : 'text'}
                                value={val ?? ''}
                                onChange={(e) => setDynamicConfig({
                                  ...dynamicConfig,
                                  [key]: type === 'number' ? Number(e.target.value) : e.target.value
                                })}
                                className="w-full px-2 py-1 border border-input bg-background text-foreground rounded-md"
                              />
                            )}
                          </div>
                        );
                      })}

                    </div>
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
            {mcpConfigs.length === 0 ? (
              <div className="text-center py-8 text-muted-foreground">
                <ServerIcon />
                <p className="mt-2">暂无 MCP 服务器配置</p>
                <p className="text-sm">点击上方按钮添加第一个服务器</p>
              </div>
            ) : (
              mcpConfigs.map((config: McpConfig) => {
                const displayName = (config as any).display_name || config.name;
                const isReadonly = (config as any).readonly || (config as any).builtin;
                return (
                <div
                  key={config.id}
                  className="flex items-center justify-between gap-3 p-4 bg-card border border-border rounded-lg"
                >
                  {/* 左侧信息区：允许在 flex 布局中收缩并截断 */}
                  <div className="flex items-center space-x-3 flex-1 min-w-0">
                    <div className="w-3 h-3 rounded-full bg-green-500" />
                    <div className="min-w-0 flex-1">
                      <div className="flex items-center space-x-2 min-w-0">
                        <h4 className="font-medium text-foreground truncate" title={displayName}>
                          {displayName}
                        </h4>
                        <span className={`px-2 py-1 text-xs rounded-full ${
                          config.type === 'http' 
                            ? 'bg-blue-100 text-blue-800 dark:bg-blue-900 dark:text-blue-200'
                            : 'bg-purple-100 text-purple-800 dark:bg-purple-900 dark:text-purple-200'
                        }`}>
                          {config.type === 'http' ? 'HTTP' : 'Stdio'}
                        </span>
                        {isReadonly && (
                          <span className="px-2 py-1 text-xs rounded-full bg-muted text-muted-foreground">内置</span>
                        )}
                      </div>
                      {/* 命令/URL：长文本单行省略，悬浮可查看完整 */}
                      <p className="text-xs sm:text-sm text-muted-foreground truncate break-all font-mono" title={config.command}>
                        {config.command}
                      </p>
                      {/* 调试：开发环境输出每条配置的关键信息，定位 args 渲染问题 */}
                      {typeof window !== 'undefined' && (import.meta as any).env && (import.meta as any).env.DEV && (
                        <pre className="hidden">{JSON.stringify({ id: (config as any).id, type: (config as any).type, args: (config as any).args, cwd: (config as any).cwd }, null, 2)}</pre>
                      )}
                      {config.type === 'stdio' && (
                        <div className="mt-1 space-y-0.5 min-w-0">
                          {(config as any).cwd && (
                            <div className="text-xs text-muted-foreground truncate" title={String((config as any).cwd)}>
                              cwd: <span className="text-foreground">{String((config as any).cwd)}</span>
                            </div>
                          )}
                          <div className="text-xs text-muted-foreground truncate" title={(() => {
                            const raw = (config as any).args;
                            let argsArr: string[] | null = null;
                            let fromJson = false;
                            if (Array.isArray(raw)) {
                              argsArr = (raw as any[]).map((x) => String(x));
                            } else if (typeof raw === 'string' && raw.trim() !== '') {
                              try {
                                const parsed = JSON.parse(raw);
                                if (Array.isArray(parsed)) {
                                  argsArr = parsed.map((x: any) => String(x));
                                  fromJson = true;
                                }
                              } catch {}
                            }
                            const display = Array.isArray(argsArr) && argsArr.length > 0
                              ? argsArr.join(', ')
                              : (typeof raw === 'string' && !fromJson ? raw : '—');
                            return String(display);
                          })()}>
                            {(() => {
                              const raw = (config as any).args;
                              let argsArr: string[] | null = null;
                              let fromJson = false;
                              if (Array.isArray(raw)) {
                                argsArr = (raw as any[]).map((x) => String(x));
                              } else if (typeof raw === 'string' && raw.trim() !== '') {
                                try {
                                  const parsed = JSON.parse(raw);
                                  if (Array.isArray(parsed)) {
                                    argsArr = parsed.map((x: any) => String(x));
                                    fromJson = true;
                                  }
                                } catch {}
                              }
                              const display = Array.isArray(argsArr) && argsArr.length > 0
                                ? argsArr.join(', ')
                                : (typeof raw === 'string' && !fromJson ? raw : '—');
                              return (
                                <span>
                                  参数(args)：<span className="text-foreground">{display}</span>
                                </span>
                              );
                            })()}
                          </div>
                        </div>
                      )}
                    </div>
                  </div>
                  
                  {/* 右侧操作区：不收缩，避免被长文本挤压 */}
                  <div className="flex items-center space-x-2 shrink-0">
                    <button
                      onClick={() => startEdit(config)}
                      disabled={isReadonly}
                      className={`p-2 text-muted-foreground transition-colors ${isReadonly ? 'opacity-50 cursor-not-allowed' : 'hover:text-blue-600'}`}
                      title="编辑"
                    >
                      <EditIcon />
                    </button>
                    
                    <button
                      onClick={() => handleDeleteServer(config.id)}
                      disabled={isReadonly}
                      className={`p-2 text-muted-foreground transition-colors ${isReadonly ? 'opacity-50 cursor-not-allowed' : 'hover:text-red-600'}`}
                      title="删除"
                    >
                      <TrashIcon />
                    </button>
                  </div>
                </div>
              )})
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
    </>
  );
};

export default McpManager;
