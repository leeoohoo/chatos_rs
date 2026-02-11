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

// 设置图标组件
const SettingsIcon = () => (
  <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10.325 4.317a1 1 0 011.35-.936l.094.047 1.2.6a1 1 0 00.894 0l1.2-.6a1 1 0 011.444.89l.006.099v1.36a1 1 0 00.292.707l.962.963a1 1 0 01.083 1.32l-.083.094-.962.963a1 1 0 00-.292.707v1.36a1 1 0 01-1.45.894l-.094-.047-1.2-.6a1 1 0 00-.894 0l-1.2.6a1 1 0 01-1.444-.89l-.006-.099v-1.36a1 1 0 00-.292-.707l-.962-.963a1 1 0 01-.083-1.32l.083-.094.962-.963a1 1 0 00.292-.707v-1.36z" />
    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 10.5a1.5 1.5 0 110 3 1.5 1.5 0 010-3z" />
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

  // 内置 MCP 设置面板状态
  const [settingsConfig, setSettingsConfig] = useState<McpConfig | null>(null);
  const [settingsLoading, setSettingsLoading] = useState<boolean>(false);
  const [settingsError, setSettingsError] = useState<string | null>(null);
  const [settingsSummary, setSettingsSummary] = useState<any>(null);
  const [gitRepositoryInput, setGitRepositoryInput] = useState('');
  const [gitBranchInput, setGitBranchInput] = useState('');
  const [gitAgentsPathInput, setGitAgentsPathInput] = useState('');
  const [gitSkillsPathInput, setGitSkillsPathInput] = useState('');
  const [settingsSubmitting, setSettingsSubmitting] = useState<'git' | 'plugin' | 'plugin_all' | null>(null);
  const [settingsTab, setSettingsTab] = useState<'mcp' | 'overview' | 'git' | 'marketplace'>('mcp');
  const [settingsNotice, setSettingsNotice] = useState<string | null>(null);
  const [lastGitImportResult, setLastGitImportResult] = useState<any>(null);
  const [settingsPluginSearch, setSettingsPluginSearch] = useState('');
  const [settingsPluginFilter, setSettingsPluginFilter] = useState<'all' | 'installed' | 'pending'>('all');
  const [settingsMcpLoading, setSettingsMcpLoading] = useState<boolean>(false);
  const [settingsMcpSaving, setSettingsMcpSaving] = useState<boolean>(false);
  const [settingsMcpOptions, setSettingsMcpOptions] = useState<any[]>([]);
  const [settingsMcpEnabledIds, setSettingsMcpEnabledIds] = useState<string[]>([]);
  const [settingsMcpConfigured, setSettingsMcpConfigured] = useState<boolean>(false);
  const [settingsMcpUpdatedAt, setSettingsMcpUpdatedAt] = useState<string | null>(null);
  const [settingsMcpSearch, setSettingsMcpSearch] = useState('');

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

  const normalizeIdList = (items: any): string[] => {
    if (!Array.isArray(items)) return [];
    const normalized = items
      .map((item) => String(item || '').trim())
      .filter(Boolean);
    return Array.from(new Set(normalized)).sort();
  };

  const applyMcpPermissionState = (payload: any) => {
    const options = Array.isArray(payload?.options) ? payload.options : [];
    const normalizedOptions = options
      .map((item: any) => {
        const id = String(item?.id || '').trim();
        if (!id) return null;
        return {
          ...item,
          id,
          name: String(item?.name || id),
          display_name: String(item?.display_name || item?.name || id),
          tool_prefix: String(item?.tool_prefix || ''),
          config_enabled: item?.config_enabled !== false,
        };
      })
      .filter(Boolean) as any[];

    const payloadEnabledIds = normalizeIdList(payload?.enabled_mcp_ids);
    const fallbackEnabledIds = normalizeIdList(
      normalizedOptions
        .filter((item: any) => item?.enabled)
        .map((item: any) => item?.id)
    );

    setSettingsMcpOptions(normalizedOptions);
    setSettingsMcpEnabledIds(payloadEnabledIds.length > 0 || payload?.configured ? payloadEnabledIds : fallbackEnabledIds);
    setSettingsMcpConfigured(Boolean(payload?.configured));
    setSettingsMcpUpdatedAt(typeof payload?.updated_at === 'string' ? payload.updated_at : null);
  };

  const loadBuiltinSettings = async (configId: string) => {
    setSettingsLoading(true);
    setSettingsMcpLoading(true);
    setSettingsError(null);
    try {
      const [summaryRes, permissionsRes] = await Promise.all([
        apiClient.getBuiltinMcpSettings(configId),
        apiClient.getBuiltinMcpPermissions(configId),
      ]);
      const summaryData = (summaryRes as any)?.data || null;
      const permissionsData = (permissionsRes as any)?.data || null;
      setSettingsSummary(summaryData);
      applyMcpPermissionState(permissionsData);
    } catch (error: any) {
      setSettingsSummary(null);
      setSettingsMcpOptions([]);
      setSettingsMcpEnabledIds([]);
      setSettingsMcpConfigured(false);
      setSettingsMcpUpdatedAt(null);
      setSettingsError(error?.message || '读取内置 MCP 设置失败');
    } finally {
      setSettingsLoading(false);
      setSettingsMcpLoading(false);
    }
  };

  const handleSaveMcpPermissions = async () => {
    if (!settingsConfig) return;

    setSettingsMcpSaving(true);
    setSettingsError(null);
    setSettingsNotice(null);
    try {
      const response = await apiClient.updateBuiltinMcpPermissions(settingsConfig.id, {
        enabled_mcp_ids: normalizeIdList(settingsMcpEnabledIds),
      });
      const payload = (response as any)?.data || null;
      applyMcpPermissionState(payload);
      setSettingsNotice('Sub-agent 可用 MCP 权限已保存。');
    } catch (error: any) {
      setSettingsError(error?.message || '保存 MCP 权限失败');
    } finally {
      setSettingsMcpSaving(false);
    }
  };

  const openBuiltinSettings = async (config: McpConfig) => {
    setSettingsConfig(config);
    setSettingsError(null);
    setSettingsNotice(null);
    setLastGitImportResult(null);
    setSettingsTab('mcp');
    setSettingsSummary(null);
    setGitRepositoryInput('');
    setGitBranchInput('');
    setGitAgentsPathInput('');
    setGitSkillsPathInput('');
    setSettingsPluginSearch('');
    setSettingsPluginFilter('all');
    setSettingsMcpOptions([]);
    setSettingsMcpEnabledIds([]);
    setSettingsMcpConfigured(false);
    setSettingsMcpUpdatedAt(null);
    setSettingsMcpSearch('');
    await loadBuiltinSettings(config.id);
  };

  const closeBuiltinSettings = () => {
    setSettingsConfig(null);
    setSettingsLoading(false);
    setSettingsMcpLoading(false);
    setSettingsMcpSaving(false);
    setSettingsError(null);
    setSettingsNotice(null);
    setLastGitImportResult(null);
    setSettingsSummary(null);
    setGitRepositoryInput('');
    setGitBranchInput('');
    setGitAgentsPathInput('');
    setGitSkillsPathInput('');
    setSettingsSubmitting(null);
    setSettingsTab('mcp');
    setSettingsPluginSearch('');
    setSettingsPluginFilter('all');
    setSettingsMcpOptions([]);
    setSettingsMcpEnabledIds([]);
    setSettingsMcpConfigured(false);
    setSettingsMcpUpdatedAt(null);
    setSettingsMcpSearch('');
  };


  const handleImportFromGit = async () => {
    if (!settingsConfig) return;
    const repository = gitRepositoryInput.trim();
    if (!repository) {
      setSettingsError('Please enter a Git repository URL first.');
      return;
    }

    setSettingsSubmitting('git');
    setSettingsError(null);
    setSettingsNotice(null);
    setLastGitImportResult(null);
    try {
      const response = await apiClient.importBuiltinMcpFromGit(settingsConfig.id, {
        repository,
        branch: gitBranchInput.trim() || undefined,
        agents_path: gitAgentsPathInput.trim() || undefined,
        skills_path: gitSkillsPathInput.trim() || undefined,
      });
      const result = (response as any)?.data || null;
      setLastGitImportResult(result);

      const importedAgents = !!result?.imported?.agents;
      const importedSkills = !!result?.imported?.skills;
      if (importedAgents && importedSkills) {
        setSettingsNotice('Git import succeeded: agents and skills both updated.');
      } else if (importedAgents) {
        setSettingsNotice('Git import succeeded: only agents were updated.');
      } else if (importedSkills) {
        setSettingsNotice('Git import succeeded: only skills were updated.');
      } else {
        setSettingsNotice('Git import request finished but nothing was imported.');
      }

      setSettingsTab('marketplace');
      await loadBuiltinSettings(settingsConfig.id);
    } catch (error: any) {
      setSettingsError(error?.message || 'Git import failed.');
    } finally {
      setSettingsSubmitting(null);
    }
  };

  const handleInstallPlugin = async (source: string) => {
    if (!settingsConfig) return;
    const normalizedSource = source.trim();
    if (!normalizedSource) {
      setSettingsError('plugin source 不能为空');
      return;
    }

    setSettingsSubmitting('plugin');
    setSettingsError(null);
    setSettingsNotice(null);
    try {
      const response = await apiClient.installBuiltinMcpPlugin(settingsConfig.id, {
        source: normalizedSource,
      });
      const result = (response as any)?.data || null;
      const installedCount = typeof result?.installed === 'number' ? result.installed : null;
      const skippedCount = typeof result?.skipped === 'number' ? result.skipped : null;
      setSettingsNotice(
        `Plugin 安装完成${installedCount !== null ? `，installed: ${installedCount}` : ''}${skippedCount !== null ? `，skipped: ${skippedCount}` : ''}`
      );
      await loadBuiltinSettings(settingsConfig.id);
      setSettingsTab('overview');
    } catch (error: any) {
      setSettingsError(error?.message || '安装 plugin 失败');
    } finally {
      setSettingsSubmitting(null);
    }
  };

  const handleInstallAllPlugins = async () => {
    if (!settingsConfig) return;

    setSettingsSubmitting('plugin_all');
    setSettingsError(null);
    setSettingsNotice(null);
    try {
      const response = await apiClient.installBuiltinMcpPlugin(settingsConfig.id, {
        install_all: true,
      });
      const result = (response as any)?.data || null;
      const touchedCount = typeof result?.touched === 'number' ? result.touched : null;
      const installedCount = typeof result?.installed === 'number' ? result.installed : null;
      const skippedCount = typeof result?.skipped === 'number' ? result.skipped : null;
      setSettingsNotice(
        `批量安装完成${touchedCount !== null ? `，matched: ${touchedCount}` : ''}${installedCount !== null ? `，installed: ${installedCount}` : ''}${skippedCount !== null ? `，skipped: ${skippedCount}` : ''}`
      );
      setSettingsTab('overview');
      await loadBuiltinSettings(settingsConfig.id);
    } catch (error: any) {
      setSettingsError(error?.message || '批量安装 plugin 失败');
    } finally {
      setSettingsSubmitting(null);
    }
  };

  const settingsAgents = ((settingsSummary?.items?.agents || []) as any[]);
  const settingsSkills = ((settingsSummary?.items?.skills || []) as any[]);
  const settingsPlugins = ((settingsSummary?.items?.plugins || []) as any[]);
  const lastGitPluginDetails = ((lastGitImportResult?.results?.plugins?.details || []) as any[]);

  const settingsMcpEnabledSet = new Set(settingsMcpEnabledIds);
  const settingsMcpAllIds = settingsMcpOptions
    .map((item: any) => String(item?.id || '').trim())
    .filter(Boolean);
  const filteredSettingsMcpOptions = settingsMcpOptions.filter((item: any) => {
    const search = settingsMcpSearch.trim().toLowerCase();
    if (!search) return true;
    const haystack = [item?.display_name, item?.name, item?.id, item?.tool_prefix, item?.command]
      .map((value) => String(value || '').toLowerCase())
      .join(' ');
    return haystack.includes(search);
  });

  const isPluginInstalled = (plugin: any) => {
    if (typeof plugin?.installed === 'boolean') {
      return plugin.installed;
    }
    const installedAgents = Number(plugin?.counts?.agents?.installed ?? plugin?.agents ?? 0);
    const installedSkills = Number(plugin?.counts?.skills?.installed ?? plugin?.skills ?? 0);
    const installedCommands = Number(plugin?.counts?.commands?.installed ?? plugin?.commands ?? 0);
    return installedAgents + installedSkills + installedCommands > 0;
  };

  const pluginInstalledTotal = (plugin: any) => {
    const installedAgents = Number(plugin?.counts?.agents?.installed ?? plugin?.agents ?? 0);
    const installedSkills = Number(plugin?.counts?.skills?.installed ?? plugin?.skills ?? 0);
    const installedCommands = Number(plugin?.counts?.commands?.installed ?? plugin?.commands ?? 0);
    return installedAgents + installedSkills + installedCommands;
  };

  const pluginDiscoverableTotal = (plugin: any) => {
    const discoverableAgents = Number(plugin?.counts?.agents?.discoverable ?? 0);
    const discoverableSkills = Number(plugin?.counts?.skills?.discoverable ?? 0);
    const discoverableCommands = Number(plugin?.counts?.commands?.discoverable ?? 0);
    return discoverableAgents + discoverableSkills + discoverableCommands;
  };

  const filteredSettingsPlugins = settingsPlugins.filter((plugin: any) => {
    const search = settingsPluginSearch.trim().toLowerCase();
    if (search) {
      const haystack = [plugin?.name, plugin?.source, plugin?.category, plugin?.description]
        .map((item) => String(item || '').toLowerCase())
        .join(' ');
      if (!haystack.includes(search)) {
        return false;
      }
    }

    if (settingsPluginFilter === 'installed') {
      return isPluginInstalled(plugin);
    }
    if (settingsPluginFilter === 'pending') {
      return !isPluginInstalled(plugin);
    }
    return true;
  });

  const toggleMcpPermissionOption = (id: string, enabled: boolean) => {
    const targetId = String(id || '').trim();
    if (!targetId) return;
    setSettingsMcpEnabledIds((prev: string[]) => {
      const next = new Set(prev);
      if (enabled) {
        next.add(targetId);
      } else {
        next.delete(targetId);
      }
      return Array.from(next).sort();
    });
  };

  const enableAllMcpPermissions = () => {
    setSettingsMcpEnabledIds(Array.from(new Set(settingsMcpAllIds)).sort());
  };

  const clearAllMcpPermissions = () => {
    setSettingsMcpEnabledIds([]);
  };

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
                const supportsSettings = Boolean((config as any).supports_settings);
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
                    {supportsSettings && (
                      <button
                        onClick={() => { void openBuiltinSettings(config); }}
                        className="p-2 text-muted-foreground transition-colors hover:text-emerald-600"
                        title="设置"
                      >
                        <SettingsIcon />
                      </button>
                    )}

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

      {settingsConfig && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4">
          <div className="w-full max-w-6xl max-h-[92vh] overflow-hidden rounded-xl border border-border bg-card shadow-2xl">
            <div className="flex items-center justify-between border-b px-5 py-3">
              <div>
                <h3 className="text-base font-semibold text-foreground">内置 MCP 设置</h3>
                <p className="text-xs text-muted-foreground mt-1">
                  {(settingsConfig as any)?.display_name || settingsConfig.name}
                </p>
              </div>
              <button
                type="button"
                className="p-2 text-muted-foreground hover:text-foreground"
                onClick={closeBuiltinSettings}
              >
                <XMarkIcon />
              </button>
            </div>

            <div className="p-6 space-y-5 overflow-auto max-h-[calc(92vh-70px)]">
              {settingsLoading ? (
                <div className="text-sm text-muted-foreground">加载中...</div>
              ) : (
                <>
                  {settingsSummary && (
                    <div className="rounded-lg border border-border bg-muted/40 p-4 space-y-2">
                      <div className="text-xs text-muted-foreground">当前路径</div>
                      <div className="text-xs font-mono break-all">root: {settingsSummary?.paths?.root || '—'}</div>
                      <div className="text-xs font-mono break-all">agents: {settingsSummary?.paths?.registry || '—'}</div>
                      <div className="text-xs font-mono break-all">skills: {settingsSummary?.paths?.marketplace || '—'}</div>
                      <div className="text-xs font-mono break-all">git-cache: {settingsSummary?.paths?.git_cache_root || '—'}</div>
                      <div className="text-xs font-mono break-all">mcp-permissions: {settingsSummary?.paths?.mcp_permissions || '—'}</div>
                      <div className="text-xs text-muted-foreground pt-1">
                        已导入 agents: {settingsSummary?.counts?.agents ?? 0}（registry: {settingsSummary?.counts?.registry_agents ?? 0} / marketplace: {settingsSummary?.counts?.marketplace_agents ?? 0}），plugins: {settingsSummary?.counts?.plugins ?? 0}，skills条目: {settingsSummary?.counts?.skills_entries ?? 0}，可安装插件: {settingsSummary?.counts?.installable_plugins ?? 0}
                      </div>
                    </div>
                  )}

                  {settingsError && (
                    <div className="rounded-md border border-red-500/30 bg-red-500/10 px-3 py-2 text-xs text-red-600">
                      {settingsError}
                    </div>
                  )}

                  {settingsNotice && (
                    <div className="rounded-md border border-emerald-500/30 bg-emerald-500/10 px-3 py-2 text-xs text-emerald-600">
                      {settingsNotice}
                    </div>
                  )}

                  {settingsSummary && (
                    <div className="space-y-4">
                      <div className="grid grid-cols-1 gap-3 sm:grid-cols-4">
                        <div className="rounded-lg border border-border bg-muted/20 p-3">
                          <div className="text-xs text-muted-foreground">Agents</div>
                          <div className="mt-1 text-lg font-semibold text-foreground">{settingsSummary?.counts?.agents ?? 0}</div>
                          <div className="text-[11px] text-muted-foreground">
                            registry: {settingsSummary?.counts?.registry_agents ?? 0} / marketplace: {settingsSummary?.counts?.marketplace_agents ?? 0}
                          </div>
                        </div>
                        <div className="rounded-lg border border-border bg-muted/20 p-3">
                          <div className="text-xs text-muted-foreground">Skills</div>
                          <div className="mt-1 text-lg font-semibold text-foreground">{settingsSummary?.counts?.skills_entries ?? 0}</div>
                          <div className="text-[11px] text-muted-foreground">total entries from skills/marketplace</div>
                        </div>
                        <div className="rounded-lg border border-border bg-muted/20 p-3">
                          <div className="text-xs text-muted-foreground">Plugins</div>
                          <div className="mt-1 text-lg font-semibold text-foreground">{settingsSummary?.counts?.plugins ?? 0}</div>
                          <div className="text-[11px] text-muted-foreground">
                            installable: {settingsSummary?.counts?.installable_plugins ?? 0} / discoverable skills: {settingsSummary?.counts?.discovered_skills ?? 0}
                          </div>
                          <div className="text-[11px] text-muted-foreground">
                            registry: {settingsSummary?.valid?.registry ? 'ok' : 'invalid'} / marketplace: {settingsSummary?.valid?.marketplace ? 'ok' : 'invalid'}
                          </div>
                        </div>
                        <div className="rounded-lg border border-border bg-muted/20 p-3">
                          <div className="text-xs text-muted-foreground">MCP Permissions</div>
                          <div className="mt-1 text-lg font-semibold text-foreground">
                            {settingsMcpEnabledIds.length}/{settingsMcpOptions.length}
                          </div>
                          <div className="text-[11px] text-muted-foreground">
                            {settingsMcpConfigured ? 'customized' : 'default(all enabled)'}
                          </div>
                          <div className="text-[11px] text-muted-foreground">
                            {settingsMcpUpdatedAt ? `updated: ${settingsMcpUpdatedAt}` : 'not saved yet'}
                          </div>
                        </div>
                      </div>

                      <div className="rounded-lg border border-border bg-muted/20 p-1">
                        <div className="grid grid-cols-1 gap-1 sm:grid-cols-4">
                          <button
                            type="button"
                            onClick={() => setSettingsTab('mcp')}
                            className={`rounded-md px-3 py-2 text-xs font-medium transition-colors ${settingsTab === 'mcp' ? 'bg-background text-foreground shadow-sm' : 'text-muted-foreground hover:bg-background/50 hover:text-foreground'}`}
                          >
                            MCP 权限
                          </button>
                          <button
                            type="button"
                            onClick={() => setSettingsTab('overview')}
                            className={`rounded-md px-3 py-2 text-xs font-medium transition-colors ${settingsTab === 'overview' ? 'bg-background text-foreground shadow-sm' : 'text-muted-foreground hover:bg-background/50 hover:text-foreground'}`}
                          >
                            已导入内容
                          </button>
                          <button
                            type="button"
                            onClick={() => setSettingsTab('git')}
                            className={`rounded-md px-3 py-2 text-xs font-medium transition-colors ${settingsTab === 'git' ? 'bg-background text-foreground shadow-sm' : 'text-muted-foreground hover:bg-background/50 hover:text-foreground'}`}
                          >
                            第一步：Git 导入
                          </button>
                          <button
                            type="button"
                            onClick={() => setSettingsTab('marketplace')}
                            className={`rounded-md px-3 py-2 text-xs font-medium transition-colors ${settingsTab === 'marketplace' ? 'bg-background text-foreground shadow-sm' : 'text-muted-foreground hover:bg-background/50 hover:text-foreground'}`}
                          >
                            第二步：安装插件
                          </button>
                        </div>
                      </div>

                      {settingsTab === 'mcp' && (
                        <div className="rounded-lg border border-border bg-muted/20 p-4 space-y-4">
                          <div className="flex flex-col gap-3 lg:flex-row lg:items-start lg:justify-between">
                            <div>
                              <div className="text-sm font-medium text-foreground">Sub-agent 可用 MCP</div>
                              <div className="text-xs text-muted-foreground mt-1">
                                这里控制 sub-agent 在执行时允许访问的 MCP（不包含它自己）。
                              </div>
                            </div>
                            <div className="flex items-center gap-2">
                              <button
                                type="button"
                                onClick={enableAllMcpPermissions}
                                disabled={settingsMcpSaving || settingsMcpLoading || settingsMcpOptions.length === 0}
                                className="px-2.5 py-1.5 text-xs rounded-md border border-border bg-background hover:bg-background/80 disabled:opacity-50 disabled:cursor-not-allowed"
                              >
                                全选
                              </button>
                              <button
                                type="button"
                                onClick={clearAllMcpPermissions}
                                disabled={settingsMcpSaving || settingsMcpLoading || settingsMcpOptions.length === 0}
                                className="px-2.5 py-1.5 text-xs rounded-md border border-border bg-background hover:bg-background/80 disabled:opacity-50 disabled:cursor-not-allowed"
                              >
                                全不选
                              </button>
                              <button
                                type="button"
                                onClick={() => { void handleSaveMcpPermissions(); }}
                                disabled={settingsMcpSaving || settingsMcpLoading}
                                className="px-3 py-1.5 text-xs rounded-md bg-indigo-600 text-white hover:bg-indigo-700 disabled:opacity-50 disabled:cursor-not-allowed"
                              >
                                {settingsMcpSaving ? '保存中...' : '保存权限'}
                              </button>
                            </div>
                          </div>

                          <div className="grid grid-cols-1 gap-3 lg:grid-cols-3">
                            <input
                              type="text"
                              value={settingsMcpSearch}
                              onChange={(event) => setSettingsMcpSearch(event.target.value)}
                              placeholder="搜索 MCP 名称 / ID / 工具前缀（不区分大小写）"
                              className="h-10 w-full rounded-md border border-input bg-background px-3 text-sm outline-none focus:ring-2 focus:ring-ring lg:col-span-2"
                            />
                            <div className="h-10 rounded-md border border-border/60 bg-background/60 px-3 text-xs text-muted-foreground flex items-center justify-between">
                              <span>已选择</span>
                              <span>{settingsMcpEnabledIds.length}/{settingsMcpOptions.length}</span>
                            </div>
                          </div>

                          {settingsMcpLoading ? (
                            <div className="rounded-md border border-border/60 bg-background/60 px-3 py-4 text-xs text-muted-foreground">
                              正在加载 MCP 权限列表...
                            </div>
                          ) : filteredSettingsMcpOptions.length === 0 ? (
                            <div className="rounded-md border border-border/60 bg-background/60 px-3 py-4 text-xs text-muted-foreground">
                              没有匹配的 MCP。
                            </div>
                          ) : (
                            <div className="max-h-[52vh] space-y-2 overflow-auto pr-1">
                              {filteredSettingsMcpOptions.map((item: any) => {
                                const id = String(item?.id || '');
                                const enabled = settingsMcpEnabledSet.has(id);
                                const builtin = Boolean(item?.builtin);
                                const configEnabled = item?.config_enabled !== false;
                                const displayName = item?.display_name || item?.name || id;
                                const toolPrefix = item?.tool_prefix || '';
                                return (
                                  <label
                                    key={id}
                                    className="flex items-start gap-3 rounded-md border border-border/60 bg-background/70 px-3 py-2 cursor-pointer"
                                  >
                                    <input
                                      type="checkbox"
                                      checked={enabled}
                                      onChange={(event) => toggleMcpPermissionOption(id, event.target.checked)}
                                      className="mt-0.5 h-4 w-4 rounded border-border"
                                    />
                                    <div className="min-w-0 flex-1">
                                      <div className="flex items-center gap-2">
                                        <div className="text-sm text-foreground truncate">{displayName}</div>
                                        <span className={`rounded px-1.5 py-0.5 text-[10px] ${builtin ? 'bg-sky-500/10 text-sky-600' : 'bg-violet-500/10 text-violet-600'}`}>
                                          {builtin ? 'builtin' : 'custom'}
                                        </span>
                                        {!configEnabled && (
                                          <span className="rounded px-1.5 py-0.5 text-[10px] bg-amber-500/10 text-amber-600">
                                            config disabled
                                          </span>
                                        )}
                                      </div>
                                      <div className="mt-0.5 text-[11px] text-muted-foreground break-all">id: {id}</div>
                                      <div className="text-[11px] text-muted-foreground break-all">tool prefix: {toolPrefix || '—'}</div>
                                    </div>
                                  </label>
                                );
                              })}
                            </div>
                          )}
                        </div>
                      )}

                      {settingsTab === 'overview' && (
                        <div className="space-y-4">
                          <div className="rounded-lg border border-border bg-muted/20 p-4">
                            <div className="flex items-center justify-between">
                              <div className="text-sm font-medium text-foreground">Plugins Summary</div>
                              <div className="text-xs text-muted-foreground">{settingsPlugins.length}</div>
                            </div>
                            <div className="mt-3 max-h-44 space-y-2 overflow-auto pr-1">
                              {settingsPlugins.length === 0 ? (
                                <div className="text-xs text-muted-foreground">No plugin metadata found</div>
                              ) : (
                                settingsPlugins.map((plugin: any, idx: number) => (
                                  <div key={`${plugin?.source || plugin?.name || 'plugin'}-${idx}`} className="rounded-md border border-border/60 bg-background/60 px-2 py-1.5">
                                    <div className="text-xs text-foreground truncate">{plugin?.name || plugin?.source || 'plugin'}</div>
                                    <div className="mt-0.5 text-[11px] text-muted-foreground truncate">
                                      source: {plugin?.source || 'unknown'} | installed A/S/C: {plugin?.counts?.agents?.installed ?? plugin?.agents ?? 0}/{plugin?.counts?.skills?.installed ?? plugin?.skills ?? 0}/{plugin?.counts?.commands?.installed ?? plugin?.commands ?? 0}
                                    </div>
                                    <div className="text-[11px] text-muted-foreground truncate">
                                      discoverable A/S/C: {plugin?.counts?.agents?.discoverable ?? 0}/{plugin?.counts?.skills?.discoverable ?? 0}/{plugin?.counts?.commands?.discoverable ?? 0}
                                    </div>
                                  </div>
                                ))
                              )}
                            </div>
                          </div>

                          <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
                            <div className="rounded-lg border border-border bg-muted/20 p-4">
                              <div className="flex items-center justify-between">
                                <div className="text-sm font-medium text-foreground">Imported Agents</div>
                                <div className="text-xs text-muted-foreground">{settingsAgents.length}</div>
                              </div>
                              <div className="mt-3 max-h-56 space-y-2 overflow-auto pr-1">
                                {settingsAgents.length === 0 ? (
                                  <div className="text-xs text-muted-foreground">No agent data（先到 Marketplace 安装插件）</div>
                                ) : (
                                  settingsAgents.map((agent: any, idx: number) => (
                                    <div key={`${agent?.id || agent?.path || 'agent'}-${idx}`} className="rounded-md border border-border/60 bg-background/60 px-2 py-1.5">
                                      <div className="flex items-start justify-between gap-2">
                                        <div className="text-xs text-foreground truncate">{agent?.name || agent?.id || agent?.path || 'Unnamed Agent'}</div>
                                        <span className={`rounded px-1.5 py-0.5 text-[10px] ${agent?.kind === 'marketplace' ? 'bg-indigo-500/10 text-indigo-600' : 'bg-emerald-500/10 text-emerald-600'}`}>
                                          {agent?.kind === 'marketplace' ? 'marketplace' : 'registry'}
                                        </span>
                                      </div>
                                      <div className="mt-0.5 text-[11px] text-muted-foreground truncate">
                                        {agent?.kind === 'marketplace'
                                          ? `${agent?.plugin || 'plugin'} | ${agent?.path || ''}`
                                          : `id: ${agent?.id || ''}${agent?.category ? ` | ${agent?.category}` : ''}`}
                                      </div>
                                    </div>
                                  ))
                                )}
                              </div>
                            </div>

                            <div className="rounded-lg border border-border bg-muted/20 p-4">
                              <div className="flex items-center justify-between">
                                <div className="text-sm font-medium text-foreground">Imported Skills</div>
                                <div className="text-xs text-muted-foreground">{settingsSkills.length}</div>
                              </div>
                              <div className="mt-3 max-h-56 space-y-2 overflow-auto pr-1">
                                {settingsSkills.length === 0 ? (
                                  <div className="text-xs text-muted-foreground">No skill data（先到 Marketplace 安装插件）</div>
                                ) : (
                                  settingsSkills.map((skill: any, idx: number) => (
                                    <div key={`${skill?.id || skill?.path || 'skill'}-${idx}`} className="rounded-md border border-border/60 bg-background/60 px-2 py-1.5">
                                      <div className="text-xs text-foreground truncate">{skill?.name || skill?.path || 'Unnamed Skill'}</div>
                                      <div className="mt-0.5 text-[11px] text-muted-foreground truncate">
                                        {(skill?.plugin || 'plugin') + (skill?.path ? ` | ${skill.path}` : '')}
                                      </div>
                                    </div>
                                  ))
                                )}
                              </div>
                            </div>
                          </div>
                        </div>
                      )}

                      {settingsTab === 'marketplace' && (
                        <div className="rounded-lg border border-border bg-muted/20 p-4 space-y-4">
                          <div className="flex flex-col gap-3 lg:flex-row lg:items-center lg:justify-between">
                            <div>
                              <div className="text-sm font-medium text-foreground">Marketplace 插件安装</div>
                              <div className="text-xs text-muted-foreground mt-1">
                                先 Git 导入，再在这里安装插件，安装后会自动写入 agents / skills / commands。
                              </div>
                            </div>
                            <button
                              type="button"
                              onClick={() => { void handleInstallAllPlugins(); }}
                              disabled={settingsSubmitting !== null}
                              className="px-3 py-1.5 text-xs rounded-md bg-indigo-600 text-white hover:bg-indigo-700 disabled:opacity-50 disabled:cursor-not-allowed"
                            >
                              {settingsSubmitting === 'plugin_all' ? 'Installing...' : 'Install All Plugins'}
                            </button>
                          </div>

                          <div className="grid grid-cols-1 gap-3 lg:grid-cols-3">
                            <input
                              type="text"
                              value={settingsPluginSearch}
                              onChange={(event) => setSettingsPluginSearch(event.target.value)}
                              placeholder="搜索插件名称 / source（不区分大小写）"
                              className="h-10 w-full rounded-md border border-input bg-background px-3 text-sm outline-none focus:ring-2 focus:ring-ring lg:col-span-2"
                            />
                            <select
                              value={settingsPluginFilter}
                              onChange={(event) => setSettingsPluginFilter(event.target.value as 'all' | 'installed' | 'pending')}
                              className="h-10 w-full rounded-md border border-input bg-background px-3 text-sm outline-none focus:ring-2 focus:ring-ring"
                            >
                              <option value="all">全部</option>
                              <option value="installed">已安装</option>
                              <option value="pending">待安装</option>
                            </select>
                          </div>

                          <div className="rounded-md border border-border/60 bg-background/50 p-3 text-xs text-muted-foreground">
                            当前可安装插件: {settingsSummary?.counts?.installable_plugins ?? 0} / 可发现 agents: {settingsSummary?.counts?.discovered_agents ?? 0} / skills: {settingsSummary?.counts?.discovered_skills ?? 0} / commands: {settingsSummary?.counts?.discovered_commands ?? 0}
                          </div>

                          <div className="max-h-[54vh] space-y-2 overflow-auto pr-1">
                            {filteredSettingsPlugins.length === 0 ? (
                              <div className="rounded-md border border-border/60 bg-background/60 px-3 py-2 text-xs text-muted-foreground">
                                没有匹配的插件。
                              </div>
                            ) : (
                              filteredSettingsPlugins.map((plugin: any, idx: number) => {
                                const installed = isPluginInstalled(plugin);
                                const exists = plugin?.exists !== false;
                                const discoverableTotal = pluginDiscoverableTotal(plugin);
                                const installedTotal = pluginInstalledTotal(plugin);
                                const discoverableAgents = Number(plugin?.counts?.agents?.discoverable ?? 0);
                                const discoverableSkills = Number(plugin?.counts?.skills?.discoverable ?? 0);
                                const discoverableCommands = Number(plugin?.counts?.commands?.discoverable ?? 0);
                                const installedAgents = Number(plugin?.counts?.agents?.installed ?? plugin?.agents ?? 0);
                                const installedSkills = Number(plugin?.counts?.skills?.installed ?? plugin?.skills ?? 0);
                                const installedCommands = Number(plugin?.counts?.commands?.installed ?? plugin?.commands ?? 0);
                                const source = String(plugin?.source || '');
                                const canInstall = exists && discoverableTotal > 0;

                                return (
                                  <div
                                    key={`${source || plugin?.name || 'plugin'}-${idx}`}
                                    className="rounded-md border border-border/60 bg-background/70 px-3 py-2"
                                  >
                                    <div className="flex items-start justify-between gap-3">
                                      <div className="min-w-0 flex-1">
                                        <div className="flex items-center gap-2">
                                          <div className="text-sm text-foreground truncate">{plugin?.name || source || 'plugin'}</div>
                                          <span
                                            className={`rounded px-1.5 py-0.5 text-[10px] ${installed ? 'bg-emerald-500/10 text-emerald-600' : 'bg-amber-500/10 text-amber-600'}`}
                                          >
                                            {installed ? 'installed' : 'pending'}
                                          </span>
                                          {!exists && (
                                            <span className="rounded px-1.5 py-0.5 text-[10px] bg-red-500/10 text-red-600">
                                              source missing
                                            </span>
                                          )}
                                        </div>
                                        <div className="mt-0.5 text-[11px] text-muted-foreground break-all">{source || 'unknown source'}</div>
                                        <div className="mt-1 text-[11px] text-muted-foreground">
                                          installed A/S/C: {installedAgents}/{installedSkills}/{installedCommands}（total: {installedTotal}）
                                        </div>
                                        <div className="text-[11px] text-muted-foreground">
                                          discoverable A/S/C: {discoverableAgents}/{discoverableSkills}/{discoverableCommands}（total: {discoverableTotal}）
                                        </div>
                                      </div>
                                      <button
                                        type="button"
                                        onClick={() => { void handleInstallPlugin(source); }}
                                        disabled={settingsSubmitting !== null || !canInstall}
                                        className="shrink-0 px-2.5 py-1.5 text-[11px] rounded-md bg-indigo-600 text-white hover:bg-indigo-700 disabled:opacity-50 disabled:cursor-not-allowed"
                                      >
                                        {settingsSubmitting === 'plugin' ? 'Installing...' : installed ? 'Reinstall' : 'Install'}
                                      </button>
                                    </div>
                                  </div>
                                );
                              })
                            )}
                          </div>
                        </div>
                      )}

                      {settingsTab === 'git' && (
                        <div className="rounded-lg border border-border bg-muted/25 p-4 space-y-4">
                          <div className="space-y-1">
                            <div className="text-base font-semibold text-foreground">Import agents and skills from Git</div>
                            <div className="text-xs text-muted-foreground">Auto-detects subagents.json/agents.json and marketplace.json/skills.json.</div>
                          </div>

                          <div className="grid grid-cols-1 gap-3 lg:grid-cols-2">
                            <input
                              type="text"
                              value={gitRepositoryInput}
                              onChange={(event) => setGitRepositoryInput(event.target.value)}
                              placeholder="Repository URL, e.g. https://github.com/org/repo.git"
                              className="lg:col-span-2 h-10 w-full rounded-md border border-input bg-background px-3 text-sm outline-none focus:ring-2 focus:ring-ring"
                            />
                            <input
                              type="text"
                              value={gitBranchInput}
                              onChange={(event) => setGitBranchInput(event.target.value)}
                              placeholder="Branch (optional, default branch if empty)"
                              className="h-10 w-full rounded-md border border-input bg-background px-3 text-sm outline-none focus:ring-2 focus:ring-ring"
                            />
                            <input
                              type="text"
                              value={gitAgentsPathInput}
                              onChange={(event) => setGitAgentsPathInput(event.target.value)}
                              placeholder="agents file path (optional)"
                              className="h-10 w-full rounded-md border border-input bg-background px-3 text-sm outline-none focus:ring-2 focus:ring-ring"
                            />
                            <input
                              type="text"
                              value={gitSkillsPathInput}
                              onChange={(event) => setGitSkillsPathInput(event.target.value)}
                              placeholder="skills/marketplace file path (optional)"
                              className="lg:col-span-2 h-10 w-full rounded-md border border-input bg-background px-3 text-sm outline-none focus:ring-2 focus:ring-ring"
                            />
                          </div>

                          <div className="rounded-md border border-border/60 bg-background/50 p-3 text-xs text-muted-foreground space-y-1">
                            <div className="font-medium text-foreground">Auto-detection rules</div>
                            <div>agents: subagents.json ? agents.json</div>
                            <div>skills: marketplace.json ? skills.json</div>
                            <div>If your repo layout is custom, fill the path fields above.</div>
                          </div>

                          <button
                            type="button"
                            onClick={() => { void handleImportFromGit(); }}
                            disabled={settingsSubmitting !== null}
                            className="px-4 py-2 text-sm rounded-md bg-indigo-600 text-white hover:bg-indigo-700 disabled:opacity-50 disabled:cursor-not-allowed"
                          >
                            {settingsSubmitting === 'git' ? 'Importing...' : 'Import from Git'}
                          </button>

                          {lastGitImportResult && (
                            <div className="rounded-md border border-border bg-background/70 p-3 space-y-3">
                              <div className="text-xs font-medium text-foreground">Last Git import result</div>
                              <div className="grid grid-cols-1 gap-2 lg:grid-cols-2 text-[11px] text-muted-foreground">
                                <div className="font-mono break-all">repo: {lastGitImportResult?.repository || '?'}</div>
                                <div className="font-mono break-all">branch: {lastGitImportResult?.branch || 'default'}</div>
                                <div className="font-mono break-all lg:col-span-2">repo-path: {lastGitImportResult?.repo_path || '?'}</div>
                                <div className="font-mono break-all">agents-file: {lastGitImportResult?.files?.agents || 'not found'}</div>
                                <div className="font-mono break-all">skills-file: {lastGitImportResult?.files?.skills || 'not found'}</div>
                              </div>
                              <div className="text-[11px] text-muted-foreground">
                                imported.agents: {String(!!lastGitImportResult?.imported?.agents)} | imported.skills: {String(!!lastGitImportResult?.imported?.skills)} | plugins copied: {lastGitImportResult?.results?.plugins?.copied ?? 0} | plugins skipped: {lastGitImportResult?.results?.plugins?.skipped ?? 0}
                              </div>
                              {lastGitPluginDetails.length > 0 && (
                                <div className="max-h-40 overflow-auto space-y-1 pr-1">
                                  {lastGitPluginDetails.map((item: any, idx: number) => (
                                    <div key={`${item?.source || 'plugin'}-${idx}`} className="rounded border border-border/60 bg-muted/20 px-2 py-1 text-[11px] text-muted-foreground">
                                      <span className={item?.ok ? 'text-emerald-600' : 'text-amber-600'}>{item?.ok ? 'ok' : 'skip'}</span>
                                      {' | '}
                                      {item?.source || '(empty source)'}
                                      {item?.dest ? ` -> ${item.dest}` : ''}
                                      {item?.reason ? ` (${item.reason})` : ''}
                                    </div>
                                  ))}
                                </div>
                              )}
                            </div>
                          )}
                        </div>
                      )}

                    </div>
                  )}
                </>
              )}
            </div>
          </div>
        </div>
      )}

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
