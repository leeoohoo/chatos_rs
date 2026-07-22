// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';
import { Download, RefreshCw, Sparkles } from 'lucide-react';

import { api, type AgentPromptUpdateStatus } from '../api';

export function AgentPromptUpdateSettings() {
  const [status, setStatus] = React.useState<AgentPromptUpdateStatus | null>(null);
  const [checking, setChecking] = React.useState(false);
  const [updating, setUpdating] = React.useState(false);
  const [message, setMessage] = React.useState<string | null>(null);
  const [error, setError] = React.useState<string | null>(null);

  const loadStatus = React.useCallback(async () => {
    setError(null);
    try {
      setStatus(await api.agentPromptStatus());
    } catch (err) {
      setError(err instanceof Error ? err.message : '读取系统 Agent 配置版本失败');
    }
  }, []);

  React.useEffect(() => {
    void loadStatus();
  }, [loadStatus]);

  const checkUpdates = async () => {
    setChecking(true);
    setMessage(null);
    setError(null);
    try {
      const next = await api.checkAgentPromptUpdates();
      setStatus(next);
      setMessage(next.update_available ? '检测到新的系统 Agent 配置。' : '当前已经是最新版本。');
    } catch (err) {
      setError(err instanceof Error ? err.message : '检查系统 Agent 配置更新失败');
      await loadStatus();
    } finally {
      setChecking(false);
    }
  };

  const update = async () => {
    setUpdating(true);
    setMessage(null);
    setError(null);
    try {
      const next = await api.updateAgentPrompts();
      setStatus(next);
      setMessage(`系统 Agent Prompt 与插件配置已更新到版本 ${next.installed_bundle_version}。`);
    } catch (err) {
      setError(err instanceof Error ? err.message : '更新系统 Agent 配置失败');
      await loadStatus();
    } finally {
      setUpdating(false);
    }
  };

  return (
    <section className="panel">
      <div className="panelHeader">
        <div>
          <h2><Sparkles size={18} />系统 Agent 配置</h2>
          <p>检测云端已发布版本，由你确认后原子同步 Prompt 与插件管理中的 MCP、Skill、Agent 状态和权限策略。</p>
        </div>
        <button className="iconButton" onClick={() => void loadStatus()} title="刷新本机版本">
          <RefreshCw size={17} />
        </button>
      </div>
      {message ? <div className="banner">{message}</div> : null}
      {error ? <div className="formError">{error}</div> : null}
      <div className={`developerModeCard ${status?.update_available ? 'active' : ''}`}>
        <div className="developerEndpointGrid">
          <div><span>当前版本</span><code>{status?.installed_bundle_version || '未初始化'}</code></div>
          <div><span>最新版本</span><code>{status?.remote_bundle_version || '尚未检查'}</code></div>
          <div>
            <span>Prompt</span>
            <code>{status ? `${status.prompt_count}/${status.expected_prompt_count}` : '--'}</code>
          </div>
          <div>
            <span>插件配置</span>
            <code>{status ? `${status.capability_count}/${status.expected_capability_count}` : '--'}</code>
          </div>
          <div>
            <span>状态</span>
            <code>{statusLabel(status)}</code>
          </div>
        </div>
        {status?.last_synced_at ? <small>上次更新：{formatTime(status.last_synced_at)}</small> : null}
        {status?.last_error ? <small>上次检查失败：{status.last_error}</small> : null}
      </div>
      <div className="buttonRow">
        <button
          type="button"
          className="ghostButton compact"
          disabled={checking || updating || !status?.configured}
          onClick={() => void checkUpdates()}
        >
          <RefreshCw className={checking ? 'spinIcon' : ''} size={15} />
          {checking ? '检查中' : '检查更新'}
        </button>
        <button
          type="button"
          className="primaryButton compact"
          disabled={checking || updating || !status?.configured || (!status.update_available && status.initialized)}
          onClick={() => void update()}
        >
          <Download size={15} />
          {updating ? '更新中' : status?.initialized ? '更新' : '初始化'}
        </button>
      </div>
    </section>
  );
}

function statusLabel(status: AgentPromptUpdateStatus | null): string {
  if (!status) return '读取中';
  if (!status.configured) return '等待登录';
  if (!status.initialized) return '需要初始化';
  if (status.update_available) return '有新版本';
  return '已安装';
}

function formatTime(value: string): string {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
}
