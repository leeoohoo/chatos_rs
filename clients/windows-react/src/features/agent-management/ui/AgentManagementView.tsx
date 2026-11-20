import { useEffect, useMemo, useRef, useState } from "react";
import type { AskUserPort } from "../../ask-user/ports/AskUserPort";
import type { ConversationAttachmentPort } from "../../conversation-attachments/ports/ConversationAttachmentPort";
import type { ConversationComposerPort } from "../../conversation-composer/ports/ConversationComposerPort";
import type { ConversationHistoryPort } from "../../conversation-history/ports/ConversationHistoryPort";
import { ConversationHistoryView } from "../../conversation-history/ui/ConversationHistoryView";
import type { ConversationRealtimePort } from "../../conversation-realtime/ports/ConversationRealtimePort";
import type { ConversationRuntimeSettingsPort } from "../../conversation-runtime-settings/ports/ConversationRuntimeSettingsPort";
import type { RunInspectorPort } from "../../run-inspector/ports/RunInspectorPort";
import { useAgentManagementStore } from "../application/agentManagementStore";
import { useAgentTeamStore } from "../application/agentTeamStore";
import {
  agentProfessionLabels,
  type AgentProfileEditorCatalog,
  type AgentProfileEditorData,
  type AgentProfileSummary,
  type PreparedAgentConversation,
} from "../domain/agentManagement";
import type { AgentManagementPort } from "../ports/AgentManagementPort";
import {
  AgentConversationActivity,
  AgentProfileActivity,
  AgentStaffingProposalPanel,
} from "./AgentActivityViews";
import { AgentSkillManagementSheet } from "./AgentSkillManagementSheet";
import { AgentCard, AgentProfileEditor } from "./AgentProfileViews";
import { AgentTeamCatalog } from "./AgentTeamCatalog";
import {
  AgentTeamMemberSheet,
  CreateAgentTeamSheet,
  InviteAgentToTeamSheet,
} from "./AgentTeamSheets";
import { Pagination } from "./AgentManagementUiPrimitives";
import type { ChatOSLanguage } from "../../general-settings/domain/generalSettings";
import { LocalAgentDocuments } from "./LocalAgentDocuments";
import "./agentManagement.css";

export { TeamSystemMessage } from "./AgentTeamMessagePresentation";

export {
  isCancellableTeamRunDetailStatus,
  shouldPollTeamRunDetail,
  teamRunDetailStatusLabel,
} from "./agentTeamRunPresentation";

export function AgentManagementView({
  ownerUserId,
  port,
  historyPort,
  attachmentPort,
  realtimePort,
  runtimeSettingsPort,
  runInspectorPort,
  askUserPort,
  contextLanguage = "zh-CN",
  onContextLanguageChange = async () => undefined,
  onOpenRequirementSurveys = () => undefined,
  navigationTarget = null,
  onNavigationHandled = () => undefined,
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
  contextLanguage?: ChatOSLanguage;
  onContextLanguageChange?(language: ChatOSLanguage): Promise<void> | void;
  onOpenRequirementSurveys?(): void;
  navigationTarget?: { roomId: string; todoId: string; requestId: number } | null;
  onNavigationHandled?(requestId: number): void;
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
  const teamOwnerUserId = useAgentTeamStore((state) => state.ownerUserId);
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
  const [surface, setSurface] = useState<"agents" | "documents" | "teams">("agents");
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
  const [documentsRefreshSequence, setDocumentsRefreshSequence] = useState(0);
  const [editingTeamMemberId, setEditingTeamMemberId] = useState<string | null>(null);
  const [navigationError, setNavigationError] = useState<string | null>(null);
  const startedNavigationRequest = useRef<number | null>(null);
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
    if (!navigationTarget || startedNavigationRequest.current === navigationTarget.requestId) return;
    setSurface("teams");
    setDirectConversation(null);
    setNavigationError(null);
    if (!port.listTeams || !port.loadTeam) {
      startedNavigationRequest.current = navigationTarget.requestId;
      setNavigationError("当前客户端未提供团队任务导航能力。");
      onNavigationHandled(navigationTarget.requestId);
      return;
    }
    if (teamOwnerUserId !== ownerUserId || isLoadingTeams) return;
    if (!teams.some((team) => team.id === navigationTarget.roomId)) {
      startedNavigationRequest.current = navigationTarget.requestId;
      setNavigationError("无法定位来源团队；该团队可能已被删除或不属于当前账号。");
      onNavigationHandled(navigationTarget.requestId);
      return;
    }
    startedNavigationRequest.current = navigationTarget.requestId;
    setEditingTeamMemberId(null);
    void selectTeam(port, navigationTarget.roomId).then(() => {
      const state = useAgentTeamStore.getState();
      if (state.detail?.team.id !== navigationTarget.roomId) {
        setNavigationError("来源团队暂时无法打开，请刷新后重试。");
        onNavigationHandled(navigationTarget.requestId);
      }
    });
  }, [
    isLoadingTeams,
    navigationTarget,
    onNavigationHandled,
    ownerUserId,
    port,
    selectTeam,
    teamOwnerUserId,
    teams,
  ]);

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
          <AgentStaffingProposalPanel
            ownerUserId={ownerUserId}
            port={port}
            profiles={profiles}
            sourceRoomId={directConversation.roomId}
            teams={teams}
            onResolved={async () => {
              await refresh(port);
              setActivityRefreshSequence((current) => current + 1);
            }}
          />
          <ConversationHistoryView
            agentBound
            assistantName={directConversation.title}
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
      <div className="agent-management__workspace">
        <aside className="agent-management__navigation" aria-label="Agent 工作区">
          <div className="agent-management__navigation-heading">
            <span aria-hidden="true">✦</span>
            <div><strong>Agent</strong><small>私聊与项目团队</small></div>
            <button
              aria-label="创建 Agent 团队"
              disabled={!port.createTeam || projects.length === 0 || isCreatingTeam}
              onClick={() => setShowsTeamCreator(true)}
              type="button"
            >＋</button>
          </div>
          <div className="agent-management__navigation-group">
            <span>工作区</span>
            <button aria-current={surface === "agents" ? "page" : undefined} className={surface === "agents" ? "is-active" : ""} onClick={() => setSurface("agents")} type="button">▣ <b>Agent 管理</b><small data-label={`${profiles.length} 个已创建 Agent`} /></button>
            <button aria-current={surface === "documents" ? "page" : undefined} className={surface === "documents" ? "is-active" : ""} onClick={() => setSurface("documents")} type="button">⌕ <b>本地 Agent 文档</b><small>跨设备发现与预览</small></button>
          </div>
          <div className="agent-management__navigation-group">
            <span>团队</span>
            {teams.map((team) => (
              <button key={team.id} aria-label={team.name} aria-current={surface === "teams" && selectedTeamId === team.id ? "page" : undefined} className={surface === "teams" && selectedTeamId === team.id ? "is-active" : ""} onClick={() => { setSurface("teams"); onSelectTeamFromNav(team.id); }} type="button">♣ <b data-label={team.name} /><small data-label={team.projectId} /></button>
            ))}
            {teams.length === 0 && <small className="agent-management__navigation-empty">还没有项目团队</small>}
          </div>
          <div className="agent-management__navigation-group agent-management__navigation-group--private">
            <span>私聊</span>
            {visibleProfiles.slice(0, 10).map((profile) => (
              <button key={profile.id} onClick={() => void openDirect(profile)} type="button"><span className="agent-management__navigation-avatar">{profile.name.slice(0, 1)}</span><b data-label={profile.name} /><small data-label={agentProfessionLabels[profile.professionKey] ?? "Agent"} /></button>
            ))}
            {visibleProfiles.length === 0 && <small className="agent-management__navigation-empty">还没有私聊</small>}
          </div>
        </aside>
        <section className="agent-management__panel">
        {navigationError && (
          <div className="agent-management__error" role="alert">
            <span>{navigationError}</span>
            <button onClick={() => setNavigationError(null)} type="button">关闭</button>
          </div>
        )}
        {!(surface === "teams" && selectedTeamId) && <header className="agent-management__toolbar">
          <div>
            <h2>{surface === "documents" ? "本地 Agent 文档" : surface === "teams" ? "项目 Agent 团队" : "Agent 管理"}</h2>
            <p>{surface === "documents" ? "显示当前账户 Local Agent Host 保存的 Markdown 文档。" : surface === "teams" ? "按项目组织 Agent 团队。" : "管理 Agent、模型和权限。"}</p>
          </div>
          <div className="agent-management__actions">
            {surface === "agents" ? <>
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
            </> : surface === "documents" ? <button aria-label="刷新本地 Agent 文档" onClick={() => setDocumentsRefreshSequence((value) => value + 1)} type="button">↻ 刷新</button> : <>
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
        </header>}

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

        {surface === "documents" ? (
          <LocalAgentDocuments
            ownerUserId={ownerUserId}
            port={port}
            refreshSequence={documentsRefreshSequence}
          />
        ) : surface === "teams" ? (
          <AgentTeamCatalog
            attachmentPort={attachmentPort}
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
            navigationTarget={navigationTarget}
            onNavigationHandled={onNavigationHandled}
            teams={teams}
            onCreate={() => setShowsTeamCreator(true)}
            onInvite={() => setShowsAgentInvite(true)}
            onManageMember={setEditingTeamMemberId}
            onOpenRequirementSurveys={onOpenRequirementSurveys}
            onRetry={() => void refreshTeams(port)}
            onStaffingResolved={async (teamId) => {
              await Promise.all([refresh(port), refreshTeams(port)]);
              await selectTeam(port, teamId);
            }}
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
            <p>创建后可以直接私聊，也可以加入项目团队。</p>
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
      </div>
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
          initialLanguage={contextLanguage}
          ownerUserId={ownerUserId}
          port={port}
          onClose={() => setShowsSkillManagement(false)}
          onLanguageChange={onContextLanguageChange}
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
            const created = await createTeam(port, input);
            if (created) {
              setShowsTeamCreator(false);
              setSurface("teams");
            }
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

  function onSelectTeamFromNav(teamId: string) {
    setEditingTeamMemberId(null);
    void selectTeam(port, teamId);
  }
}
