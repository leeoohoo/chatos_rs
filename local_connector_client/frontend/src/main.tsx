// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';
import ReactDOM from 'react-dom/client';
import {
  Brain,
  Cpu,
  FolderOpen,
  LockKeyhole,
  Moon,
  Plug,
  RefreshCw,
  Server,
  Settings2,
  Shield,
  ShieldCheck,
  Sun,
  Terminal,
  Wifi,
} from 'lucide-react';

import {
  api,
  type ConnectorStatus,
} from './api';
import { ApprovalPanel } from './components/ApprovalPanel';
import {
  ConnectionCard,
  LocalBoundaryPanel,
  WorkspacePanel,
} from './components/ConnectionPanels';
import { ModelConfigPanel } from './components/ModelConfigPanel';
import { McpConfigPanel } from './components/McpConfigPanel';
import { RuntimeSettingsPanel } from './components/RuntimeSettingsPanel';
import { SandboxPanel } from './components/SandboxPanel';
import { TerminalPanel } from './components/TerminalPanel';
import './styles.css';
import './styles-terminal.css';
import './styles-approval.css';
import './styles-models.css';
import './styles-mcp.css';
import './styles-command-history.css';
import './styles-sandbox.css';
import './styles-responsive.css';

type AppTab = 'overview' | 'workspaces' | 'mcps' | 'terminal' | 'models' | 'approval' | 'settings' | 'sandbox';
type LocalIcon = typeof Server;
type ThemeMode = 'light' | 'dark';

const TABS: Array<{
  id: AppTab;
  label: string;
  eyebrow: string;
  description: string;
  icon: LocalIcon;
}> = [
  {
    id: 'overview',
    label: '设备配对',
    eyebrow: 'CONNECTION',
    description: '查看本机设备、云端连接与安全边界。',
    icon: Server,
  },
  {
    id: 'workspaces',
    label: '开放目录',
    eyebrow: 'WORKSPACES',
    description: '管理 Chat OS 可以访问的本地工作目录。',
    icon: FolderOpen,
  },
  {
    id: 'mcps',
    label: 'MCP 配置',
    eyebrow: 'LOCAL MCP',
    description: '管理仅由当前设备执行的个人 MCP 工具。',
    icon: Plug,
  },
  {
    id: 'terminal',
    label: '本机终端',
    eyebrow: 'TERMINAL',
    description: '验证命令链路，并查看本机执行历史。',
    icon: Terminal,
  },
  {
    id: 'models',
    label: '模型配置',
    eyebrow: 'MODELS',
    description: '配置本地 Agent 使用的模型与运行参数。',
    icon: Brain,
  },
  {
    id: 'approval',
    label: '命令审批',
    eyebrow: 'APPROVAL',
    description: '控制敏感命令的审批级别、白名单与历史。',
    icon: ShieldCheck,
  },
  {
    id: 'settings',
    label: '运行配置',
    eyebrow: 'RUNTIME',
    description: '调整 Local Connector Core 的本机运行参数。',
    icon: Settings2,
  },
  {
    id: 'sandbox',
    label: '本地沙箱',
    eyebrow: 'SANDBOX',
    description: '管理 Docker 隔离环境、镜像与运行实例。',
    icon: Shield,
  },
];

function ShellApp() {
  const [status, setStatus] = React.useState<ConnectorStatus | null>(null);
  const [refreshing, setRefreshing] = React.useState(false);

  const refresh = React.useCallback(async () => {
    setRefreshing(true);
    try {
      setStatus(await api.status());
    } catch {
      setStatus(null);
    } finally {
      setRefreshing(false);
    }
  }, []);

  React.useEffect(() => {
    void refresh();
    const timer = window.setInterval(() => void refresh(), 5000);
    return () => window.clearInterval(timer);
  }, [refresh]);

  return (
    <div className="desktopShell">
      <div className="desktopShellBrand">
        <Cpu size={18} />
        <span>Chat OS</span>
      </div>
      <div className="desktopShellStatus">
        <span className={status?.connector_running ? 'coreStatusDot online' : 'coreStatusDot'} />
        <strong>{status?.connector_running ? '本机已连接' : status?.configured ? '等待连接' : '未授权本机'}</strong>
        {status?.user?.username ? <small>{status.user.username}</small> : null}
      </div>
      <div className="desktopShellActions">
        <button
          type="button"
          className="iconButton"
          title="刷新 Chat OS"
          aria-label="刷新 Chat OS"
          onClick={() => window.chatosLocalConnector?.reloadChatOS?.()}
        >
          <RefreshCw size={16} />
        </button>
        <button
          type="button"
          className="iconButton"
          title="刷新本机状态"
          aria-label="刷新本机状态"
          onClick={() => void refresh()}
        >
          <Wifi className={refreshing ? 'spinIcon' : ''} size={16} />
        </button>
        <button
          type="button"
          className="shellSettingsButton"
          onClick={() => window.chatosLocalConnector?.openSettings?.()}
        >
          <Settings2 size={16} />
          <span>设置</span>
        </button>
      </div>
    </div>
  );
}

function SettingsApp() {
  const [status, setStatus] = React.useState<ConnectorStatus | null>(null);
  const [loading, setLoading] = React.useState(true);
  const [refreshing, setRefreshing] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  const [activeTab, setActiveTab] = React.useState<AppTab>('workspaces');
  const [theme, setTheme] = React.useState<ThemeMode>(initialTheme);

  const refresh = React.useCallback(async () => {
    setRefreshing(true);
    setError(null);
    try {
      setStatus(await api.status());
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Local Connector Core 未启动');
    } finally {
      setLoading(false);
      setRefreshing(false);
    }
  }, []);

  React.useEffect(() => {
    void refresh();
  }, [refresh]);

  React.useEffect(() => {
    document.documentElement.dataset.theme = theme;
    document.documentElement.style.colorScheme = theme;
    try {
      window.localStorage.setItem('local-connector-theme', theme);
    } catch {
      // The desktop shell may temporarily disable storage during startup.
    }
  }, [theme]);

  if (loading) {
    return (
      <div className="screen center loadingScreen">
        <div className="loadingMark"><Cpu size={28} /></div>
        <div className="loadingCopy">
          <span>LOCAL RUNTIME</span>
          <strong>正在连接 Local Connector Core</strong>
        </div>
        <div className="loadingTrack"><span /></div>
      </div>
    );
  }

  const activeTabInfo = TABS.find((tab) => tab.id === activeTab) || TABS[0];
  const ActiveIcon = activeTabInfo.icon;

  return (
    <div className="screen">
      <header className="topbar" data-tauri-drag-region>
        <div className="brand">
          <div className="brandMark"><Cpu size={22} /></div>
          <div className="brandCopy">
            <span>Chat OS</span>
            <h1>Local Connector</h1>
          </div>
        </div>
        <div className="topbarActions">
          <div className="coreStatus">
            <span className={status?.connector_running ? 'coreStatusDot online' : 'coreStatusDot'} />
            <div>
              <span>LOCAL CORE</span>
              <strong>{status?.connector_running ? '连接正常' : status?.configured ? '等待连接' : '等待配对'}</strong>
            </div>
          </div>
          <button
            type="button"
            className="iconButton topbarTheme"
            onClick={() => setTheme((current) => current === 'dark' ? 'light' : 'dark')}
            title={theme === 'dark' ? '切换到浅色模式' : '切换到深色模式'}
            aria-label={theme === 'dark' ? '切换到浅色模式' : '切换到深色模式'}
          >
            {theme === 'dark' ? <Sun size={18} /> : <Moon size={18} />}
          </button>
          <button
            type="button"
            className="iconButton topbarRefresh"
            onClick={() => void refresh()}
            title="刷新状态"
          >
            <RefreshCw className={refreshing ? 'spinIcon' : ''} size={18} />
          </button>
        </div>
      </header>

      {error ? <div className="banner error globalBanner">{error}</div> : null}

      {!status?.configured ? (
        <main className="authStage">
          <section className="authIntro">
            <span className="pageEyebrow">SECURE DEVICE BRIDGE</span>
            <h2>让 Chat OS 安全地连接<br />这台电脑。</h2>
            <p>本地目录、终端与 Docker 沙箱始终留在当前设备。云端只能通过已配对连接发起受控请求。</p>
            <div className="authFeatures">
              <div>
                <FolderOpen size={17} />
                <span><strong>目录按需开放</strong><small>未授权路径默认不可见</small></span>
              </div>
              <div>
                <LockKeyhole size={17} />
                <span><strong>命令审批保护</strong><small>高风险操作可逐条确认</small></span>
              </div>
              <div>
                <Wifi size={17} />
                <span><strong>本机能力桥接</strong><small>连接状态实时可见</small></span>
              </div>
            </div>
          </section>
          <section className="authPanel desktopAuthNotice">
            <span className="pageEyebrow">SINGLE SIGN-ON</span>
            <h3>请先在 Chat OS 页面登录</h3>
            <p>
              桌面端只保留 Chat OS 一个登录入口。登录成功后，本机会通过一次性授权票据自动完成 Local Connector 配对。
            </p>
            <button type="button" className="primaryButton" onClick={() => void refresh()}>
              刷新本机状态
            </button>
          </section>
        </main>
      ) : (
        <main className="workbench">
          <TabNav activeTab={activeTab} onChange={setActiveTab} />
          <section className="contentArea">
            <header className="pageIntro">
              <div className="pageIcon"><ActiveIcon size={21} /></div>
              <div>
                <span className="pageEyebrow">{activeTabInfo.eyebrow}</span>
                <h2>{activeTabInfo.label}</h2>
                <p>{activeTabInfo.description}</p>
              </div>
            </header>
            <div className="contentView">
              {activeTab === 'overview' ? (
                <div className="tabGrid">
                  <ConnectionCard status={status} onStatus={setStatus} />
                  <LocalBoundaryPanel status={status} />
                </div>
              ) : null}
              {activeTab === 'workspaces' ? <WorkspacePanel status={status} onStatus={setStatus} /> : null}
              {activeTab === 'mcps' ? <McpConfigPanel /> : null}
              {activeTab === 'terminal' ? <TerminalPanel status={status} /> : null}
              {activeTab === 'models' ? <ModelConfigPanel /> : null}
              {activeTab === 'approval' ? <ApprovalPanel /> : null}
              {activeTab === 'settings' ? <RuntimeSettingsPanel /> : null}
              {activeTab === 'sandbox' ? (
                <SandboxPanel status={status} onStatus={setStatus} onRefresh={refresh} />
              ) : null}
            </div>
          </section>
        </main>
      )}
    </div>
  );
}

function Root() {
  const params = new URLSearchParams(window.location.search);
  return params.get('view') === 'shell' ? <ShellApp /> : <SettingsApp />;
}

function initialTheme(): ThemeMode {
  if (typeof window === 'undefined') {
    return 'dark';
  }
  try {
    const saved = window.localStorage.getItem('local-connector-theme');
    if (saved === 'light' || saved === 'dark') {
      return saved;
    }
  } catch {
    // Fall back to the operating system preference.
  }
  return window.matchMedia('(prefers-color-scheme: light)').matches ? 'light' : 'dark';
}

function TabNav({
  activeTab,
  onChange,
}: {
  activeTab: AppTab;
  onChange: (tab: AppTab) => void;
}) {
  return (
    <nav className="tabs" aria-label="Local Connector sections">
      <span className="navLabel">CONTROL CENTER</span>
      <div className="tabList">
        {TABS.map((tab) => {
          const Icon = tab.icon;
          return (
            <button
              key={tab.id}
              type="button"
              className={activeTab === tab.id ? 'active' : ''}
              onClick={() => onChange(tab.id)}
            >
              <span className="tabIcon"><Icon size={17} /></span>
              <span>{tab.label}</span>
            </button>
          );
        })}
      </div>
      <div className="localBoundaryBadge">
        <ShieldCheck size={18} />
        <span><strong>本机安全边界</strong><small>敏感能力仅在设备内执行</small></span>
      </div>
    </nav>
  );
}

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
  <React.StrictMode>
    <Root />
  </React.StrictMode>,
);
