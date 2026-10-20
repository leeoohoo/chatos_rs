import { useEffect, useRef, useState } from "react";
import { useToolApprovalStore } from "../application/toolApprovalStore";
import {
  allowsToolApprovalDecision,
  riskPresentation,
  type ToolApproval,
  type ToolApprovalDecision,
} from "../domain/toolApproval";
import type { ToolApprovalPort } from "../ports/ToolApprovalPort";

const FALLBACK_POLL_MILLISECONDS = 2_000;

export function GlobalToolApprovalOverlay({
  ownerUserId,
  hostReady,
  port,
}: {
  ownerUserId: string;
  hostReady: boolean;
  port: ToolApprovalPort;
}) {
  const approvals = useToolApprovalStore((state) => state.approvals);
  const hasLoaded = useToolApprovalStore((state) => state.hasLoaded);
  const loadError = useToolApprovalStore((state) => state.loadError);
  const activate = useToolApprovalStore((state) => state.activate);
  const suspend = useToolApprovalStore((state) => state.suspend);
  const refresh = useToolApprovalStore((state) => state.refresh);
  const reset = useToolApprovalStore((state) => state.reset);
  const [expanded, setExpanded] = useState(true);
  const [activeIndex, setActiveIndex] = useState(0);
  const knownIds = useRef<Set<string>>(new Set());

  useEffect(() => {
    if (hostReady) void activate(port, ownerUserId);
    else suspend(ownerUserId);
  }, [activate, hostReady, ownerUserId, port, suspend]);

  useEffect(() => () => reset(), [ownerUserId, reset]);

  useEffect(() => {
    if (!hostReady) return;
    const timer = window.setInterval(() => void refresh(port), FALLBACK_POLL_MILLISECONDS);
    return () => window.clearInterval(timer);
  }, [hostReady, port, refresh]);

  useEffect(() => {
    const nextIds = new Set(approvals.map((approval) => approval.invocationId));
    if ([...nextIds].some((id) => !knownIds.current.has(id))) setExpanded(true);
    knownIds.current = nextIds;
    setActiveIndex((index) => Math.min(index, Math.max(0, approvals.length - 1)));
  }, [approvals]);

  if (approvals.length === 0) {
    if (!hostReady || !hasLoaded || !loadError) return null;
    return (
      <aside className="approval-overlay approval-overlay--status" role="status">
        <span className="approval-overlay__bell" aria-hidden="true">!</span>
        <div><strong>审批队列暂不可用</strong><small>{loadError}</small></div>
        <button onClick={() => void refresh(port)} type="button">重试</button>
      </aside>
    );
  }

  if (!expanded) {
    return (
      <button
        aria-label={`${approvals.length} 项等待审批，点击展开`}
        className="approval-overlay approval-overlay--compact"
        onClick={() => setExpanded(true)}
        type="button"
      >
        <span className="approval-overlay__bell" aria-hidden="true">!</span>
        <span><strong>{approvals.length} 项等待你的审批</strong><small>点击展开并处理</small></span>
        <span aria-hidden="true">‹</span>
      </button>
    );
  }

  const approval = approvals[activeIndex] ?? approvals[0];
  return (
    <aside className="approval-overlay approval-overlay--expanded" aria-label="命令审批">
      <header className="approval-overlay__header">
        <span className="approval-overlay__bell" aria-hidden="true">!</span>
        <div><strong>命令审批</strong><small>{approvals.length} 项等待处理</small></div>
        <button aria-label="收起审批浮层" onClick={() => setExpanded(false)} type="button">›</button>
      </header>
      <ApprovalContent approval={approval} hostReady={hostReady} port={port} />
      <footer className="approval-overlay__navigation">
        {approvals.length > 1 && (
          <>
            <button
              aria-label="上一条审批"
              disabled={activeIndex === 0}
              onClick={() => setActiveIndex((index) => Math.max(0, index - 1))}
              type="button"
            >‹</button>
            <span>{activeIndex + 1} / {approvals.length}</span>
            <button
              aria-label="下一条审批"
              disabled={activeIndex >= approvals.length - 1}
              onClick={() => setActiveIndex((index) => Math.min(approvals.length - 1, index + 1))}
              type="button"
            >›</button>
          </>
        )}
        <span className="approval-overlay__navigation-spacer" />
        {!hostReady && <small>Host 正在恢复，暂时不能处理</small>}
      </footer>
    </aside>
  );
}

function ApprovalContent({
  approval,
  hostReady,
  port,
}: {
  approval: ToolApproval;
  hostReady: boolean;
  port: ToolApprovalPort;
}) {
  const actionId = useToolApprovalStore((state) => state.actionIds[approval.invocationId]);
  const error = useToolApprovalStore((state) => state.approvalErrors[approval.invocationId]);
  const decide = useToolApprovalStore((state) => state.decide);
  const clearError = useToolApprovalStore((state) => state.clearApprovalError);
  const risk = riskPresentation(approval.risk);
  const isResolving = actionId != null;
  const resolve = (decision: ToolApprovalDecision) => {
    void decide(port, approval, decision);
  };

  return (
    <>
      <div className="approval-overlay__content">
        <div className="approval-overlay__meta">
          <span className={`approval-risk approval-risk--${approval.risk}`}>{risk.symbol} {risk.label}</span>
          <span>{approval.source}</span>
          <time>{formatApprovalTime(approval.createdAtUnixMs)}</time>
        </div>
        {approval.purpose && (
          <section className="approval-purpose">
            <small>运行目的</small>
            <strong>{approval.purpose}</strong>
          </section>
        )}
        <details className="approval-command" open>
          <summary>命令详情</summary>
          <pre>{approval.command}</pre>
        </details>
        <p className="approval-cwd"><span aria-hidden="true">▣</span>{approval.cwd}</p>
        {approval.reason && <p className="approval-reason">{approval.reason}</p>}
        {error && (
          <div className="approval-error" role="alert">
            <span>{error}</span>
            <button aria-label="关闭审批错误" onClick={() => clearError(approval.invocationId)} type="button">×</button>
          </div>
        )}
      </div>
      <div className="approval-overlay__actions">
        {isResolving && <span className="button-spinner" aria-label="正在处理审批" />}
        {allowsToolApprovalDecision(approval, "decline") && (
          <button
            className="approval-decline"
            disabled={!hostReady || isResolving}
            onClick={() => resolve("decline")}
            type="button"
          >拒绝</button>
        )}
        {allowsToolApprovalDecision(approval, "accept") && (
          <button
            className="approval-accept-once"
            disabled={!hostReady || isResolving}
            onClick={() => resolve("accept")}
            type="button"
          >仅本次允许</button>
        )}
        {allowsToolApprovalDecision(approval, "acceptForSession") && (
          <button
            className="primary-button"
            disabled={!hostReady || isResolving}
            onClick={() => resolve("acceptForSession")}
            type="button"
          >本次会话允许</button>
        )}
      </div>
    </>
  );
}

function formatApprovalTime(unixMilliseconds: number) {
  const date = new Date(unixMilliseconds);
  return Number.isNaN(date.getTime())
    ? ""
    : new Intl.DateTimeFormat("zh-CN", { hour: "2-digit", minute: "2-digit" }).format(date);
}
