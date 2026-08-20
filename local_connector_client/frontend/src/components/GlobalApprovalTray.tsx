// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';
import {
  BellRing,
  CheckCircle2,
  ChevronLeft,
  ChevronRight,
  ExternalLink,
  Minimize2,
  RefreshCw,
  ShieldAlert,
  XCircle,
} from 'lucide-react';

import {
  api,
  type ApprovalHistoryEntry,
  type PendingApprovalItem,
  type RequestPermissionProfile,
} from '../api';
import { projectLabel, riskLabel, riskStatusClass } from '../utils/approvalFormat';
import { formatHistoryTime, sourceLabel } from '../utils/terminalFormat';

interface ApprovalOutcome {
  command: string;
  decision: 'approved' | 'denied';
  decisionSource: string;
  id: string;
  reason?: string | null;
}

export function GlobalApprovalTray() {
  const [pending, setPending] = React.useState<PendingApprovalItem[]>([]);
  const [reviewing, setReviewing] = React.useState<PendingApprovalItem[]>([]);
  const [outcome, setOutcome] = React.useState<ApprovalOutcome | null>(null);
  const [expanded, setExpanded] = React.useState(false);
  const [activeIndex, setActiveIndex] = React.useState(0);
  const [confirmationResponses, setConfirmationResponses] = React.useState<Record<string, string>>({});
  const [busy, setBusy] = React.useState<Record<string, boolean>>({});
  const [error, setError] = React.useState<string | null>(null);
  const knownApprovalIds = React.useRef<Set<string>>(new Set());
  const knownHistoryIds = React.useRef<Set<string> | null>(null);

  const load = React.useCallback(async () => {
    try {
      const [next, settings] = await Promise.all([
        api.pendingApprovals(),
        api.approvalSettings().catch(() => null),
      ]);
      const nextPending = next.items;
      const pendingRequestIds = new Set(nextPending.map((item) => item.request_id));
      const nextReviewing = (next.reviewing || [])
        .filter((item) => !pendingRequestIds.has(item.request_id));
      const nextApprovalIds = new Set([
        ...nextPending.map((item) => `pending:${item.request_id}`),
        ...nextReviewing.map((item) => `reviewing:${item.request_id}`),
      ]);
      const hasNewApproval = [...nextApprovalIds]
        .some((id) => !knownApprovalIds.current.has(id));
      knownApprovalIds.current = nextApprovalIds;
      setPending(nextPending);
      setReviewing(nextReviewing);
      setActiveIndex((current) => Math.min(
        current,
        Math.max(0, nextPending.length + nextReviewing.length - 1),
      ));

      if (settings) {
        const nextHistoryIds = new Set(settings.history.map((entry) => entry.id));
        if (knownHistoryIds.current) {
          const newest = [...settings.history]
            .reverse()
            .find((entry) => !knownHistoryIds.current?.has(entry.id));
          if (newest) setOutcome(outcomeFromHistory(newest));
        }
        knownHistoryIds.current = nextHistoryIds;
      }

      if (hasNewApproval) {
        setExpanded(true);
      } else if (!nextPending.length && !nextReviewing.length) {
        setExpanded(false);
        setError(null);
      }
    } catch {
      // Keep the overlay quiet while the local core is restarting.
    }
  }, []);

  React.useEffect(() => {
    let stopped = false;
    let timer: number | undefined;
    const poll = async () => {
      await load();
      if (!stopped) timer = window.setTimeout(poll, document.hidden ? 5000 : 2000);
    };
    const refreshNow = () => void load();
    void poll();
    document.addEventListener('visibilitychange', refreshNow);
    window.addEventListener('focus', refreshNow);
    return () => {
      stopped = true;
      if (timer !== undefined) window.clearTimeout(timer);
      document.removeEventListener('visibilitychange', refreshNow);
      window.removeEventListener('focus', refreshNow);
    };
  }, [load]);

  React.useEffect(() => {
    if (!outcome) return undefined;
    const timer = window.setTimeout(() => setOutcome(null), 4000);
    return () => window.clearTimeout(timer);
  }, [outcome]);

  const total = pending.length + reviewing.length;
  const visibleApprovals = React.useMemo(
    () => [
      ...pending.map((item) => ({ item, phase: 'pending' as const })),
      ...reviewing.map((item) => ({ item, phase: 'reviewing' as const })),
    ],
    [pending, reviewing],
  );
  const visible = Boolean(total || outcome);
  React.useEffect(() => {
    const mode = visible ? (expanded ? 'expanded' : 'compact') : 'hidden';
    void window.chatosLocalConnector?.setApprovalOverlayMode?.(mode);
  }, [expanded, visible]);

  React.useEffect(() => () => {
    void window.chatosLocalConnector?.setApprovalOverlayMode?.('hidden');
  }, []);

  const openApproval = React.useCallback(async () => {
    await window.chatosLocalConnector?.openSettings?.('approval');
  }, []);

  const approve = async (item: PendingApprovalItem) => {
    const confirmationResponse = confirmationResponses[item.id]?.trim() || '';
    if (item.confirmation && confirmationResponse !== item.confirmation.challenge) {
      setError(`请输入完整确认口令 ${item.confirmation.challenge}`);
      return;
    }
    setBusy((current) => ({ ...current, [item.id]: true }));
    setError(null);
    try {
      await api.approvePendingApproval(item.id, {
        decision: 'accept',
        risk_acknowledged: Boolean(item.confirmation),
        confirmation_response: item.confirmation ? confirmationResponse : undefined,
      });
      setOutcome({
        command: item.command,
        decision: 'approved',
        decisionSource: 'user',
        id: `local-approved-${item.id}`,
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
      await api.denyPendingApproval(item.id, { reason: '已从全局审批浮层拒绝' });
      setOutcome({
        command: item.command,
        decision: 'denied',
        decisionSource: 'user',
        id: `local-denied-${item.id}`,
      });
      await load();
    } catch (err) {
      setError(err instanceof Error ? err.message : '拒绝失败');
    } finally {
      setBusy((current) => ({ ...current, [item.id]: false }));
    }
  };

  if (!visible) return null;

  const activeApproval = visibleApprovals[activeIndex] || null;
  const activeItem = activeApproval?.item || null;
  const isReviewing = activeApproval?.phase === 'reviewing';
  if (!expanded) {
    const showOutcome = Boolean(outcome && !pending.length);
    return (
      <aside className="globalApprovalTray">
        <button
          type="button"
          className={`globalApprovalSummary${showOutcome ? ` outcome ${outcome?.decision}` : ''}`}
          onClick={() => {
            if (total) setExpanded(true);
            else void openApproval();
          }}
          aria-expanded={false}
        >
          <span className="globalApprovalIcon">
            {showOutcome
              ? outcome?.decision === 'approved' ? <CheckCircle2 size={16} /> : <XCircle size={16} />
              : pending.length ? <BellRing size={15} /> : <RefreshCw className="spinIcon" size={15} />}
          </span>
          <span>
            <strong>{outcome && showOutcome ? outcomeLabel(outcome) : pending.length ? `${pending.length} 项等待你的审批` : `${reviewing.length} 项 AI 审批中`}</strong>
            <small>{showOutcome ? outcome?.command : pending.length ? '点击展开并处理' : '完成后会显示审批结果'}</small>
          </span>
        </button>
      </aside>
    );
  }

  return (
    <aside className="globalApprovalTray expanded">
      <div className="globalApprovalPopover">
        <div className="globalApprovalHeader">
          <div>
            <strong><BellRing size={15} />命令审批</strong>
            <span>{pending.length} 项等待处理，{reviewing.length} 项正在 AI 审核</span>
          </div>
          <div className="globalApprovalHeaderActions">
            <button type="button" className="iconButton" title="收起审批浮层" aria-label="收起审批浮层" onClick={() => setExpanded(false)}>
              <Minimize2 size={15} />
            </button>
            <button type="button" className="iconButton" title="打开完整审批页" aria-label="打开完整审批页" onClick={() => void openApproval()}>
              <ExternalLink size={15} />
            </button>
          </div>
        </div>

        {outcome ? (
          <div className={`globalApprovalOutcome ${outcome.decision}`}>
            {outcome.decision === 'approved' ? <CheckCircle2 size={14} /> : <XCircle size={14} />}
            <span>{outcomeLabel(outcome)}</span>
          </div>
        ) : null}
        {error ? <div className="formError">{error}</div> : null}
        {reviewing.length ? (
          <div className="globalApprovalReviewing">
            <RefreshCw className="spinIcon" size={13} />
            <span>{reviewing.length} 条命令正在等待 AI 审批结果</span>
          </div>
        ) : null}

        {activeItem ? (
          <div className="globalApprovalItem">
            <div className="globalApprovalCommandLine">
              {isReviewing ? (
                <span className="status warn"><RefreshCw className="spinIcon" size={13} />AI 审核中</span>
              ) : null}
              <span className={riskStatusClass(activeItem.risk)}>{riskLabel(activeItem.risk)}</span>
              <code>{activeItem.command}</code>
            </div>
            <div className="globalApprovalMeta">
              <span>{sourceLabel(activeItem.source)} · {projectLabel(activeItem.project_key)} · {formatHistoryTime(activeItem.created_at)}</span>
              <code>{activeItem.cwd}</code>
            </div>
            {activeItem.reason ? <div className="globalApprovalReason">{activeItem.reason}</div> : null}
            {activeItem.requested_permissions ? (
              <div className="globalApprovalReason">请求权限：{formatRequestedPermissions(activeItem.requested_permissions)}</div>
            ) : null}
            {activeItem.action_audit ? (
              <div className="globalApprovalAudit">
                <strong>操作审计 · {activeItem.action_audit.operation}</strong>
                {activeItem.action_audit.details?.length ? (
                  <span>{activeItem.action_audit.details.map((detail) => `${detail.key}: ${detail.value}`).join(' · ')}</span>
                ) : null}
              </div>
            ) : null}
            {activeItem.confirmation ? (
              <label className="globalApprovalConfirmation">
                <span><ShieldAlert size={14} />高风险操作，请输入确认口令</span>
                <code>{activeItem.confirmation.challenge}</code>
                <input
                  autoComplete="off"
                  spellCheck={false}
                  value={confirmationResponses[activeItem.id] || ''}
                  onChange={(event) => setConfirmationResponses((current) => ({ ...current, [activeItem.id]: event.target.value }))}
                  placeholder={activeItem.confirmation.challenge}
                />
              </label>
            ) : null}
            <div className="globalApprovalFooter">
              <div className="globalApprovalPager">
                <button type="button" className="iconButton" title="上一条" aria-label="上一条" disabled={activeIndex === 0} onClick={() => setActiveIndex((current) => Math.max(0, current - 1))}>
                  <ChevronLeft size={15} />
                </button>
                <span>{activeIndex + 1} / {visibleApprovals.length}</span>
                <button type="button" className="iconButton" title="下一条" aria-label="下一条" disabled={activeIndex >= visibleApprovals.length - 1} onClick={() => setActiveIndex((current) => Math.min(visibleApprovals.length - 1, current + 1))}>
                  <ChevronRight size={15} />
                </button>
              </div>
              {isReviewing ? (
                <div className="globalApprovalReviewState">
                  <RefreshCw className="spinIcon" size={13} />等待 AI 审批结果
                </div>
              ) : (
                <div className="globalApprovalActions">
                  <button type="button" className="ghostButton compact dangerText" disabled={busy[activeItem.id]} onClick={() => void deny(activeItem)}>
                    <XCircle size={15} />拒绝
                  </button>
                  <button
                    type="button"
                    className="primaryButton compact"
                    disabled={busy[activeItem.id] || Boolean(activeItem.confirmation && confirmationResponses[activeItem.id]?.trim() !== activeItem.confirmation.challenge)}
                    onClick={() => void approve(activeItem)}
                  >
                    <CheckCircle2 size={15} />本次通过
                  </button>
                </div>
              )}
            </div>
          </div>
        ) : (
          <div className="globalApprovalWaiting">
            <RefreshCw className="spinIcon" size={16} />
            <span>AI 正在审核命令，完成后会自动更新。</span>
          </div>
        )}
      </div>
    </aside>
  );
}

function outcomeFromHistory(entry: ApprovalHistoryEntry): ApprovalOutcome {
  return {
    command: entry.normalized_command || entry.command,
    decision: entry.decision === 'approved' ? 'approved' : 'denied',
    decisionSource: entry.decision_source,
    id: entry.id,
    reason: entry.reason,
  };
}

function outcomeLabel(outcome: ApprovalOutcome): string {
  const actor = outcome.decisionSource === 'ai' ? 'AI' : outcome.decisionSource === 'user' ? '你' : '策略';
  return `${actor}${outcome.decision === 'approved' ? '已通过' : '已拒绝'}`;
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
