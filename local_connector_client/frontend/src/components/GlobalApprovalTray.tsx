// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';
import { BellRing, CheckCircle2, ExternalLink, RefreshCw, ShieldCheck, XCircle } from 'lucide-react';

import {
  api,
  type PendingApprovalItem,
  type RequestPermissionProfile,
} from '../api';
import {
  projectLabel,
  riskLabel,
  riskStatusClass,
} from '../utils/approvalFormat';
import {
  formatHistoryTime,
  sourceLabel,
} from '../utils/terminalFormat';

export function GlobalApprovalTray({
  onOpenApproval,
}: {
  onOpenApproval?: () => void | Promise<void>;
}) {
  const [pending, setPending] = React.useState<PendingApprovalItem[]>([]);
  const [reviewing, setReviewing] = React.useState<PendingApprovalItem[]>([]);
  const [expanded, setExpanded] = React.useState(false);
  const [busy, setBusy] = React.useState<Record<string, boolean>>({});
  const [error, setError] = React.useState<string | null>(null);

  const load = React.useCallback(async () => {
    try {
      const next = await api.pendingApprovals();
      setPending(next.items);
      setReviewing(next.reviewing || []);
      if (!next.items.length && !(next.reviewing || []).length) {
        setExpanded(false);
        setError(null);
      }
    } catch {
      // The tray should not cover the main app when the local core is restarting.
    }
  }, []);

  React.useEffect(() => {
    void load();
    const interval = window.setInterval(() => void load(), 2500);
    return () => window.clearInterval(interval);
  }, [load]);

  const openApproval = React.useCallback(async () => {
    try {
      window.localStorage.setItem('local-connector-next-tab', 'approval');
    } catch {
      // Storage is best-effort in the desktop shell.
    }
    if (onOpenApproval) {
      await onOpenApproval();
      return;
    }
    await window.chatosLocalConnector?.openSettings?.();
  }, [onOpenApproval]);

  const resolve = async (item: PendingApprovalItem, remember: boolean) => {
    const decision = remember ? 'acceptForSession' : 'accept';
    setBusy((current) => ({ ...current, [item.id]: true }));
    setError(null);
    try {
      await api.approvePendingApproval(item.id, {
        decision,
        risk_acknowledged: remember,
      });
      await load();
    } catch (err) {
      setError(err instanceof Error ? err.message : '审批失败');
    } finally {
      setBusy((current) => ({ ...current, [item.id]: false }));
    }
  };

  const deny = async (item: PendingApprovalItem) => {
    setBusy((current) => ({ ...current, [item.id]: true }));
    setError(null);
    try {
      await api.denyPendingApproval(item.id, {
        reason: '已从全局审批入口快速拒绝',
      });
      await load();
    } catch (err) {
      setError(err instanceof Error ? err.message : '拒绝失败');
    } finally {
      setBusy((current) => ({ ...current, [item.id]: false }));
    }
  };

  const total = pending.length + reviewing.length;
  if (!total) {
    return null;
  }

  const visiblePending = pending.slice(0, 3);
  const hiddenPendingCount = Math.max(0, pending.length - visiblePending.length);

  return (
    <aside className={expanded ? 'globalApprovalTray expanded' : 'globalApprovalTray'}>
      <button
        type="button"
        className="globalApprovalSummary"
        onClick={() => setExpanded((current) => !current)}
        aria-expanded={expanded}
      >
        <span className="globalApprovalIcon">
          {reviewing.length ? <RefreshCw className="spinIcon" size={15} /> : <BellRing size={15} />}
        </span>
        <span>
          <strong>{pending.length ? `${pending.length} 项待审批` : `${reviewing.length} 项 AI 审批中`}</strong>
          <small>{pending.length && reviewing.length ? `${reviewing.length} 项正在 AI 审核` : '点击展开快速处理'}</small>
        </span>
      </button>

      {expanded ? (
        <div className="globalApprovalPopover">
          <div className="globalApprovalHeader">
            <div>
              <strong>命令审批</strong>
              <span>{pending.length} 项等待处理，{reviewing.length} 项正在 AI 审核</span>
            </div>
            <button
              type="button"
              className="iconButton"
              title="打开完整审批页"
              aria-label="打开完整审批页"
              onClick={() => void openApproval()}
            >
              <ExternalLink size={15} />
            </button>
          </div>
          {error ? <div className="formError">{error}</div> : null}
          {reviewing.length ? (
            <div className="globalApprovalReviewing">
              <RefreshCw className="spinIcon" size={13} />
              <span>{reviewing.length} 条命令正在等待 AI 审批结果</span>
            </div>
          ) : null}
          <div className="globalApprovalList">
            {visiblePending.map((item) => {
              const canRemember = !item.confirmation
                && (item.available_decisions?.includes('acceptForSession') ?? true);
              return (
                <div className="globalApprovalItem" key={item.id}>
                  <div className="globalApprovalCommandLine">
                    <span className={riskStatusClass(item.risk)}>{riskLabel(item.risk)}</span>
                    <strong>{item.command}</strong>
                  </div>
                  <span className="globalApprovalMeta">
                    {sourceLabel(item.source)} · {projectLabel(item.project_key)} · {formatHistoryTime(item.created_at)}
                  </span>
                  {item.reason ? <span className="globalApprovalReason">{item.reason}</span> : null}
                  {item.requested_permissions ? (
                    <span className="globalApprovalReason">
                      {formatRequestedPermissions(item.requested_permissions)}
                    </span>
                  ) : null}
                  <div className="globalApprovalActions">
                    {item.confirmation ? (
                      <button
                        type="button"
                        className="ghostButton compact"
                        onClick={() => void openApproval()}
                      >
                        <ShieldCheck size={15} />去审批
                      </button>
                    ) : (
                      <button
                        type="button"
                        className="primaryButton compact"
                        disabled={busy[item.id]}
                        onClick={() => void resolve(item, false)}
                      >
                        <CheckCircle2 size={15} />本次通过
                      </button>
                    )}
                    {canRemember ? (
                      <button
                        type="button"
                        className="ghostButton compact"
                        disabled={busy[item.id]}
                        onClick={() => void resolve(item, true)}
                      >
                        <ShieldCheck size={15} />本会话
                      </button>
                    ) : null}
                    <button
                      type="button"
                      className="ghostButton compact dangerText"
                      disabled={busy[item.id]}
                      onClick={() => void deny(item)}
                    >
                      <XCircle size={15} />拒绝
                    </button>
                  </div>
                </div>
              );
            })}
          </div>
          {hiddenPendingCount ? (
            <button
              type="button"
              className="globalApprovalMore"
              onClick={() => void openApproval()}
            >
              还有 {hiddenPendingCount} 项，打开完整审批页
            </button>
          ) : null}
        </div>
      ) : null}
    </aside>
  );
}

function formatRequestedPermissions(permissions: RequestPermissionProfile): string {
  const labels: string[] = [];
  const fileSystem = permissions.fileSystem;
  for (const entry of fileSystem?.entries || []) {
    const path = entry.path.type === 'path'
      ? entry.path.path
      : entry.path.type === 'glob_pattern'
        ? entry.path.pattern
        : `${entry.path.value.kind}${entry.path.value.subpath ? `/${entry.path.value.subpath}` : ''}`;
    labels.push(`文件 ${entry.access}: ${path}`);
  }
  for (const path of fileSystem?.read || []) labels.push(`文件 read: ${path}`);
  for (const path of fileSystem?.write || []) labels.push(`文件 write: ${path}`);
  if (permissions.network?.enabled) labels.push('网络访问');
  return labels.join('；') || '无有效增量权限';
}
