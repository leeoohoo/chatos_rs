// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';
import ReactDOM from 'react-dom/client';
import {
  Activity,
  CheckCircle2,
  ChevronLeft,
  CloudOff,
  Container,
  Cpu,
  FolderOpen,
  HardDrive,
  Image,
  Layers,
  ListChecks,
  LogOut,
  Play,
  Plus,
  RefreshCw,
  Server,
  Settings2,
  Shield,
  Terminal,
  Trash2,
} from 'lucide-react';

import {
  api,
  type CommandHistoryEntry,
  type ConnectorStatus,
  type FsEntry,
  type SandboxImageCatalog,
  type SandboxImageJob,
  type SandboxLease,
} from './api';
import './styles.css';

const DEFAULT_CLOUD_URL = 'http://127.0.0.1:39230';
const DEFAULT_USER_SERVICE_URL = 'http://127.0.0.1:39190';
type AppTab = 'overview' | 'workspaces' | 'terminal' | 'sandbox';
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

function AuthPanel({ onDone }: { onDone: (status: ConnectorStatus) => void }) {
  const [mode, setMode] = React.useState<'login' | 'register'>('login');
  const [cloudBaseUrl, setCloudBaseUrl] = React.useState(DEFAULT_CLOUD_URL);
  const [userServiceBaseUrl, setUserServiceBaseUrl] = React.useState(DEFAULT_USER_SERVICE_URL);
  const [username, setUsername] = React.useState('');
  const [displayName, setDisplayName] = React.useState('');
  const [password, setPassword] = React.useState('');
  const [deviceName, setDeviceName] = React.useState(defaultDeviceName());
  const [submitting, setSubmitting] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);

  const submit = async (event: React.FormEvent) => {
    event.preventDefault();
    setSubmitting(true);
    setError(null);
    try {
      const payload = {
        cloud_base_url: cloudBaseUrl,
        user_service_base_url: userServiceBaseUrl,
        username,
        password,
        device_name: deviceName,
      };
      const next =
        mode === 'login'
          ? await api.login(payload)
          : await api.register({
              ...payload,
              display_name: displayName,
            });
      onDone(next);
    } catch (err) {
      setError(err instanceof Error ? err.message : '登录失败');
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <form className="authPanel" onSubmit={submit}>
      <div className="segmented">
        <button type="button" className={mode === 'login' ? 'active' : ''} onClick={() => setMode('login')}>
          登录
        </button>
        <button type="button" className={mode === 'register' ? 'active' : ''} onClick={() => setMode('register')}>
          注册
        </button>
      </div>
      <label>
        Local Connector Service
        <input value={cloudBaseUrl} onChange={(event) => setCloudBaseUrl(event.target.value)} />
      </label>
      <label>
        User Service
        <input value={userServiceBaseUrl} onChange={(event) => setUserServiceBaseUrl(event.target.value)} />
      </label>
      <div className="twoCols">
        <label>
          用户名
          <input value={username} onChange={(event) => setUsername(event.target.value)} />
        </label>
        <label>
          设备名
          <input value={deviceName} onChange={(event) => setDeviceName(event.target.value)} />
        </label>
      </div>
      {mode === 'register' ? (
        <label>
          显示名
          <input value={displayName} onChange={(event) => setDisplayName(event.target.value)} />
        </label>
      ) : null}
      <label>
        密码
        <input type="password" value={password} onChange={(event) => setPassword(event.target.value)} />
      </label>
      {error ? <div className="formError">{error}</div> : null}
      <button className="primaryButton" disabled={submitting}>
        {submitting ? '处理中...' : mode === 'login' ? '登录并配对设备' : '注册并配对设备'}
      </button>
    </form>
  );
}

function ConnectionCard({
  status,
  onStatus,
}: {
  status: ConnectorStatus;
  onStatus: (status: ConnectorStatus) => void;
}) {
  const logout = async () => {
    onStatus(await api.logout());
  };
  return (
    <section className="panel">
      <div className="panelHeader">
        <div>
          <h2><Server size={18} />连接状态</h2>
          <p>{status.cloud_base_url}</p>
        </div>
        <span className={status.connector_running ? 'status ok' : 'status warn'}>
          {status.connector_running ? '已连接' : '未连接'}
        </span>
      </div>
      <div className="metaGrid">
        <div><span>用户</span><strong>{status.user?.display_name || status.user?.username || '-'}</strong></div>
        <div><span>设备</span><strong>{status.device_name || status.device_id || '-'}</strong></div>
        <div><span>Device ID</span><strong className="mono">{status.device_id || '-'}</strong></div>
      </div>
      <button className="ghostButton" onClick={() => void logout()}>
        <LogOut size={16} />退出本机配对
      </button>
    </section>
  );
}

function LocalBoundaryPanel({ status }: { status: ConnectorStatus }) {
  return (
    <section className="panel">
      <div className="panelHeader">
        <div>
          <h2><CloudOff size={18} />本机边界</h2>
          <p>目录、终端和沙箱运行在当前电脑，云端只通过已登录设备的长连接发起授权请求。</p>
        </div>
      </div>
      <div className="metaGrid">
        <div>
          <span>开放目录</span>
          <strong>{status.workspaces.length} 个</strong>
        </div>
        <div>
          <span>本地沙箱</span>
          <strong>{status.sandbox.enabled ? '已开启' : '已关闭'}</strong>
        </div>
        <div>
          <span>Docker</span>
          <strong>{status.docker.installed ? (status.docker.running ? '运行中' : '未运行') : '未安装'}</strong>
        </div>
      </div>
      <div className="boundaryList">
        <div><CheckCircle2 size={16} />Local Connector Core 执行 Docker、文件和终端操作。</div>
        <div><CheckCircle2 size={16} />Local Connector Service 只负责登录设备、保存配对和 relay 消息。</div>
        <div><CheckCircle2 size={16} />本地沙箱不调用云端 Sandbox Manager，也不复用云端沙箱实例。</div>
      </div>
    </section>
  );
}

function WorkspacePanel({
  status,
  onStatus,
}: {
  status: ConnectorStatus;
  onStatus: (status: ConnectorStatus) => void;
}) {
  const [pickerOpen, setPickerOpen] = React.useState(false);
  return (
    <section className="panel">
      <div className="panelHeader">
        <div>
          <h2><FolderOpen size={18} />开放目录</h2>
          <p>默认关闭。只有这里授权过的目录，ChatOS 才能看到并用于创建项目或终端。</p>
        </div>
        <button className="primaryButton compact" onClick={() => setPickerOpen(true)}>
          <Plus size={16} />开放目录
        </button>
      </div>
      {status.workspaces.length === 0 ? (
        <div className="emptyState">还没有开放任何本地目录。</div>
      ) : (
        <div className="workspaceList">
          {status.workspaces.map((workspace) => (
            <div className="workspaceRow" key={workspace.id}>
              <div>
                <strong>{workspace.alias}</strong>
                <span>{workspace.absolute_root}</span>
              </div>
              <button
                className="iconButton danger"
                title="移除"
                onClick={async () => onStatus(await api.removeWorkspace(workspace.id))}
              >
                <Trash2 size={16} />
              </button>
            </div>
          ))}
        </div>
      )}
      {pickerOpen ? <DirectoryPicker onClose={() => setPickerOpen(false)} onStatus={onStatus} /> : null}
    </section>
  );
}

function DirectoryPicker({
  onClose,
  onStatus,
}: {
  onClose: () => void;
  onStatus: (status: ConnectorStatus) => void;
}) {
  const [path, setPath] = React.useState<string | null>(null);
  const [items, setItems] = React.useState<FsEntry[]>([]);
  const [parent, setParent] = React.useState<string | null>(null);
  const [selected, setSelected] = React.useState<Set<string>>(new Set());
  const [alias, setAlias] = React.useState('');
  const [error, setError] = React.useState<string | null>(null);

  const load = React.useCallback(async (nextPath?: string | null) => {
    setError(null);
    try {
      const result = await api.fsList(nextPath);
      setPath(result.path);
      setParent(result.parent || null);
      setItems(result.entries.filter((entry) => entry.is_dir));
    } catch (err) {
      setError(err instanceof Error ? err.message : '目录读取失败');
    }
  }, []);

  React.useEffect(() => {
    void load(null);
  }, [load]);

  const toggle = (entryPath: string) => {
    setSelected((current) => {
      const next = new Set(current);
      if (next.has(entryPath)) {
        next.delete(entryPath);
      } else {
        next.add(entryPath);
      }
      return next;
    });
  };

  const apply = async () => {
    setError(null);
    try {
      let latest: ConnectorStatus | null = null;
      for (const selectedPath of selected) {
        latest = await api.addWorkspace({
          path: selectedPath,
          alias: selected.size === 1 ? alias.trim() || undefined : undefined,
        });
      }
      if (latest) {
        onStatus(latest);
      }
      onClose();
    } catch (err) {
      setError(err instanceof Error ? err.message : '授权目录失败');
    }
  };

  return (
    <div className="modalBackdrop">
      <div className="modal">
        <div className="panelHeader">
          <div>
            <h2>选择要开放的本地目录</h2>
            <p className="mono">{path || '-'}</p>
          </div>
          <button className="iconButton" onClick={onClose}>×</button>
        </div>
        <div className="pickerToolbar">
          <button className="ghostButton" disabled={!parent} onClick={() => void load(parent)}>
            <ChevronLeft size={16} />上一级
          </button>
          <button className="ghostButton" disabled={!path} onClick={() => path && toggle(path)}>
            选择当前目录
          </button>
        </div>
        <div className="dirList">
          {items.map((entry) => (
            <div className="dirRow" key={entry.path}>
              <input
                type="checkbox"
                checked={selected.has(entry.path)}
                onChange={() => toggle(entry.path)}
              />
              <button type="button" onClick={() => void load(entry.path)}>
                <FolderOpen size={15} />{entry.name}
              </button>
            </div>
          ))}
        </div>
        <label>
          单目录别名
          <input value={alias} onChange={(event) => setAlias(event.target.value)} />
        </label>
        {selected.size > 0 ? (
          <div className="selectionSummary">已选择 {selected.size} 个目录</div>
        ) : null}
        {error ? <div className="formError">{error}</div> : null}
        <div className="modalActions">
          <button className="ghostButton" onClick={onClose}>取消</button>
          <button className="primaryButton compact" disabled={selected.size === 0} onClick={() => void apply()}>
            开放所选目录
          </button>
        </div>
      </div>
    </div>
  );
}

function TerminalPanel({ status }: { status: ConnectorStatus }) {
  const [workspaceId, setWorkspaceId] = React.useState(status.workspaces[0]?.id || '');
  const [command, setCommand] = React.useState('pwd');
  const [args, setArgs] = React.useState('');
  const [output, setOutput] = React.useState('');
  const [running, setRunning] = React.useState(false);
  const [history, setHistory] = React.useState<CommandHistoryEntry[]>([]);
  const [sourceFilter, setSourceFilter] = React.useState('all');
  const [historyLoading, setHistoryLoading] = React.useState(false);
  const [historyError, setHistoryError] = React.useState<string | null>(null);

  React.useEffect(() => {
    if (!workspaceId && status.workspaces[0]?.id) {
      setWorkspaceId(status.workspaces[0].id);
    }
  }, [status.workspaces, workspaceId]);

  const refreshHistory = React.useCallback(async () => {
    setHistoryLoading(true);
    setHistoryError(null);
    try {
      const result = await api.commandHistory({
        limit: 200,
      });
      setHistory(result.entries);
    } catch (err) {
      setHistoryError(err instanceof Error ? err.message : '读取命令历史失败');
    } finally {
      setHistoryLoading(false);
    }
  }, []);

  React.useEffect(() => {
    void refreshHistory();
  }, [refreshHistory]);

  React.useEffect(() => {
    const interval = window.setInterval(() => {
      void refreshHistory();
    }, 5000);
    return () => window.clearInterval(interval);
  }, [refreshHistory]);

  const run = async () => {
    if (!workspaceId || !command.trim()) {
      return;
    }
    setRunning(true);
    try {
      const result = await api.terminalExec({
        workspace_id: workspaceId,
        command: command.trim(),
        args: splitArgs(args),
      });
      setOutput(formatTerminalResult(result));
      await refreshHistory();
    } catch (err) {
      setOutput(err instanceof Error ? err.message : '执行失败');
    } finally {
      setRunning(false);
    }
  };

  const clearHistory = async () => {
    setHistoryError(null);
    try {
      const result = await api.clearCommandHistory();
      setHistory(result.entries);
    } catch (err) {
      setHistoryError(err instanceof Error ? err.message : '清空命令历史失败');
    }
  };

  const visibleHistory = React.useMemo(
    () =>
      history.filter((entry) => {
        if (sourceFilter === 'all') {
          return true;
        }
        return sourceGroup(entry.source) === sourceFilter;
      }),
    [history, sourceFilter],
  );

  return (
    <section className="terminalPage">
      <section className="panel">
        <div className="panelHeader">
          <div>
            <h2><Terminal size={18} />终端执行</h2>
            <p>这里通过云端 relay 回到本机执行，用来验证 ChatOS 侧终端链路。</p>
          </div>
        </div>
        <div className="terminalForm">
          <select value={workspaceId} onChange={(event) => setWorkspaceId(event.target.value)}>
            {status.workspaces.map((workspace) => (
              <option value={workspace.id} key={workspace.id}>{workspace.alias}</option>
            ))}
          </select>
          <input value={command} onChange={(event) => setCommand(event.target.value)} placeholder="command" />
          <input value={args} onChange={(event) => setArgs(event.target.value)} placeholder="args, e.g. check -p app" />
          <button className="primaryButton compact" disabled={running || !workspaceId} onClick={() => void run()}>
            <Play size={16} />{running ? '执行中' : '执行'}
          </button>
        </div>
        <pre className="output">{output || '暂无输出'}</pre>
      </section>

      <section className="panel">
        <div className="panelHeader">
          <div>
            <h2><ListChecks size={18} />命令历史</h2>
            <p>展示 ChatOS、Task Runner 和当前页面触发过的本机执行记录。</p>
          </div>
          <div className="headerActions terminalHistoryActions">
            <select value={sourceFilter} onChange={(event) => setSourceFilter(event.target.value)}>
              <option value="all">全部来源</option>
              <option value="chatos_terminal">ChatOS 终端</option>
              <option value="task_runner">Task Runner</option>
              <option value="local_connector_ui">Local Connector 页面</option>
            </select>
            <button className="iconButton" onClick={() => void refreshHistory()} title="刷新命令历史">
              <RefreshCw size={17} />
            </button>
            <button className="iconButton danger" onClick={() => void clearHistory()} title="清空命令历史">
              <Trash2 size={17} />
            </button>
          </div>
        </div>
        {historyError ? <div className="formError">{historyError}</div> : null}
        <div className="commandHistoryList">
          {visibleHistory.map((entry) => (
            <details className="commandHistoryRow" key={entry.id}>
              <summary>
                <div className="commandHistoryMain">
                  <div className="historyMetaLine">
                    <span className="historySource">{sourceLabel(entry.source)}</span>
                    <span className={historyStatusClass(entry.status)}>{statusLabel(entry.status)}</span>
                    {typeof entry.exit_code === 'number' ? <span className="historyExit">exit {entry.exit_code}</span> : null}
                    {entry.tool_name ? <span className="historyTool">{entry.tool_name}</span> : null}
                  </div>
                  <strong className="commandDisplay">{entry.display || entry.command}</strong>
                  <span className="historySubline">
                    {formatHistoryTime(entry.started_at)}
                    {entry.workspace_alias ? ` · ${entry.workspace_alias}` : ''}
                    {entry.cwd ? ` · ${entry.cwd}` : ''}
                  </span>
                </div>
              </summary>
              <div className="historyDetails">
                {entry.request_id ? <div><span>request</span><code>{entry.request_id}</code></div> : null}
                {entry.terminal_session_id ? <div><span>session</span><code>{entry.terminal_session_id}</code></div> : null}
                {entry.sandbox_id ? <div><span>sandbox</span><code>{entry.sandbox_id}</code></div> : null}
                {entry.error ? <pre className="historyPreview errorPreview">{entry.error}</pre> : null}
                {entry.stdout_preview ? <pre className="historyPreview">{entry.stdout_preview}</pre> : null}
                {entry.stderr_preview ? <pre className="historyPreview errorPreview">{entry.stderr_preview}</pre> : null}
                {!entry.error && !entry.stdout_preview && !entry.stderr_preview ? (
                  <div className="emptyState compactEmpty">暂无输出预览</div>
                ) : null}
              </div>
            </details>
          ))}
          {!visibleHistory.length ? (
            <div className="emptyState">{historyLoading ? '正在读取命令历史...' : '还没有命令历史'}</div>
          ) : null}
        </div>
      </section>
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

function splitArgs(value: string): string[] {
  return value
    .split(/\s+/)
    .map((item) => item.trim())
    .filter(Boolean);
}

function sourceLabel(source: string) {
  const labels: Record<string, string> = {
    chatos_terminal_exec: 'ChatOS 终端',
    chatos_terminal_session: 'ChatOS 终端',
    local_mcp: 'Task Runner',
    task_runner_sandbox: 'Task Runner',
    local_connector_ui: 'Local Connector 页面',
  };
  return labels[source] || source;
}

function sourceGroup(source: string) {
  if (source === 'chatos_terminal_exec' || source === 'chatos_terminal_session') {
    return 'chatos_terminal';
  }
  if (source === 'local_mcp' || source === 'task_runner_sandbox') {
    return 'task_runner';
  }
  return source;
}

function statusLabel(status: string) {
  const labels: Record<string, string> = {
    succeeded: '成功',
    failed: '失败',
    timed_out: '超时',
    submitted: '已提交',
    blocked: '已拦截',
  };
  return labels[status] || status;
}

function historyStatusClass(status: string) {
  if (status === 'succeeded' || status === 'submitted') {
    return 'status ok';
  }
  if (status === 'failed' || status === 'timed_out' || status === 'blocked') {
    return 'status bad';
  }
  return 'status warn';
}

function formatHistoryTime(value: string) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }
  return date.toLocaleString();
}

function formatTerminalResult(result: { stdout: string; stderr: string; exit_code?: number | null; success: boolean }) {
  return [
    `exit_code: ${result.exit_code ?? '-'}`,
    `success: ${result.success}`,
    '',
    result.stdout ? `stdout:\n${result.stdout}` : 'stdout: <empty>',
    result.stderr ? `stderr:\n${result.stderr}` : 'stderr: <empty>',
  ].join('\n');
}

function defaultDeviceName(): string {
  return typeof navigator !== 'undefined' ? `Local Connector - ${navigator.platform || 'Desktop'}` : 'Local Connector';
}

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
