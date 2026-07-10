// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';
import { BellRing, CheckCircle2, ListChecks, RefreshCw, ShieldCheck, XCircle } from 'lucide-react';

import { api, type ApprovalMode, type ApprovalSettings, type PendingApprovalItem } from '../api';
import {
  approvalDecisionClass,
  approvalDecisionLabel,
  approvalModeDescription,
  approvalModeLabel,
  decisionSourceLabel,
  projectLabel,
  riskLabel,
  riskStatusClass,
} from '../utils/approvalFormat';
import {
  formatHistoryTime,
  sourceLabel,
} from '../utils/terminalFormat';

export function ApprovalPanel() {
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
