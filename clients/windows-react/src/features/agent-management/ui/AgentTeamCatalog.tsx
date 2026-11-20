import { useEffect, useMemo, useState } from "react";
import type { ConversationAttachmentPort } from "../../conversation-attachments/ports/ConversationAttachmentPort";
import type { RunInspectorPort } from "../../run-inspector/ports/RunInspectorPort";
import type {
  AgentProfileSummary,
  AgentTeamDetail,
  AgentTeamSummary,
} from "../domain/agentManagement";
import type { AgentManagementPort } from "../ports/AgentManagementPort";
import { AgentStaffingProposalPanel } from "./AgentActivityViews";
import { AgentTeamAssetsView } from "./AgentTeamAssetsView";
import { AgentTeamConversationView } from "./AgentTeamConversationView";
import { AgentTeamOverviewView } from "./AgentTeamOverviewView";
import { AgentTeamRunsView } from "./AgentTeamRunsView";
import { AgentTeamTodosView } from "./AgentTeamTodosView";
import { Row } from "./AgentManagementUiPrimitives";

export function AgentTeamCatalog({
  attachmentPort,
  detail,
  error,
  isDetailLoading,
  isInviting,
  isMutatingMember,
  isLoading,
  profiles,
  ownerUserId,
  port,
  runInspectorPort,
  projects,
  selectedTeamId,
  navigationTarget,
  onNavigationHandled,
  teams,
  onCreate,
  onInvite,
  onManageMember,
  onOpenRequirementSurveys,
  onRetry,
  onStaffingResolved,
  onSelect,
}: {
  attachmentPort?: ConversationAttachmentPort;
  detail: AgentTeamDetail | null;
  error: string | null;
  isDetailLoading: boolean;
  isInviting: boolean;
  isMutatingMember: boolean;
  isLoading: boolean;
  profiles: AgentProfileSummary[];
  ownerUserId: string;
  port: AgentManagementPort;
  runInspectorPort?: RunInspectorPort;
  projects: Array<{ id: string; title: string }>;
  selectedTeamId: string | null;
  navigationTarget: { roomId: string; todoId: string; requestId: number } | null;
  onNavigationHandled(requestId: number): void;
  teams: AgentTeamSummary[];
  onCreate(): void;
  onInvite(): void;
  onManageMember(agentId: string): void;
  onOpenRequirementSurveys(): void;
  onRetry(): void;
  onStaffingResolved(teamId: string): Promise<void>;
  onSelect(teamId: string): void;
}) {
  const [workspaceSection, setWorkspaceSection] = useState<"overview" | "members" | "conversation" | "todos" | "research" | "assets" | "runs">("overview");
  const [selectedRunAgentId, setSelectedRunAgentId] = useState<string | null>(null);
  useEffect(() => {
    setWorkspaceSection("overview");
    setSelectedRunAgentId(null);
  }, [selectedTeamId]);
  useEffect(() => {
    if (navigationTarget?.roomId === selectedTeamId) setWorkspaceSection("todos");
  }, [navigationTarget, selectedTeamId]);
  const projectNames = useMemo(
    () => new Map(projects.map((project) => [project.id, project.title])),
    [projects],
  );
  return (
    <div className={selectedTeamId ? "agent-team-catalog agent-team-catalog--selected" : "agent-team-catalog"}>
      {error && (
        <div className="agent-management__error" role="alert">
          <span>{error}</span>
          <button onClick={onRetry} type="button">重试</button>
        </div>
      )}
      {isLoading && teams.length === 0 ? (
        <div className="agent-management__state"><span className="mini-spinner" />正在读取 Agent 团队…</div>
      ) : teams.length === 0 ? (
        <div className="agent-management__empty">
          <span aria-hidden="true">◉</span>
          <h2>还没有项目团队</h2>
          <p>{projects.length > 0
            ? "选择一个已有项目，为它建立独立的 Agent 团队空间。"
            : "请先创建项目，再为项目建立 Agent 团队。"}</p>
          <button className="primary-button" disabled={projects.length === 0} onClick={onCreate} type="button">
            创建 Agent 团队
          </button>
        </div>
      ) : (
        <>
          {!selectedTeamId && <div className="agent-team-grid" aria-label="项目 Agent 团队">
            {teams.map((team) => (
            <article
              className={selectedTeamId === team.id ? "agent-team-card is-selected" : "agent-team-card"}
              key={team.id}
              onClick={() => onSelect(team.id)}
            >
              <header>
                <span aria-hidden="true">◉</span>
                <div>
                  <h3>{team.name}</h3>
                  <p>{projectNames.get(team.projectId) ?? team.projectId}</p>
                </div>
              </header>
              <p>{team.goal || "尚未设置团队目标"}</p>
              <dl>
                <Row label="成员" value={`${team.memberCount} 位 Agent`} />
                <Row label="项目经理" value={team.projectManagerAgentId ? "已指定" : "未指定"} />
                <Row label="默认 Agent" value={team.defaultAgentId ? "已指定" : "未指定"} />
              </dl>
              <button
                aria-pressed={selectedTeamId === team.id}
                onClick={(event) => { event.stopPropagation(); onSelect(team.id); }}
                type="button"
              >查看成员</button>
            </article>
            ))}
          </div>}
          {selectedTeamId && (
            <section
              aria-label={{
                overview: "Agent 团队总览", members: "Agent 团队成员",
                conversation: "Agent 团队聊天", todos: "Agent 团队任务", research: "Agent 团队需求调研",
                assets: "Agent 团队共享资产", runs: "Agent 团队运行",
              }[workspaceSection]}
              className={workspaceSection === "members"
                ? "agent-team-members"
                : "agent-team-members agent-team-members--conversation"}
            >
              <header>
                <div>
                  <h3>{detail?.team.id === selectedTeamId ? detail.team.name : "正在读取团队空间"}</h3>
                  <p>{detail?.team.id === selectedTeamId
                    ? detail.team.goal || "尚未设置团队目标"
                    : "正在读取项目团队的真实状态。"}</p>
                </div>
                <div>
                  {isDetailLoading && <span aria-label="正在读取团队成员" className="mini-spinner" />}
                  <div className="agent-team-members__segmented" aria-label="团队工作区">
                    <button aria-pressed={workspaceSection === "overview"} onClick={() => setWorkspaceSection("overview")} type="button">总览</button>
                    <button aria-pressed={workspaceSection === "conversation"} onClick={() => setWorkspaceSection("conversation")} type="button">聊天</button>
                    <button aria-pressed={workspaceSection === "todos"} onClick={() => setWorkspaceSection("todos")} type="button">任务</button>
                    <button aria-pressed={workspaceSection === "research"} onClick={() => setWorkspaceSection("research")} type="button">需求调研</button>
                    <button aria-pressed={workspaceSection === "assets"} onClick={() => setWorkspaceSection("assets")} type="button">共享资产</button>
                    <button aria-pressed={workspaceSection === "runs"} onClick={() => setWorkspaceSection("runs")} type="button">运行</button>
                  </div>
                </div>
              </header>
              <div className="agent-team-workspace__body">
                <div className="agent-team-workspace__content">
              {detail?.team.id === selectedTeamId && (
                <AgentStaffingProposalPanel
                  ownerUserId={ownerUserId}
                  port={port}
                  profiles={profiles}
                  sourceRoomId={detail.team.id}
                  teams={teams}
                  onResolved={() => onStaffingResolved(detail.team.id)}
                />
              )}
              {workspaceSection === "overview" && detail?.team.id === selectedTeamId ? (
                <AgentTeamOverviewView
                  detail={detail}
                  ownerUserId={ownerUserId}
                  port={port}
                  profiles={profiles}
                  onOpen={setWorkspaceSection}
                />
              ) : workspaceSection === "conversation" && detail?.team.id === selectedTeamId ? (
                <AgentTeamConversationView
                  key={detail.team.id}
                  attachmentPort={attachmentPort}
                  detail={detail}
                  ownerUserId={ownerUserId}
                  port={port}
                  profiles={profiles}
                />
              ) : workspaceSection === "todos" && detail?.team.id === selectedTeamId ? (
                <AgentTeamTodosView
                  detail={detail}
                  focusTarget={navigationTarget?.roomId === detail.team.id ? navigationTarget : null}
                  onFocusHandled={onNavigationHandled}
                  ownerUserId={ownerUserId}
                  port={port}
                />
              ) : workspaceSection === "assets" && detail?.team.id === selectedTeamId ? (
                <AgentTeamAssetsView detail={detail} ownerUserId={ownerUserId} port={port} profiles={profiles} />
              ) : workspaceSection === "research" && detail?.team.id === selectedTeamId ? (
                <ProjectRequirementSurveysView onOpenRequirementSurveys={onOpenRequirementSurveys} />
              ) : workspaceSection === "runs" && detail?.team.id === selectedTeamId ? (
                <AgentTeamRunsView
                  detail={detail}
                  ownerUserId={ownerUserId}
                  port={port}
                  profiles={profiles}
                  runInspectorPort={runInspectorPort}
                  selectedAgentId={selectedRunAgentId}
                  onSelectAgent={setSelectedRunAgentId}
                />
              ) : detail?.team.id === selectedTeamId && detail.members.length > 0 ? (
                <div className="agent-team-members__list">
                  {detail.members.map((member) => {
                    const profile = profiles.find((candidate) => candidate.id === member.agentId);
                    return (
                      <article key={member.agentId}>
                        <span aria-hidden="true">{(profile?.name ?? member.role).slice(0, 1).toUpperCase()}</span>
                        <div>
                          <strong>{profile?.name ?? member.role}</strong>
                          <small>{member.role}{member.responsibility ? ` · ${member.responsibility}` : ""}</small>
                        </div>
                        <div className="agent-team-members__actions">
                          <em>{detail.team.projectManagerAgentId === member.agentId
                            ? "项目经理"
                            : detail.team.defaultAgentId === member.agentId ? "默认 Agent" : "成员"}</em>
                          <button
                            disabled={isMutatingMember}
                            onClick={() => onManageMember(member.agentId)}
                            type="button"
                          >管理</button>
                        </div>
                      </article>
                    );
                  })}
                </div>
              ) : !isDetailLoading && detail?.team.id === selectedTeamId ? (
                <div className="agent-team-members__empty">团队还没有成员，请邀请一个已有 Agent。</div>
              ) : null}
                </div>
                <section aria-label="Agent 团队成员" className="agent-team-member-rail">
                  <header><div><h4>团队成员</h4><p>点击成员查看运行状态</p></div><strong>{detail?.team.id === selectedTeamId ? detail.members.length : "—"}</strong></header>
                  <div className="agent-team-member-rail__list">
                    {detail?.team.id === selectedTeamId && detail.members.map((member) => {
                      const profile = profiles.find((candidate) => candidate.id === member.agentId);
                      const label = profile?.name ?? member.role;
                      return (
                        <article className={workspaceSection === "runs" && selectedRunAgentId === member.agentId ? "is-selected" : ""} key={member.agentId}>
                          <button
                            aria-label={`查看${label}的运行状态`}
                            aria-pressed={workspaceSection === "runs" && selectedRunAgentId === member.agentId}
                            className="agent-team-member-rail__summary"
                            onClick={() => {
                              setSelectedRunAgentId(member.agentId);
                              setWorkspaceSection("runs");
                            }}
                            type="button"
                          >
                            <span aria-hidden="true">{label.slice(0, 1).toUpperCase()}</span>
                            <div>
                              <strong>{label}</strong>
                              <small>{member.responsibility || member.role}</small>
                              {(detail.team.projectManagerAgentId === member.agentId || detail.team.defaultAgentId === member.agentId) && <em>{detail.team.projectManagerAgentId === member.agentId ? "项目经理" : "默认 Agent"}</em>}
                            </div>
                            <i aria-label="在线" />
                          </button>
                          <button disabled={isMutatingMember} onClick={() => onManageMember(member.agentId)} type="button">管理</button>
                        </article>
                      );
                    })}
                    {detail?.team.id === selectedTeamId && detail.members.length === 0 && <p>团队还没有成员</p>}
                  </div>
                  <button
                    aria-label="邀请 Agent"
                    disabled={!detail || detail.team.id !== selectedTeamId || isInviting
                      || detail.members.length >= profiles.length}
                    onClick={onInvite}
                    type="button"
                  >♙ 邀请 Agent</button>
                </section>
              </div>
            </section>
          )}
        </>
      )}
    </div>
  );
}
function ProjectRequirementSurveysView({
  onOpenRequirementSurveys,
}: {
  onOpenRequirementSurveys(): void;
}) {
  return (
    <div className="agent-team-surveys">
      <svg aria-hidden="true" className="agent-team-surveys__icon" viewBox="0 0 24 24">
        <path d="M9 5.5h6M9.75 3h4.5a1.25 1.25 0 0 1 1.25 1.25v1.5A1.25 1.25 0 0 1 14.25 7h-4.5A1.25 1.25 0 0 1 8.5 5.75v-1.5A1.25 1.25 0 0 1 9.75 3Z" />
        <path d="M8.5 5H7a2 2 0 0 0-2 2v12a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V7a2 2 0 0 0-2-2h-1.5M8.5 11h7M8.5 15h7M8.5 18h4" />
      </svg>
      <strong>需求调研已迁移</strong>
      <p>Task 调研现在由 Local Agent Host 保存并恢复原 Run，不再属于 Agent 群聊。</p>
      <button className="primary-button" onClick={onOpenRequirementSurveys} type="button">打开需求调研</button>
    </div>
  );
}
