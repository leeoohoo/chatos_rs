import React from 'react';
import type { McpConfig } from '../../types';

type SettingsTab = 'mcp' | 'overview' | 'git' | 'marketplace';
type SettingsSubmitting = 'git' | 'plugin' | 'plugin_all' | null;

interface BuiltinSettingsModalState {
  settingsConfig: McpConfig | null;
  settingsLoading: boolean;
  settingsSummary: any;
  settingsError: string | null;
  settingsNotice: string | null;
  settingsMcpEnabledIds: string[];
  settingsMcpOptions: any[];
  settingsMcpConfigured: boolean;
  settingsMcpUpdatedAt: string | null;
  settingsTab: SettingsTab;
  settingsMcpSaving: boolean;
  settingsMcpLoading: boolean;
  settingsSelectedSystemContextId: string;
  settingsSystemContextOptions: any[];
  settingsMcpSearch: string;
  filteredSettingsMcpOptions: any[];
  settingsMcpEnabledSet: Set<string>;
  settingsPlugins: any[];
  settingsAgents: any[];
  settingsSkills: any[];
  settingsSubmitting: SettingsSubmitting;
  settingsPluginSearch: string;
  settingsPluginFilter: 'all' | 'installed' | 'pending';
  filteredSettingsPlugins: any[];
  gitRepositoryInput: string;
  gitBranchInput: string;
  gitAgentsPathInput: string;
  gitSkillsPathInput: string;
  lastGitImportResult: any;
  lastGitPluginDetails: any[];
}

interface BuiltinSettingsModalActions {
  onClose: () => void;
  onSettingsTabChange: (tab: SettingsTab) => void;
  onEnableAllMcpPermissions: () => void;
  onClearAllMcpPermissions: () => void;
  onSaveMcpPermissions: () => void;
  onSettingsSelectedSystemContextIdChange: (value: string) => void;
  onSettingsMcpSearchChange: (value: string) => void;
  onToggleMcpPermissionOption: (id: string, checked: boolean) => void;
  onInstallAllPlugins: () => void;
  onSettingsPluginSearchChange: (value: string) => void;
  onSettingsPluginFilterChange: (value: 'all' | 'installed' | 'pending') => void;
  onInstallPlugin: (source: string) => void;
  isPluginInstalled: (plugin: any) => boolean;
  pluginDiscoverableTotal: (plugin: any) => number;
  pluginInstalledTotal: (plugin: any) => number;
  onGitRepositoryInputChange: (value: string) => void;
  onGitBranchInputChange: (value: string) => void;
  onGitAgentsPathInputChange: (value: string) => void;
  onGitSkillsPathInputChange: (value: string) => void;
  onImportFromGit: () => void;
}

interface BuiltinSettingsModalProps {
  state: BuiltinSettingsModalState;
  actions: BuiltinSettingsModalActions;
}

const XMarkIcon = () => (
  <svg className="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
  </svg>
);

const BuiltinSettingsModal: React.FC<BuiltinSettingsModalProps> = ({ state, actions }) => {
  if (!state.settingsConfig) {
    return null;
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4">
      <div className="w-full max-w-6xl max-h-[92vh] overflow-hidden rounded-xl border border-border bg-card shadow-2xl">
        <div className="flex items-center justify-between border-b px-5 py-3">
          <div>
            <h3 className="text-base font-semibold text-foreground">内置 MCP 设置</h3>
            <p className="text-xs text-muted-foreground mt-1">
              {(state.settingsConfig as any)?.display_name || state.settingsConfig.name}
            </p>
          </div>
          <button
            type="button"
            className="p-2 text-muted-foreground hover:text-foreground"
            onClick={actions.onClose}
          >
            <XMarkIcon />
          </button>
        </div>

        <div className="p-6 space-y-5 overflow-auto max-h-[calc(92vh-70px)]">
          {state.settingsLoading ? (
            <div className="text-sm text-muted-foreground">加载中...</div>
          ) : (
            <>
              {state.settingsSummary && (
                <div className="rounded-lg border border-border bg-muted/40 p-4 space-y-2">
                  <div className="text-xs text-muted-foreground">当前路径</div>
                  <div className="text-xs font-mono break-all">root: {state.settingsSummary?.paths?.root || '—'}</div>
                  <div className="text-xs font-mono break-all">agents: {state.settingsSummary?.paths?.registry || '—'}</div>
                  <div className="text-xs font-mono break-all">skills: {state.settingsSummary?.paths?.marketplace || '—'}</div>
                  <div className="text-xs font-mono break-all">git-cache: {state.settingsSummary?.paths?.git_cache_root || '—'}</div>
                  <div className="text-xs font-mono break-all">mcp-permissions: {state.settingsSummary?.paths?.mcp_permissions || '—'}</div>
                  <div className="text-xs text-muted-foreground pt-1">
                    已导入 agents: {state.settingsSummary?.counts?.agents ?? 0}（registry: {state.settingsSummary?.counts?.registry_agents ?? 0} / marketplace: {state.settingsSummary?.counts?.marketplace_agents ?? 0}），plugins: {state.settingsSummary?.counts?.plugins ?? 0}，skills条目: {state.settingsSummary?.counts?.skills_entries ?? 0}，可安装插件: {state.settingsSummary?.counts?.installable_plugins ?? 0}
                  </div>
                </div>
              )}

              {state.settingsError && (
                <div className="rounded-md border border-red-500/30 bg-red-500/10 px-3 py-2 text-xs text-red-600">
                  {state.settingsError}
                </div>
              )}

              {state.settingsNotice && (
                <div className="rounded-md border border-emerald-500/30 bg-emerald-500/10 px-3 py-2 text-xs text-emerald-600">
                  {state.settingsNotice}
                </div>
              )}

              {state.settingsSummary && (
                <div className="space-y-4">
                  <div className="grid grid-cols-1 gap-3 sm:grid-cols-4">
                    <div className="rounded-lg border border-border bg-muted/20 p-3">
                      <div className="text-xs text-muted-foreground">Agents</div>
                      <div className="mt-1 text-lg font-semibold text-foreground">{state.settingsSummary?.counts?.agents ?? 0}</div>
                      <div className="text-[11px] text-muted-foreground">
                        registry: {state.settingsSummary?.counts?.registry_agents ?? 0} / marketplace: {state.settingsSummary?.counts?.marketplace_agents ?? 0}
                      </div>
                    </div>
                    <div className="rounded-lg border border-border bg-muted/20 p-3">
                      <div className="text-xs text-muted-foreground">Skills</div>
                      <div className="mt-1 text-lg font-semibold text-foreground">{state.settingsSummary?.counts?.skills_entries ?? 0}</div>
                      <div className="text-[11px] text-muted-foreground">total entries from skills/marketplace</div>
                    </div>
                    <div className="rounded-lg border border-border bg-muted/20 p-3">
                      <div className="text-xs text-muted-foreground">Plugins</div>
                      <div className="mt-1 text-lg font-semibold text-foreground">{state.settingsSummary?.counts?.plugins ?? 0}</div>
                      <div className="text-[11px] text-muted-foreground">
                        installable: {state.settingsSummary?.counts?.installable_plugins ?? 0} / discoverable skills: {state.settingsSummary?.counts?.discovered_skills ?? 0}
                      </div>
                      <div className="text-[11px] text-muted-foreground">
                        registry: {state.settingsSummary?.valid?.registry ? 'ok' : 'invalid'} / marketplace: {state.settingsSummary?.valid?.marketplace ? 'ok' : 'invalid'}
                      </div>
                    </div>
                    <div className="rounded-lg border border-border bg-muted/20 p-3">
                      <div className="text-xs text-muted-foreground">MCP Permissions</div>
                      <div className="mt-1 text-lg font-semibold text-foreground">
                        {state.settingsMcpEnabledIds.length}/{state.settingsMcpOptions.length}
                      </div>
                      <div className="text-[11px] text-muted-foreground">
                        {state.settingsMcpConfigured ? 'customized' : 'default(all enabled)'}
                      </div>
                      <div className="text-[11px] text-muted-foreground">
                        {state.settingsMcpUpdatedAt ? `updated: ${state.settingsMcpUpdatedAt}` : 'not saved yet'}
                      </div>
                    </div>
                  </div>

                  <div className="rounded-lg border border-border bg-muted/20 p-1">
                    <div className="grid grid-cols-1 gap-1 sm:grid-cols-4">
                      <button
                        type="button"
                        onClick={() => actions.onSettingsTabChange('mcp')}
                        className={`rounded-md px-3 py-2 text-xs font-medium transition-colors ${state.settingsTab === 'mcp' ? 'bg-background text-foreground shadow-sm' : 'text-muted-foreground hover:bg-background/50 hover:text-foreground'}`}
                      >
                        MCP 权限
                      </button>
                      <button
                        type="button"
                        onClick={() => actions.onSettingsTabChange('overview')}
                        className={`rounded-md px-3 py-2 text-xs font-medium transition-colors ${state.settingsTab === 'overview' ? 'bg-background text-foreground shadow-sm' : 'text-muted-foreground hover:bg-background/50 hover:text-foreground'}`}
                      >
                        已导入内容
                      </button>
                      <button
                        type="button"
                        onClick={() => actions.onSettingsTabChange('git')}
                        className={`rounded-md px-3 py-2 text-xs font-medium transition-colors ${state.settingsTab === 'git' ? 'bg-background text-foreground shadow-sm' : 'text-muted-foreground hover:bg-background/50 hover:text-foreground'}`}
                      >
                        第一步：Git 导入
                      </button>
                      <button
                        type="button"
                        onClick={() => actions.onSettingsTabChange('marketplace')}
                        className={`rounded-md px-3 py-2 text-xs font-medium transition-colors ${state.settingsTab === 'marketplace' ? 'bg-background text-foreground shadow-sm' : 'text-muted-foreground hover:bg-background/50 hover:text-foreground'}`}
                      >
                        第二步：安装插件
                      </button>
                    </div>
                  </div>

                  {state.settingsTab === 'mcp' && (
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
                            onClick={actions.onEnableAllMcpPermissions}
                            disabled={state.settingsMcpSaving || state.settingsMcpLoading || state.settingsMcpOptions.length === 0}
                            className="px-2.5 py-1.5 text-xs rounded-md border border-border bg-background hover:bg-background/80 disabled:opacity-50 disabled:cursor-not-allowed"
                          >
                            全选
                          </button>
                          <button
                            type="button"
                            onClick={actions.onClearAllMcpPermissions}
                            disabled={state.settingsMcpSaving || state.settingsMcpLoading || state.settingsMcpOptions.length === 0}
                            className="px-2.5 py-1.5 text-xs rounded-md border border-border bg-background hover:bg-background/80 disabled:opacity-50 disabled:cursor-not-allowed"
                          >
                            全不选
                          </button>
                          <button
                            type="button"
                            onClick={actions.onSaveMcpPermissions}
                            disabled={state.settingsMcpSaving || state.settingsMcpLoading}
                            className="px-3 py-1.5 text-xs rounded-md bg-indigo-600 text-white hover:bg-indigo-700 disabled:opacity-50 disabled:cursor-not-allowed"
                          >
                            {state.settingsMcpSaving ? '保存中...' : '保存权限'}
                          </button>
                        </div>
                      </div>

                      <div className="rounded-md border border-border/60 bg-background/60 p-3 space-y-2">
                        <div className="text-xs font-medium text-foreground">Sub-agent 系统提示词</div>
                        <select
                          value={state.settingsSelectedSystemContextId}
                          onChange={(event) => actions.onSettingsSelectedSystemContextIdChange(event.target.value)}
                          className="h-10 w-full rounded-md border border-input bg-background px-3 text-sm outline-none focus:ring-2 focus:ring-ring"
                        >
                          <option value="">不使用额外系统提示词</option>
                          {state.settingsSystemContextOptions.map((item: any) => (
                            <option key={item.id} value={item.id}>
                              {item.name || item.id}{item.is_active ? '（当前激活）' : ''}
                            </option>
                          ))}
                        </select>
                        <div className="text-[11px] text-muted-foreground">
                          执行 sub-agent 时会先注入该系统提示词，再执行 sub-agent 自身提示词。
                        </div>
                      </div>

                      <div className="grid grid-cols-1 gap-3 lg:grid-cols-3">
                        <input
                          type="text"
                          value={state.settingsMcpSearch}
                          onChange={(event) => actions.onSettingsMcpSearchChange(event.target.value)}
                          placeholder="搜索 MCP 名称 / ID / 工具前缀（不区分大小写）"
                          className="h-10 w-full rounded-md border border-input bg-background px-3 text-sm outline-none focus:ring-2 focus:ring-ring lg:col-span-2"
                        />
                        <div className="h-10 rounded-md border border-border/60 bg-background/60 px-3 text-xs text-muted-foreground flex items-center justify-between">
                          <span>已选择</span>
                          <span>{state.settingsMcpEnabledIds.length}/{state.settingsMcpOptions.length}</span>
                        </div>
                      </div>

                      {state.settingsMcpLoading ? (
                        <div className="rounded-md border border-border/60 bg-background/60 px-3 py-4 text-xs text-muted-foreground">
                          正在加载 MCP 权限列表...
                        </div>
                      ) : state.filteredSettingsMcpOptions.length === 0 ? (
                        <div className="rounded-md border border-border/60 bg-background/60 px-3 py-4 text-xs text-muted-foreground">
                          没有匹配的 MCP。
                        </div>
                      ) : (
                        <div className="max-h-[52vh] space-y-2 overflow-auto pr-1">
                          {state.filteredSettingsMcpOptions.map((item: any) => {
                            const id = String(item?.id || '');
                            const enabled = state.settingsMcpEnabledSet.has(id);
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
                                  onChange={(event) => actions.onToggleMcpPermissionOption(id, event.target.checked)}
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

                  {state.settingsTab === 'overview' && (
                    <div className="space-y-4">
                      <div className="rounded-lg border border-border bg-muted/20 p-4">
                        <div className="flex items-center justify-between">
                          <div className="text-sm font-medium text-foreground">Plugins Summary</div>
                          <div className="text-xs text-muted-foreground">{state.settingsPlugins.length}</div>
                        </div>
                        <div className="mt-3 max-h-44 space-y-2 overflow-auto pr-1">
                          {state.settingsPlugins.length === 0 ? (
                            <div className="text-xs text-muted-foreground">No plugin metadata found</div>
                          ) : (
                            state.settingsPlugins.map((plugin: any, idx: number) => (
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
                            <div className="text-xs text-muted-foreground">{state.settingsAgents.length}</div>
                          </div>
                          <div className="mt-3 max-h-56 space-y-2 overflow-auto pr-1">
                            {state.settingsAgents.length === 0 ? (
                              <div className="text-xs text-muted-foreground">No agent data（先到 Marketplace 安装插件）</div>
                            ) : (
                              state.settingsAgents.map((agent: any, idx: number) => (
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
                            <div className="text-xs text-muted-foreground">{state.settingsSkills.length}</div>
                          </div>
                          <div className="mt-3 max-h-56 space-y-2 overflow-auto pr-1">
                            {state.settingsSkills.length === 0 ? (
                              <div className="text-xs text-muted-foreground">No skill data（先到 Marketplace 安装插件）</div>
                            ) : (
                              state.settingsSkills.map((skill: any, idx: number) => (
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

                  {state.settingsTab === 'marketplace' && (
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
                          onClick={actions.onInstallAllPlugins}
                          disabled={state.settingsSubmitting !== null}
                          className="px-3 py-1.5 text-xs rounded-md bg-indigo-600 text-white hover:bg-indigo-700 disabled:opacity-50 disabled:cursor-not-allowed"
                        >
                          {state.settingsSubmitting === 'plugin_all' ? 'Installing...' : 'Install All Plugins'}
                        </button>
                      </div>

                      <div className="grid grid-cols-1 gap-3 lg:grid-cols-3">
                        <input
                          type="text"
                          value={state.settingsPluginSearch}
                          onChange={(event) => actions.onSettingsPluginSearchChange(event.target.value)}
                          placeholder="搜索插件名称 / source（不区分大小写）"
                          className="h-10 w-full rounded-md border border-input bg-background px-3 text-sm outline-none focus:ring-2 focus:ring-ring lg:col-span-2"
                        />
                        <select
                          value={state.settingsPluginFilter}
                          onChange={(event) => actions.onSettingsPluginFilterChange(event.target.value as 'all' | 'installed' | 'pending')}
                          className="h-10 w-full rounded-md border border-input bg-background px-3 text-sm outline-none focus:ring-2 focus:ring-ring"
                        >
                          <option value="all">全部</option>
                          <option value="installed">已安装</option>
                          <option value="pending">待安装</option>
                        </select>
                      </div>

                      <div className="rounded-md border border-border/60 bg-background/50 p-3 text-xs text-muted-foreground">
                        当前可安装插件: {state.settingsSummary?.counts?.installable_plugins ?? 0} / 可发现 agents: {state.settingsSummary?.counts?.discovered_agents ?? 0} / skills: {state.settingsSummary?.counts?.discovered_skills ?? 0} / commands: {state.settingsSummary?.counts?.discovered_commands ?? 0}
                      </div>

                      <div className="max-h-[54vh] space-y-2 overflow-auto pr-1">
                        {state.filteredSettingsPlugins.length === 0 ? (
                          <div className="rounded-md border border-border/60 bg-background/60 px-3 py-2 text-xs text-muted-foreground">
                            没有匹配的插件。
                          </div>
                        ) : (
                          state.filteredSettingsPlugins.map((plugin: any, idx: number) => {
                            const installed = actions.isPluginInstalled(plugin);
                            const exists = plugin?.exists !== false;
                            const discoverableTotal = actions.pluginDiscoverableTotal(plugin);
                            const installedTotal = actions.pluginInstalledTotal(plugin);
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
                                    onClick={() => actions.onInstallPlugin(source)}
                                    disabled={state.settingsSubmitting !== null || !canInstall}
                                    className="shrink-0 px-2.5 py-1.5 text-[11px] rounded-md bg-indigo-600 text-white hover:bg-indigo-700 disabled:opacity-50 disabled:cursor-not-allowed"
                                  >
                                    {state.settingsSubmitting === 'plugin' ? 'Installing...' : installed ? 'Reinstall' : 'Install'}
                                  </button>
                                </div>
                              </div>
                            );
                          })
                        )}
                      </div>
                    </div>
                  )}

                  {state.settingsTab === 'git' && (
                    <div className="rounded-lg border border-border bg-muted/25 p-4 space-y-4">
                      <div className="space-y-1">
                        <div className="text-base font-semibold text-foreground">Import agents and skills from Git</div>
                        <div className="text-xs text-muted-foreground">Auto-detects subagents.json/agents.json and marketplace.json/skills.json.</div>
                      </div>

                      <div className="grid grid-cols-1 gap-3 lg:grid-cols-2">
                        <input
                          type="text"
                          value={state.gitRepositoryInput}
                          onChange={(event) => actions.onGitRepositoryInputChange(event.target.value)}
                          placeholder="Repository URL, e.g. https://github.com/org/repo.git"
                          className="lg:col-span-2 h-10 w-full rounded-md border border-input bg-background px-3 text-sm outline-none focus:ring-2 focus:ring-ring"
                        />
                        <input
                          type="text"
                          value={state.gitBranchInput}
                          onChange={(event) => actions.onGitBranchInputChange(event.target.value)}
                          placeholder="Branch (optional, default branch if empty)"
                          className="h-10 w-full rounded-md border border-input bg-background px-3 text-sm outline-none focus:ring-2 focus:ring-ring"
                        />
                        <input
                          type="text"
                          value={state.gitAgentsPathInput}
                          onChange={(event) => actions.onGitAgentsPathInputChange(event.target.value)}
                          placeholder="agents file path (optional)"
                          className="h-10 w-full rounded-md border border-input bg-background px-3 text-sm outline-none focus:ring-2 focus:ring-ring"
                        />
                        <input
                          type="text"
                          value={state.gitSkillsPathInput}
                          onChange={(event) => actions.onGitSkillsPathInputChange(event.target.value)}
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
                        onClick={actions.onImportFromGit}
                        disabled={state.settingsSubmitting !== null}
                        className="px-4 py-2 text-sm rounded-md bg-indigo-600 text-white hover:bg-indigo-700 disabled:opacity-50 disabled:cursor-not-allowed"
                      >
                        {state.settingsSubmitting === 'git' ? 'Importing...' : 'Import from Git'}
                      </button>

                      {state.lastGitImportResult && (
                        <div className="rounded-md border border-border bg-background/70 p-3 space-y-3">
                          <div className="text-xs font-medium text-foreground">Last Git import result</div>
                          <div className="grid grid-cols-1 gap-2 lg:grid-cols-2 text-[11px] text-muted-foreground">
                            <div className="font-mono break-all">repo: {state.lastGitImportResult?.repository || '?'}</div>
                            <div className="font-mono break-all">branch: {state.lastGitImportResult?.branch || 'default'}</div>
                            <div className="font-mono break-all lg:col-span-2">repo-path: {state.lastGitImportResult?.repo_path || '?'}</div>
                            <div className="font-mono break-all">agents-file: {state.lastGitImportResult?.files?.agents || 'not found'}</div>
                            <div className="font-mono break-all">skills-file: {state.lastGitImportResult?.files?.skills || 'not found'}</div>
                          </div>
                          <div className="text-[11px] text-muted-foreground">
                            imported.agents: {String(!!state.lastGitImportResult?.imported?.agents)} | imported.skills: {String(!!state.lastGitImportResult?.imported?.skills)} | plugins copied: {state.lastGitImportResult?.results?.plugins?.copied ?? 0} | plugins skipped: {state.lastGitImportResult?.results?.plugins?.skipped ?? 0}
                          </div>
                          {state.lastGitPluginDetails.length > 0 && (
                            <div className="max-h-40 overflow-auto space-y-1 pr-1">
                              {state.lastGitPluginDetails.map((item: any, idx: number) => (
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
  );
};

export default BuiltinSettingsModal;
