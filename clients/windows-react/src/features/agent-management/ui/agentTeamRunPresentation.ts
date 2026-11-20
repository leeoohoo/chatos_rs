import type { AgentTeamRunSummary } from "../domain/agentManagement";

export function teamRunStatusLabel(status: "pending" | "running" | "completed" | "failed" | "cancelled") {
  return {
    pending: "等待执行",
    running: "正在运行",
    completed: "已完成",
    failed: "失败",
    cancelled: "已取消",
  }[status];
}

export function teamRunCheckpointStatusLabel(status: AgentTeamRunSummary["runStatus"]) {
  return {
    ready: "等待开始",
    running: "运行中",
    succeeded: "已完成",
    failed: "失败",
    cancelled: "已取消",
  }[status];
}

export function teamRunDetailStatusLabel(
  detailStatus: string,
  summaryStatus: AgentTeamRunSummary["runStatus"],
) {
  switch (detailStatus.trim().toLowerCase()) {
    case "reviewing_followup":
      return "审查追加要求";
    case "waiting_confirmation":
      return "等待确认";
    default:
      return teamRunCheckpointStatusLabel(summaryStatus);
  }
}

export function shouldPollTeamRunDetail(status: string) {
  return new Set([
    "ready", "running", "queued", "model_ready", "model_running",
    "waiting_tool_result", "continuation_ready", "retry_scheduled",
    "reviewing_followup", "waiting_confirmation",
  ]).has(status.trim().toLowerCase());
}

export function isCancellableTeamRunDetailStatus(status: string) {
  return shouldPollTeamRunDetail(status) || status.trim().toLowerCase() === "needs_review";
}

export function teamRunStatusClass(status: AgentTeamRunSummary["runStatus"]) {
  return status === "succeeded" ? "completed" : status;
}

export function teamRunMemoryThreadId(teamId: string, run: AgentTeamRunSummary) {
  return run.lane === "executor"
    ? `agent-team-executor-${run.deliveryId}`
    : `agent-team-v2-${teamId}-${run.agentId}`;
}

export function formatRunElapsed(createdAtUnixMs: number, updatedAtUnixMs: number) {
  return `${(Math.max(0, updatedAtUnixMs - createdAtUnixMs) / 1_000).toFixed(1)} 秒`;
}

export function teamRunTriggerLabel(trigger: "mention" | "default_agent" | "agent_mention" | "heartbeat" | "todo" | "todo_status") {
  return {
    mention: "Human 点名",
    default_agent: "默认 Agent",
    agent_mention: "Agent 点名",
    heartbeat: "主动巡检",
    todo: "团队任务",
    todo_status: "任务状态",
  }[trigger];
}
