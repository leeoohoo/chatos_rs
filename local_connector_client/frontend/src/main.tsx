// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';
import ReactDOM from 'react-dom/client';
import {
  Brain,
  FolderOpen,
  RefreshCw,
  Server,
  Settings2,
  Shield,
  ShieldCheck,
  Terminal,
} from 'lucide-react';

import {
  api,
  type ConnectorStatus,
} from './api';
import { ApprovalPanel } from './components/ApprovalPanel';
import {
  AuthPanel,
  ConnectionCard,
  LocalBoundaryPanel,
  WorkspacePanel,
} from './components/ConnectionPanels';
import { ModelConfigPanel } from './components/ModelConfigPanel';
import { RuntimeSettingsPanel } from './components/RuntimeSettingsPanel';
import { SandboxPanel } from './components/SandboxPanel';
import { TerminalPanel } from './components/TerminalPanel';
import './styles.css';
import './styles-terminal.css';
import './styles-approval.css';
import './styles-models.css';
import './styles-command-history.css';
import './styles-sandbox.css';
import './styles-responsive.css';

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

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
