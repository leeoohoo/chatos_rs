import React, { useState } from 'react';

import { useConfirmDialog } from '../hooks/useConfirmDialog';
import { apiClient } from '../lib/api/client';
import { useChatStore } from '../lib/store';
import { useChatStoreFromContext } from '../lib/store/ChatStoreContext';
import type { McpConfig } from '../types';
import ConfirmDialog from './ui/ConfirmDialog';
import {
  EditIcon,
  PlusIcon,
  ServerIcon,
  TrashIcon,
  XMarkIcon,
} from './mcpManager/icons';

interface McpManagerProps {
  onClose?: () => void;
  store?: any;
}

interface McpFormData {
  name: string;
  command: string;
  type: 'http' | 'stdio';
  cwd?: string;
  argsInput?: string;
}

const McpManager: React.FC<McpManagerProps> = ({ onClose, store: externalStore }) => {
  let storeData;
  if (externalStore) {
    storeData = externalStore();
  } else {
    try {
      storeData = useChatStoreFromContext();
    } catch (_) {
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
    argsInput: '',
  });

  const [configLoading, setConfigLoading] = useState<boolean>(false);
  const [configError, setConfigError] = useState<string | null>(null);
  const [dynamicConfig, setDynamicConfig] = useState<Record<string, any>>({});

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

  const resetForm = () => {
    setFormData({
      name: '',
      command: '',
      type: 'stdio',
      cwd: '',
      argsInput: '',
    });
    setEditingConfig(null);
    setShowAddForm(false);
    setDynamicConfig({});
    setConfigError(null);
    setConfigLoading(false);
  };

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
      args: formData.argsInput?.trim()
        ? formData.argsInput
            .split(',')
            .map((value) => value.trim())
            .filter(Boolean)
        : undefined,
      createdAt: new Date(),
      updatedAt: new Date(),
    };

    await updateMcpConfig(newConfig as McpConfig);
    resetForm();
  };

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
      args: formData.argsInput?.trim()
        ? formData.argsInput
            .split(',')
            .map((value) => value.trim())
            .filter(Boolean)
        : undefined,
      updatedAt: new Date(),
    };
    await updateMcpConfig(updatedConfig);
    resetForm();
  };

  const startEdit = (config: McpConfig) => {
    if ((config as any)?.readonly || (config as any)?.builtin) {
      return;
    }
    setEditingConfig(config);

    let argsInput = '';
    const rawArgs: any = (config as any).args;
    if (Array.isArray(rawArgs)) {
      argsInput = rawArgs.map((x: any) => String(x)).join(', ');
    } else if (typeof rawArgs === 'string' && rawArgs.trim() !== '') {
      try {
        const parsed = JSON.parse(rawArgs);
        if (Array.isArray(parsed)) {
          argsInput = parsed.map((x: any) => String(x)).join(', ');
        } else {
          argsInput = rawArgs;
        }
      } catch {
        argsInput = rawArgs;
      }
    }

    setFormData({
      name: (config as any).name,
      command: (config as any).command,
      type: (config as any).type || 'stdio',
      cwd: (config as any).cwd || '',
      argsInput,
    });
    setShowAddForm(true);
    setDynamicConfig({});
    setConfigError(null);
    setConfigLoading(false);
  };

  const handleDeleteServer = async (id: string) => {
    const config = mcpConfigs.find((item: McpConfig) => item.id === id);
    if ((config as any)?.readonly || (config as any)?.builtin) {
      return;
    }
    showConfirmDialog({
      title: '删除 MCP 服务器',
      message: `确定要删除服务器 "${config?.name || 'Unknown'}" 吗？此操作无法撤销。`,
      confirmText: '删除',
      cancelText: '取消',
      type: 'danger',
      onConfirm: () => deleteMcpConfig(id),
    });
  };

  return (
    <>
      <div className="fixed inset-0 bg-black/50 backdrop-blur-sm z-40" onClick={onClose} />

      <div className="fixed right-0 top-0 h-full w-[520px] sm:w-[560px] bg-card z-50 shadow-xl breathing-border flex flex-col">
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

        <div className="p-4 flex-1 overflow-y-auto overflow-x-hidden">
          {!showAddForm && (
            <button
              onClick={() => setShowAddForm(true)}
              className="w-full mb-6 p-4 border-2 border-dashed border-border rounded-lg hover:border-blue-500 transition-colors flex items-center justify-center space-x-2 text-muted-foreground hover:text-blue-600"
            >
              <PlusIcon />
              <span>添加 MCP 服务器</span>
            </button>
          )}

          {showAddForm && (
            <form
              onSubmit={editingConfig ? handleEditServer : handleAddServer}
              className="mb-6 p-4 bg-muted rounded-lg"
            >
              <h3 className="text-lg font-medium text-foreground mb-4">
                {editingConfig ? '编辑服务器' : '添加新服务器'}
              </h3>

              <div className="space-y-4">
                <div>
                  <label className="block text-sm font-medium text-foreground mb-2">服务器名称</label>
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
                  <label className="block text-sm font-medium text-foreground mb-2">协议类型</label>
                  <select
                    value={formData.type}
                    onChange={(e) =>
                      setFormData({ ...formData, type: e.target.value as 'http' | 'stdio' })
                    }
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
                                  ? formData.argsInput
                                      .split(',')
                                      .map((s) => s.trim())
                                      .filter(Boolean)
                                  : undefined,
                                env: undefined,
                                cwd: formData.cwd?.trim() ? formData.cwd.trim() : undefined,
                                alias: null,
                              });
                              if (res && (res as any).success && (res as any).config) {
                                setDynamicConfig((res as any).config);
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

                {formData.type === 'stdio' && Object.keys(dynamicConfig).length > 0 && (
                  <div className="mt-4 p-3 border border-border rounded-md bg-card">
                    <div className="flex items-center justify-between mb-2">
                      <span className="text-sm font-medium text-foreground">
                        服务器可配置参数（动态解析）
                      </span>
                    </div>
                    {configError && <p className="text-xs text-red-600 mb-2">{configError}</p>}
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
                                  onChange={(e) =>
                                    setDynamicConfig({ ...dynamicConfig, [key]: e.target.checked })
                                  }
                                  className="h-4 w-4 text-blue-600 focus:ring-blue-500 border-gray-300 rounded"
                                />
                                <span className="ml-2 text-xs">{String(val)}</span>
                              </div>
                            ) : isArray ? (
                              <input
                                type="text"
                                value={(val as any[]).join(', ')}
                                onChange={(e) =>
                                  setDynamicConfig({
                                    ...dynamicConfig,
                                    [key]: e.target.value
                                      .split(',')
                                      .map((s) => s.trim())
                                      .filter(Boolean),
                                  })
                                }
                                className="w-full px-2 py-1 border border-input bg-background text-foreground rounded-md"
                              />
                            ) : (
                              <input
                                type={type === 'number' ? 'number' : 'text'}
                                value={val ?? ''}
                                onChange={(e) =>
                                  setDynamicConfig({
                                    ...dynamicConfig,
                                    [key]: type === 'number' ? Number(e.target.value) : e.target.value,
                                  })
                                }
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
                        onClick={() => startEdit(config)}
                        disabled={isReadonly}
                        className={`p-2 text-muted-foreground transition-colors ${
                          isReadonly ? 'opacity-50 cursor-not-allowed' : 'hover:text-blue-600'
                        }`}
                        title="编辑"
                      >
                        <EditIcon />
                      </button>
                      <button
                        onClick={() => handleDeleteServer(config.id)}
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
              })
            )}
          </div>
        </div>
      </div>

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

