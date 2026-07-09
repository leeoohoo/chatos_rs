// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';
import ReactDOM from 'react-dom/client';
import {
  Activity,
  BellRing,
  Brain,
  CheckCircle2,
  CloudOff,
  Container,
  Cpu,
  FolderOpen,
  HardDrive,
  Image,
  Layers,
  ListChecks,
  KeyRound,
  Play,
  RefreshCw,
  Server,
  Settings2,
  Shield,
  ShieldCheck,
  Terminal,
  Trash2,
  XCircle,
} from 'lucide-react';

import {
  api,
  type ApprovalMode,
  type ApprovalSettings,
  type ConnectorStatus,
  type LocalModelConfig,
  type LocalModelCatalogResponse,
  type LocalModelSettings,
  type LocalRuntimeSettings,
  type PendingApprovalItem,
  type SandboxImageCatalog,
  type SandboxImageJob,
  type SandboxLease,
} from './api';
import {
  AuthPanel,
  ConnectionCard,
  LocalBoundaryPanel,
  WorkspacePanel,
} from './components/ConnectionPanels';
import { TerminalPanel } from './components/TerminalPanel';
import {
  approvalDecisionClass,
  approvalDecisionLabel,
  approvalModeDescription,
  approvalModeLabel,
  decisionSourceLabel,
  projectLabel,
  riskLabel,
  riskStatusClass,
} from './utils/approvalFormat';
import {
  formatHistoryTime,
  sourceLabel,
} from './utils/terminalFormat';
import {
  buildImportedModelConfigPayload,
  buildModelConfigPayload,
  buildProviderPreviewPayload,
  buildTaskModelConfigPayload,
  emptyModelDraft,
  emptyTaskModelDraft,
  findExistingImportedModel,
  formatProviderModelOption,
  groupLocalModelProviders,
  normalizeThinkingLevelForProvider,
  numericInput,
  providerLabel,
  taskDraftChanged,
  taskDraftFromModel,
  thinkingOptionsForProvider,
  thinkingValueForProvider,
  type LocalModelProviderGroup,
  type ModelDraftState,
  type TaskModelDraft,
} from './utils/modelConfigState';
import './styles.css';

const DEFAULT_AI_AGENT_MAX_ITERATIONS = 600;
type AppTab = 'overview' | 'workspaces' | 'terminal' | 'models' | 'approval' | 'settings' | 'sandbox';
type LocalIcon = typeof Server;

function App() {
  const [status, setStatus] = React.useState<ConnectorStatus | null>(null);
  const [loading, setLoading] = React.useState(true);
  const [error, setError] = React.useState<string | null>(null);
  const [activeTab, setActiveTab] = React.useState<AppTab>('workspaces');

  const refresh = React.useCallback(async () => {
    setError(null);
    try {
      setStatus(await api.status());
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Local Connector Core 未启动');
    } finally {
      setLoading(false);
    }
  }, []);

  React.useEffect(() => {
    void refresh();
  }, [refresh]);

  if (loading) {
    return <div className="screen center">正在连接本机 Local Connector Core...</div>;
  }

  return (
    <div className="screen">
      <header className="topbar">
        <div>
          <h1>ChatOS Local Connector</h1>
          <p>本地目录、终端和沙箱能力只在这台电脑上授权。</p>
        </div>
        <button className="iconButton" onClick={() => void refresh()} title="刷新">
          <RefreshCw size={18} />
        </button>
      </header>

      {error ? <div className="banner error">{error}</div> : null}

      {!status?.configured ? (
        <AuthPanel onDone={setStatus} />
      ) : (
        <main className="workbench">
          <TabNav activeTab={activeTab} onChange={setActiveTab} />
          {activeTab === 'overview' ? (
            <div className="tabGrid">
              <ConnectionCard status={status} onStatus={setStatus} />
              <LocalBoundaryPanel status={status} />
            </div>
          ) : null}
          {activeTab === 'workspaces' ? <WorkspacePanel status={status} onStatus={setStatus} /> : null}
          {activeTab === 'terminal' ? <TerminalPanel status={status} /> : null}
          {activeTab === 'models' ? <ModelConfigPanel /> : null}
          {activeTab === 'approval' ? <ApprovalPanel /> : null}
          {activeTab === 'settings' ? <RuntimeSettingsPanel /> : null}
          {activeTab === 'sandbox' ? (
            <SandboxPanel status={status} onStatus={setStatus} onRefresh={refresh} />
          ) : null}
        </main>
      )}
    </div>
  );
}

function TabNav({
  activeTab,
  onChange,
}: {
  activeTab: AppTab;
  onChange: (tab: AppTab) => void;
}) {
  const tabs: Array<{ id: AppTab; label: string; icon: LocalIcon }> = [
    { id: 'overview', label: '配对', icon: Server },
    { id: 'workspaces', label: '目录', icon: FolderOpen },
    { id: 'terminal', label: '终端', icon: Terminal },
    { id: 'models', label: '模型', icon: Brain },
    { id: 'approval', label: '审批', icon: ShieldCheck },
    { id: 'settings', label: '配置', icon: Settings2 },
    { id: 'sandbox', label: '沙箱', icon: Shield },
  ];
  return (
    <nav className="tabs" aria-label="Local Connector sections">
      {tabs.map((tab) => {
        const Icon = tab.icon;
        return (
          <button
            key={tab.id}
            type="button"
            className={activeTab === tab.id ? 'active' : ''}
            onClick={() => onChange(tab.id)}
          >
            <Icon size={16} />
            {tab.label}
          </button>
        );
      })}
    </nav>
  );
}

function ApprovalPanel() {
  const [settings, setSettings] = React.useState<ApprovalSettings | null>(null);
  const [pending, setPending] = React.useState<PendingApprovalItem[]>([]);
  const [reviewing, setReviewing] = React.useState<PendingApprovalItem[]>([]);
  const [rememberAllow, setRememberAllow] = React.useState<Record<string, boolean>>({});
  const [denyReasons, setDenyReasons] = React.useState<Record<string, string>>({});
  const [loading, setLoading] = React.useState(true);
  const [saving, setSaving] = React.useState(false);
  const [message, setMessage] = React.useState<string | null>(null);
  const [error, setError] = React.useState<string | null>(null);

  const loadSettings = React.useCallback(async () => {
    setError(null);
    try {
      const [nextSettings, nextPending] = await Promise.all([
        api.approvalSettings(),
        api.pendingApprovals(),
      ]);
      setSettings(nextSettings);
      setPending(nextPending.items);
      setReviewing(nextPending.reviewing || []);
    } catch (err) {
      setError(err instanceof Error ? err.message : '读取审批设置失败');
    } finally {
      setLoading(false);
    }
  }, []);

  const loadPending = React.useCallback(async () => {
    try {
      const nextPending = await api.pendingApprovals();
      setPending(nextPending.items);
      setReviewing(nextPending.reviewing || []);
    } catch {
      // The settings fetch path surfaces connection errors; polling stays quiet.
    }
  }, []);

  React.useEffect(() => {
    void loadSettings();
  }, [loadSettings]);

  React.useEffect(() => {
    const interval = window.setInterval(() => {
      void loadPending();
    }, 2500);
    return () => window.clearInterval(interval);
  }, [loadPending]);

  const saveMode = async (mode: ApprovalMode) => {
    if (!settings) {
      return;
    }
    setSaving(true);
    setMessage(null);
    setError(null);
    try {
      const next = await api.updateApprovalSettings({ default_mode: mode });
      setSettings(next);
      setMessage(`审批级别已切换为 ${approvalModeLabel(mode)}`);
    } catch (err) {
      setError(err instanceof Error ? err.message : '保存审批级别失败');
    } finally {
      setSaving(false);
    }
  };

  const approve = async (item: PendingApprovalItem) => {
    setError(null);
    try {
      await api.approvePendingApproval(item.id, {
        remember_allow: rememberAllow[item.id] || false,
      });
      setMessage(`已通过: ${item.command}`);
      await loadSettings();
    } catch (err) {
      setError(err instanceof Error ? err.message : '审批失败');
    }
  };

  const deny = async (item: PendingApprovalItem) => {
    setError(null);
    try {
      await api.denyPendingApproval(item.id, {
        reason: denyReasons[item.id]?.trim() || undefined,
      });
      setMessage(`已拒绝: ${item.command}`);
      await loadSettings();
    } catch (err) {
      setError(err instanceof Error ? err.message : '拒绝失败');
    }
  };

  if (loading) {
    return <section className="panel"><div className="emptyState">正在读取审批设置...</div></section>;
  }

  if (!settings) {
    return (
      <section className="panel">
        <div className="emptyState">{error || '审批设置不可用'}</div>
      </section>
    );
  }

  const history = [...settings.history].reverse().slice(0, 80);
  const whitelist = [...settings.whitelist].reverse();

  return (
    <section className="approvalPage">
      <section className="panel">
        <div className="panelHeader">
          <div>
            <h2><ShieldCheck size={18} />命令审批</h2>
            <p>当前级别: {approvalModeLabel(settings.default_mode)}</p>
          </div>
          <button className="iconButton" onClick={() => void loadSettings()} title="刷新审批">
            <RefreshCw size={17} />
          </button>
        </div>
        <div className="approvalModeGrid">
          {(['request_approval', 'auto_approval', 'full_control'] as ApprovalMode[]).map((mode) => (
            <button
              key={mode}
              type="button"
              className={settings.default_mode === mode ? 'approvalMode active' : 'approvalMode'}
              disabled={saving}
              onClick={() => void saveMode(mode)}
            >
              <strong>{approvalModeLabel(mode)}</strong>
              <span>{approvalModeDescription(mode)}</span>
            </button>
          ))}
        </div>
        {message ? <div className="banner">{message}</div> : null}
        {error ? <div className="formError">{error}</div> : null}
      </section>

      <section className="panel">
        <div className="panelHeader">
          <div>
            <h2><BellRing size={18} />待审批</h2>
            <p>
              {reviewing.length
                ? `${reviewing.length} 条命令正在 AI 审批`
                : pending.length
                  ? `${pending.length} 条命令等待处理`
                  : '当前没有待审批命令'}
            </p>
          </div>
        </div>
        <div className="approvalList">
          {reviewing.map((item) => (
            <div className="approvalPendingRow approvalReviewingRow" key={item.id}>
              <div className="approvalCommandLine">
                <span className="status warn"><RefreshCw className="spinIcon" size={13} />AI 审批中</span>
                <span className={riskStatusClass(item.risk)}>{riskLabel(item.risk)}</span>
                <strong>{item.command}</strong>
              </div>
              <div className="approvalSubline">
                {sourceLabel(item.source)} · {projectLabel(item.project_key)} · {item.cwd} · {formatHistoryTime(item.created_at)}
              </div>
              {item.reason ? <div className="approvalReason">{item.reason}</div> : null}
            </div>
          ))}
          {pending.map((item) => (
            <div className="approvalPendingRow" key={item.id}>
              <div className="approvalCommandLine">
                <span className={riskStatusClass(item.risk)}>{riskLabel(item.risk)}</span>
                <strong>{item.command}</strong>
              </div>
              <div className="approvalSubline">
                {sourceLabel(item.source)} · {projectLabel(item.project_key)} · {item.cwd} · {formatHistoryTime(item.created_at)}
              </div>
              {item.reason ? <div className="approvalReason">{item.reason}</div> : null}
              <div className="approvalActions">
                <label className="inlineCheck">
                  <input
                    type="checkbox"
                    checked={rememberAllow[item.id] || false}
                    onChange={(event) => setRememberAllow((current) => ({
                      ...current,
                      [item.id]: event.target.checked,
                    }))}
                  />
                  始终允许
                </label>
                <input
                  value={denyReasons[item.id] || ''}
                  onChange={(event) => setDenyReasons((current) => ({
                    ...current,
                    [item.id]: event.target.value,
                  }))}
                  placeholder="拒绝原因"
                />
                <button className="primaryButton compact" onClick={() => void approve(item)}>
                  <CheckCircle2 size={16} />通过
                </button>
                <button className="ghostButton compact dangerText" onClick={() => void deny(item)}>
                  <XCircle size={16} />拒绝
                </button>
              </div>
            </div>
          ))}
          {!pending.length && !reviewing.length ? <div className="emptyState">没有待审批命令。</div> : null}
        </div>
      </section>

      <section className="panel">
        <div className="panelHeader">
          <div>
            <h2><ListChecks size={18} />白名单</h2>
            <p>{whitelist.length ? `${whitelist.length} 条始终允许命令` : '还没有白名单命令'}</p>
          </div>
        </div>
        <div className="approvalList">
          {whitelist.map((entry) => (
            <div className="approvalSimpleRow" key={entry.id}>
              <div>
                <strong>{entry.command_display}</strong>
                <span>{projectLabel(entry.project_key)} · {entry.cwd_scope} · {decisionSourceLabel(entry.created_by)} · {formatHistoryTime(entry.created_at)}</span>
              </div>
              <span className={entry.enabled ? 'status ok' : 'status warn'}>{entry.enabled ? '启用' : '停用'}</span>
            </div>
          ))}
          {!whitelist.length ? <div className="emptyState">白名单为空。</div> : null}
        </div>
      </section>

      <section className="panel">
        <div className="panelHeader">
          <div>
            <h2><ListChecks size={18} />审批历史</h2>
            <p>{history.length ? `最近 ${history.length} 条` : '还没有审批历史'}</p>
          </div>
        </div>
        <div className="approvalList">
          {history.map((entry) => (
            <div className="approvalSimpleRow" key={entry.id}>
              <div>
                <div className="approvalCommandLine">
                  <span className={approvalDecisionClass(entry.decision)}>{approvalDecisionLabel(entry.decision)}</span>
                  <span className={riskStatusClass(entry.risk)}>{riskLabel(entry.risk)}</span>
                  <strong>{entry.normalized_command}</strong>
                </div>
                <span>{approvalModeLabel(entry.mode)} · {sourceLabel(entry.source)} · {entry.cwd} · {formatHistoryTime(entry.created_at)}</span>
                {entry.reason ? <span>{entry.reason}</span> : null}
              </div>
            </div>
          ))}
          {!history.length ? <div className="emptyState">审批历史为空。</div> : null}
        </div>
      </section>
    </section>
  );
}

function RuntimeSettingsPanel() {
  const [settings, setSettings] = React.useState<LocalRuntimeSettings>({
    ai_agent_max_iterations: DEFAULT_AI_AGENT_MAX_ITERATIONS,
  });
  const [loading, setLoading] = React.useState(true);
  const [saving, setSaving] = React.useState(false);
  const [message, setMessage] = React.useState<string | null>(null);
  const [error, setError] = React.useState<string | null>(null);

  const load = React.useCallback(async () => {
    setError(null);
    try {
      const next = await api.runtimeSettings();
      setSettings({
        ai_agent_max_iterations: next.ai_agent_max_iterations || DEFAULT_AI_AGENT_MAX_ITERATIONS,
      });
    } catch (err) {
      setError(err instanceof Error ? err.message : '读取运行配置失败');
    } finally {
      setLoading(false);
    }
  }, []);

  React.useEffect(() => {
    void load();
  }, [load]);

  const save = async () => {
    setSaving(true);
    setMessage(null);
    setError(null);
    try {
      const next = await api.updateRuntimeSettings({
        ai_agent_max_iterations: Math.max(
          1,
          Number(settings.ai_agent_max_iterations) || DEFAULT_AI_AGENT_MAX_ITERATIONS,
        ),
      });
      setSettings(next);
      setMessage('运行配置已保存');
    } catch (err) {
      setError(err instanceof Error ? err.message : '保存运行配置失败');
    } finally {
      setSaving(false);
    }
  };

  if (loading) {
    return <section className="panel"><div className="emptyState">正在读取运行配置...</div></section>;
  }

  return (
    <section className="settingsPage">
      <section className="panel">
        <div className="panelHeader">
          <div>
            <h2><Settings2 size={18} />运行配置</h2>
            <p>本机 Agent 运行参数</p>
          </div>
          <button className="iconButton" onClick={() => void load()} title="刷新配置">
            <RefreshCw size={17} />
          </button>
        </div>
        {message ? <div className="banner">{message}</div> : null}
        {error ? <div className="formError">{error}</div> : null}
        <div className="settingsFormGrid">
          <label>
            Agent 最大迭代次数
            <input
              type="number"
              min="1"
              step="1"
              value={settings.ai_agent_max_iterations}
              onChange={(event) =>
                setSettings({
                  ...settings,
                  ai_agent_max_iterations: Number(event.target.value) || DEFAULT_AI_AGENT_MAX_ITERATIONS,
                })
              }
            />
          </label>
        </div>
        <button className="primaryButton compact" disabled={saving} onClick={() => void save()}>
          {saving ? '保存中' : '保存配置'}
        </button>
      </section>
    </section>
  );
}

function ModelConfigPanel() {
  const [items, setItems] = React.useState<LocalModelConfig[]>([]);
  const [settings, setSettings] = React.useState<LocalModelSettings>({});
  const [draft, setDraft] = React.useState<ModelDraftState>(emptyModelDraft());
  const [loading, setLoading] = React.useState(true);
  const [saving, setSaving] = React.useState(false);
  const [message, setMessage] = React.useState<string | null>(null);
  const [error, setError] = React.useState<string | null>(null);
  const [modelCatalog, setModelCatalog] = React.useState<LocalModelCatalogResponse | null>(null);
  const [modelCatalogLoading, setModelCatalogLoading] = React.useState(false);
  const [modelCatalogError, setModelCatalogError] = React.useState<string | null>(null);

  const load = React.useCallback(async () => {
    setError(null);
    try {
      const next = await api.modelConfigs();
      setItems(next.items);
      setSettings(next.settings || {});
    } catch (err) {
      setError(err instanceof Error ? err.message : '读取模型配置失败');
    } finally {
      setLoading(false);
    }
  }, []);

  React.useEffect(() => {
    void load();
  }, [load]);

  const clearCatalog = () => {
    setModelCatalog(null);
    setModelCatalogError(null);
  };

  const resetDraft = () => {
    setDraft(emptyModelDraft());
    clearCatalog();
  };

  const updateDraft = (patch: Partial<ModelDraftState>) => {
    setDraft((current) => ({
      ...current,
      ...patch,
    }));
    clearCatalog();
  };

  const refreshModelCatalog = async () => {
    setModelCatalogLoading(true);
    setModelCatalogError(null);
    try {
      const catalog = await api.previewModelCatalog(buildProviderPreviewPayload(draft));
      setModelCatalog(catalog);
    } catch (err) {
      setModelCatalogError(err instanceof Error ? err.message : '读取模型列表失败');
    } finally {
      setModelCatalogLoading(false);
    }
  };

  const saveModel = async () => {
    setSaving(true);
    setMessage(null);
    setError(null);
    try {
      const catalog = modelCatalog?.models.length
        ? modelCatalog
        : await api.previewModelCatalog(buildProviderPreviewPayload(draft));
      if (!catalog.models.length) {
        throw new Error(catalog.error || '没有读取到可导入的模型，请先确认 API Key 和 Base URL 后刷新模型。');
      }
      let savedCount = 0;
      for (const providerModel of catalog.models) {
        const existing = findExistingImportedModel(items, draft, catalog.base_url, providerModel.id);
        const payload = buildImportedModelConfigPayload(draft, providerModel, catalog.base_url, existing);
        if (existing) {
          await api.updateModelConfig(existing.id, payload);
        } else {
          await api.saveModelConfig(payload);
        }
        savedCount += 1;
      }
      setMessage(`已保存供应商配置并导入 ${savedCount} 个模型`);
      resetDraft();
      await load();
    } catch (err) {
      setError(err instanceof Error ? err.message : '保存供应商配置失败');
    } finally {
      setSaving(false);
    }
  };

  const providerGroups = React.useMemo(
    () => groupLocalModelProviders(items),
    [items],
  );

  const providerDraftFromGroup = (group: LocalModelProviderGroup): ModelDraftState => ({
    ...emptyModelDraft(),
    id: group.items[0]?.id,
    name: group.name,
    provider: group.provider,
    base_url: group.base_url,
    api_key_text: '',
    clear_api_key: false,
    enabled: group.enabled_count > 0,
    supports_images: group.supports_images,
    supports_reasoning: group.supports_reasoning,
    supports_responses: group.supports_responses,
  });

  const editProviderGroup = (group: LocalModelProviderGroup) => {
    clearCatalog();
    setDraft(providerDraftFromGroup(group));
  };

  const refreshProviderGroup = async (group: LocalModelProviderGroup) => {
    const nextDraft = providerDraftFromGroup(group);
    setDraft(nextDraft);
    setModelCatalogLoading(true);
    setModelCatalogError(null);
    setMessage(null);
    setError(null);
    try {
      const catalog = await api.previewModelCatalog(buildProviderPreviewPayload(nextDraft));
      setModelCatalog(catalog);
      if (catalog.models.length) {
        setMessage(`已读取供应商 ${group.name} 的 ${catalog.models.length} 个模型`);
      } else if (catalog.error) {
        setModelCatalogError(catalog.error);
      }
    } catch (err) {
      setModelCatalogError(err instanceof Error ? err.message : '读取模型列表失败');
    } finally {
      setModelCatalogLoading(false);
    }
  };

  const syncProviderGroup = async (group: LocalModelProviderGroup) => {
    setSaving(true);
    setMessage(null);
    setError(null);
    try {
      for (const item of group.items) {
        await api.syncModelConfig(item.id);
      }
      setMessage(`已同步供应商 ${group.name} 的 ${group.items.length} 个模型`);
      await load();
    } catch (err) {
      setError(err instanceof Error ? err.message : '同步供应商元信息失败');
    } finally {
      setSaving(false);
    }
  };

  const editModel = (item: LocalModelConfig) => {
    clearCatalog();
    setDraft({
      id: item.id,
      server_model_config_id: item.server_model_config_id || undefined,
      name: item.name,
      provider: item.provider,
      model: item.model,
      base_url: item.base_url || '',
      api_key_text: '',
      clear_api_key: false,
      enabled: item.enabled,
      supports_images: item.supports_images,
      supports_reasoning: item.supports_reasoning,
      supports_responses: item.supports_responses,
      thinking_level: item.thinking_level || '',
      task_usage_scenario: item.task_usage_scenario || '',
      task_thinking_level: item.task_thinking_level || '',
      temperature: item.temperature ?? null,
      max_output_tokens: item.max_output_tokens ?? null,
    });
  };

  const deleteModel = async (item: LocalModelConfig) => {
    setMessage(null);
    setError(null);
    try {
      await api.deleteModelConfig(item.id);
      if (draft.id === item.id) {
        resetDraft();
      }
      setMessage(`模型已删除: ${item.name}`);
      await load();
    } catch (err) {
      setError(err instanceof Error ? err.message : '删除模型配置失败');
    }
  };

  const syncModel = async (item: LocalModelConfig) => {
    setMessage(null);
    setError(null);
    try {
      const synced = await api.syncModelConfig(item.id);
      setMessage(`元信息已同步: ${synced.name}`);
      await load();
    } catch (err) {
      setError(err instanceof Error ? err.message : '同步模型元信息失败');
    }
  };

  const saveSettings = async () => {
    setSaving(true);
    setMessage(null);
    setError(null);
    try {
      const next = await api.saveModelSettings(settings);
      setSettings(next);
      setMessage('默认模型设置已同步');
      await load();
    } catch (err) {
      setError(err instanceof Error ? err.message : '保存默认模型设置失败');
    } finally {
      setSaving(false);
    }
  };

  const enabledModels = items.filter((item) => item.enabled && item.model.trim());
  const modelById = React.useMemo(
    () => new Map(enabledModels.map((item) => [item.id, item])),
    [enabledModels],
  );
  const memoryModel = modelById.get(settings.memory_summary_model_config_id || '') || null;
  const projectAgentModel =
    modelById.get(settings.project_management_agent_model_config_id || '') || null;
  const approvalModel =
    modelById.get(settings.command_approval_model_config_id || '') || enabledModels[0] || null;

  return (
    <section className="modelPage">
      <section className="panel">
        <div className="panelHeader">
          <div>
            <h2><Brain size={18} />本地模型配置</h2>
            <p>API Key 和 Base URL 只保存在这台电脑；服务端只保存模型元信息和本地映射 id。</p>
          </div>
          <button className="iconButton" onClick={() => void load()} title="刷新模型">
            <RefreshCw size={17} />
          </button>
        </div>
        {message ? <div className="banner">{message}</div> : null}
        {error ? <div className="formError">{error}</div> : null}
        <div className="modelLayout">
          <div className="modelList">
            <div className="modelSectionTitle">
              <span>供应商配置</span>
              <small>{providerGroups.length} 个</small>
            </div>
            {providerGroups.map((group) => (
              <div className="modelRow providerRow" key={group.key}>
                <div>
                  <div className="modelTitleLine">
                    <strong>{group.name}</strong>
                    <span className="status ok">{group.items.length} 个模型</span>
                    <span className={group.enabled_count > 0 ? 'status ok' : 'status warn'}>
                      {group.enabled_count} 个启用
                    </span>
                    <span className={group.has_api_key ? 'status ok' : 'status bad'}>
                      {group.has_api_key ? 'Key 已保存' : '缺少 Key'}
                    </span>
                  </div>
                  <span>{providerLabel(group.provider)} · {group.base_url || '默认 Base URL'}</span>
                  <div className="providerModelChips">
                    {group.items.slice(0, 8).map((item) => (
                      <span key={item.id}>{item.model}</span>
                    ))}
                    {group.items.length > 8 ? <span>+{group.items.length - 8}</span> : null}
                  </div>
                </div>
                <div className="modelActions">
                  <button className="iconButton" title="编辑供应商" onClick={() => editProviderGroup(group)}>
                    <Settings2 size={16} />
                  </button>
                  <button
                    className="iconButton"
                    title="刷新供应商模型列表"
                    onClick={() => void refreshProviderGroup(group)}
                  >
                    <RefreshCw size={16} />
                  </button>
                  <button
                    className="iconButton"
                    title="同步供应商下的模型元信息"
                    onClick={() => void syncProviderGroup(group)}
                  >
                    <CheckCircle2 size={16} />
                  </button>
                </div>
              </div>
            ))}
            {!providerGroups.length ? (
              <div className="emptyState">{loading ? '正在读取供应商配置...' : '还没有保存过的供应商。'}</div>
            ) : null}

            <div className="modelSectionTitle">
              <span>导入模型</span>
              <small>{items.length} 个</small>
            </div>
            {items.map((item) => (
              <div className="modelRow" key={item.id}>
                <div>
                  <div className="modelTitleLine">
                    <strong>{item.name}</strong>
                    <span className={item.enabled ? 'status ok' : 'status warn'}>
                      {item.enabled ? '启用' : '停用'}
                    </span>
                    <span className={item.has_api_key ? 'status ok' : 'status bad'}>
                      {item.has_api_key ? 'Key 已保存' : '缺少 Key'}
                    </span>
                  </div>
                  <span>{item.provider} · {item.model}</span>
                  <span className="mono">{item.server_model_config_id || '未同步到服务端'}</span>
                </div>
                <div className="modelActions">
                  <button className="iconButton" title="编辑" onClick={() => editModel(item)}>
                    <Settings2 size={16} />
                  </button>
                  <button className="iconButton" title="同步元信息" onClick={() => void syncModel(item)}>
                    <RefreshCw size={16} />
                  </button>
                  <button className="iconButton danger" title="删除" onClick={() => void deleteModel(item)}>
                    <Trash2 size={16} />
                  </button>
                </div>
              </div>
            ))}
            {!items.length ? (
              <div className="emptyState">{loading ? '正在读取模型配置...' : '还没有导入具体模型。'}</div>
            ) : null}
          </div>

          <div className="modelEditor">
            <div className="panelHeader compactHeader">
              <div>
                <h2><KeyRound size={18} />{draft.id ? '编辑供应商' : '添加供应商'}</h2>
                <p>{draft.id ? '留空 API Key 会沿用本机已保存的值。' : '保存后会导入供应商返回的具体模型。'}</p>
              </div>
              {draft.id ? (
                <button className="ghostButton compact" onClick={() => resetDraft()}>
                  新建
                </button>
              ) : null}
            </div>
            <div className="approvalFormGrid">
              <label>
                名称
                <input value={draft.name} onChange={(event) => setDraft({ ...draft, name: event.target.value })} />
              </label>
              <label>
                Provider
                <select
                  value={draft.provider || 'gpt'}
                  onChange={(event) => updateDraft({ provider: event.target.value })}
                >
                  <option value="gpt">OpenAI</option>
                  <option value="openai_compatible">OpenAI Compatible</option>
                  <option value="deepseek">DeepSeek</option>
                  <option value="kimi">Kimi</option>
                  <option value="minimax">MiniMax</option>
                </select>
              </label>
              <label>
                Base URL
                <input
                  value={draft.base_url || ''}
                  onChange={(event) => updateDraft({ base_url: event.target.value })}
                />
              </label>
              <label>
                API Key
                <input
                  type="password"
                  value={draft.api_key_text}
                  onChange={(event) => updateDraft({ api_key_text: event.target.value, clear_api_key: false })}
                />
              </label>
              <div className="modelCatalogField">
                <span className="fieldLabel">供应商模型</span>
                <div className="modelSelectRow">
                  <button
                    type="button"
                    className="ghostButton compact"
                    onClick={() => void refreshModelCatalog()}
                    disabled={modelCatalogLoading || Boolean(draft.clear_api_key)}
                  >
                    <RefreshCw size={15} />
                    {modelCatalogLoading ? '读取中' : '刷新模型列表'}
                  </button>
                  <span className="catalogStatus">
                    {modelCatalog
                      ? `${modelCatalog.source === 'live' ? '已读取' : '使用缓存'} ${modelCatalog.models.length} 个模型 · ${modelCatalog.base_url}`
                      : '保存时会按供应商返回的模型列表导入具体模型。'}
                  </span>
                </div>
                {modelCatalog?.models.length ? (
                  <div className="catalogModelList">
                    {modelCatalog.models.slice(0, 12).map((model) => (
                      <span key={model.id}>{formatProviderModelOption(model)}</span>
                    ))}
                    {modelCatalog.models.length > 12 ? <span>+{modelCatalog.models.length - 12}</span> : null}
                  </div>
                ) : null}
                {modelCatalog?.error ? <span className="catalogError">{modelCatalog.error}</span> : null}
                {modelCatalogError ? <span className="catalogError">{modelCatalogError}</span> : null}
              </div>
              <label className="inlineSwitch">
                <span>启用</span>
                <input type="checkbox" checked={draft.enabled ?? true} onChange={(event) => setDraft({ ...draft, enabled: event.target.checked })} />
              </label>
              <label className="inlineSwitch">
                <span>Responses API</span>
                <input type="checkbox" checked={draft.supports_responses ?? true} onChange={(event) => setDraft({ ...draft, supports_responses: event.target.checked })} />
              </label>
              <label className="inlineSwitch">
                <span>图片输入</span>
                <input type="checkbox" checked={draft.supports_images || false} onChange={(event) => setDraft({ ...draft, supports_images: event.target.checked })} />
              </label>
              <label className="inlineSwitch">
                <span>Reasoning</span>
                <input type="checkbox" checked={draft.supports_reasoning || false} onChange={(event) => setDraft({ ...draft, supports_reasoning: event.target.checked })} />
              </label>
              {draft.id ? (
                <label className="inlineSwitch">
                  <span>清除已保存 Key</span>
                  <input
                    type="checkbox"
                    checked={draft.clear_api_key || false}
                    onChange={(event) =>
                      updateDraft({
                        clear_api_key: event.target.checked,
                        api_key_text: event.target.checked ? '' : draft.api_key_text,
                      })
                    }
                  />
                </label>
              ) : null}
            </div>
            <button className="primaryButton compact" disabled={saving || !draft.name.trim()} onClick={() => void saveModel()}>
              {saving ? '保存中' : '保存并导入模型'}
            </button>
          </div>
        </div>
      </section>

      <section className="panel">
        <div className="panelHeader">
          <div>
            <h2><Settings2 size={18} />默认模型</h2>
            <p>这些设置会同步模型 id 到服务端，服务端需要运行时再向本机换取 key。</p>
          </div>
          <button className="primaryButton compact" disabled={saving} onClick={() => void saveSettings()}>
            保存默认设置
          </button>
        </div>
        <div className="approvalFormGrid">
          <label>
            Memory 总结模型
            <select
              value={settings.memory_summary_model_config_id || ''}
              onChange={(event) => {
                const modelId = event.target.value || null;
                const nextModel = modelById.get(modelId || '') || null;
                setSettings({
                  ...settings,
                  memory_summary_model_config_id: modelId,
                  memory_summary_thinking_level: normalizeThinkingLevelForProvider(
                    nextModel?.provider,
                    settings.memory_summary_thinking_level,
                  ),
                });
              }}
            >
              <option value="">不指定</option>
              {enabledModels.map((item) => (
                <option key={item.id} value={item.id}>{item.name} · {item.model}</option>
              ))}
            </select>
          </label>
          <label>
            Memory Thinking
            <select
              value={thinkingValueForProvider(memoryModel?.provider, settings.memory_summary_thinking_level)}
              disabled={!memoryModel}
              onChange={(event) =>
                setSettings({ ...settings, memory_summary_thinking_level: event.target.value || null })
              }
            >
              {thinkingOptionsForProvider(memoryModel?.provider).map((option) => (
                <option key={option.value || 'default'} value={option.value}>
                  {option.label}
                </option>
              ))}
            </select>
          </label>
          <label>
            项目管理 Agent 模型
            <select
              value={settings.project_management_agent_model_config_id || ''}
              onChange={(event) => {
                const modelId = event.target.value || null;
                const nextModel = modelById.get(modelId || '') || null;
                setSettings({
                  ...settings,
                  project_management_agent_model_config_id: modelId,
                  project_management_agent_thinking_level: normalizeThinkingLevelForProvider(
                    nextModel?.provider,
                    settings.project_management_agent_thinking_level,
                  ),
                });
              }}
            >
              <option value="">不指定</option>
              {enabledModels.map((item) => (
                <option key={item.id} value={item.id}>{item.name} · {item.model}</option>
              ))}
            </select>
          </label>
          <label>
            Agent Thinking
            <select
              value={thinkingValueForProvider(projectAgentModel?.provider, settings.project_management_agent_thinking_level)}
              disabled={!projectAgentModel}
              onChange={(event) =>
                setSettings({ ...settings, project_management_agent_thinking_level: event.target.value || null })
              }
            >
              {thinkingOptionsForProvider(projectAgentModel?.provider).map((option) => (
                <option key={option.value || 'default'} value={option.value}>
                  {option.label}
                </option>
              ))}
            </select>
          </label>
          <label>
            命令审批模型
            <select
              value={settings.command_approval_model_config_id || ''}
              onChange={(event) => {
                const modelId = event.target.value || null;
                const nextModel = modelById.get(modelId || '') || enabledModels[0] || null;
                setSettings({
                  ...settings,
                  command_approval_model_config_id: modelId,
                  command_approval_thinking_level: normalizeThinkingLevelForProvider(
                    nextModel?.provider,
                    settings.command_approval_thinking_level,
                  ),
                });
              }}
            >
              <option value="">自动选择可用模型</option>
              {enabledModels.map((item) => (
                <option key={item.id} value={item.id}>{item.name} · {item.model}</option>
              ))}
            </select>
          </label>
          <label>
            审批 Thinking
            <select
              value={thinkingValueForProvider(approvalModel?.provider, settings.command_approval_thinking_level)}
              disabled={!approvalModel}
              onChange={(event) =>
                setSettings({ ...settings, command_approval_thinking_level: event.target.value || null })
              }
            >
              {thinkingOptionsForProvider(approvalModel?.provider).map((option) => (
                <option key={option.value || 'default'} value={option.value}>
                  {option.label}
                </option>
              ))}
            </select>
          </label>
        </div>
      </section>

      <TaskModelSettingsSection items={items} loading={loading} onReload={load} />
    </section>
  );
}

function TaskModelSettingsSection({
  items,
  loading,
  onReload,
}: {
  items: LocalModelConfig[];
  loading: boolean;
  onReload: () => Promise<void>;
}) {
  const [drafts, setDrafts] = React.useState<Record<string, TaskModelDraft>>({});
  const [saving, setSaving] = React.useState(false);
  const [message, setMessage] = React.useState<string | null>(null);
  const [error, setError] = React.useState<string | null>(null);

  const concreteModels = React.useMemo(
    () => items.filter((item) => item.model.trim()),
    [items],
  );

  React.useEffect(() => {
    setDrafts(Object.fromEntries(concreteModels.map((item) => [item.id, taskDraftFromModel(item)])));
  }, [concreteModels]);

  const updateDraft = (id: string, patch: Partial<TaskModelDraft>) => {
    setDrafts((current) => ({
      ...current,
      [id]: {
        ...(current[id] || emptyTaskModelDraft()),
        ...patch,
      },
    }));
  };

  const save = async () => {
    setSaving(true);
    setMessage(null);
    setError(null);
    try {
      let changed = 0;
      for (const item of concreteModels) {
        const draft = drafts[item.id];
        if (!draft || !taskDraftChanged(item, draft)) {
          continue;
        }
        await api.updateModelConfig(item.id, buildTaskModelConfigPayload(item, draft));
        changed += 1;
      }
      setMessage(changed ? `已保存 ${changed} 个任务模型配置` : '任务模型配置没有变化');
      await onReload();
    } catch (err) {
      setError(err instanceof Error ? err.message : '保存任务模型配置失败');
    } finally {
      setSaving(false);
    }
  };

  return (
    <section className="panel">
      <div className="panelHeader">
        <div>
          <h2><ListChecks size={18} />任务模型设置</h2>
          <p>按具体模型配置任务用途、任务 thinking 和任务运行参数。</p>
        </div>
        <div className="headerActions">
          <button className="iconButton" onClick={() => void onReload()} title="刷新任务模型">
            <RefreshCw size={17} />
          </button>
          <button className="primaryButton compact" disabled={loading || saving} onClick={() => void save()}>
            {saving ? '保存中' : '保存任务设置'}
          </button>
        </div>
      </div>
      {message ? <div className="banner">{message}</div> : null}
      {error ? <div className="formError">{error}</div> : null}
      <div className="taskModelList">
        {concreteModels.map((item) => {
          const draft = drafts[item.id] || taskDraftFromModel(item);
          return (
            <div className={draft.enabled ? 'taskModelRow' : 'taskModelRow muted'} key={item.id}>
              <div className="taskModelRowHeader">
                <div>
                  <strong>{item.name}</strong>
                  <span>{item.provider} · {item.model}</span>
                </div>
                <div className="modelActions">
                  <span className={draft.enabled ? 'status ok' : 'status warn'}>
                    {draft.enabled ? '启用' : '停用'}
                  </span>
                  <button
                    className="ghostButton compact"
                    type="button"
                    onClick={() => updateDraft(item.id, { enabled: !draft.enabled })}
                  >
                    {draft.enabled ? '停用' : '启用'}
                  </button>
                </div>
              </div>
              <div className="taskModelGrid">
                <label>
                  Task 用途
                  <input
                    value={draft.task_usage_scenario}
                    onChange={(event) => updateDraft(item.id, { task_usage_scenario: event.target.value })}
                    placeholder="例如: task planning / coding / review"
                  />
                </label>
                <label>
                  Task Thinking
                  <select
                    value={draft.task_thinking_level}
                    onChange={(event) => updateDraft(item.id, { task_thinking_level: event.target.value })}
                  >
                    {thinkingOptionsForProvider(item.provider).map((option) => (
                      <option key={option.value || 'default'} value={option.value}>
                        {option.label}
                      </option>
                    ))}
                  </select>
                </label>
                <label>
                  Temperature
                  <input
                    type="number"
                    step="0.1"
                    value={draft.temperature ?? ''}
                    onChange={(event) => updateDraft(item.id, { temperature: numericInput(event.target.value) })}
                  />
                </label>
                <label>
                  Max Tokens
                  <input
                    type="number"
                    value={draft.max_output_tokens ?? ''}
                    onChange={(event) => updateDraft(item.id, { max_output_tokens: numericInput(event.target.value) })}
                  />
                </label>
              </div>
            </div>
          );
        })}
        {!concreteModels.length ? (
          <div className="emptyState">{loading ? '正在读取任务模型配置...' : '还没有可配置的具体模型。'}</div>
        ) : null}
      </div>
    </section>
  );
}

function SandboxPanel({
  status,
  onStatus,
  onRefresh,
}: {
  status: ConnectorStatus;
  onStatus: (status: ConnectorStatus) => void;
  onRefresh: () => Promise<void>;
}) {
  const [catalog, setCatalog] = React.useState<SandboxImageCatalog | null>(null);
  const [jobs, setJobs] = React.useState<SandboxImageJob[]>([]);
  const [leases, setLeases] = React.useState<SandboxLease[]>([]);
  const [features, setFeatures] = React.useState<Record<string, string>>({});
  const [customScript, setCustomScript] = React.useState('');
  const [message, setMessage] = React.useState<string | null>(null);
  const [loadingDetails, setLoadingDetails] = React.useState(false);
  const [building, setBuilding] = React.useState(false);

  const refreshSandboxDetails = React.useCallback(async () => {
    if (!status.sandbox.enabled) {
      setCatalog(null);
      setJobs([]);
      setLeases([]);
      return;
    }
    setLoadingDetails(true);
    try {
      const [next, nextJobs, nextLeases] = await Promise.all([
        api.sandboxImages(),
        api.sandboxImageJobs(),
        api.sandboxLeases(),
      ]);
      setCatalog(next);
      setJobs(nextJobs);
      setLeases(nextLeases);
      setFeatures((current) => {
        const merged = { ...current };
        for (const feature of next.features) {
          if (typeof merged[feature.id] !== 'string') {
            merged[feature.id] = '';
          }
        }
        return merged;
      });
    } catch (err) {
      setMessage(err instanceof Error ? err.message : '读取镜像信息失败');
    } finally {
      setLoadingDetails(false);
    }
  }, [status.sandbox.enabled]);

  React.useEffect(() => {
    void refreshSandboxDetails();
  }, [refreshSandboxDetails]);

  React.useEffect(() => {
    if (!status.sandbox.enabled) {
      return;
    }
    const interval = window.setInterval(() => {
      void refreshSandboxDetails();
    }, jobs.some((job) => job.status === 'running') ? 2500 : 6000);
    return () => window.clearInterval(interval);
  }, [jobs, refreshSandboxDetails, status.sandbox.enabled]);

  const setEnabled = async (enabled: boolean) => {
    setMessage(null);
    try {
      const next = await api.setSandboxEnabled({ enabled });
      onStatus(next);
      setMessage(enabled ? '本地沙箱已开启' : '本地沙箱已关闭');
      await onRefresh();
    } catch (err) {
      setMessage(err instanceof Error ? err.message : '沙箱设置失败');
    }
  };

  const selectedFeatures = Object.entries(features)
    .filter(([, version]) => version)
    .map(([id, version]) => `${id}@${version}`);

  const initialize = async () => {
    setMessage(null);
    setBuilding(true);
    try {
      const job = await api.initializeSandboxImage({
        features: selectedFeatures,
        custom_build_script: customScript.trim() || undefined,
      });
      setMessage(`镜像任务已创建: ${job.image_name}`);
      await refreshSandboxDetails();
    } catch (err) {
      setMessage(err instanceof Error ? err.message : '创建镜像失败');
    } finally {
      setBuilding(false);
    }
  };

  return (
    <section className="sandboxPage">
      <div className="panel sandboxHero">
        <div className="panelHeader">
          <div>
            <h2><Shield size={18} />本地沙箱</h2>
            <p>Local Connector Core 在本机 Docker 内创建、启动和释放沙箱；Local Connector Service 只转发长连接消息。</p>
          </div>
          <div className="headerActions">
            <button className="iconButton" onClick={() => void refreshSandboxDetails()} title="刷新沙箱">
              <RefreshCw size={17} />
            </button>
            <label className="switch">
              <input
                type="checkbox"
                checked={status.sandbox.enabled}
                onChange={(event) => void setEnabled(event.target.checked)}
              />
              <span />
            </label>
          </div>
        </div>
        <div className="sandboxStatusGrid">
          <StatusTile
            icon={Container}
            label="沙箱开关"
            value={status.sandbox.enabled ? '已开启' : '已关闭'}
            tone={status.sandbox.enabled ? 'ok' : 'muted'}
          />
          <StatusTile
            icon={HardDrive}
            label="Docker"
            value={status.docker.installed ? (status.docker.running ? '运行中' : '未运行') : '未安装'}
            detail={status.docker.version || status.docker.error || undefined}
            tone={status.docker.installed && status.docker.running ? 'ok' : 'warn'}
          />
          <StatusTile
            icon={Cpu}
            label="运行后端"
            value={status.sandbox.backend || 'docker'}
            detail={status.sandbox.isolation || 'local_docker'}
            tone="ok"
          />
          <StatusTile
            icon={Image}
            label="默认镜像"
            value={status.sandbox.selected_image_ref || 'chatos-sandbox-agent:latest'}
            tone="muted"
          />
        </div>
        <div className="boundaryList sandboxBoundary">
          <div><CloudOff size={16} />不调用云端 Sandbox Manager，不使用云端沙箱实例。</div>
          <div><Activity size={16} />Task Runner 请求经 Local Connector 长连接转到本机执行。</div>
          <div><Layers size={16} />可复用 common 里的镜像定义和 Dockerfile 生成逻辑，但运行时状态属于本机。</div>
        </div>
        {message ? <div className="banner">{message}</div> : null}
      </div>

      {status.sandbox.enabled ? (
        <>
          <div className="sandboxContentGrid">
            <section className="panel">
              <div className="panelHeader">
                <div>
                  <h2><Settings2 size={18} />创建沙箱镜像</h2>
                  <p>选择本机 Docker 镜像内要预装的运行时。</p>
                </div>
                <button
                  className="primaryButton compact"
                  disabled={building || (selectedFeatures.length === 0 && !customScript.trim())}
                  onClick={() => void initialize()}
                >
                  {building ? '创建中' : '创建镜像'}
                </button>
              </div>
              {catalog ? (
                <>
                  <div className="runtimeGrid">
                    {catalog.features.map((feature) => (
                      <label key={feature.id} className="runtimeSelect">
                        <span>
                          <strong>{feature.label}</strong>
                          <small>{feature.description}</small>
                        </span>
                        <select
                          value={features[feature.id] || ''}
                          onChange={(event) => setFeatures((current) => ({
                            ...current,
                            [feature.id]: event.target.value,
                          }))}
                        >
                          <option value="">不安装</option>
                          {feature.versions.map((version) => (
                            <option key={version.id} value={version.id}>
                              {version.label}{version.default ? ' · 推荐' : ''}
                            </option>
                          ))}
                        </select>
                      </label>
                    ))}
                  </div>
                  <label className="scriptEditor">
                    自定义构建脚本
                    <textarea
                      value={customScript}
                      onChange={(event) => setCustomScript(event.target.value)}
                      rows={7}
                      placeholder="apt-get update && apt-get install -y ..."
                    />
                  </label>
                </>
              ) : (
                <div className="emptyState">{loadingDetails ? '正在读取本地镜像配置...' : '暂无镜像配置'}</div>
              )}
            </section>

            <section className="panel">
              <div className="panelHeader">
                <div>
                  <h2><Image size={18} />本地镜像</h2>
                  <p>这些镜像只存在于当前电脑的 Docker 环境。</p>
                </div>
              </div>
              <div className="imageList">
                {(catalog?.images || []).map((image) => (
                  <div className="imageRow" key={image.id}>
                    <div>
                      <strong>{image.image_ref}</strong>
                      <span>{image.features.length ? image.features.join(', ') : 'base'}</span>
                    </div>
                    <span className="status ok">{image.id === 'default' ? '默认' : '本机'}</span>
                  </div>
                ))}
                {!catalog?.images.length ? <div className="emptyState">还没有读取到本地沙箱镜像。</div> : null}
              </div>
            </section>
          </div>

          <section className="panel">
            <div className="panelHeader">
              <div>
                <h2><ListChecks size={18} />镜像任务</h2>
                <p>构建日志保留在 Local Connector Core 内存中。</p>
              </div>
            </div>
            {jobs.length ? (
              <div className="jobList">
                {jobs.map((job) => (
                  <details className="jobRow" key={job.id} open={job.status === 'running' || Boolean(job.error)}>
                    <summary>
                      <span>
                        <strong>{job.image_name}</strong>
                        <small>{job.features.length ? job.features.join(', ') : 'base'}</small>
                      </span>
                      <span className={job.status === 'succeeded' ? 'status ok' : job.status === 'failed' ? 'status bad' : 'status warn'}>
                        {job.status}
                      </span>
                    </summary>
                    {job.error ? <div className="formError">{job.error}</div> : null}
                    <pre className="logText">{job.output || '暂无日志'}</pre>
                  </details>
                ))}
              </div>
            ) : (
              <div className="emptyState">还没有镜像构建任务。</div>
            )}
          </section>

          <section className="panel">
            <div className="panelHeader">
              <div>
                <h2><Container size={18} />当前沙箱</h2>
                <p>Task Runner 运行时创建的本机 Docker 沙箱租约。</p>
              </div>
            </div>
            {leases.length ? (
              <div className="leaseTable">
                <div className="leaseHeader">
                  <span>Sandbox</span>
                  <span>Run</span>
                  <span>Image</span>
                  <span>Status</span>
                </div>
                {leases.map((lease) => (
                  <div className="leaseRow" key={lease.id}>
                    <span className="mono">{lease.sandbox_id}</span>
                    <span className="mono">{lease.run_id}</span>
                    <span>{lease.image_ref || '-'}</span>
                    <span className={lease.status === 'ready' ? 'status ok' : 'status warn'}>{lease.status}</span>
                  </div>
                ))}
              </div>
            ) : (
              <div className="emptyState">当前没有运行中的本地沙箱。</div>
            )}
          </section>
        </>
      ) : (
        <section className="panel">
          <div className="emptyState">本地沙箱默认关闭。打开开关后会检查 Docker，并在本机 Docker 内创建沙箱。</div>
        </section>
      )}
    </section>
  );
}

function StatusTile({
  icon: Icon,
  label,
  value,
  detail,
  tone,
}: {
  icon: LocalIcon;
  label: string;
  value: string;
  detail?: string;
  tone: 'ok' | 'warn' | 'muted';
}) {
  return (
    <div className={`statusTile ${tone}`}>
      <Icon size={18} />
      <span>{label}</span>
      <strong>{value}</strong>
      {detail ? <small>{detail}</small> : null}
    </div>
  );
}

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
