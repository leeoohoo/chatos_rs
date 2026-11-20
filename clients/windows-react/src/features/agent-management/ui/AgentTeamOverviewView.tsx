import { useEffect, useMemo, useState } from "react";
import { useAgentTeamRunStore } from "../application/agentTeamRunStore";
import { useAgentTeamTodoStore } from "../application/agentTeamTodoStore";
import {
  isLegacyAgentProjectManagerBrief,
  type AgentProfileSummary,
  type AgentProjectDashboardSnapshot,
  type AgentTeamDetail,
} from "../domain/agentManagement";
import type { AgentManagementPort } from "../ports/AgentManagementPort";
import { todoStatusLabel } from "./AgentTeamTodosView";

export function AgentTeamOverviewView({
  detail,
  ownerUserId,
  port,
  profiles,
  onOpen,
}: {
  detail: AgentTeamDetail;
  ownerUserId: string;
  port: AgentManagementPort;
  profiles: AgentProfileSummary[];
  onOpen(section: "conversation" | "todos" | "research" | "assets" | "runs"): void;
}) {
  const [dashboardSnapshot, setDashboardSnapshot] = useState<AgentProjectDashboardSnapshot | null>(null);
  const [dashboardError, setDashboardError] = useState<string | null>(null);
  const [isDashboardLoading, setIsDashboardLoading] = useState(false);
  const [isLegacyBriefExpanded, setIsLegacyBriefExpanded] = useState(false);
  const snapshot = useAgentTeamRunStore((state) => state.snapshot);
  const runError = useAgentTeamRunStore((state) => state.error);
  const activateRuns = useAgentTeamRunStore((state) => state.activate);
  const refreshRuns = useAgentTeamRunStore((state) => state.refresh);
  const resetRuns = useAgentTeamRunStore((state) => state.reset);
  const todoTeamId = useAgentTeamTodoStore((state) => state.teamId);
  const todos = useAgentTeamTodoStore((state) => state.todos);
  const todoError = useAgentTeamTodoStore((state) => state.error);
  const activateTodos = useAgentTeamTodoStore((state) => state.activate);
  const refreshTodos = useAgentTeamTodoStore((state) => state.refresh);
  const resetTodos = useAgentTeamTodoStore((state) => state.reset);
  const visibleSnapshot = snapshot?.teamId === detail.team.id ? snapshot : null;
  const activeCount = (visibleSnapshot?.pendingDeliveryCount ?? 0)
    + (visibleSnapshot?.runningDeliveryCount ?? 0);
  const visibleTodos = todoTeamId === detail.team.id ? todos : [];
  const inProgress = visibleTodos.filter((todo) => todo.status === "in_progress");
  const pending = visibleTodos.filter((todo) => todo.status === "pending");
  const blocked = visibleTodos.filter((todo) => todo.status === "blocked");
  const completed = visibleTodos.filter((todo) => todo.status === "completed");
  const activeTodos = visibleTodos.filter((todo) => !["completed", "cancelled"].includes(todo.status));
  const dashboard = dashboardSnapshot?.dashboard?.teamId === detail.team.id
    ? dashboardSnapshot.dashboard
    : null;
  const isLegacyBrief = dashboard ? isLegacyAgentProjectManagerBrief(dashboard.summary) : false;
  const legacyBriefRegionId = `agent-team-manager-brief-${detail.team.id}`;
  const failedRuns = visibleSnapshot?.runs.filter((run) => (
    run.deliveryStatus === "failed" || run.runStatus === "failed"
  )) ?? [];
  const humanIssues = dashboard?.issues.filter((issue) => issue.owner === "human") ?? [];
  const attentionCount = failedRuns.length
    + (dashboardSnapshot?.pendingApprovalCount ?? 0)
    + humanIssues.length;
  const currentMilestone = dashboard?.milestones.find((milestone) => milestone.status === "in_progress")
    ?? dashboard?.milestones.find((milestone) => milestone.status === "pending")
    ?? null;
  const effectiveHealth = failedRuns.length > 0 || blocked.length > 0
    ? dashboard?.health === "blocked" ? "blocked" : "at_risk"
    : dashboard?.health === "completed" && activeTodos.length > 0 ? "at_risk"
      : dashboard?.health ?? "on_track";
  const profileNames = useMemo(
    () => new Map(profiles.map((profile) => [profile.id, profile.name])),
    [profiles],
  );

  useEffect(() => {
    if (port.loadTeamRuns) void activateRuns(port, ownerUserId, detail.team.id);
    else resetRuns();
    if (port.listTeamTodos) void activateTodos(port, ownerUserId, detail.team.id);
    else resetTodos();
    let cancelled = false;
    if (port.loadTeamDashboard) {
      setIsDashboardLoading(true);
      setDashboardError(null);
      void port.loadTeamDashboard(ownerUserId, detail.team.id).then((value) => {
        if (!cancelled) setDashboardSnapshot(value);
      }).catch((error: unknown) => {
        if (!cancelled) setDashboardError(error instanceof Error ? error.message : "项目总览读取失败。");
      }).finally(() => {
        if (!cancelled) setIsDashboardLoading(false);
      });
    } else {
      setDashboardSnapshot(null);
      setDashboardError(null);
    }
    return () => { cancelled = true; resetRuns(); resetTodos(); };
  }, [activateRuns, activateTodos, detail.team.id, ownerUserId, port, resetRuns, resetTodos]);

  useEffect(() => {
    if (!port.loadTeamRuns || activeCount === 0) return undefined;
    const timer = window.setInterval(() => {
      void refreshRuns(port);
      void refreshTodos(port);
      if (port.loadTeamDashboard) {
        void port.loadTeamDashboard(ownerUserId, detail.team.id).then(setDashboardSnapshot).catch(() => undefined);
      }
    }, 1_000);
    return () => window.clearInterval(timer);
  }, [activeCount, detail.team.id, ownerUserId, port, refreshRuns, refreshTodos]);

  useEffect(() => {
    setIsLegacyBriefExpanded(false);
  }, [dashboard?.revision, detail.team.id]);

  const retryDashboard = () => {
    if (!port.loadTeamDashboard) return;
    setIsDashboardLoading(true);
    setDashboardError(null);
    void port.loadTeamDashboard(ownerUserId, detail.team.id)
      .then(setDashboardSnapshot)
      .catch((error: unknown) => setDashboardError(error instanceof Error ? error.message : "项目总览读取失败。"))
      .finally(() => setIsDashboardLoading(false));
  };

  return (
    <div className="agent-team-overview">
      <section className="agent-team-dashboard-summary">
        <div><small>当前阶段</small><h4>{isDashboardLoading && !dashboardSnapshot ? "正在读取项目总览" : dashboard?.phase ?? "等待项目经理建立总览"} <em className={`is-${effectiveHealth}`}>{dashboardHealthLabel(effectiveHealth)}</em></h4><p>{detail.team.goal || "项目目标尚未填写"}</p></div>
        <div className="agent-team-dashboard-milestone"><strong><span>{currentMilestone?.title ?? "尚未建立里程碑"}</span><b>{currentMilestone?.progressPercent ?? 0}%</b></strong><i><span style={{ width: `${currentMilestone?.progressPercent ?? 0}%` }} /></i><small>{dashboard ? `项目经理更新于 ${formatDashboardDate(dashboard.updatedAtUnixMs)}` : "等待项目经理首次维护"}</small></div>
        <div className="agent-team-dashboard-metrics">
          <button onClick={() => onOpen("todos")} type="button"><strong>{inProgress.length}</strong><span>进行中</span></button>
          <button onClick={() => onOpen("todos")} type="button"><strong>{pending.length}</strong><span>等待</span></button>
          <button onClick={() => onOpen("todos")} type="button"><strong>{blocked.length}</strong><span>经理协调</span></button>
          <button onClick={() => onOpen("runs")} type="button"><strong>{attentionCount}</strong><span>需你处理</span></button>
        </div>
      </section>
      {(runError || todoError || dashboardError) && (
        <div className="agent-team-conversation__error" role="alert">
          <span>{runError ?? todoError ?? dashboardError}</span><button onClick={() => { void refreshRuns(port); void refreshTodos(port); retryDashboard(); }} type="button">重试</button>
        </div>
      )}
      <div className="agent-team-overview__body">
        <section>
          <header><div><h4>需要你处理</h4><p>仅显示必须由你决定或输入的事项</p></div><b>{attentionCount}</b></header>
          {attentionCount === 0 && <div className="agent-team-dashboard-clear">● <span>当前没有需要你处理的事项</span></div>}
          {failedRuns.slice(0, 3).map((run) => <button className="agent-team-dashboard-attention" key={run.runId} onClick={() => onOpen("runs")} type="button"><span>!</span><div><strong>检查失败的 Agent Run</strong><small>{profileNames.get(run.agentId) ?? "Agent"} · {run.lastError || "运行需要检查"}</small></div><b>›</b></button>)}
          {(dashboardSnapshot?.pendingApprovalCount ?? 0) > 0 && <button className="agent-team-dashboard-attention" onClick={() => onOpen("conversation")} type="button"><span>♙</span><div><strong>审批 Agent 提案</strong><small>{dashboardSnapshot?.pendingApprovalCount} 项成员变更等待确认</small></div><b>›</b></button>}
          {humanIssues.slice(0, 3).map((issue) => <button className={`agent-team-dashboard-attention is-${issue.severity}`} key={issue.id} onClick={() => onOpen(issue.relatedTodoId ? "todos" : "conversation")} type="button"><span>!</span><div><strong>{issue.title}</strong><small>{issue.requestedAction}</small></div><b>›</b></button>)}
        </section>
        <section>
          <header><div><h4>当前执行</h4><p>任务状态由系统实时汇总</p></div></header>
          <div className="agent-team-dashboard-status" aria-label="任务状态汇总"><i style={{ flex: Math.max(1, completed.length) }} /><i style={{ flex: inProgress.length }} /><i style={{ flex: blocked.length }} /><i style={{ flex: pending.length }} /></div>
          {activeTodos.slice(0, 4).map((todo) => <article key={todo.id}><span className={`is-${todo.status}`} /><div><strong>{todo.title}</strong><small>{profileNames.get(todo.agentId) ?? "Agent"}</small></div><em>{todoStatusLabel(todo.status)}</em></article>)}
          <button onClick={() => onOpen("todos")} type="button">查看完整任务板</button>
        </section>
        <section>
          <header><div><h4>里程碑</h4><p>由项目经理维护，任务事实由系统校验</p></div></header>
          {!dashboard?.milestones.length && <div className="agent-team-overview__empty">项目经理尚未建立里程碑。</div>}
          {dashboard?.milestones.slice(0, 5).map((milestone) => <article key={milestone.id}><span className={`is-${milestone.status}`} /><div><strong>{milestone.title}</strong><small>{milestone.detail || dashboardMilestoneStatusLabel(milestone.status)}</small></div><em>{milestone.progressPercent}%</em></article>)}
        </section>
        <section>
          <header><div><h4>项目经理简报</h4><p>{dashboard ? `第 ${dashboard.revision} 版` : "尚未维护"}</p></div></header>
          {isLegacyBrief && dashboard ? (
            <>
              <div className="agent-team-dashboard-legacy-note">
                <span aria-hidden="true">▤</span>
                <p>历史简报包含较多技术记录，已默认收起。最新状态请看当前阶段、里程碑和执行情况。</p>
              </div>
              <button
                aria-controls={legacyBriefRegionId}
                aria-expanded={isLegacyBriefExpanded}
                className="agent-team-dashboard-legacy-toggle"
                onClick={() => setIsLegacyBriefExpanded((expanded) => !expanded)}
                type="button"
              >
                {isLegacyBriefExpanded ? "收起历史完整记录" : "查看历史完整记录"}
              </button>
              {isLegacyBriefExpanded && (
                <div className="agent-team-dashboard-legacy-content" id={legacyBriefRegionId}>
                  <p>{dashboard.summary}</p>
                  {!!dashboard.nextSteps.length && <div className="agent-team-dashboard-next"><strong>下一步</strong>{dashboard.nextSteps.slice(0, 5).map((step, index) => <span key={`${index}-${step}`}>{index + 1}. {step}</span>)}</div>}
                </div>
              )}
            </>
          ) : (
            <>
              <p className="agent-team-dashboard-brief">{dashboard?.summary ?? "项目经理会在收到任务状态、调研和审批变化后维护阶段判断、风险与下一步。"}</p>
              {!!dashboard?.nextSteps.length && <div className="agent-team-dashboard-next"><strong>下一步</strong>{dashboard.nextSteps.slice(0, 5).map((step, index) => <span key={`${index}-${step}`}>{index + 1}. {step}</span>)}</div>}
            </>
          )}
          <button onClick={() => onOpen("runs")} type="button">查看运行依据</button>
        </section>
      </div>
    </div>
  );
}

function dashboardHealthLabel(health: "on_track" | "at_risk" | "blocked" | "completed") {
  return ({ on_track: "正常", at_risk: "有风险", blocked: "阻塞", completed: "已完成" })[health];
}

function dashboardMilestoneStatusLabel(status: "pending" | "in_progress" | "blocked" | "completed") {
  return ({ pending: "等待", in_progress: "进行中", blocked: "阻塞", completed: "已完成" })[status];
}

function formatDashboardDate(unixMilliseconds: number) {
  return new Intl.DateTimeFormat("zh-CN", { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" }).format(new Date(unixMilliseconds));
}
