import { useEffect, useLayoutEffect, useRef, useState } from "react";
import type { RunInspectorDetail } from "../../run-inspector/domain/runInspector";
import type { RunInspectorPort } from "../../run-inspector/ports/RunInspectorPort";
import type { AgentTeamRunSummary } from "../domain/agentManagement";
import type { AgentManagementPort } from "../ports/AgentManagementPort";
import { SafeMarkdownPreview } from "../../notepad/ui/SafeMarkdownPreview";
import { getFocusableModalElements } from "../../../shared/modalAccessibility";
import { formatTeamMessageTime, Row } from "./AgentManagementUiPrimitives";
import {
  formatRunElapsed,
  isCancellableTeamRunDetailStatus,
  shouldPollTeamRunDetail,
  teamRunDetailStatusLabel,
  teamRunStatusLabel,
  teamRunTriggerLabel,
} from "./agentTeamRunPresentation";

export function AgentTeamRunDetailSheet({
  agentPort,
  agentName,
  ownerUserId,
  port,
  run,
  teamId,
  onClose,
  onRetried,
}: {
  agentPort: AgentManagementPort;
  agentName: string;
  ownerUserId: string;
  port: RunInspectorPort;
  run: AgentTeamRunSummary;
  teamId: string;
  onClose(): void;
  onRetried(): Promise<void>;
}) {
  const runId = run.runId;
  const [detail, setDetail] = useState<RunInspectorDetail | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [isLoadingOlder, setIsLoadingOlder] = useState(false);
  const [isRetrying, setIsRetrying] = useState(false);
  const [isCancelling, setIsCancelling] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const dialogRef = useRef<HTMLElement | null>(null);
  const closeButtonRef = useRef<HTMLButtonElement | null>(null);
  const canRetry = detail?.run.status.toLowerCase() === "needs_review" && Boolean(agentPort.retryTeamRun);
  const canCancel = Boolean(detail && agentPort.cancelTeamRun
    && isCancellableTeamRunDetailStatus(detail.run.status));

  useEffect(() => {
    let active = true;
    setDetail(null);
    setIsLoading(true);
    setError(null);
    void port.loadRun(ownerUserId, runId, 0).then((snapshot) => {
      if (!active || snapshot.run.runId !== runId) return;
      setDetail(snapshot);
      setIsLoading(false);
    }).catch((failure: unknown) => {
      if (!active) return;
      setError(failure instanceof Error ? failure.message : "Run 详情暂时不可用。");
      setIsLoading(false);
    });
    return () => { active = false; };
  }, [ownerUserId, port, runId]);

  useEffect(() => {
    if (!detail || !shouldPollTeamRunDetail(detail.run.status)) return undefined;
    let active = true;
    const timer = window.setInterval(() => {
      void port.loadRun(ownerUserId, runId, 0).then((snapshot) => {
        if (active && snapshot.run.runId === runId) setDetail(snapshot);
      }).catch(() => undefined);
    }, 1_500);
    return () => {
      active = false;
      window.clearInterval(timer);
    };
  }, [detail, ownerUserId, port, runId]);

  useLayoutEffect(() => {
    const dialog = dialogRef.current;
    if (!dialog) return undefined;
    closeButtonRef.current?.focus({ preventScroll: true });

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        event.stopPropagation();
        onClose();
        return;
      }
      if (event.key !== "Tab") return;
      const focusable = getFocusableModalElements(dialog);
      if (focusable.length === 0) {
        event.preventDefault();
        dialog.focus({ preventScroll: true });
        return;
      }
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      const activeElement = document.activeElement;
      if (!dialog.contains(activeElement)) {
        event.preventDefault();
        (event.shiftKey ? last : first).focus({ preventScroll: true });
      } else if (event.shiftKey && activeElement === first) {
        event.preventDefault();
        last.focus({ preventScroll: true });
      } else if (!event.shiftKey && activeElement === last) {
        event.preventDefault();
        first.focus({ preventScroll: true });
      }
    };

    document.addEventListener("keydown", handleKeyDown, true);
    return () => document.removeEventListener("keydown", handleKeyDown, true);
  }, [onClose]);

  return (
    <div className="agent-editor-backdrop" role="presentation" onMouseDown={onClose}>
      <section
        aria-label="团队 Run 检查器"
        aria-modal="true"
        className="agent-team-run-detail"
        onMouseDown={(event) => event.stopPropagation()}
        ref={dialogRef}
        role="dialog"
        tabIndex={-1}
      >
        <header>
          <div><h2>Run 检查器</h2><p>{agentName} · {run.lane === "manager" ? "通讯" : "任务执行"}</p></div>
          <button onClick={onClose} ref={closeButtonRef} type="button">完成</button>
        </header>
        <div className="agent-team-run-detail__body">
          {isLoading ? (
            <div className="agent-team-runs__state"><span className="mini-spinner" />正在读取 Run…</div>
          ) : error && !detail ? (
            <div className="agent-team-runs__state is-error"><strong>无法读取 Run</strong><span>{error}</span></div>
          ) : detail ? (
            <>
              {error && <div className="agent-team-run-detail__error" role="alert">{error}</div>}
              <section className="agent-team-run-detail__section">
                <h3>运行信息</h3>
                <dl>
                  <Row label="状态" value={teamRunDetailStatusLabel(detail.run.status, run.runStatus)} />
                  <Row label="触发" value={teamRunTriggerLabel(run.triggerKind)} />
                  <Row label="Delivery" value={teamRunStatusLabel(run.deliveryStatus)} />
                  <Row label="Memory" value={run.lane === "manager" ? "Agent 长期通讯" : "Todo 独立执行"} />
                  <Row label="模型调用" value={`${detail.run.iteration} / ${detail.run.maxIterations}`} />
                  <Row label="运行耗时" value={formatRunElapsed(detail.run.createdAtUnixMs, detail.run.updatedAtUnixMs)} />
                  <Row label="开始时间" value={formatTeamMessageTime(detail.run.createdAtUnixMs)} />
                  <Row label="更新时间" value={formatTeamMessageTime(detail.run.updatedAtUnixMs)} />
                </dl>
                {run.lastError && <div className="agent-team-run-detail__error" role="alert">{run.lastError}</div>}
                {detail.run.resultSummary && <div className="agent-team-run-detail__summary"><strong>结果</strong><SafeMarkdownPreview markdown={detail.run.resultSummary} /></div>}
              </section>
              {detail.run.errorMessage && <div className="agent-team-run-detail__error" role="alert">{detail.run.errorMessage}</div>}
              <section className="agent-team-run-detail__section">
                <h3>工具调用</h3>
                {(detail.toolCalls ?? []).length === 0 ? (
                  <p className="agent-team-run-detail__empty">本次 Run 没有已记录的工具调用。</p>
                ) : (detail.toolCalls ?? []).map((tool) => (
                  <article className="agent-team-run-detail__tool" key={tool.callId}>
                    <div>
                      <strong>{tool.displayName}</strong>
                      <small>{tool.route ? `路由 · ${tool.route}` : tool.toolName}</small>
                    </div>
                    <span>{tool.status}{tool.resultSizeBytes != null ? ` · 结果 ${tool.resultSizeBytes} B` : ""}</span>
                  </article>
                ))}
              </section>
              <section className="agent-team-run-detail__section">
                <h3>运行事件</h3>
                {detail.events.length === 0 ? (
                  <p className="agent-team-run-detail__empty">本次 Run 还没有执行事件。</p>
                ) : detail.events.map((event) => (
                  <article className="agent-team-run-detail__event" key={event.eventId}>
                    <span aria-hidden="true" />
                    <div>
                      <header><strong>{event.eventType}</strong><time>{formatTeamMessageTime(event.createdAtUnixMs)}</time></header>
                      {event.summary && <p>{event.summary}</p>}
                      {event.status && <small>{event.status}</small>}
                    </div>
                  </article>
                ))}
                {detail.eventsHasMore && (
                  <button
                    disabled={isLoadingOlder}
                    onClick={() => {
                      setIsLoadingOlder(true);
                      void port.loadRun(ownerUserId, runId, detail.nextOffset).then((older) => {
                        if (older.run.runId !== runId) return;
                        setDetail((current) => current ? {
                          ...older,
                          events: [...current.events, ...older.events],
                        } : older);
                      }).catch((failure: unknown) => {
                        setError(failure instanceof Error ? failure.message : "较早事件读取失败。");
                      }).finally(() => setIsLoadingOlder(false));
                    }}
                    type="button"
                  >{isLoadingOlder ? "读取中…" : "加载较早事件"}</button>
                )}
              </section>
            </>
          ) : null}
        </div>
        {(canRetry || canCancel) && <footer>
          {canRetry && (
            <button
              className="is-danger"
              disabled={isRetrying || isCancelling}
              onClick={() => {
                setIsRetrying(true);
                setError(null);
                void agentPort.retryTeamRun!(ownerUserId, teamId, runId).then(async () => {
                  const snapshot = await port.loadRun(ownerUserId, runId, 0);
                  setDetail(snapshot);
                  await onRetried();
                }).catch((failure: unknown) => {
                  setError(failure instanceof Error ? failure.message : "中断步骤重试失败。");
                }).finally(() => setIsRetrying(false));
              }}
              type="button"
            >{isRetrying ? "正在重试…" : "重试中断步骤"}</button>
          )}
          {canCancel && (
            <button
              className="is-danger"
              disabled={isCancelling || isRetrying}
              onClick={() => {
                if (!window.confirm("结束这个未完成的 Agent Run？已完成的消息和审计记录会保留。")) return;
                setIsCancelling(true);
                setError(null);
                void agentPort.cancelTeamRun!(ownerUserId, teamId, runId).then(async () => {
                  const snapshot = await port.loadRun(ownerUserId, runId, 0);
                  setDetail(snapshot);
                  await onRetried();
                }).catch((failure: unknown) => {
                  setError(failure instanceof Error ? failure.message : "团队 Run 结束失败。请刷新后重试。");
                }).finally(() => setIsCancelling(false));
              }}
              type="button"
            >{isCancelling ? "正在结束…" : "结束运行"}</button>
          )}
        </footer>}
      </section>
    </div>
  );
}
