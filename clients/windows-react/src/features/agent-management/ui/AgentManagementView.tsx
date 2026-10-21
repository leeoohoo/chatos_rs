import { useEffect, useMemo, useRef, useState } from "react";
import type { AskUserPort } from "../../ask-user/ports/AskUserPort";
import type { ConversationAttachmentPort } from "../../conversation-attachments/ports/ConversationAttachmentPort";
import type { ConversationComposerPort } from "../../conversation-composer/ports/ConversationComposerPort";
import type { ConversationHistoryPort } from "../../conversation-history/ports/ConversationHistoryPort";
import { ConversationHistoryView } from "../../conversation-history/ui/ConversationHistoryView";
import type { ConversationRealtimePort } from "../../conversation-realtime/ports/ConversationRealtimePort";
import type { ConversationRuntimeSettingsPort } from "../../conversation-runtime-settings/ports/ConversationRuntimeSettingsPort";
import type { RunInspectorPort } from "../../run-inspector/ports/RunInspectorPort";
import type { RunInspectorDetail } from "../../run-inspector/domain/runInspector";
import { useAgentManagementStore } from "../application/agentManagementStore";
import { type AgentTeamMemberMutation, useAgentTeamStore } from "../application/agentTeamStore";
import { useAgentTeamConversationStore } from "../application/agentTeamConversationStore";
import { useAgentTeamRunStore } from "../application/agentTeamRunStore";
import { useAgentTeamAssetStore } from "../application/agentTeamAssetStore";
import { useAgentTeamTodoStore } from "../application/agentTeamTodoStore";
import { useProjectRequirementSurveyStore } from "../application/projectRequirementSurveyStore";
import {
  agentProfessionLabels,
  heartbeatIntervalLabel,
  type AgentProfileEditorCatalog,
  type AgentProfileEditorData,
  type AgentProfileMutationInput,
  type AgentProfileSummary,
  type AgentAvatarDraft,
  type AgentAvatarHistoryAsset,
  type AgentConversationActivitySnapshot,
  type AgentProfileActivitySnapshot,
  type AgentTeamSummary,
  type AgentTeamDetail,
  type AgentTeamAsset,
  type AgentTeamAssetCategory,
  type AgentTeamTodo,
  type ProjectRequirementSurvey,
  type ProjectRequirementSurveyAnswers,
  type CreateAgentTeamInput,
  type PreparedAgentConversation,
} from "../domain/agentManagement";
import type { AgentManagementPort } from "../ports/AgentManagementPort";
import { AgentSkillManagementSheet } from "./AgentSkillManagementSheet";
import "./agentManagement.css";

export function AgentManagementView({
  ownerUserId,
  port,
  historyPort,
  attachmentPort,
  realtimePort,
  runtimeSettingsPort,
  runInspectorPort,
  askUserPort,
  projects = [],
}: {
  ownerUserId: string;
  port: AgentManagementPort;
  historyPort?: ConversationHistoryPort;
  attachmentPort?: ConversationAttachmentPort;
  realtimePort?: ConversationRealtimePort;
  runtimeSettingsPort?: ConversationRuntimeSettingsPort;
  runInspectorPort?: RunInspectorPort;
  askUserPort?: AskUserPort;
  projects?: Array<{ id: string; title: string }>;
}) {
  const profiles = useAgentManagementStore((state) => state.profiles);
  const editorCatalog = useAgentManagementStore((state) => state.editorCatalog);
  const isLoading = useAgentManagementStore((state) => state.isLoading);
  const isCatalogLoading = useAgentManagementStore((state) => state.isCatalogLoading);
  const isEditorLoading = useAgentManagementStore((state) => state.isEditorLoading);
  const isSaving = useAgentManagementStore((state) => state.isSaving);
  const error = useAgentManagementStore((state) => state.error);
  const activate = useAgentManagementStore((state) => state.activate);
  const refresh = useAgentManagementStore((state) => state.refresh);
  const loadEditorCatalog = useAgentManagementStore((state) => state.loadEditorCatalog);
  const loadProfileEditor = useAgentManagementStore((state) => state.loadProfileEditor);
  const saveProfile = useAgentManagementStore((state) => state.saveProfile);
  const reset = useAgentManagementStore((state) => state.reset);
  const teams = useAgentTeamStore((state) => state.teams);
  const selectedTeamId = useAgentTeamStore((state) => state.selectedTeamId);
  const teamDetail = useAgentTeamStore((state) => state.detail);
  const isLoadingTeams = useAgentTeamStore((state) => state.isLoading);
  const isCreatingTeam = useAgentTeamStore((state) => state.isCreating);
  const isTeamDetailLoading = useAgentTeamStore((state) => state.isDetailLoading);
  const isInvitingAgent = useAgentTeamStore((state) => state.isInviting);
  const isMutatingTeamMember = useAgentTeamStore((state) => state.isMutatingMember);
  const mutatingAgentId = useAgentTeamStore((state) => state.mutatingAgentId);
  const teamError = useAgentTeamStore((state) => state.error);
  const activateTeams = useAgentTeamStore((state) => state.activate);
  const refreshTeams = useAgentTeamStore((state) => state.refresh);
  const createTeam = useAgentTeamStore((state) => state.createTeam);
  const selectTeam = useAgentTeamStore((state) => state.selectTeam);
  const inviteAgent = useAgentTeamStore((state) => state.inviteAgent);
  const mutateTeamMember = useAgentTeamStore((state) => state.mutateMember);
  const resetTeams = useAgentTeamStore((state) => state.reset);
  const [surface, setSurface] = useState<"agents" | "teams">("agents");
  const [page, setPage] = useState(0);
  const [pageSize, setPageSize] = useState(20);
  const [preparingId, setPreparingId] = useState<string | null>(null);
  const [preparingDirectId, setPreparingDirectId] = useState<string | null>(null);
  const [directConversation, setDirectConversation] = useState<PreparedAgentConversation | null>(null);
  const [directError, setDirectError] = useState<string | null>(null);
  const [activityRefreshSequence, setActivityRefreshSequence] = useState(0);
  const [selectedAgentId, setSelectedAgentId] = useState<string | null>(null);
  const [showsSkillManagement, setShowsSkillManagement] = useState(false);
  const [showsTeamCreator, setShowsTeamCreator] = useState(false);
  const [showsAgentInvite, setShowsAgentInvite] = useState(false);
  const [editingTeamMemberId, setEditingTeamMemberId] = useState<string | null>(null);
  const [editor, setEditor] = useState<{
    catalog: AgentProfileEditorCatalog;
    data: AgentProfileEditorData | null;
  } | null>(null);
  const pageCount = Math.max(1, Math.ceil(profiles.length / pageSize));
  const canOpenDirect = Boolean(
    port.prepareConversation
    && port.sendConversationDraft
    && historyPort
    && attachmentPort
    && realtimePort
    && runtimeSettingsPort
    && runInspectorPort
    && askUserPort,
  );
  const visibleProfiles = useMemo(
    () => profiles.slice(page * pageSize, page * pageSize + pageSize),
    [page, pageSize, profiles],
  );
  const selectedProfile = useMemo(
    () => profiles.find((profile) => profile.id === selectedAgentId) ?? null,
    [profiles, selectedAgentId],
  );

  useEffect(() => {
    setEditor(null);
    setPreparingId(null);
    setPreparingDirectId(null);
    setDirectConversation(null);
    setDirectError(null);
    setActivityRefreshSequence(0);
    setSelectedAgentId(null);
    setShowsSkillManagement(false);
    setShowsTeamCreator(false);
    setShowsAgentInvite(false);
    setEditingTeamMemberId(null);
    void activate(port, ownerUserId).then(() => loadEditorCatalog(port, false));
    if (port.listTeams) void activateTeams(port, ownerUserId);
    return () => {
      reset();
      resetTeams();
    };
  }, [activate, activateTeams, loadEditorCatalog, ownerUserId, port, reset, resetTeams]);

  useEffect(() => {
    setPage((current) => Math.min(current, pageCount - 1));
  }, [pageCount]);

  useEffect(() => {
    if (selectedAgentId && profiles.length > 0 && !selectedProfile) setSelectedAgentId(null);
  }, [profiles.length, selectedAgentId, selectedProfile]);

  const openEditor = async (profile: AgentProfileSummary | null) => {
    const identity = profile?.id ?? "new-agent";
    setPreparingId(identity);
    const [catalog, data] = await Promise.all([
      loadEditorCatalog(port),
      profile ? loadProfileEditor(port, profile.id) : Promise.resolve(null),
    ]);
    setPreparingId(null);
    if (catalog && (!profile || data)) setEditor({ catalog, data });
  };

  const openDirect = async (profile: AgentProfileSummary) => {
    if (preparingDirectId || !port.prepareConversation || !canOpenDirect) return;
    setPreparingDirectId(profile.id);
    setDirectError(null);
    try {
      const prepared = await port.prepareConversation(ownerUserId, profile.id);
      if (prepared.agentId !== profile.id) throw new Error("Agent 私聊身份不一致，请刷新后重试。");
      setDirectConversation(prepared);
    } catch (failure) {
      setDirectError(failure instanceof Error ? failure.message : "Agent 私聊暂时无法打开。");
    } finally {
      setPreparingDirectId(null);
    }
  };

  const directComposerPort = useMemo<ConversationComposerPort>(() => ({
    sendDraft: async (input) => {
      const active = directConversation;
      if (!active
        || input.ownerUserId !== ownerUserId
        || input.conversationId !== active.conversationId
        || input.submission.explicitCapabilitySelection != null
        || !port.sendConversationDraft) {
        throw new Error("Agent 私聊发送上下文无效，请重新打开会话。");
      }
      try {
        return await port.sendConversationDraft({
          ownerUserId,
          roomId: active.roomId,
          guidanceTurnId: input.guidanceTurnId,
          submission: {
            content: input.submission.content,
            attachmentIds: input.submission.attachmentIds,
          },
        });
      } finally {
        setActivityRefreshSequence((current) => current + 1);
      }
    },
  }), [directConversation, ownerUserId, port]);

  if (directConversation
    && historyPort
    && attachmentPort
    && realtimePort
    && runtimeSettingsPort
    && runInspectorPort
    && askUserPort) {
    return (
      <div className="agent-management agent-direct" aria-label={`${directConversation.title} Agent 私聊`}>
        <section className="agent-management__panel agent-direct__panel">
          <header className="agent-management__toolbar agent-direct__toolbar">
            <div className="agent-direct__heading">
              <button
                aria-label="返回 Agent 管理"
                className="agent-direct__back"
                onClick={() => setDirectConversation(null)}
                type="button"
              >‹</button>
              <div>
                <h2>{directConversation.title}</h2>
                <p>Agent 私聊 · 模型、身份、权限与 Skill 由本机原生层锁定</p>
              </div>
            </div>
          </header>
          {port.loadConversationActivity && (
            <AgentConversationActivity
              conversation={directConversation}
              ownerUserId={ownerUserId}
              port={port}
              refreshSequence={activityRefreshSequence}
            />
          )}
          <ConversationHistoryView
            agentBound
            askUserPort={askUserPort}
            attachmentPort={attachmentPort}
            composerPort={directComposerPort}
            conversationId={directConversation.conversationId}
            ownerUserId={ownerUserId}
            port={historyPort}
            realtimePort={realtimePort}
            runInspectorPort={runInspectorPort}
            runtimeSettingsPort={runtimeSettingsPort}
          />
        </section>
      </div>
    );
  }

  return (
    <div className="agent-management" aria-label="Agent 管理">
      <section className="agent-management__panel">
        <header className="agent-management__toolbar">
          <div>
            <h2>Agent 管理</h2>
            <p>{surface === "agents" ? "管理 Agent、模型和权限。" : "按项目组织 Agent 团队。"}</p>
          </div>
          <div className="agent-management__actions">
            <div className="agent-management__segmented" aria-label="Agent 管理视图">
              <button aria-pressed={surface === "agents"} onClick={() => setSurface("agents")} type="button">Agent</button>
              <button aria-pressed={surface === "teams"} onClick={() => setSurface("teams")} type="button">团队</button>
            </div>
            {surface === "agents" ? <>
              <span>{isSaving ? "正在保存" : "本机资料"}</span>
              <button
                disabled={!port.loadSkillCatalog || !port.saveSkillOverride || !port.resetSkillOverride}
                onClick={() => setShowsSkillManagement(true)}
                type="button"
              >Skill 管理</button>
              <button
                className="primary-button"
                disabled={isCatalogLoading || isEditorLoading || preparingId === "new-agent"}
                onClick={() => void openEditor(null)}
                type="button"
              >{preparingId === "new-agent" ? "准备中…" : "＋ 创建 Agent"}</button>
              <button
                aria-label="刷新 Agent"
                disabled={isLoading}
                onClick={() => void refresh(port)}
                type="button"
              >↻</button>
            </> : <>
              <button
                className="primary-button"
                disabled={!port.createTeam || projects.length === 0 || isCreatingTeam}
                onClick={() => setShowsTeamCreator(true)}
                type="button"
              >＋ 创建团队</button>
              <button
                aria-label="刷新 Agent 团队"
                disabled={isLoadingTeams || !port.listTeams}
                onClick={() => void refreshTeams(port)}
                type="button"
              >↻</button>
            </>}
          </div>
        </header>

        {surface === "agents" && error && !editor && (
          <div className="agent-management__error" role="alert">
            <span>{error}</span>
            <button onClick={() => void refresh(port)} type="button">重试</button>
          </div>
        )}

        {surface === "agents" && directError && (
          <div className="agent-management__error" role="alert">
            <span>{directError}</span>
            <button onClick={() => setDirectError(null)} type="button">关闭</button>
          </div>
        )}

        {surface === "teams" ? (
          <AgentTeamCatalog
            detail={teamDetail}
            error={teamError}
            isDetailLoading={isTeamDetailLoading}
            isInviting={isInvitingAgent}
            isMutatingMember={isMutatingTeamMember}
            isLoading={isLoadingTeams}
            profiles={profiles}
            ownerUserId={ownerUserId}
            port={port}
            runInspectorPort={runInspectorPort}
            projects={projects}
            selectedTeamId={selectedTeamId}
            teams={teams}
            onCreate={() => setShowsTeamCreator(true)}
            onInvite={() => setShowsAgentInvite(true)}
            onManageMember={setEditingTeamMemberId}
            onRetry={() => void refreshTeams(port)}
            onSelect={(teamId) => {
              setEditingTeamMemberId(null);
              void selectTeam(port, teamId);
            }}
          />
        ) : isLoading && profiles.length === 0 ? (
          <div className="agent-management__state"><span className="mini-spinner" />正在读取 Agent…</div>
        ) : profiles.length === 0 ? (
          <div className="agent-management__empty">
            <span aria-hidden="true">◎</span>
            <h2>还没有 Agent</h2>
            <p>创建一个 Agent，并为它选择模型、职业、职责和本机权限。</p>
            <button
              className="primary-button"
              disabled={isCatalogLoading || preparingId === "new-agent"}
              onClick={() => void openEditor(null)}
              type="button"
            >{preparingId === "new-agent" ? "准备中…" : "创建 Agent"}</button>
          </div>
        ) : (
          <>
            <div className="agent-management__grid">
              {visibleProfiles.map((profile) => (
                <AgentCard
                  isSelected={selectedAgentId === profile.id}
                  key={profile.id}
                  preparing={preparingId === profile.id}
                  profile={profile}
                  modelLabel={editorCatalog
                    ? editorCatalog.models.find((model) => model.id === profile.modelConfigId)?.displayName
                      ?? `${profile.modelConfigId}（不可用）`
                    : profile.modelConfigId}
                  onEdit={() => void openEditor(profile)}
                  onDirect={() => void openDirect(profile)}
                  onSelect={() => setSelectedAgentId(profile.id)}
                  preparingDirect={preparingDirectId === profile.id}
                  directAvailable={canOpenDirect}
                />
              ))}
            </div>
            <Pagination
              page={page}
              pageCount={pageCount}
              pageSize={pageSize}
              total={profiles.length}
              onPageChange={setPage}
              onPageSizeChange={(size) => { setPageSize(size); setPage(0); }}
            />
            {selectedProfile && port.loadProfileActivity && (
              <AgentProfileActivity
                ownerUserId={ownerUserId}
                port={port}
                profile={selectedProfile}
              />
            )}
          </>
        )}
      </section>
      {editor && (
        <AgentProfileEditor
          catalog={editor.catalog}
          data={editor.data}
          error={error}
          ownerUserId={ownerUserId}
          port={port}
          saving={isSaving}
          onCancel={() => setEditor(null)}
          onSave={async (input) => {
            if (await saveProfile(port, input)) setEditor(null);
          }}
        />
      )}
      {showsSkillManagement && (
        <AgentSkillManagementSheet
          ownerUserId={ownerUserId}
          port={port}
          onClose={() => setShowsSkillManagement(false)}
        />
      )}
      {showsTeamCreator && (
        <CreateAgentTeamSheet
          error={teamError}
          existingTeams={teams}
          isCreating={isCreatingTeam}
          projects={projects}
          onCancel={() => setShowsTeamCreator(false)}
          onCreate={async (input) => {
            if (await createTeam(port, input)) setShowsTeamCreator(false);
          }}
        />
      )}
      {showsAgentInvite && teamDetail && selectedTeamId === teamDetail.team.id && (
        <InviteAgentToTeamSheet
          detail={teamDetail}
          error={teamError}
          isInviting={isInvitingAgent}
          profiles={profiles}
          onCancel={() => setShowsAgentInvite(false)}
          onInvite={async (agentId) => {
            if (await inviteAgent(port, teamDetail.team.id, agentId)) setShowsAgentInvite(false);
          }}
        />
      )}
      {editingTeamMemberId && teamDetail && selectedTeamId === teamDetail.team.id && (() => {
        const member = teamDetail.members.find((candidate) => candidate.agentId === editingTeamMemberId);
        if (!member) return null;
        return (
          <AgentTeamMemberSheet
            detail={teamDetail}
            error={teamError}
            isMutating={isMutatingTeamMember && mutatingAgentId === member.agentId}
            key={`${teamDetail.team.id}:${member.agentId}`}
            member={member}
            profile={profiles.find((candidate) => candidate.id === member.agentId) ?? null}
            onCancel={() => setEditingTeamMemberId(null)}
            onMutate={async (mutation) => {
              const succeeded = await mutateTeamMember(
                port, teamDetail.team.id, member.agentId, mutation,
              );
              if (succeeded) setEditingTeamMemberId(null);
            }}
          />
        );
      })()}
    </div>
  );
}

function AgentTeamCatalog({
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
  teams,
  onCreate,
  onInvite,
  onManageMember,
  onRetry,
  onSelect,
}: {
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
  teams: AgentTeamSummary[];
  onCreate(): void;
  onInvite(): void;
  onManageMember(agentId: string): void;
  onRetry(): void;
  onSelect(teamId: string): void;
}) {
  const [workspaceSection, setWorkspaceSection] = useState<"overview" | "members" | "conversation" | "todos" | "research" | "assets" | "runs">("members");
  useEffect(() => setWorkspaceSection("members"), [selectedTeamId]);
  const projectNames = useMemo(
    () => new Map(projects.map((project) => [project.id, project.title])),
    [projects],
  );
  return (
    <div className="agent-team-catalog">
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
          <div className="agent-team-grid" aria-label="项目 Agent 团队">
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
          </div>
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
                  <h3>{detail?.team.id === selectedTeamId
                    ? `${detail.team.name} · ${{
                      overview: "总览", members: "团队成员", conversation: "团队聊天",
                      todos: "团队任务", research: "需求调研", assets: "共享资产", runs: "运行",
                    }[workspaceSection]}`
                    : "正在读取团队空间"}</h3>
                  <p>{{
                    overview: "查看团队成员、消息与真实执行状态。",
                    members: "同一个 Agent 可以加入多个团队；角色和职责按当前团队隔离。",
                    conversation: "消息、路由与执行状态均来自本机原生事实。",
                    todos: "创建、分配并跟踪来自本机 SQLite 与 Host 的真实任务。",
                    research: "填写主聊天或任务为当前项目发起的真实 Host 调研单。",
                    assets: "维护带版本历史的项目背景、进度、技术栈与架构决策。",
                    runs: "按更新时间查看通讯与任务执行 Run。",
                  }[workspaceSection]}</p>
                </div>
                <div>
                  {isDetailLoading && <span aria-label="正在读取团队成员" className="mini-spinner" />}
                  <div className="agent-team-members__segmented" aria-label="团队工作区">
                    <button aria-pressed={workspaceSection === "overview"} onClick={() => setWorkspaceSection("overview")} type="button">总览</button>
                    <button aria-pressed={workspaceSection === "members"} onClick={() => setWorkspaceSection("members")} type="button">成员</button>
                    <button aria-pressed={workspaceSection === "conversation"} onClick={() => setWorkspaceSection("conversation")} type="button">聊天</button>
                    <button aria-pressed={workspaceSection === "todos"} onClick={() => setWorkspaceSection("todos")} type="button">任务</button>
                    <button aria-pressed={workspaceSection === "research"} onClick={() => setWorkspaceSection("research")} type="button">需求调研</button>
                    <button aria-pressed={workspaceSection === "assets"} onClick={() => setWorkspaceSection("assets")} type="button">共享资产</button>
                    <button aria-pressed={workspaceSection === "runs"} onClick={() => setWorkspaceSection("runs")} type="button">运行</button>
                  </div>
                  {workspaceSection === "members" && <button
                    disabled={!detail || detail.team.id !== selectedTeamId || isInviting
                      || detail.members.length >= profiles.length}
                    onClick={onInvite}
                    type="button"
                  >邀请 Agent</button>}
                </div>
              </header>
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
                  detail={detail}
                  ownerUserId={ownerUserId}
                  port={port}
                  profiles={profiles}
                />
              ) : workspaceSection === "todos" && detail?.team.id === selectedTeamId ? (
                <AgentTeamTodosView
                  detail={detail}
                  ownerUserId={ownerUserId}
                  port={port}
                />
              ) : workspaceSection === "assets" && detail?.team.id === selectedTeamId ? (
                <AgentTeamAssetsView detail={detail} ownerUserId={ownerUserId} port={port} />
              ) : workspaceSection === "research" && detail?.team.id === selectedTeamId ? (
                <ProjectRequirementSurveysView detail={detail} ownerUserId={ownerUserId} port={port} />
              ) : workspaceSection === "runs" && detail?.team.id === selectedTeamId ? (
                <AgentTeamRunsView
                  detail={detail}
                  ownerUserId={ownerUserId}
                  port={port}
                  profiles={profiles}
                  runInspectorPort={runInspectorPort}
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
            </section>
          )}
        </>
      )}
    </div>
  );
}

function AgentTeamOverviewView({
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
  onOpen(section: "members" | "conversation" | "research" | "assets" | "runs"): void;
}) {
  const snapshot = useAgentTeamRunStore((state) => state.snapshot);
  const isLoading = useAgentTeamRunStore((state) => state.isLoading);
  const error = useAgentTeamRunStore((state) => state.error);
  const activate = useAgentTeamRunStore((state) => state.activate);
  const refresh = useAgentTeamRunStore((state) => state.refresh);
  const reset = useAgentTeamRunStore((state) => state.reset);
  const visibleSnapshot = snapshot?.teamId === detail.team.id ? snapshot : null;
  const activeCount = (visibleSnapshot?.pendingDeliveryCount ?? 0)
    + (visibleSnapshot?.runningDeliveryCount ?? 0);
  const profileNames = useMemo(
    () => new Map(profiles.map((profile) => [profile.id, profile.name])),
    [profiles],
  );

  useEffect(() => {
    if (port.loadTeamRuns) void activate(port, ownerUserId, detail.team.id);
    else reset();
    return reset;
  }, [activate, detail.team.id, ownerUserId, port, reset]);

  useEffect(() => {
    if (!port.loadTeamRuns || activeCount === 0) return undefined;
    const timer = window.setInterval(() => void refresh(port), 1_000);
    return () => window.clearInterval(timer);
  }, [activeCount, port, refresh]);

  return (
    <div className="agent-team-overview">
      <div className="agent-team-overview__metrics">
        <button onClick={() => onOpen("members")} type="button"><strong>{detail.members.length}</strong><span>团队成员</span></button>
        <button onClick={() => onOpen("conversation")} type="button"><strong>{visibleSnapshot?.messageCount ?? "—"}</strong><span>团队消息</span></button>
        <button onClick={() => onOpen("runs")} type="button"><strong>{activeCount}</strong><span>正在处理</span></button>
        <button onClick={() => onOpen("runs")} type="button"><strong>{visibleSnapshot?.completedDeliveryCount ?? "—"}</strong><span>已完成</span></button>
        <button onClick={() => onOpen("runs")} type="button"><strong>{visibleSnapshot?.failedDeliveryCount ?? "—"}</strong><span>失败</span></button>
      </div>
      {error && (
        <div className="agent-team-conversation__error" role="alert">
          <span>{error}</span><button onClick={() => void refresh(port)} type="button">重试</button>
        </div>
      )}
      <div className="agent-team-overview__body">
        <section>
          <header><div><h4>团队目标</h4><p>{detail.team.goal || "尚未设置团队目标"}</p></div></header>
          <div className="agent-team-overview__assignments">
            <div><span>项目经理</span><strong>{detail.team.projectManagerAgentId
              ? profileNames.get(detail.team.projectManagerAgentId) ?? detail.team.projectManagerAgentId
              : "未指定"}</strong></div>
            <div><span>默认 Agent</span><strong>{detail.team.defaultAgentId
              ? profileNames.get(detail.team.defaultAgentId) ?? detail.team.defaultAgentId
              : "未指定"}</strong></div>
          </div>
        </section>
        <section>
          <header>
            <div><h4>最近运行</h4><p>按更新时间展示真实 Host Run</p></div>
            <button onClick={() => onOpen("runs")} type="button">查看全部</button>
          </header>
          {isLoading && !visibleSnapshot ? (
            <div className="agent-team-overview__empty"><span className="mini-spinner" />正在读取…</div>
          ) : visibleSnapshot?.runs.length ? visibleSnapshot.runs.slice(0, 4).map((run) => (
            <article key={run.runId}>
              <span className={`is-${run.deliveryStatus}`} aria-hidden="true" />
              <div><strong>{profileNames.get(run.agentId) ?? run.agentId}</strong><small>{teamRunTriggerLabel(run.triggerKind)}</small></div>
              <em>{teamRunStatusLabel(run.deliveryStatus)}</em>
            </article>
          )) : <div className="agent-team-overview__empty">还没有运行记录</div>}
        </section>
      </div>
    </div>
  );
}

const teamTodoColumns: Array<{
  key: "pending" | "in_progress" | "blocked" | "done";
  label: string;
  note: string;
}> = [
  { key: "pending", label: "待处理", note: "等待 executor lane" },
  { key: "in_progress", label: "执行中", note: "由 Host 对账" },
  { key: "blocked", label: "已阻塞", note: "需要明确处理" },
  { key: "done", label: "已结束", note: "完成或取消" },
];

function AgentTeamTodosView({
  detail,
  ownerUserId,
  port,
}: {
  detail: AgentTeamDetail;
  ownerUserId: string;
  port: AgentManagementPort;
}) {
  const todos = useAgentTeamTodoStore((state) => state.todos);
  const isLoading = useAgentTeamTodoStore((state) => state.isLoading);
  const mutationTodoId = useAgentTeamTodoStore((state) => state.mutationTodoId);
  const error = useAgentTeamTodoStore((state) => state.error);
  const activate = useAgentTeamTodoStore((state) => state.activate);
  const refresh = useAgentTeamTodoStore((state) => state.refresh);
  const save = useAgentTeamTodoStore((state) => state.save);
  const move = useAgentTeamTodoStore((state) => state.move);
  const cancel = useAgentTeamTodoStore((state) => state.cancel);
  const reset = useAgentTeamTodoStore((state) => state.reset);
  const [editingTodo, setEditingTodo] = useState<AgentTeamTodo | "new" | null>(null);
  const cycling = useRef(false);
  const supportsTodos = Boolean(
    port.listTeamTodos && port.createTeamTodo && port.updateTeamTodo
    && port.reorderTeamTodos && port.cancelTeamTodo && port.dispatchNextTeamDelivery,
  );

  useEffect(() => {
    setEditingTodo(null);
    if (supportsTodos) void activate(port, ownerUserId, detail.team.id);
    else reset();
    return reset;
  }, [activate, detail.team.id, ownerUserId, port, reset, supportsTodos]);

  useEffect(() => {
    if (!supportsTodos) return undefined;
    let cancelled = false;
    const cycle = async () => {
      if (cycling.current || cancelled) return;
      cycling.current = true;
      try {
        await port.loadTeamRuns?.(ownerUserId, detail.team.id);
        await refresh(port);
        if (!cancelled) {
          await port.dispatchNextTeamDelivery?.(ownerUserId, detail.team.id);
          await refresh(port);
        }
      } catch {
        // The stores retain the last trusted projection and expose their own retry state.
      } finally {
        cycling.current = false;
      }
    };
    const timer = window.setInterval(() => void cycle(), 2_000);
    void cycle();
    return () => { cancelled = true; window.clearInterval(timer); };
  }, [detail.team.id, ownerUserId, port, refresh, supportsTodos]);

  if (!supportsTodos) {
    return (
      <div className="agent-team-todos__state is-error" role="alert">
        <strong>当前版本尚未连接团队任务</strong><span>请更新本机客户端后重试。</span>
      </div>
    );
  }

  const grouped = new Map(teamTodoColumns.map((column) => [column.key, [] as AgentTeamTodo[]]));
  for (const todo of todos) {
    const key = todo.status === "completed" || todo.status === "cancelled" ? "done" : todo.status;
    grouped.get(key)?.push(todo);
  }

  return (
    <div className="agent-team-todos">
      <div className="agent-team-todos__toolbar">
        <div><strong>团队任务板</strong><span>依赖、优先级、executor Delivery 与 Host Run 均为真实记录。</span></div>
        <div>
          <button aria-label="刷新团队任务" disabled={isLoading} onClick={() => void refresh(port)} type="button">↻</button>
          <button
            className="primary-button"
            disabled={detail.members.length === 0 || mutationTodoId != null}
            onClick={() => setEditingTodo("new")}
            type="button"
          >新建任务</button>
        </div>
      </div>
      {error && (
        <div className="agent-team-conversation__error" role="alert">
          <span>{error}</span><button onClick={() => void refresh(port)} type="button">刷新</button>
        </div>
      )}
      {isLoading && todos.length === 0 ? (
        <div className="agent-team-todos__state"><span className="mini-spinner" />正在读取团队任务…</div>
      ) : (
        <div className="agent-team-todos__board">
          {teamTodoColumns.map((column) => {
            const items = grouped.get(column.key) ?? [];
            return (
              <section className={`agent-team-todo-column is-${column.key}`} key={column.key}>
                <header><div><strong>{column.label}</strong><small>{column.note}</small></div><span>{items.length}</span></header>
                <div>
                  {items.map((todo, index) => (
                    <article className={`agent-team-todo-card is-${todo.status}`} key={todo.id}>
                      <header>
                        <span>优先级 {todo.priority}</span>
                        <em>{todo.agentName}</em>
                      </header>
                      <button onClick={() => setEditingTodo(todo)} type="button">
                        <strong>{todo.title}</strong>
                        <p>{todo.detail || todo.executionContract.objective}</p>
                      </button>
                      {todo.dependencyIds.length > 0 && (
                        <small>依赖 {todo.dependencyIds.length} 项</small>
                      )}
                      {todo.blockedReason && <div className="agent-team-todo-card__notice">{todo.blockedReason}</div>}
                      {todo.result && <div className="agent-team-todo-card__result">{todo.result}</div>}
                      <footer>
                        {todo.status === "pending" && (
                          <>
                            <button disabled={index === 0 || mutationTodoId != null} onClick={() => void move(port, todo.id, -1)} type="button">上移</button>
                            <button disabled={index === items.length - 1 || mutationTodoId != null} onClick={() => void move(port, todo.id, 1)} type="button">下移</button>
                          </>
                        )}
                        {(todo.status === "pending" || todo.status === "in_progress" || todo.status === "blocked") && (
                          <button
                            className="is-danger"
                            disabled={mutationTodoId != null}
                            onClick={() => void cancel(port, todo)}
                            type="button"
                          >{todo.status === "in_progress" ? "停止" : "取消"}</button>
                        )}
                        {todo.status === "in_progress" && <span>停止会先终止 Host Run，再更新任务状态</span>}
                        <time>{formatTeamMessageTime(todo.updatedAtUnixMs)}</time>
                      </footer>
                    </article>
                  ))}
                  {items.length === 0 && <div className="agent-team-todo-column__empty">暂无任务</div>}
                </div>
              </section>
            );
          })}
        </div>
      )}
      {editingTodo && (
        <AgentTeamTodoEditor
          detail={detail}
          isSaving={mutationTodoId != null}
          onClose={() => setEditingTodo(null)}
          onSave={async (input) => {
            const succeeded = await save(port, input);
            if (succeeded) {
              setEditingTodo(null);
              if (port.dispatchNextTeamDelivery) {
                try { await port.dispatchNextTeamDelivery(ownerUserId, detail.team.id); } catch { /* surfaced by refresh */ }
              }
              await refresh(port);
            }
          }}
          todo={editingTodo === "new" ? null : editingTodo}
          todos={todos}
        />
      )}
    </div>
  );
}

function AgentTeamTodoEditor({
  detail,
  isSaving,
  onClose,
  onSave,
  todo,
  todos,
}: {
  detail: AgentTeamDetail;
  isSaving: boolean;
  onClose(): void;
  onSave(input: {
    todoId: string | null;
    agentId: string;
    title: string;
    detail: string;
    priority: number;
    dependencyIds: string[];
    expectedUpdatedAtUnixMs: number | null;
  }): Promise<void>;
  todo: AgentTeamTodo | null;
  todos: AgentTeamTodo[];
}) {
  const [agentId, setAgentId] = useState(todo?.agentId ?? detail.members[0]?.agentId ?? "");
  const [title, setTitle] = useState(todo?.title ?? "");
  const [description, setDescription] = useState(todo?.detail ?? "");
  const [priority, setPriority] = useState(todo?.priority ?? 50);
  const [dependencyIds, setDependencyIds] = useState(todo?.dependencyIds ?? []);
  const editable = todo == null || todo.status === "pending" || todo.status === "blocked";
  const candidates = todos.filter((candidate) => candidate.id !== todo?.id && candidate.status !== "cancelled");
  const canSave = editable && agentId && title.trim().length > 0 && title.trim().length <= 500
    && description.length <= 16_000 && priority >= 0 && priority <= 100;
  return (
    <div className="agent-editor-backdrop" onMouseDown={onClose} role="presentation">
      <section aria-label={todo ? "团队任务详情" : "新建团队任务"} aria-modal="true" className="agent-team-todo-editor" onMouseDown={(event) => event.stopPropagation()} role="dialog">
        <header>
          <div><span>{todo ? teamTodoStatusLabel(todo.status) : "新的工作契约"}</span><h2>{todo ? "团队任务详情" : "新建团队任务"}</h2></div>
          <button aria-label="关闭团队任务" disabled={isSaving} onClick={onClose} type="button">×</button>
        </header>
        <div className="agent-team-todo-editor__body">
          <label>执行 Agent<select disabled={todo != null || !editable} onChange={(event) => setAgentId(event.target.value)} value={agentId}>
            {detail.members.map((member) => <option key={member.agentId} value={member.agentId}>{member.role}</option>)}
          </select></label>
          <label>任务标题<input disabled={!editable} maxLength={500} onChange={(event) => setTitle(event.target.value)} value={title} /></label>
          <label>任务说明<textarea disabled={!editable} maxLength={16_000} onChange={(event) => setDescription(event.target.value)} rows={5} value={description} /></label>
          <label>优先级<div className="agent-team-todo-editor__priority"><input disabled={!editable} max={100} min={0} onChange={(event) => setPriority(Number(event.target.value))} type="range" value={priority} /><strong>{priority}</strong></div></label>
          <fieldset disabled={!editable}>
            <legend>前置依赖</legend>
            {candidates.length ? candidates.map((candidate) => (
              <label key={candidate.id}><input checked={dependencyIds.includes(candidate.id)} onChange={(event) => setDependencyIds((current) => event.target.checked ? [...current, candidate.id] : current.filter((id) => id !== candidate.id))} type="checkbox" /><span>{candidate.title}</span><em>{teamTodoStatusLabel(candidate.status)}</em></label>
            )) : <p>当前没有可选择的前置任务。</p>}
          </fieldset>
          {todo && (
            <section className="agent-team-todo-editor__contract"><strong>执行工作契约</strong><dl>
              <Row label="目标" value={todo.executionContract.objective} />
              <Row label="验收" value={todo.executionContract.acceptanceCriteria.join("；")} />
              <Row label="能力" value={todo.executionPlan.builtinCapabilities.join(" · ") || "只读"} />
            </dl></section>
          )}
        </div>
        <footer>
          <button disabled={isSaving} onClick={onClose} type="button">{editable ? "取消" : "关闭"}</button>
          {editable && <button className="primary-button" disabled={isSaving || !canSave} onClick={() => void onSave({
            todoId: todo?.id ?? null, agentId, title: title.trim(), detail: description,
            priority, dependencyIds, expectedUpdatedAtUnixMs: todo?.updatedAtUnixMs ?? null,
          })} type="button">{isSaving ? "正在保存…" : "保存任务"}</button>}
        </footer>
      </section>
    </div>
  );
}

function teamTodoStatusLabel(status: AgentTeamTodo["status"]) {
  return {
    pending: "待处理", in_progress: "执行中", blocked: "已阻塞",
    completed: "已完成", cancelled: "已取消",
  }[status];
}

const teamAssetCategoryLabels: Record<AgentTeamAssetCategory, string> = {
  overview: "项目概览",
  current_progress: "当前进度",
  tech_stack: "技术栈",
  architecture: "架构",
  conventions: "规范",
  decision: "决策",
  reference: "参考资料",
};

function ProjectRequirementSurveysView({
  detail,
  ownerUserId,
  port,
}: {
  detail: AgentTeamDetail;
  ownerUserId: string;
  port: AgentManagementPort;
}) {
  const surveys = useProjectRequirementSurveyStore((state) => state.surveys);
  const isLoading = useProjectRequirementSurveyStore((state) => state.isLoading);
  const submittingSurveyId = useProjectRequirementSurveyStore((state) => state.submittingSurveyId);
  const error = useProjectRequirementSurveyStore((state) => state.error);
  const activate = useProjectRequirementSurveyStore((state) => state.activate);
  const refresh = useProjectRequirementSurveyStore((state) => state.refresh);
  const submit = useProjectRequirementSurveyStore((state) => state.submit);
  const reset = useProjectRequirementSurveyStore((state) => state.reset);
  const [selectedSurveyId, setSelectedSurveyId] = useState<string | null>(null);
  const selectedSurvey = surveys.find((survey) => survey.surveyId === selectedSurveyId) ?? null;
  const supportsSurveys = Boolean(
    port.listProjectRequirementSurveys && port.resolveProjectRequirementSurvey,
  );

  useEffect(() => {
    setSelectedSurveyId(null);
    if (supportsSurveys) void activate(port, ownerUserId, detail.team.projectId);
    else reset();
    return reset;
  }, [activate, detail.team.projectId, ownerUserId, port, reset, supportsSurveys]);

  useEffect(() => {
    if (!supportsSurveys) return undefined;
    const timer = window.setInterval(() => void refresh(port), 30_000);
    return () => window.clearInterval(timer);
  }, [port, refresh, supportsSurveys]);

  if (!supportsSurveys) {
    return (
      <div className="agent-team-surveys__state is-error" role="alert">
        <strong>当前版本尚未连接需求调研</strong><span>请更新本机客户端后重试。</span>
      </div>
    );
  }

  return (
    <div className="agent-team-surveys">
      <div className="agent-team-surveys__toolbar">
        <div><strong>需求调研</strong><span>调研由 Local Agent Host 保存；提交后会恢复原 Run。</span></div>
        <button aria-label="刷新需求调研" disabled={isLoading} onClick={() => void refresh(port)} type="button">↻</button>
      </div>
      {error && (
        <div className="agent-team-conversation__error" role="alert">
          <span>{error}</span><button onClick={() => void refresh(port)} type="button">刷新</button>
        </div>
      )}
      <div className="agent-team-surveys__list">
        {isLoading && surveys.length === 0 ? (
          <div className="agent-team-surveys__state"><span className="mini-spinner" />正在读取需求调研…</div>
        ) : surveys.length === 0 ? (
          <div className="agent-team-surveys__state"><strong>暂无需求调研</strong><span>主聊天或任务发起的项目调研会显示在这里。</span></div>
        ) : surveys.map((survey) => (
          <button key={survey.surveyId} onClick={() => setSelectedSurveyId(survey.surveyId)} type="button">
            <span aria-hidden="true">{survey.status === "open" ? "✎" : "✓"}</span>
            <div><strong>{survey.title}</strong>{survey.description && <p>{survey.description}</p>}<small>{survey.questions.length} 个问题 · {survey.sourceTaskId ? "任务调研" : "主聊天调研"} · {formatTeamMessageTime(survey.createdAtUnixMs)}</small></div>
            <em className={`is-${survey.status}`}>{survey.status === "open" ? "待填写" : "已提交"}</em>
            <b aria-hidden="true">›</b>
          </button>
        ))}
      </div>
      {selectedSurvey && (
        <ProjectRequirementSurveySheet
          isSubmitting={submittingSurveyId === selectedSurvey.surveyId}
          key={`${selectedSurvey.surveyId}:${selectedSurvey.version}`}
          survey={selectedSurvey}
          onClose={() => setSelectedSurveyId(null)}
          onSubmit={async (answers) => {
            if (await submit(port, selectedSurvey, answers)) setSelectedSurveyId(null);
          }}
        />
      )}
    </div>
  );
}

function ProjectRequirementSurveySheet({
  survey,
  isSubmitting,
  onClose,
  onSubmit,
}: {
  survey: ProjectRequirementSurvey;
  isSubmitting: boolean;
  onClose(): void;
  onSubmit(answers: ProjectRequirementSurveyAnswers): Promise<void>;
}) {
  const [answers, setAnswers] = useState<ProjectRequirementSurveyAnswers>(survey.answers ?? {});
  const canSubmit = survey.status === "open" && survey.questions.every((question) => {
    const answer = answers[question.questionId];
    if (answer == null) return !question.required;
    if (question.responseKind === "text") return typeof answer === "string" && answer.trim().length > 0;
    if (question.responseKind === "boolean") return typeof answer === "boolean";
    if (question.responseKind === "single_choice") return typeof answer === "string" && question.options.includes(answer);
    return Array.isArray(answer) && answer.length > 0 && answer.every((value) => question.options.includes(value));
  });
  return (
    <div className="agent-editor-backdrop" role="presentation" onMouseDown={onClose}>
      <section aria-label="需求调研详情" aria-modal="true" className="agent-team-survey-sheet" onMouseDown={(event) => event.stopPropagation()} role="dialog">
        <header>
          <div><span>{survey.sourceTaskId ? "任务需求调研" : "主聊天需求调研"}</span><h2>{survey.title}</h2>{survey.description && <p>{survey.description}</p>}</div>
          <button aria-label="关闭需求调研" onClick={onClose} type="button">×</button>
        </header>
        <div className="agent-team-survey-sheet__body">
          {survey.questions.map((question, index) => (
            <fieldset disabled={survey.status === "resolved" || isSubmitting} key={question.questionId}>
              <legend>{index + 1}. {question.prompt}{question.required && <em> *</em>}</legend>
              {question.responseKind === "text" ? (
                <textarea
                  aria-label={question.prompt}
                  maxLength={16_000}
                  onChange={(event) => setAnswers((current) => ({ ...current, [question.questionId]: event.target.value }))}
                  value={typeof answers[question.questionId] === "string" ? answers[question.questionId] as string : ""}
                />
              ) : question.responseKind === "boolean" ? (
                <div className="agent-team-survey-sheet__choices">
                  {[true, false].map((value) => <label key={String(value)}><input checked={answers[question.questionId] === value} name={question.questionId} onChange={() => setAnswers((current) => ({ ...current, [question.questionId]: value }))} type="radio" />{value ? "是" : "否"}</label>)}
                </div>
              ) : (
                <div className="agent-team-survey-sheet__choices">
                  {question.options.map((option) => {
                    const multiple = question.responseKind === "multiple_choice";
                    const selected = multiple
                      ? Array.isArray(answers[question.questionId]) && (answers[question.questionId] as string[]).includes(option)
                      : answers[question.questionId] === option;
                    return <label key={option}><input checked={selected} name={question.questionId} onChange={() => setAnswers((current) => {
                      if (!multiple) return { ...current, [question.questionId]: option };
                      const values = Array.isArray(current[question.questionId]) ? current[question.questionId] as string[] : [];
                      return { ...current, [question.questionId]: selected ? values.filter((value) => value !== option) : [...values, option] };
                    })} type={multiple ? "checkbox" : "radio"} />{option}</label>;
                  })}
                </div>
              )}
            </fieldset>
          ))}
          {survey.status === "resolved" && <div className="agent-team-survey-sheet__resolved">✓ 你的回答已提交，原 Run 已恢复。</div>}
        </div>
        <footer>
          <button onClick={onClose} type="button">{survey.status === "resolved" ? "完成" : "取消"}</button>
          {survey.status === "open" && <button className="primary-button" disabled={!canSubmit || isSubmitting} onClick={() => void onSubmit(answers)} type="button">{isSubmitting ? "提交中…" : "提交并继续"}</button>}
        </footer>
      </section>
    </div>
  );
}

function AgentTeamAssetsView({
  detail,
  ownerUserId,
  port,
}: {
  detail: AgentTeamDetail;
  ownerUserId: string;
  port: AgentManagementPort;
}) {
  const assets = useAgentTeamAssetStore((state) => state.assets);
  const revisions = useAgentTeamAssetStore((state) => state.revisions);
  const isLoading = useAgentTeamAssetStore((state) => state.isLoading);
  const mutationAssetId = useAgentTeamAssetStore((state) => state.mutationAssetId);
  const loadingRevisionAssetId = useAgentTeamAssetStore((state) => state.loadingRevisionAssetId);
  const error = useAgentTeamAssetStore((state) => state.error);
  const activate = useAgentTeamAssetStore((state) => state.activate);
  const refresh = useAgentTeamAssetStore((state) => state.refresh);
  const save = useAgentTeamAssetStore((state) => state.save);
  const archive = useAgentTeamAssetStore((state) => state.archive);
  const loadRevisions = useAgentTeamAssetStore((state) => state.loadRevisions);
  const reset = useAgentTeamAssetStore((state) => state.reset);
  const [expandedIds, setExpandedIds] = useState<string[]>([]);
  const [editorAsset, setEditorAsset] = useState<AgentTeamAsset | "new" | null>(null);
  const [historyAsset, setHistoryAsset] = useState<AgentTeamAsset | null>(null);
  const supportsAssets = Boolean(
    port.listTeamAssets && port.saveTeamAsset && port.archiveTeamAsset && port.listTeamAssetRevisions,
  );

  useEffect(() => {
    if (supportsAssets) void activate(port, ownerUserId, detail.team.id);
    else reset();
    return reset;
  }, [activate, detail.team.id, ownerUserId, port, reset, supportsAssets]);

  if (!supportsAssets) {
    return (
      <div className="agent-team-assets__state is-error" role="alert">
        <strong>当前版本尚未连接团队共享资产</strong>
        <span>请更新本机客户端后重试。</span>
      </div>
    );
  }

  return (
    <div className="agent-team-assets">
      <div className="agent-team-assets__toolbar">
        <div><strong>团队共享资产</strong><span>任务启动时会固定读取当时的 revision。</span></div>
        <button className="primary-button" onClick={() => setEditorAsset("new")} type="button">＋ 新建资产</button>
      </div>
      {error && (
        <div className="agent-team-conversation__error" role="alert">
          <span>{error}</span><button onClick={() => void refresh(port)} type="button">刷新</button>
        </div>
      )}
      <div className="agent-team-assets__list">
        {isLoading && assets.length === 0 ? (
          <div className="agent-team-assets__state"><span className="mini-spinner" />正在读取共享资产…</div>
        ) : assets.length === 0 ? (
          <div className="agent-team-assets__state">
            <strong>还没有共享资产</strong><span>在这里维护项目背景、进度、技术栈和架构决策。</span>
          </div>
        ) : assets.map((asset) => {
          const expanded = expandedIds.includes(asset.id);
          return (
            <article key={asset.id}>
              <header>
                <button
                  aria-expanded={expanded}
                  onClick={() => setExpandedIds((current) => expanded
                    ? current.filter((id) => id !== asset.id)
                    : [...current, asset.id])}
                  type="button"
                >
                  <span aria-hidden="true">{expanded ? "⌄" : "›"}</span>
                  <strong>{asset.title}</strong>
                  <em>{teamAssetCategoryLabels[asset.category]}</em>
                </button>
                <small>r{asset.revision}</small>
                <div>
                  <button onClick={() => setEditorAsset(asset)} type="button">编辑</button>
                  <button onClick={() => {
                    setHistoryAsset(asset);
                    void loadRevisions(port, asset);
                  }} type="button">版本</button>
                  <button
                    disabled={mutationAssetId != null}
                    onClick={() => void archive(port, asset)}
                    type="button"
                  >归档</button>
                </div>
              </header>
              {expanded && <pre>{asset.markdown || "（空文档）"}</pre>}
            </article>
          );
        })}
      </div>
      {editorAsset && (
        <AgentTeamAssetEditor
          asset={editorAsset === "new" ? null : editorAsset}
          isSaving={mutationAssetId != null}
          key={editorAsset === "new" ? "new" : `${editorAsset.id}:${editorAsset.revision}`}
          onClose={() => setEditorAsset(null)}
          onSave={async (input) => {
            if (await save(port, input)) setEditorAsset(null);
          }}
        />
      )}
      {historyAsset && (
        <AgentTeamAssetHistory
          asset={historyAsset}
          isLoading={loadingRevisionAssetId === historyAsset.id}
          revisions={revisions[historyAsset.id] ?? []}
          onClose={() => setHistoryAsset(null)}
        />
      )}
    </div>
  );
}

function AgentTeamAssetEditor({
  asset,
  isSaving,
  onClose,
  onSave,
}: {
  asset: AgentTeamAsset | null;
  isSaving: boolean;
  onClose(): void;
  onSave(input: {
    assetId: string | null;
    category: AgentTeamAssetCategory;
    title: string;
    markdown: string;
    expectedRevision: number | null;
  }): Promise<void>;
}) {
  const [category, setCategory] = useState<AgentTeamAssetCategory>(asset?.category ?? "overview");
  const [title, setTitle] = useState(asset?.title ?? "");
  const [markdown, setMarkdown] = useState(asset?.markdown ?? "# 项目上下文\n\n");
  return (
    <div className="agent-editor-backdrop" role="presentation" onMouseDown={onClose}>
      <section
        aria-label={asset ? "编辑共享资产" : "新建共享资产"}
        aria-modal="true"
        className="agent-team-asset-editor"
        onMouseDown={(event) => event.stopPropagation()}
        role="dialog"
      >
        <header>
          <div><h2>{asset ? "编辑共享资产" : "新建共享资产"}</h2><p>Markdown 文档 · {asset ? `当前 r${asset.revision}` : "首次保存为 r1"}</p></div>
          <button aria-label="关闭共享资产编辑器" onClick={onClose} type="button">×</button>
        </header>
        <div className="agent-team-asset-editor__body">
          <label><span>分类</span><select value={category} onChange={(event) => setCategory(event.target.value as AgentTeamAssetCategory)}>
            {Object.entries(teamAssetCategoryLabels).map(([value, label]) => <option key={value} value={value}>{label}</option>)}
          </select></label>
          <label><span>标题</span><input maxLength={240} onChange={(event) => setTitle(event.target.value)} value={title} /></label>
          <label><span>Markdown</span><textarea maxLength={128_000} onChange={(event) => setMarkdown(event.target.value)} value={markdown} /></label>
          <section><strong>预览</strong><pre>{markdown || "（空文档）"}</pre></section>
        </div>
        <footer>
          <button disabled={isSaving} onClick={onClose} type="button">取消</button>
          <button
            className="primary-button"
            disabled={!title.trim() || isSaving}
            onClick={() => void onSave({
              assetId: asset?.id ?? null,
              category,
              title: title.trim(),
              markdown,
              expectedRevision: asset?.revision ?? null,
            })}
            type="button"
          >{isSaving ? "保存中…" : "保存"}</button>
        </footer>
      </section>
    </div>
  );
}

function AgentTeamAssetHistory({
  asset,
  isLoading,
  revisions,
  onClose,
}: {
  asset: AgentTeamAsset;
  isLoading: boolean;
  revisions: Array<{ revision: number; title: string; markdown: string; createdAtUnixMs: number }>;
  onClose(): void;
}) {
  const [selectedRevision, setSelectedRevision] = useState<number | null>(null);
  const selected = revisions.find((revision) => revision.revision === selectedRevision) ?? revisions[0];
  return (
    <div className="agent-editor-backdrop" role="presentation" onMouseDown={onClose}>
      <section aria-label="共享资产版本历史" aria-modal="true" className="agent-team-asset-history" onMouseDown={(event) => event.stopPropagation()} role="dialog">
        <header><div><h2>版本历史</h2><p>{asset.title}</p></div><button aria-label="关闭共享资产版本历史" onClick={onClose} type="button">×</button></header>
        <div className="agent-team-asset-history__body">
          {isLoading && revisions.length === 0 ? <div className="agent-team-assets__state"><span className="mini-spinner" />正在读取版本历史…</div> : revisions.length === 0 ? (
            <div className="agent-team-assets__state"><strong>没有版本记录</strong><span>保存资产后，历史版本会显示在这里。</span></div>
          ) : (
            <>
              <nav aria-label="共享资产版本">
                {revisions.map((revision) => <button aria-pressed={selected?.revision === revision.revision} key={revision.revision} onClick={() => setSelectedRevision(revision.revision)} type="button"><strong>r{revision.revision}{revision.revision === asset.revision ? " · 当前" : ""}</strong><span>{revision.title}</span><small>{formatTeamMessageTime(revision.createdAtUnixMs)}</small></button>)}
              </nav>
              <section><header><strong>{selected?.title}</strong><span>r{selected?.revision}</span></header><pre>{selected?.markdown}</pre></section>
            </>
          )}
        </div>
        <footer><button className="primary-button" onClick={onClose} type="button">完成</button></footer>
      </section>
    </div>
  );
}

function AgentTeamRunsView({
  detail,
  ownerUserId,
  port,
  profiles,
  runInspectorPort,
}: {
  detail: AgentTeamDetail;
  ownerUserId: string;
  port: AgentManagementPort;
  profiles: AgentProfileSummary[];
  runInspectorPort?: RunInspectorPort;
}) {
  const snapshot = useAgentTeamRunStore((state) => state.snapshot);
  const isLoading = useAgentTeamRunStore((state) => state.isLoading);
  const error = useAgentTeamRunStore((state) => state.error);
  const activate = useAgentTeamRunStore((state) => state.activate);
  const refresh = useAgentTeamRunStore((state) => state.refresh);
  const reset = useAgentTeamRunStore((state) => state.reset);
  const [selectedAgentId, setSelectedAgentId] = useState<string | null>(null);
  const [selectedRunId, setSelectedRunId] = useState<string | null>(null);
  const visibleSnapshot = snapshot?.teamId === detail.team.id ? snapshot : null;
  const hasActiveRun = visibleSnapshot?.runs.some(
    (run) => run.deliveryStatus === "pending" || run.deliveryStatus === "running",
  ) ?? false;
  const visibleRuns = selectedAgentId
    ? visibleSnapshot?.runs.filter((run) => run.agentId === selectedAgentId) ?? []
    : visibleSnapshot?.runs ?? [];
  const profileNames = useMemo(
    () => new Map(profiles.map((profile) => [profile.id, profile.name])),
    [profiles],
  );

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
      <div className="agent-team-runs__filters" aria-label="按 Agent 筛选运行记录">
        <button aria-pressed={selectedAgentId == null} onClick={() => setSelectedAgentId(null)} type="button">全部</button>
        {detail.members.map((member) => (
          <button
            aria-pressed={selectedAgentId === member.agentId}
            key={member.agentId}
            onClick={() => setSelectedAgentId(member.agentId)}
            type="button"
          >{profileNames.get(member.agentId) ?? member.role}</button>
        ))}
        <button aria-label="刷新团队运行记录" disabled={isLoading} onClick={() => void refresh(port)} type="button">↻</button>
      </div>
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
            <strong>{selectedAgentId ? "这个 Agent 还没有运行记录" : "还没有运行记录"}</strong>
            <span>Agent 被唤醒后，通讯与任务执行 Run 会显示在这里。</span>
          </div>
        ) : visibleRuns.map((run) => (
          <article key={run.runId}>
            <header>
              <strong>{profileNames.get(run.agentId) ?? run.agentId}</strong>
              <span className={`agent-team-runs__lane is-${run.lane}`}>
                {run.lane === "manager" ? "通讯" : "任务执行"}
              </span>
              <em className={`is-${run.deliveryStatus}`}>{teamRunStatusLabel(run.deliveryStatus)}</em>
            </header>
            <p>{teamRunTriggerLabel(run.triggerKind)} · Run {run.runId}</p>
            <footer>
              <time>{formatTeamMessageTime(run.updatedAtUnixMs)}</time>
              {run.lastError && <span title={run.lastError}>{run.lastError}</span>}
              <button
                disabled={!runInspectorPort}
                onClick={() => setSelectedRunId(run.runId)}
                type="button"
              >查看详情</button>
            </footer>
          </article>
        ))}
      </div>
      {selectedRunId && runInspectorPort && (
        <AgentTeamRunDetailSheet
          agentName={profileNames.get(
            visibleSnapshot?.runs.find((run) => run.runId === selectedRunId)?.agentId ?? "",
          ) ?? "Agent"}
          ownerUserId={ownerUserId}
          port={runInspectorPort}
          runId={selectedRunId}
          onClose={() => setSelectedRunId(null)}
        />
      )}
    </div>
  );
}

function AgentTeamRunDetailSheet({
  agentName,
  ownerUserId,
  port,
  runId,
  onClose,
}: {
  agentName: string;
  ownerUserId: string;
  port: RunInspectorPort;
  runId: string;
  onClose(): void;
}) {
  const [detail, setDetail] = useState<RunInspectorDetail | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [isLoadingOlder, setIsLoadingOlder] = useState(false);
  const [error, setError] = useState<string | null>(null);

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
    if (!detail || !["ready", "running"].includes(detail.run.status.toLowerCase())) return undefined;
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

  return (
    <div className="agent-editor-backdrop" role="presentation" onMouseDown={onClose}>
      <section
        aria-label="团队 Run 检查器"
        aria-modal="true"
        className="agent-team-run-detail"
        onMouseDown={(event) => event.stopPropagation()}
        role="dialog"
      >
        <header>
          <div><h2>Run 检查器</h2><p>{agentName} · {runId}</p></div>
          <button aria-label="关闭团队 Run 检查器" onClick={onClose} type="button">×</button>
        </header>
        <div className="agent-team-run-detail__body">
          {isLoading ? (
            <div className="agent-team-runs__state"><span className="mini-spinner" />正在读取 Run…</div>
          ) : error && !detail ? (
            <div className="agent-team-runs__state is-error"><strong>无法读取 Run</strong><span>{error}</span></div>
          ) : detail ? (
            <>
              {error && <div className="agent-team-run-detail__error" role="alert">{error}</div>}
              <dl>
                <Row label="状态" value={detail.run.status} />
                <Row label="迭代" value={`${detail.run.iteration} / ${detail.run.maxIterations}`} />
                <Row label="更新时间" value={formatTeamMessageTime(detail.run.updatedAtUnixMs)} />
              </dl>
              {detail.run.resultSummary && <div className="agent-team-run-detail__summary"><strong>结果摘要</strong><p>{detail.run.resultSummary}</p></div>}
              {detail.run.errorMessage && <div className="agent-team-run-detail__error" role="alert">{detail.run.errorMessage}</div>}
              <section>
                <h3>执行事件</h3>
                {detail.events.length === 0 ? (
                  <p className="agent-team-run-detail__empty">本次 Run 还没有执行事件。</p>
                ) : detail.events.map((event) => (
                  <article key={event.eventId}>
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
        <footer><button className="primary-button" onClick={onClose} type="button">完成</button></footer>
      </section>
    </div>
  );
}

function teamRunStatusLabel(status: "pending" | "running" | "completed" | "failed" | "cancelled") {
  return {
    pending: "等待执行",
    running: "正在运行",
    completed: "已完成",
    failed: "失败",
    cancelled: "已取消",
  }[status];
}

function teamRunTriggerLabel(trigger: "mention" | "default_agent" | "agent_mention" | "heartbeat" | "todo" | "todo_status") {
  return {
    mention: "Human 点名",
    default_agent: "默认 Agent",
    agent_mention: "Agent 点名",
    heartbeat: "主动巡检",
    todo: "团队任务",
    todo_status: "任务状态",
  }[trigger];
}

function AgentTeamConversationView({
  detail,
  ownerUserId,
  port,
  profiles,
}: {
  detail: AgentTeamDetail;
  ownerUserId: string;
  port: AgentManagementPort;
  profiles: AgentProfileSummary[];
}) {
  const snapshot = useAgentTeamConversationStore((state) => state.snapshot);
  const isLoading = useAgentTeamConversationStore((state) => state.isLoading);
  const isSending = useAgentTeamConversationStore((state) => state.isSending);
  const isCycling = useAgentTeamConversationStore((state) => state.isCycling);
  const error = useAgentTeamConversationStore((state) => state.error);
  const activate = useAgentTeamConversationStore((state) => state.activate);
  const refresh = useAgentTeamConversationStore((state) => state.refresh);
  const send = useAgentTeamConversationStore((state) => state.send);
  const reset = useAgentTeamConversationStore((state) => state.reset);
  const [content, setContent] = useState("");
  const [mentionedAgentIds, setMentionedAgentIds] = useState<string[]>([]);
  const supportsConversation = Boolean(
    port.loadTeamConversation
    && port.queueTeamMessage
    && port.dispatchNextTeamDelivery
    && port.loadTeamMemberActivity,
  );
  const memberProfiles = useMemo(() => detail.members.map((member) => ({
    member,
    profile: profiles.find((candidate) => candidate.id === member.agentId) ?? null,
  })), [detail.members, profiles]);
  const profileNames = useMemo(
    () => new Map(profiles.map((profile) => [profile.id, profile.name])),
    [profiles],
  );
  const visibleSnapshot = snapshot?.teamId === detail.team.id ? snapshot : null;
  const hasActiveDelivery = visibleSnapshot?.deliveries.some(
    (delivery) => delivery.status === "pending" || delivery.status === "running",
  ) ?? false;
  const canRoute = mentionedAgentIds.length > 0 || detail.team.defaultAgentId != null;
  const canSend = supportsConversation && content.trim().length > 0 && canRoute
    && !isSending && !isCycling;

  useEffect(() => {
    if (supportsConversation) void activate(port, ownerUserId, detail.team.id);
    else reset();
    return reset;
  }, [activate, detail.team.id, ownerUserId, port, reset, supportsConversation]);

  useEffect(() => {
    if (!supportsConversation || !hasActiveDelivery) return undefined;
    const timer = window.setInterval(() => void refresh(port), 1_000);
    return () => window.clearInterval(timer);
  }, [hasActiveDelivery, port, refresh, supportsConversation]);

  useEffect(() => {
    const memberIds = new Set(detail.members.map((member) => member.agentId));
    setMentionedAgentIds((current) => current.filter((agentId) => memberIds.has(agentId)));
  }, [detail.members]);

  if (!supportsConversation) {
    return (
      <div className="agent-team-conversation__state is-error" role="alert">
        <strong>当前版本尚未连接团队聊天</strong>
        <span>请更新本机客户端后再发送团队消息。</span>
      </div>
    );
  }

  const deliveriesByMessage = new Map<string, NonNullable<typeof visibleSnapshot>["deliveries"]>();
  for (const delivery of visibleSnapshot?.deliveries ?? []) {
    const deliveries = deliveriesByMessage.get(delivery.messageId) ?? [];
    deliveries.push(delivery);
    deliveriesByMessage.set(delivery.messageId, deliveries);
  }

  return (
    <div className="agent-team-conversation">
      <div aria-live="polite" className="agent-team-conversation__messages">
        {isLoading && !visibleSnapshot ? (
          <div className="agent-team-conversation__state"><span className="mini-spinner" />正在读取团队消息…</div>
        ) : visibleSnapshot?.messages.length ? visibleSnapshot.messages.map((message) => {
          const senderName = message.senderKind === "human"
            ? "你"
            : message.senderKind === "system"
              ? "系统"
              : profileNames.get(message.senderId) ?? message.senderId;
          const deliveries = deliveriesByMessage.get(message.id) ?? [];
          return (
            <article className={`agent-team-message agent-team-message--${message.senderKind}`} key={message.id}>
              <div className="agent-team-message__meta">
                <strong>{senderName}</strong>
                <time>{formatTeamMessageTime(message.createdAtUnixMs)}</time>
              </div>
              <p>{message.content}</p>
              {message.mentionedAgentIds.length > 0 && (
                <div className="agent-team-message__mentions">
                  {message.mentionedAgentIds.map((agentId) => (
                    <span key={agentId}>@{profileNames.get(agentId) ?? agentId}</span>
                  ))}
                </div>
              )}
              {deliveries.length > 0 && (
                <div className="agent-team-message__deliveries" aria-label="消息执行状态">
                  {deliveries.map((delivery) => (
                    <span className={`is-${delivery.status}`} key={delivery.id} title={delivery.lastError ?? undefined}>
                      {profileNames.get(delivery.targetAgentId) ?? delivery.targetAgentId}
                      {` · ${teamDeliveryStatusLabel(delivery.status)}`}
                      {delivery.lastError ? `：${delivery.lastError}` : ""}
                    </span>
                  ))}
                </div>
              )}
            </article>
          );
        }) : (
          <div className="agent-team-conversation__state">
            <strong>开始团队协作</strong>
            <span>选择一个或多个 Agent，或直接交给默认 Agent。</span>
          </div>
        )}
      </div>
      {error && (
        <div className="agent-team-conversation__error" role="alert">
          <span>{error}</span>
          <button disabled={isCycling} onClick={() => void refresh(port)} type="button">重试</button>
        </div>
      )}
      <div className="agent-team-composer">
        <div aria-label="选择接收消息的 Agent" className="agent-team-composer__mentions">
          {memberProfiles.map(({ member, profile }) => {
            const selected = mentionedAgentIds.includes(member.agentId);
            return (
              <button
                aria-pressed={selected}
                key={member.agentId}
                onClick={() => setMentionedAgentIds((current) => selected
                  ? current.filter((agentId) => agentId !== member.agentId)
                  : [...current, member.agentId])}
                type="button"
              >@{profile?.name ?? member.role}</button>
            );
          })}
          {mentionedAgentIds.length === 0 && detail.team.defaultAgentId && (
            <span>默认发送给 {profileNames.get(detail.team.defaultAgentId) ?? "默认 Agent"}</span>
          )}
        </div>
        <div className="agent-team-composer__input">
          <textarea
            aria-label="团队消息"
            disabled={isSending}
            maxLength={1_000_000}
            onChange={(event) => setContent(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter" && !event.shiftKey && !event.nativeEvent.isComposing && canSend) {
                event.preventDefault();
                void send(port, content, mentionedAgentIds).then((succeeded) => {
                  if (succeeded) {
                    setContent("");
                    setMentionedAgentIds([]);
                  }
                });
              }
            }}
            placeholder={canRoute ? "给团队发送消息…" : "请先选择一个 Agent"}
            rows={2}
            value={content}
          />
          <button
            aria-label="发送团队消息"
            className="primary-button"
            disabled={!canSend}
            onClick={() => void send(port, content, mentionedAgentIds).then((succeeded) => {
              if (succeeded) {
                setContent("");
                setMentionedAgentIds([]);
              }
            })}
            type="button"
          >{isSending ? "发送中…" : "发送"}</button>
        </div>
        {!canRoute && <small>当前团队没有默认 Agent，请先选择一个接收者。</small>}
      </div>
    </div>
  );
}

function teamDeliveryStatusLabel(status: "pending" | "running" | "completed" | "failed" | "cancelled") {
  return {
    pending: "等待执行",
    running: "正在执行",
    completed: "已完成",
    failed: "执行失败",
    cancelled: "已取消",
  }[status];
}

function formatTeamMessageTime(unixMs: number) {
  return new Intl.DateTimeFormat("zh-CN", {
    month: "numeric", day: "numeric", hour: "2-digit", minute: "2-digit",
  }).format(new Date(unixMs));
}

function AgentTeamMemberSheet({
  detail,
  error,
  isMutating,
  member,
  profile,
  onCancel,
  onMutate,
}: {
  detail: AgentTeamDetail;
  error: string | null;
  isMutating: boolean;
  member: AgentTeamDetail["members"][number];
  profile: AgentProfileSummary | null;
  onCancel(): void;
  onMutate(mutation: AgentTeamMemberMutation): Promise<void>;
}) {
  const [role, setRole] = useState(member.role);
  const [responsibility, setResponsibility] = useState(member.responsibility);
  const [confirmingRemoval, setConfirmingRemoval] = useState(false);
  const isDefault = detail.team.defaultAgentId === member.agentId;
  const isProjectManager = detail.team.projectManagerAgentId === member.agentId;
  const canManageProject = profile?.professionKey === "project_manager";
  const normalizedRole = role.trim();
  const normalizedResponsibility = responsibility.trim();
  const canSave = normalizedRole.length > 0 && normalizedRole.length <= 160
    && normalizedResponsibility.length <= 8_000;
  return (
    <div className="agent-editor-backdrop" role="presentation">
      <section aria-label="管理团队成员" aria-modal="true" className="agent-editor agent-team-member-editor" role="dialog">
        <header>
          <div><h2>管理团队成员</h2><p>{profile?.name ?? member.agentId}</p></div>
          <button aria-label="关闭团队成员管理" disabled={isMutating} onClick={onCancel} type="button">×</button>
        </header>
        <div className="agent-editor__body">
          {error && <div className="agent-editor__error" role="alert">{error}</div>}
          <label>当前项目角色<input maxLength={160} onChange={(event) => setRole(event.target.value)} value={role} /></label>
          <label>当前项目职责<textarea maxLength={8_000} onChange={(event) => setResponsibility(event.target.value)} rows={4} value={responsibility} /></label>
          <div className="agent-team-member-editor__assignments">
            <div>
              <strong>默认 Agent</strong>
              <small>Human 未点名发送团队消息时，由默认 Agent 接收。</small>
              <button
                disabled={isMutating || isDefault}
                onClick={() => void onMutate({ kind: "setDefault" })}
                type="button"
              >{isDefault ? "当前默认 Agent" : "设为默认 Agent"}</button>
            </div>
            <div>
              <strong>项目经理</strong>
              <small>{canManageProject
                ? "负责团队任务板的创建、分配、优先级和依赖管理。"
                : "只有职业为“项目经理”的 Agent 才能担任此角色。"}</small>
              <button
                disabled={isMutating || isProjectManager || !canManageProject}
                onClick={() => void onMutate({ kind: "setProjectManager" })}
                type="button"
              >{isProjectManager ? "当前项目经理" : "设为项目经理"}</button>
            </div>
          </div>
          <div className="agent-team-member-editor__danger">
            <div><strong>移出团队</strong><small>{isProjectManager
              ? "当前项目经理必须先完成交接。"
              : "移出后，该 Agent 不再接收此团队的任务和消息。"}</small></div>
            {confirmingRemoval ? (
              <div>
                <button disabled={isMutating} onClick={() => setConfirmingRemoval(false)} type="button">取消</button>
                <button
                  className="danger-button"
                  disabled={isMutating || isProjectManager}
                  onClick={() => void onMutate({ kind: "remove" })}
                  type="button"
                >确认移出</button>
              </div>
            ) : (
              <button disabled={isMutating || isProjectManager} onClick={() => setConfirmingRemoval(true)} type="button">移出团队</button>
            )}
          </div>
        </div>
        <footer>
          <button disabled={isMutating} onClick={onCancel} type="button">取消</button>
          <button
            className="primary-button"
            disabled={isMutating || !canSave}
            onClick={() => void onMutate({
              kind: "update",
              input: { role: normalizedRole, responsibility: normalizedResponsibility },
            })}
            type="button"
          >{isMutating ? "正在保存…" : "保存"}</button>
        </footer>
      </section>
    </div>
  );
}

function InviteAgentToTeamSheet({
  detail,
  error,
  isInviting,
  profiles,
  onCancel,
  onInvite,
}: {
  detail: AgentTeamDetail;
  error: string | null;
  isInviting: boolean;
  profiles: AgentProfileSummary[];
  onCancel(): void;
  onInvite(agentId: string): Promise<void>;
}) {
  const availableProfiles = useMemo(() => {
    const memberIds = new Set(detail.members.map((member) => member.agentId));
    return profiles.filter((profile) => !memberIds.has(profile.id));
  }, [detail.members, profiles]);
  const [agentId, setAgentId] = useState(availableProfiles[0]?.id ?? "");

  useEffect(() => {
    if (!availableProfiles.some((profile) => profile.id === agentId)) {
      setAgentId(availableProfiles[0]?.id ?? "");
    }
  }, [agentId, availableProfiles]);

  return (
    <div className="agent-editor-backdrop" role="presentation">
      <section aria-label="邀请 Agent" aria-modal="true" className="agent-editor agent-team-invite" role="dialog">
        <header>
          <h2>邀请 Agent</h2>
          <button aria-label="关闭邀请 Agent" disabled={isInviting} onClick={onCancel} type="button">×</button>
        </header>
        <div className="agent-editor__body">
          <p className="agent-team-creator__intro">选择一个已有 Agent 加入“{detail.team.name}”。</p>
          {error && <div className="agent-editor__error" role="alert">{error}</div>}
          {availableProfiles.length === 0 ? (
            <div className="agent-team-creator__empty">所有已有 Agent 都已加入当前团队，或尚未创建 Agent。</div>
          ) : (
            <div aria-label="可邀请 Agent" className="agent-team-invite__list" role="listbox">
              {availableProfiles.map((profile) => (
                <button
                  aria-selected={profile.id === agentId}
                  key={profile.id}
                  onClick={() => setAgentId(profile.id)}
                  role="option"
                  type="button"
                >
                  <span aria-hidden="true">{profile.name.slice(0, 1).toUpperCase()}</span>
                  <div><strong>{profile.name}</strong><small>{agentProfessionLabels[profile.professionKey]}</small></div>
                  {profile.id === agentId && <em aria-hidden="true">✓</em>}
                </button>
              ))}
            </div>
          )}
        </div>
        <footer>
          <button disabled={isInviting} onClick={onCancel} type="button">取消</button>
          <button
            className="primary-button"
            disabled={isInviting || !agentId}
            onClick={() => void onInvite(agentId)}
            type="button"
          >{isInviting ? "正在邀请…" : "邀请"}</button>
        </footer>
      </section>
    </div>
  );
}

function CreateAgentTeamSheet({
  error,
  existingTeams,
  isCreating,
  projects,
  onCancel,
  onCreate,
}: {
  error: string | null;
  existingTeams: AgentTeamSummary[];
  isCreating: boolean;
  projects: Array<{ id: string; title: string }>;
  onCancel(): void;
  onCreate(input: CreateAgentTeamInput): Promise<void>;
}) {
  const availableProjects = useMemo(() => {
    const boundProjects = new Set(existingTeams.map((team) => team.projectId));
    return projects.filter((project) => !boundProjects.has(project.id));
  }, [existingTeams, projects]);
  const [projectId, setProjectId] = useState(availableProjects[0]?.id ?? "");
  const [name, setName] = useState("");
  const [goal, setGoal] = useState("");

  useEffect(() => {
    if (!availableProjects.some((project) => project.id === projectId)) {
      setProjectId(availableProjects[0]?.id ?? "");
    }
  }, [availableProjects, projectId]);

  const projectName = availableProjects.find((project) => project.id === projectId)?.title ?? "项目";
  const resolvedName = name.trim() || `${projectName} Agent 团队`;
  return (
    <div className="agent-editor-backdrop" role="presentation">
      <section aria-label="创建 Agent 团队" aria-modal="true" className="agent-editor agent-team-creator" role="dialog">
        <header>
          <h2>创建 Agent 团队</h2>
          <button aria-label="关闭创建 Agent 团队" disabled={isCreating} onClick={onCancel} type="button">×</button>
        </header>
        <div className="agent-editor__body">
          <p className="agent-team-creator__intro">选择一个项目。只有主动创建团队的项目才会进入 Agent 团队空间。</p>
          {error && <div className="agent-editor__error" role="alert">{error}</div>}
          {availableProjects.length === 0 ? (
            <div className="agent-team-creator__empty">所有现有项目都已经建立团队，或当前还没有项目。</div>
          ) : <>
            <label>绑定项目
              <select aria-label="绑定项目" onChange={(event) => setProjectId(event.target.value)} value={projectId}>
                {availableProjects.map((project) => <option key={project.id} value={project.id}>{project.title}</option>)}
              </select>
            </label>
            <label>团队名称
              <input aria-label="团队名称" maxLength={160} onChange={(event) => setName(event.target.value)} placeholder={resolvedName} value={name} />
            </label>
            <label>团队目标（可选）
              <textarea aria-label="团队目标（可选）" maxLength={8_000} onChange={(event) => setGoal(event.target.value)} rows={4} value={goal} />
            </label>
            <small>团队创建后，再按 macOS 规则邀请 Agent、指定项目经理和配置团队职责。</small>
          </>}
        </div>
        <footer>
          <button disabled={isCreating} onClick={onCancel} type="button">取消</button>
          <button
            className="primary-button"
            disabled={isCreating || !projectId}
            onClick={() => void onCreate({ projectId, name: resolvedName, goal: goal.trim() })}
            type="button"
          >{isCreating ? "正在创建…" : "创建团队"}</button>
        </footer>
      </section>
    </div>
  );
}

function AgentConversationActivity({
  conversation,
  ownerUserId,
  port,
  refreshSequence,
}: {
  conversation: PreparedAgentConversation;
  ownerUserId: string;
  port: AgentManagementPort;
  refreshSequence: number;
}) {
  const [snapshot, setSnapshot] = useState<AgentConversationActivitySnapshot | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [isExpanded, setIsExpanded] = useState(false);
  const [retrySequence, setRetrySequence] = useState(0);

  useEffect(() => {
    const loadActivity = port.loadConversationActivity;
    if (!loadActivity) return;
    let cancelled = false;
    let timer: number | undefined;
    const load = async () => {
      try {
        const next = await loadActivity(ownerUserId, conversation.roomId);
        if (cancelled) return;
        if (next.roomId !== conversation.roomId
          || next.agentId !== conversation.agentId
          || next.conversationId !== conversation.conversationId) {
          throw new Error("Agent 运行动态身份不一致，请重新打开会话。");
        }
        setSnapshot((current) => (
          !current || next.conversationVersion >= current.conversationVersion ? next : current
        ));
        setError(null);
        setIsLoading(false);
        if (next.hasActiveDelivery) timer = window.setTimeout(() => void load(), 1_000);
      } catch (failure) {
        if (cancelled) return;
        setError(failure instanceof Error ? failure.message : "Agent 运行动态暂时无法读取。");
        setIsLoading(false);
      }
    };
    void load();
    return () => {
      cancelled = true;
      if (timer != null) window.clearTimeout(timer);
    };
  }, [conversation, ownerUserId, port, refreshSequence, retrySequence]);

  const items = snapshot?.items ?? [];
  const active = snapshot?.hasActiveDelivery ?? false;
  return (
    <section aria-label="Agent 运行动态" className="agent-activity" aria-live="polite">
      <div className="agent-activity__summary">
        <span className={active ? "agent-activity__pulse is-active" : "agent-activity__pulse"} />
        <div>
          <strong>{active ? "Agent 正在运行" : items.length ? `最近 ${items.length} 次运行` : "还没有运行记录"}</strong>
          <small>{active ? "Delivery 与 Run 状态由本机原生层持续对账" : "仅展示当前 Agent 私聊的安全运行投影"}</small>
        </div>
        {isLoading && <span className="mini-spinner" aria-label="正在读取 Agent 运行动态" />}
        {items.length > 0 && (
          <button onClick={() => setIsExpanded((value) => !value)} type="button">
            {isExpanded ? "收起" : "查看动态"}
          </button>
        )}
      </div>
      {error && (
        <div className="agent-activity__error" role="alert">
          <span>{error}</span>
          <button onClick={() => { setIsLoading(true); setRetrySequence((value) => value + 1); }} type="button">重试</button>
        </div>
      )}
      {isExpanded && items.length > 0 && (
        <div className="agent-activity__list" aria-label="Agent 最近运行">
          {items.slice(0, 20).map((item) => (
            <article key={item.deliveryId}>
              <span className={`agent-activity__status agent-activity__status--${item.deliveryStatus}`}>
                {agentDeliveryStatusLabel(item.deliveryStatus)}
              </span>
              <div>
                <strong>{agentRunStatusLabel(item.runStatus)}</strong>
                <small>{formatAgentActivityTime(item.updatedAtUnixMs)} · Run {shortIdentity(item.runId)}</small>
              </div>
              <code title={item.deliveryId}>Delivery {shortIdentity(item.deliveryId)}</code>
            </article>
          ))}
          {items.length > 20 && <small>仅显示最新 20 条，共 {items.length} 条。</small>}
        </div>
      )}
    </section>
  );
}

function AgentProfileActivity({
  ownerUserId,
  port,
  profile,
}: {
  ownerUserId: string;
  port: AgentManagementPort;
  profile: AgentProfileSummary;
}) {
  const [snapshot, setSnapshot] = useState<AgentProfileActivitySnapshot | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [retrySequence, setRetrySequence] = useState(0);

  useEffect(() => {
    const loadActivity = port.loadProfileActivity;
    if (!loadActivity) return;
    let cancelled = false;
    let timer: number | undefined;
    const load = async () => {
      try {
        const next = await loadActivity(ownerUserId, profile.id);
        if (cancelled) return;
        if (next.agentId !== profile.id) {
          throw new Error("Agent 运行动态身份不一致，请重新选择 Agent。");
        }
        setSnapshot(next);
        setError(null);
        setIsLoading(false);
        if (next.hasActiveDelivery) timer = window.setTimeout(() => void load(), 1_000);
      } catch (failure) {
        if (cancelled) return;
        setError(failure instanceof Error ? failure.message : "Agent 运行动态暂时无法读取。");
        setIsLoading(false);
      }
    };
    setIsLoading(true);
    setError(null);
    void load();
    return () => {
      cancelled = true;
      if (timer != null) window.clearTimeout(timer);
    };
  }, [ownerUserId, port, profile.id, retrySequence]);

  const current = snapshot?.agentId === profile.id ? snapshot : null;
  const items = current?.items ?? [];
  const active = current?.hasActiveDelivery ?? false;
  return (
    <section aria-label={`${profile.name} 运行动态`} className="agent-profile-activity" aria-live="polite">
      <header>
        <div>
          <h3>{profile.name} · 运行动态</h3>
          <p>最近的私聊消息处理</p>
        </div>
        <div>
          {active && <span className="agent-profile-activity__active">正在运行</span>}
          {isLoading && <span className="mini-spinner" aria-label="正在读取 Agent 运行动态" />}
          <button
            disabled={isLoading}
            onClick={() => { setIsLoading(true); setRetrySequence((value) => value + 1); }}
            type="button"
          >刷新</button>
        </div>
      </header>
      {error ? (
        <div className="agent-profile-activity__state is-error" role="alert">
          <span>{error}</span>
          <button onClick={() => setRetrySequence((value) => value + 1)} type="button">重试</button>
        </div>
      ) : !isLoading && items.length === 0 ? (
        <div className="agent-profile-activity__state">
          <strong>还没有私聊运行记录</strong>
          <span>与这个 Agent 开始私聊后，最近的 Delivery 与 Run 会显示在这里。</span>
        </div>
      ) : (
        <div className="agent-activity__list" aria-label="Agent 最近私聊运行">
          {items.slice(0, 20).map((item) => (
            <article key={item.deliveryId}>
              <span className={`agent-activity__status agent-activity__status--${item.deliveryStatus}`}>
                {agentDeliveryStatusLabel(item.deliveryStatus)}
              </span>
              <div>
                <strong>{agentRunStatusLabel(item.runStatus)}</strong>
                <small>{formatAgentActivityTime(item.updatedAtUnixMs)} · Run {shortIdentity(item.runId)}</small>
              </div>
              <code title={item.deliveryId}>Delivery {shortIdentity(item.deliveryId)}</code>
            </article>
          ))}
          {items.length > 20 && <small>仅显示最新 20 条，共 {items.length} 条。</small>}
        </div>
      )}
    </section>
  );
}

function agentDeliveryStatusLabel(status: AgentConversationActivitySnapshot["items"][number]["deliveryStatus"]) {
  return { pending: "等待", running: "执行中", completed: "完成", failed: "失败", cancelled: "已取消" }[status];
}

function agentRunStatusLabel(status: AgentConversationActivitySnapshot["items"][number]["runStatus"]) {
  return { ready: "Run 已准备", running: "Run 正在执行", succeeded: "Run 已完成", failed: "Run 执行失败", cancelled: "Run 已取消" }[status];
}

function formatAgentActivityTime(unixMilliseconds: number) {
  return new Intl.DateTimeFormat("zh-CN", {
    month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit", second: "2-digit",
  }).format(new Date(unixMilliseconds));
}

function shortIdentity(value: string) {
  return value.length <= 12 ? value : `${value.slice(0, 8)}…${value.slice(-4)}`;
}

function AgentCard({
  profile,
  modelLabel,
  preparing,
  preparingDirect,
  directAvailable,
  isSelected,
  onEdit,
  onDirect,
  onSelect,
}: {
  profile: AgentProfileSummary;
  modelLabel: string;
  preparing: boolean;
  preparingDirect: boolean;
  directAvailable: boolean;
  isSelected: boolean;
  onEdit(): void;
  onDirect(): void;
  onSelect(): void;
}) {
  return (
    <article className={isSelected ? "agent-card is-selected" : "agent-card"} onClick={onSelect}>
      <header>
        {profile.avatarPreviewUrl ? (
          <img alt="" aria-hidden="true" className="agent-card__avatar" src={profile.avatarPreviewUrl} />
        ) : (
          <span className="agent-card__avatar" aria-hidden="true">{profile.name.slice(0, 1).toUpperCase()}</span>
        )}
        <div>
          <h3>{profile.name}</h3>
          <p>{profile.canManageStaff ? "可招募和解雇成员" : "无人员管理权限"}</p>
        </div>
      </header>
      {profile.description && <p className="agent-card__description">{profile.description}</p>}
      <div className="agent-card__actions">
        <button
          disabled={preparingDirect || !directAvailable}
          onClick={(event) => { event.stopPropagation(); onDirect(); }}
          title={directAvailable ? undefined : "Agent 原生会话服务尚未就绪"}
          type="button"
        >
          {preparingDirect ? "准备中…" : "私聊"}
        </button>
        <button
          aria-pressed={isSelected}
          onClick={(event) => { event.stopPropagation(); onSelect(); }}
          type="button"
        >运行动态</button>
        <button
          disabled={preparing}
          onClick={(event) => { event.stopPropagation(); onEdit(); }}
          type="button"
        >{preparing ? "准备中…" : "编辑"}</button>
      </div>
      <dl>
        <Row label="模型" value={modelLabel} />
        <Row label="思考等级" value={profile.thinkingLevel ?? "跟随模型默认"} />
        <Row label="职业" value={agentProfessionLabels[profile.professionKey]} />
        <Row label="主动巡检" value={profile.heartbeatEnabled ? heartbeatIntervalLabel(profile.heartbeatIntervalSeconds) : "关闭"} />
      </dl>
      {(profile.canManageStaff || profile.canAccessLocalProjects) && (
        <footer>
          {profile.canManageStaff && <span>人员管理</span>}
          {profile.canAccessLocalProjects && <span>项目与团队</span>}
        </footer>
      )}
    </article>
  );
}

function Row({ label, value }: { label: string; value: string }) {
  return <div><dt>{label}</dt><dd title={value}>{value}</dd></div>;
}

function AgentProfileEditor({
  catalog,
  data,
  error,
  ownerUserId,
  port,
  saving,
  onCancel,
  onSave,
}: {
  catalog: AgentProfileEditorCatalog;
  data: AgentProfileEditorData | null;
  error: string | null;
  ownerUserId: string;
  port: AgentManagementPort;
  saving: boolean;
  onCancel(): void;
  onSave(input: AgentProfileMutationInput): Promise<void>;
}) {
  const existing = data?.profile ?? null;
  const [name, setName] = useState(existing?.name ?? "");
  const [description, setDescription] = useState(existing?.description ?? "");
  const [rolePrompt, setRolePrompt] = useState(data?.rolePrompt ?? catalog.defaultRolePrompt);
  const [modelConfigId, setModelConfigId] = useState(existing?.modelConfigId ?? catalog.models[0]?.id ?? "");
  const [thinkingLevel, setThinkingLevel] = useState(existing?.thinkingLevel ?? "");
  const [professionKey, setProfessionKey] = useState(existing?.professionKey ?? "general_member");
  const [canManageStaff, setCanManageStaff] = useState(existing?.canManageStaff ?? false);
  const [canAccessLocalProjects, setCanAccessLocalProjects] = useState(existing?.canAccessLocalProjects ?? false);
  const [heartbeatEnabled, setHeartbeatEnabled] = useState(existing?.heartbeatEnabled ?? false);
  const [heartbeatIntervalSeconds, setHeartbeatIntervalSeconds] = useState(existing?.heartbeatIntervalSeconds ?? 900);
  const [heartbeatPrompt, setHeartbeatPrompt] = useState(data?.heartbeatPrompt ?? "");
  const [avatarPresentationId] = useState(() => crypto.randomUUID().toLowerCase());
  const [avatarDraft, setAvatarDraft] = useState<AgentAvatarDraft | null>(null);
  const [avatarHistorySelection, setAvatarHistorySelection] = useState<AgentAvatarHistoryAsset | null>(null);
  const [avatarHistory, setAvatarHistory] = useState<AgentAvatarHistoryAsset[] | null>(null);
  const [showsAvatarHistory, setShowsAvatarHistory] = useState(false);
  const [isLoadingAvatarHistory, setIsLoadingAvatarHistory] = useState(false);
  const [avatarRemoved, setAvatarRemoved] = useState(false);
  const [isPickingAvatar, setIsPickingAvatar] = useState(false);
  const [avatarError, setAvatarError] = useState<string | null>(null);
  const selectedModel = catalog.models.find((model) => model.id === modelConfigId);
  const selectedProfession = catalog.professions.find((profession) => profession.key === professionKey);
  const valid = name.trim().length > 0
    && rolePrompt.trim().length > 0
    && selectedModel != null
    && (!thinkingLevel || selectedModel.thinkingLevels.some((level) => level === thinkingLevel));

  useEffect(() => () => {
    void port.discardAvatarPresentation(ownerUserId, avatarPresentationId).catch(() => undefined);
  }, [avatarPresentationId, ownerUserId, port]);

  const pickAvatar = async () => {
    if (isPickingAvatar || saving) return;
    setIsPickingAvatar(true);
    setAvatarError(null);
    try {
      const draft = await port.pickAvatar(ownerUserId, avatarPresentationId);
      if (!draft) return;
      if (avatarDraft) {
        await port.discardAvatarReferences(ownerUserId, avatarPresentationId, [avatarDraft.id]);
      }
      setAvatarDraft(draft);
      setAvatarHistorySelection(null);
      setAvatarRemoved(false);
    } catch (avatarFailure) {
      setAvatarError(avatarFailure instanceof Error ? avatarFailure.message : "头像暂时无法读取。");
    } finally {
      setIsPickingAvatar(false);
    }
  };

  const useDefaultAvatar = () => {
    if (avatarDraft) {
      void port.discardAvatarReferences(ownerUserId, avatarPresentationId, [avatarDraft.id]);
    }
    setAvatarDraft(null);
    setAvatarHistorySelection(null);
    setAvatarRemoved(true);
    setAvatarError(null);
  };

  const openAvatarHistory = async () => {
    if (isLoadingAvatarHistory || saving) return;
    if (avatarHistory) {
      setShowsAvatarHistory(true);
      return;
    }
    setIsLoadingAvatarHistory(true);
    setAvatarError(null);
    try {
      const assets = await port.loadAvatarHistory(ownerUserId);
      setAvatarHistory(assets);
      setShowsAvatarHistory(true);
    } catch (historyFailure) {
      setAvatarError(historyFailure instanceof Error ? historyFailure.message : "图片生成记录暂时无法读取。");
    } finally {
      setIsLoadingAvatarHistory(false);
    }
  };

  const selectHistoryAvatar = (asset: AgentAvatarHistoryAsset) => {
    if (avatarDraft) {
      void port.discardAvatarReferences(ownerUserId, avatarPresentationId, [avatarDraft.id]);
    }
    setAvatarDraft(null);
    setAvatarHistorySelection(asset);
    setAvatarRemoved(false);
    setShowsAvatarHistory(false);
    setAvatarError(null);
  };

  return (
    <div className="agent-editor-backdrop" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget && !saving) onCancel(); }}>
      <form
        aria-label={existing ? "编辑 Agent" : "创建 Agent"}
        aria-modal="true"
        className="agent-editor"
        onSubmit={(event) => {
          event.preventDefault();
          if (!valid || saving) return;
          void onSave({
            agentId: existing?.id ?? null,
            expectedUpdatedAtUnixMs: existing?.updatedAtUnixMs ?? null,
            name: name.trim(),
            description: description.trim(),
            rolePrompt: rolePrompt.trim(),
            modelConfigId,
            thinkingLevel: thinkingLevel ? thinkingLevel as AgentProfileMutationInput["thinkingLevel"] : null,
            professionKey,
            canManageStaff,
            canAccessLocalProjects,
            heartbeatEnabled,
            heartbeatIntervalSeconds,
            heartbeatPrompt: heartbeatPrompt.trim(),
            avatar: avatarDraft
              ? { kind: "staged", presentationId: avatarPresentationId, referenceId: avatarDraft.id }
              : avatarHistorySelection
                ? { kind: "history", recordId: avatarHistorySelection.recordId, contentToken: avatarHistorySelection.contentToken }
              : avatarRemoved ? { kind: "remove" } : { kind: "preserve" },
          });
        }}
        role="dialog"
      >
        <header><h2>{existing ? "编辑 Agent" : "创建 Agent"}</h2><button aria-label="关闭" disabled={saving} onClick={onCancel} type="button">×</button></header>
        <div className="agent-editor__body">
          {(error || avatarError) && <div className="agent-editor__error" role="alert">{error ?? avatarError}</div>}
          <div className="agent-avatar-editor">
            {(avatarDraft?.previewUrl || avatarHistorySelection?.previewUrl || (!avatarRemoved && existing?.avatarPreviewUrl)) ? (
              <img alt="Agent 头像预览" src={avatarDraft?.previewUrl ?? avatarHistorySelection?.previewUrl ?? existing?.avatarPreviewUrl ?? undefined} />
            ) : (
              <span aria-label={avatarRemoved || !existing?.hasAvatar ? "默认头像" : "当前头像"}>
                {name.trim().slice(0, 1).toUpperCase() || "A"}
              </span>
            )}
            <div>
              <button disabled={isPickingAvatar || saving} onClick={() => void pickAvatar()} type="button">
                {isPickingAvatar ? "正在读取…" : "从本机选择"}
              </button>
              <button disabled={isLoadingAvatarHistory || saving} onClick={() => void openAvatarHistory()} type="button">
                {isLoadingAvatarHistory ? "正在读取记录…" : "从生成记录选择"}
              </button>
              {(avatarDraft || avatarHistorySelection || (existing?.hasAvatar && !avatarRemoved)) && (
                <button disabled={saving} onClick={useDefaultAvatar} type="button">使用默认头像</button>
              )}
              <small>图片由原生层裁成 256 × 256 PNG；路径和图片字节不会进入 React。</small>
            </div>
          </div>
          {showsAvatarHistory && (
            <section aria-label="头像生成记录" className="agent-avatar-history">
              <header><strong>选择头像</strong><button aria-label="关闭头像生成记录" onClick={() => setShowsAvatarHistory(false)} type="button">×</button></header>
              {avatarHistory?.length ? (
                <div>{avatarHistory.map((asset) => (
                  <button key={`${asset.recordId}:${asset.contentToken}`} onClick={() => selectHistoryAvatar(asset)} type="button">
                    <img alt="生成的头像候选" src={asset.previewUrl} />
                  </button>
                ))}</div>
              ) : <p>暂无图片生成记录，仍可从本机选择。</p>}
            </section>
          )}
          <label>名称<input maxLength={120} onChange={(event) => setName(event.target.value)} placeholder="Agent 名称" value={name} /></label>
          <label>说明<textarea maxLength={2_000} onChange={(event) => setDescription(event.target.value)} placeholder="Agent 负责什么" rows={3} value={description} /></label>
          <label>模型<select onChange={(event) => { setModelConfigId(event.target.value); setThinkingLevel(""); }} value={modelConfigId}>{catalog.models.map((model) => <option key={model.id} value={model.id}>{model.displayName}</option>)}</select></label>
          <label>思考等级<select disabled={!selectedModel?.supportsReasoning} onChange={(event) => setThinkingLevel(event.target.value)} value={thinkingLevel}><option value="">跟随模型默认{selectedModel?.defaultThinkingLevel ? `（${selectedModel.defaultThinkingLevel}）` : ""}</option>{selectedModel?.thinkingLevels.map((level) => <option key={level} value={level}>{level}</option>)}</select></label>
          <label>职业<select onChange={(event) => setProfessionKey(event.target.value as typeof professionKey)} value={professionKey}>{catalog.professions.map((profession) => <option key={profession.key} value={profession.key}>{profession.categoryLabel} · {profession.label}</option>)}</select><small>{selectedProfession?.description}</small></label>
          <label>角色 Prompt<textarea maxLength={32_000} onChange={(event) => setRolePrompt(event.target.value)} rows={7} value={rolePrompt} /></label>
          <Toggle checked={canManageStaff} label="允许招募和解雇成员" note="Agent 可提交人员变更提案；所有变更仍需 Human 确认。" onChange={setCanManageStaff} />
          <Toggle checked={canAccessLocalProjects} label="允许查看项目并创建团队" note="只发布项目名称和临时选择，不向 Agent 暴露真实路径。" onChange={setCanAccessLocalProjects} />
          <Toggle checked={heartbeatEnabled} label="主动巡检" note="按周期检查团队和私聊；无事时不会发送消息。" onChange={setHeartbeatEnabled} />
          {heartbeatEnabled && <>
            <label>巡检周期<select onChange={(event) => setHeartbeatIntervalSeconds(Number(event.target.value))} value={heartbeatIntervalSeconds}>{[[60,"每分钟"],[300,"每 5 分钟"],[900,"每 15 分钟"],[1800,"每 30 分钟"],[3600,"每小时"]].map(([value,label]) => <option key={value} value={value}>{label}</option>)}</select></label>
            <label>巡检要求<textarea maxLength={8_000} onChange={(event) => setHeartbeatPrompt(event.target.value)} placeholder="例如：检查阻塞和需要主动推进的工作" rows={3} value={heartbeatPrompt} /></label>
          </>}
        </div>
        <footer><button disabled={saving} onClick={onCancel} type="button">取消</button><button className="primary-button" disabled={!valid || saving} type="submit">{saving ? "正在保存…" : existing ? "保存" : "创建"}</button></footer>
      </form>
    </div>
  );
}

function Toggle({ checked, label, note, onChange }: { checked: boolean; label: string; note: string; onChange(value: boolean): void }) {
  return <label className="agent-editor__toggle"><input checked={checked} onChange={(event) => onChange(event.target.checked)} type="checkbox" /><span><strong>{label}</strong><small>{note}</small></span></label>;
}

function Pagination({
  page,
  pageCount,
  pageSize,
  total,
  onPageChange,
  onPageSizeChange,
}: {
  page: number;
  pageCount: number;
  pageSize: number;
  total: number;
  onPageChange(page: number): void;
  onPageSizeChange(size: number): void;
}) {
  const start = total === 0 ? 0 : page * pageSize + 1;
  const end = Math.min(total, start + pageSize - 1);
  return (
    <footer className="agent-pagination">
      <span>{start}–{end} / {total}</span>
      <div>
        <label>
          <span className="sr-only">每页</span>
          <select aria-label="每页" onChange={(event) => onPageSizeChange(Number(event.target.value))} value={pageSize}>
            {[10, 20, 50].map((size) => <option key={size} value={size}>{size} / 页</option>)}
          </select>
        </label>
        <button aria-label="上一页" disabled={page === 0} onClick={() => onPageChange(page - 1)} type="button">‹</button>
        <span>{page + 1} / {pageCount}</span>
        <button aria-label="下一页" disabled={page + 1 >= pageCount} onClick={() => onPageChange(page + 1)} type="button">›</button>
      </div>
    </footer>
  );
}
