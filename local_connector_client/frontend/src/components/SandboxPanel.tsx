// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';
import {
  Container,
  RefreshCw,
  Settings2,
  Shield,
} from 'lucide-react';

import {
  api,
  type ConnectorStatus,
  type SandboxCapabilities,
  type SandboxLease,
  type SandboxSettings,
  type SandboxSettingsUpdate,
} from '../api';
import { SandboxPolicySettings } from './SandboxPolicySettings';

export function SandboxPanel({
  status,
  onRefresh,
}: {
  status: ConnectorStatus;
  onRefresh: () => Promise<void>;
}) {
  const [capabilities, setCapabilities] = React.useState<SandboxCapabilities | null>(null);
  const [settings, setSettings] = React.useState<SandboxSettings | null>(null);
  const [leases, setLeases] = React.useState<SandboxLease[]>([]);
  const [message, setMessage] = React.useState<string | null>(null);
  const [loadingDetails, setLoadingDetails] = React.useState(false);
  const [savingSettings, setSavingSettings] = React.useState(false);
  const enablingSandbox = React.useRef(false);

  const refreshSandboxConfig = React.useCallback(async () => {
    try {
      const [nextCapabilities, nextSettings] = await Promise.all([
        api.sandboxCapabilities(),
        api.sandboxSettings(),
      ]);
      setCapabilities(nextCapabilities);
      setSettings(nextSettings);
    } catch (err) {
      setMessage(err instanceof Error ? err.message : '读取沙箱设置失败');
    }
  }, []);

  const refreshSandboxDetails = React.useCallback(async () => {
    if (!status.sandbox.enabled) {
      setLeases([]);
      return;
    }
    setLoadingDetails(true);
    try {
      const nextLeases = await api.sandboxLeases();
      setLeases(nextLeases);
    } catch (err) {
      setMessage(err instanceof Error ? err.message : '读取沙箱运行信息失败');
    } finally {
      setLoadingDetails(false);
    }
  }, [status.sandbox.enabled]);

  React.useEffect(() => {
    void refreshSandboxConfig();
  }, [refreshSandboxConfig]);

  React.useEffect(() => {
    void refreshSandboxDetails();
  }, [refreshSandboxDetails]);

  React.useEffect(() => {
    if (
      status.sandbox.enabled
      || !capabilities
      || status.sandbox.permission_configuration_error
      || enablingSandbox.current
    ) {
      return;
    }
    const preferred = capabilities.backends.find(
      (capability) => capability.backend === 'local_process' && capability.status === 'ready',
    );
    if (!preferred) {
      setMessage('本机进程沙箱当前不可用，请检查本机沙箱组件。');
      return;
    }
    enablingSandbox.current = true;
    void api.updateSandboxSettings({
      enabled: true,
      default_backend: preferred.backend,
    }).then(async (nextSettings) => {
      setSettings(nextSettings);
      setMessage('安全沙箱已自动启用');
      await onRefresh();
    }).catch((err) => {
      setMessage(err instanceof Error ? err.message : '启用安全沙箱失败');
    }).finally(() => {
      enablingSandbox.current = false;
    });
  }, [capabilities, onRefresh, status.sandbox]);

  React.useEffect(() => {
    if (!status.sandbox.enabled) {
      return;
    }
    const interval = window.setInterval(() => {
      void refreshSandboxDetails();
    }, 6000);
    return () => window.clearInterval(interval);
  }, [refreshSandboxDetails, status.sandbox.enabled]);

  const saveSandboxSettings = async (
    patch: SandboxSettingsUpdate,
    label: string,
  ) => {
    setMessage(null);
    setSavingSettings(true);
    try {
      const next = await api.updateSandboxSettings(patch);
      setSettings(next);
      setMessage(`${label}已更新`);
      await Promise.all([refreshSandboxConfig(), onRefresh()]);
    } catch (err) {
      setMessage(err instanceof Error ? err.message : `${label}更新失败`);
    } finally {
      setSavingSettings(false);
    }
  };

  return (
    <section className="sandboxPage">
      <div className="panel sandboxHero">
        <div className="panelHeader">
          <div>
            <h2><Shield size={18} />安全沙箱</h2>
            <p>使用本机进程隔离，并管理文件、网络与 AI 审批。</p>
          </div>
          <div className="headerActions">
            <button
              className="iconButton"
              onClick={() => void Promise.all([
                refreshSandboxConfig(),
                refreshSandboxDetails(),
                onRefresh(),
              ])}
              title="刷新安全沙箱状态"
            >
              <RefreshCw size={17} />
            </button>
          </div>
        </div>
        {status.sandbox.permission_configuration_error ? (
          <div className="formError">
            受管权限策略尚未安全加载，沙箱执行已阻止：{status.sandbox.permission_configuration_error}
          </div>
        ) : null}
        <SandboxPolicySettings
          status={status}
          settings={settings}
          capabilities={capabilities}
          saving={savingSettings}
          onSave={saveSandboxSettings}
        />
        {message ? <div className="banner">{message}</div> : null}
      </div>

      {status.sandbox.enabled ? (
        <details className="panel sandboxAdvancedPanel">
          <summary>
            <span><Settings2 size={16} />高级运行信息</span>
            <small>查看当前运行中的本机沙箱实例</small>
          </summary>
          <div className="sandboxAdvancedContent">
          <section className="panel">
            <div className="panelHeader">
              <div>
                <h2><Container size={18} />当前沙箱</h2>
                <p>本地任务运行时创建的隔离实例。</p>
              </div>
            </div>
            {leases.length ? (
              <div className="leaseTable">
                <div className="leaseHeader">
                  <span>Sandbox</span>
                  <span>Run</span>
                  <span>Backend</span>
                  <span>Status</span>
                </div>
                {leases.map((lease) => (
                  <div className="leaseRow" key={lease.id}>
                    <span className="mono">{lease.sandbox_id}</span>
                    <span className="mono">{lease.run_id}</span>
                    <span>{lease.backend === 'local_process' ? '本机进程' : lease.backend}</span>
                    <span className={lease.status === 'ready' ? 'status ok' : 'status warn'}>{lease.status}</span>
                  </div>
                ))}
              </div>
            ) : (
                <div className="emptyState">{loadingDetails ? '正在读取本机沙箱...' : '当前没有运行中的本机沙箱。'}</div>
            )}
          </section>
          </div>
        </details>
      ) : (
        <section className="panel">
          <div className="emptyState">正在启用安全沙箱；如果持续不可用，请检查本机沙箱组件。</div>
        </section>
      )}
    </section>
  );
}
