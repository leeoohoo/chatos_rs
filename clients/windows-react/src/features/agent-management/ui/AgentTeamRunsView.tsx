import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { RunInspectorPort } from "../../run-inspector/ports/RunInspectorPort";
import { useAgentTeamRunStore } from "../application/agentTeamRunStore";
import type {
  AgentProfileSummary,
  AgentTeamDetail,
  AgentTeamRunSummary,
} from "../domain/agentManagement";
import type { AgentManagementPort } from "../ports/AgentManagementPort";
import { formatTeamMessageTime, Pagination } from "./AgentManagementUiPrimitives";
import { AgentTeamRunDetailSheet } from "./AgentTeamRunDetailSheet";
import {
  teamRunCheckpointStatusLabel,
  teamRunMemoryThreadId,
  teamRunStatusClass,
  teamRunTriggerLabel,
} from "./agentTeamRunPresentation";

export function AgentTeamRunsView({
  detail,
  ownerUserId,
  port,
  profiles,
  runInspectorPort,
  selectedAgentId,
  onSelectAgent,
}: {
  detail: AgentTeamDetail;
  ownerUserId: string;
  port: AgentManagementPort;
  profiles: AgentProfileSummary[];
  runInspectorPort?: RunInspectorPort;
  selectedAgentId: string | null;
  onSelectAgent(agentId: string | null): void;
}) {
  const snapshot = useAgentTeamRunStore((state) => state.snapshot);
  const isLoading = useAgentTeamRunStore((state) => state.isLoading);
  const error = useAgentTeamRunStore((state) => state.error);
  const activate = useAgentTeamRunStore((state) => state.activate);
  const refresh = useAgentTeamRunStore((state) => state.refresh);
  const reset = useAgentTeamRunStore((state) => state.reset);
  const [selectedRunId, setSelectedRunId] = useState<string | null>(null);
  const selectedRunTriggerRef = useRef<HTMLButtonElement | null>(null);
  const [page, setPage] = useState(0);
  const [pageSize, setPageSize] = useState(20);
  const [runMetrics, setRunMetrics] = useState<Record<string, { eventCount: number; hasMoreEvents: boolean; modelCalls: number }>>({});
  const visibleSnapshot = snapshot?.teamId === detail.team.id ? snapshot : null;
  const hasActiveRun = visibleSnapshot?.runs.some(
    (run) => run.deliveryStatus === "pending" || run.deliveryStatus === "running",
  ) ?? false;
  const visibleRuns = selectedAgentId
    ? visibleSnapshot?.runs.filter((run) => run.agentId === selectedAgentId) ?? []
    : visibleSnapshot?.runs ?? [];
  const pageCount = Math.max(1, Math.ceil(visibleRuns.length / pageSize));
  const pagedRuns = visibleRuns.slice(page * pageSize, (page + 1) * pageSize);
  const profileNames = useMemo(
    () => new Map(profiles.map((profile) => [profile.id, profile.name])),
    [profiles],
  );
  const selectedAgentName = selectedAgentId ? profileNames.get(selectedAgentId) ?? "Agent" : null;
  const selectedRun = visibleSnapshot?.runs.find((run) => run.runId === selectedRunId) ?? null;
  const openRunDetail = useCallback((runId: string, trigger: HTMLButtonElement) => {
    selectedRunTriggerRef.current = trigger;
    setSelectedRunId(runId);
  }, []);
  const closeRunDetail = useCallback(() => {
    const trigger = selectedRunTriggerRef.current;
    selectedRunTriggerRef.current = null;
    setSelectedRunId(null);
    window.requestAnimationFrame(() => {
      if (trigger?.isConnected && !trigger.disabled) trigger.focus({ preventScroll: true });
    });
  }, []);

  useEffect(() => {
    if (port.loadTeamRuns) void activate(port, ownerUserId, detail.team.id);
    else reset();
    return reset;
  }, [activate, detail.team.id, ownerUserId, port, reset]);

  useEffect(() => {
    if (!port.loadTeamRuns || !hasActiveRun) return undefined;
    const timer = window.setInterval(() => void refresh(port), 1_000);
    return () => window.clearInterval(timer);
  }, [hasActiveRun, port, refresh]);

  useEffect(() => {
    setPage(0);
  }, [selectedAgentId]);

  useEffect(() => {
    if (page >= pageCount) setPage(pageCount - 1);
  }, [page, pageCount]);

  useEffect(() => {
    if (!runInspectorPort) return undefined;
    const pendingRuns = pagedRuns.filter((run) => runMetrics[run.runId] == null);
    if (pendingRuns.length === 0) return undefined;
    let active = true;
    void Promise.allSettled(pendingRuns.map(async (run) => {
      const loaded = await runInspectorPort.loadRun(ownerUserId, run.runId, 0);
      return [run.runId, {
        eventCount: loaded.events.length,
        hasMoreEvents: loaded.eventsHasMore,
        modelCalls: loaded.run.iteration,
      }] as const;
    })).then((results) => {
      if (!active) return;
      const loadedMetrics = results.flatMap((result) => result.status === "fulfilled" ? [result.value] : []);
      if (loadedMetrics.length > 0) {
        setRunMetrics((current) => ({ ...current, ...Object.fromEntries(loadedMetrics) }));
      }
    });
    return () => { active = false; };
  }, [ownerUserId, pagedRuns, runInspectorPort, runMetrics]);

  if (!port.loadTeamRuns) {
    return (
      <div className="agent-team-runs__state is-error" role="alert">
        <strong>当前版本尚未连接团队运行记录</strong>
        <span>请更新本机客户端后重试。</span>
      </div>
    );
  }

  return (
    <div className="agent-team-runs">
      <header className="agent-team-runs__header">
        <div>
          <strong>{selectedAgentName ? `${selectedAgentName} 的运行` : "团队全部运行"}</strong>
          <span>{selectedAgentId == null ? "按更新时间查看团队内所有 Run" : "通讯与任务执行彼此独立，可同时运行"}</span>
        </div>
        <div>
          {selectedAgentId != null && <button onClick={() => onSelectAgent(null)} type="button">查看全部</button>}
          <button aria-label="刷新团队运行记录" disabled={isLoading} onClick={() => void refresh(port)} type="button">↻</button>
        </div>
      </header>
      {selectedAgentId != null && (
        <div className="agent-team-runs__lanes">
          <AgentTeamRunLaneSummary lane="manager" run={visibleRuns.find((run) => run.lane === "manager") ?? null} onInspect={openRunDetail} />
          <AgentTeamRunLaneSummary lane="executor" run={visibleRuns.find((run) => run.lane === "executor") ?? null} onInspect={openRunDetail} />
        </div>
      )}
      {error && (
        <div className="agent-team-conversation__error" role="alert">
          <span>{error}</span>
          <button onClick={() => void refresh(port)} type="button">重试</button>
        </div>
      )}
      <div className="agent-team-runs__list">
        {isLoading && !visibleSnapshot ? (
          <div className="agent-team-runs__state"><span className="mini-spinner" />正在读取运行历史…</div>
        ) : visibleRuns.length === 0 ? (
          <div className="agent-team-runs__state">
            <span aria-hidden="true" className="agent-team-runs__empty-icon">⌁</span>
            <strong>{selectedAgentId ? "这个 Agent 还没有运行记录" : "还没有运行记录"}</strong>
            <span>Agent 被唤醒后，通讯与任务执行 Run 会显示在这里。</span>
          </div>
        ) : pagedRuns.map((run) => {
          const metrics = runMetrics[run.runId];
          const agentName = profileNames.get(run.agentId) ?? run.agentId;
          const laneLabel = run.lane === "manager" ? "通讯" : "任务执行";
          const statusLabel = teamRunCheckpointStatusLabel(run.runStatus);
          const triggerLabel = teamRunTriggerLabel(run.triggerKind);
          return (
            <article key={run.runId}>
              <button
                aria-label={`查看${agentName}的${laneLabel}运行详情，${statusLabel}，${triggerLabel}，Run ${run.runId}`}
                disabled={!runInspectorPort}
                onClick={(event) => openRunDetail(run.runId, event.currentTarget)}
                type="button"
              >
                <header>
                  <strong>{agentName}</strong>
                  <span className={`agent-team-runs__lane is-${run.lane}`}>{laneLabel}</span>
                  <em className={`is-${teamRunStatusClass(run.runStatus)}`}>{statusLabel}</em>
                  <b aria-hidden="true">›</b>
                </header>
                <p>{triggerLabel} · {metrics
                  ? `${metrics.eventCount}${metrics.hasMoreEvents ? "+" : ""} 条事件 · ${metrics.modelCalls} 次模型调用`
                  : `Run ${run.runId}`}</p>
                <footer>
                  <time>{formatTeamMessageTime(run.updatedAtUnixMs)}</time>
                  <span>Memory {teamRunMemoryThreadId(detail.team.id, run)}</span>
                </footer>
                {run.lastError && <small title={run.lastError}>{run.lastError}</small>}
              </button>
            </article>
          );
        })}
        {visibleRuns.length > 0 && (
          <Pagination
            onPageChange={setPage}
            onPageSizeChange={(size) => { setPageSize(size); setPage(0); }}
            page={page}
            pageCount={pageCount}
            pageSize={pageSize}
            total={visibleRuns.length}
          />
        )}
      </div>
      {selectedRunId && selectedRun && runInspectorPort && (
        <AgentTeamRunDetailSheet
          agentPort={port}
          agentName={profileNames.get(selectedRun.agentId) ?? "Agent"}
          ownerUserId={ownerUserId}
          port={runInspectorPort}
          run={selectedRun}
          teamId={detail.team.id}
          onClose={closeRunDetail}
          onRetried={() => refresh(port)}
        />
      )}
    </div>
  );
}
function AgentTeamRunLaneSummary({
  lane,
  run,
  onInspect,
}: {
  lane: "manager" | "executor";
  run: AgentTeamRunSummary | null;
  onInspect(runId: string, trigger: HTMLButtonElement): void;
}) {
  return (
    <button disabled={!run} onClick={(event) => { if (run) onInspect(run.runId, event.currentTarget); }} type="button">
      <header>
        <span className={`agent-team-runs__lane is-${lane}`}>{lane === "manager" ? "通讯" : "任务执行"}</span>
        {run ? <em className={`is-${teamRunStatusClass(run.runStatus)}`}>{teamRunCheckpointStatusLabel(run.runStatus)}</em> : <em>暂无记录</em>}
      </header>
      {run ? <><strong>{teamRunTriggerLabel(run.triggerKind)}</strong><span>更新于 {formatTeamMessageTime(run.updatedAtUnixMs)}</span></> : (
        <span>{lane === "manager" ? "等待消息、心跳或任务状态唤醒" : "等待已启动的团队任务"}</span>
      )}
    </button>
  );
}
