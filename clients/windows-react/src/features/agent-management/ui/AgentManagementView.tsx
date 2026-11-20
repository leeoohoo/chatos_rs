import { useCallback, useEffect, useMemo, useRef, useState } from "react";
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
import { useAgentTeamRunStore } from "../application/agentTeamRunStore";
import { useAgentTeamTodoStore } from "../application/agentTeamTodoStore";
import {
  agentProfessionLabels,
  type AgentProfileEditorCatalog,
  type AgentProfileEditorData,
  type AgentProfileSummary,
  type AgentTeamSummary,
  type AgentTeamDetail,
  type AgentTeamRunSummary,
  type AgentProjectDashboardSnapshot,
  isLegacyAgentProjectManagerBrief,
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
import { AgentTeamAssetsView } from "./AgentTeamAssetsView";
import { AgentTeamConversationView } from "./AgentTeamConversationView";
import { AgentTeamRunsView } from "./AgentTeamRunsView";
import {
  AgentTeamMemberSheet,
  CreateAgentTeamSheet,
  InviteAgentToTeamSheet,
} from "./AgentTeamSheets";
import { AgentTeamTodosView, todoStatusLabel } from "./AgentTeamTodosView";
import { formatTeamMessageTime, Pagination, Row } from "./AgentManagementUiPrimitives";
import type { ChatOSLanguage } from "../../general-settings/domain/generalSettings";
import type { AgentArtifact } from "../domain/agentArtifact";
import { SafeMarkdownPreview } from "../../notepad/ui/SafeMarkdownPreview";
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

function LocalAgentDocuments({
  ownerUserId,
  port,
  refreshSequence,
}: {
  ownerUserId: string;
  port: AgentManagementPort;
  refreshSequence: number;
}) {
  const [artifacts, setArtifacts] = useState<AgentArtifact[]>([]);
  const [nextCursor, setNextCursor] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [loadingArtifactIds, setLoadingArtifactIds] = useState<Set<string>>(() => new Set());
  const [preview, setPreview] = useState<{ artifact: AgentArtifact; markdown: string } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const loadingRef = useRef(false);

  const load = useCallback(async (cursor: string | null, reset: boolean) => {
    if (loadingRef.current) return;
    if (!port.listArtifacts) {
      setError("当前客户端未提供 Agent 文档读取能力。");
      return;
    }
    loadingRef.current = true;
    setIsLoading(true);
    setError(null);
    try {
      const page = await port.listArtifacts(ownerUserId, 50, cursor);
      setArtifacts((current) => {
        if (reset) return page.artifacts;
        const known = new Set(current.map((artifact) => artifact.artifactId));
        return [...current, ...page.artifacts.filter((artifact) => !known.has(artifact.artifactId))];
      });
      setNextCursor(page.nextCursor);
    } catch (failure) {
      setError(failure instanceof Error ? failure.message : "本地 Agent 文档读取失败。");
    } finally {
      loadingRef.current = false;
      setIsLoading(false);
    }
  }, [ownerUserId, port]);

  useEffect(() => {
    setNotice(null);
    void load(null, true);
  }, [load, refreshSequence]);

  const withArtifactLoading = async (artifact: AgentArtifact, operation: () => Promise<void>) => {
    if (loadingArtifactIds.has(artifact.artifactId)) return;
    setLoadingArtifactIds((current) => new Set(current).add(artifact.artifactId));
    setError(null);
    setNotice(null);
    try {
      await operation();
    } catch (failure) {
      setError(failure instanceof Error ? failure.message : "本地文档操作失败。");
    } finally {
      setLoadingArtifactIds((current) => {
        const next = new Set(current);
        next.delete(artifact.artifactId);
        return next;
      });
    }
  };

  const openPreview = (artifact: AgentArtifact) => void withArtifactLoading(artifact, async () => {
    if (!port.previewArtifact) throw new Error("当前客户端未提供 Agent 文档预览能力。");
    const result = await port.previewArtifact(
      ownerUserId, artifact.artifactId, artifact.size, artifact.sha256,
    );
    if (result.artifactId !== artifact.artifactId) throw new Error("Agent 文档预览身份不一致。");
    setPreview({ artifact, markdown: result.markdown });
  });

  const saveArtifact = (artifact: AgentArtifact) => void withArtifactLoading(artifact, async () => {
    if (!port.saveArtifact) throw new Error("当前客户端未提供 Agent 文档另存能力。");
    const saved = await port.saveArtifact(
      ownerUserId, artifact.artifactId, artifact.name, artifact.size, artifact.sha256,
    );
    if (saved) setNotice(`“${artifact.name}”已保存。`);
  });

  return (
    <div className="agent-local-documents" aria-label="本地 Agent 文档">
      {error && <div className="agent-local-documents__error" role="alert"><span>{error}</span><button onClick={() => void load(null, true)} type="button">重试</button></div>}
      {notice && <div className="agent-local-documents__notice" role="status">{notice}</div>}
      {isLoading && artifacts.length === 0 ? (
        <div className="agent-local-documents__loading"><span className="mini-spinner" />正在读取本地文档…</div>
      ) : artifacts.length === 0 ? (
        <div className="agent-local-documents__empty">
          <span aria-hidden="true">▤⌕</span>
          <h2>还没有本地 Agent 文档</h2>
          <p>Agent 创建并由 Local Agent Host 保存的 Markdown 会出现在这里。</p>
        </div>
      ) : (
        <div className="agent-local-documents__list" role="list">
          {artifacts.map((artifact) => {
            const loading = loadingArtifactIds.has(artifact.artifactId);
            return (
              <article className="agent-local-document" key={artifact.artifactId} role="listitem">
                <span className="agent-local-document__icon" aria-hidden="true">▤</span>
                <div className="agent-local-document__copy">
                  <strong>{artifact.name}</strong>
                  <small>{formatArtifactSize(artifact.size)} · {formatArtifactDate(artifact.createdAtUnixMs)}</small>
                  <code>SHA-256 {artifact.sha256.slice(0, 12)}…</code>
                </div>
                <div className="agent-local-document__actions">
                  {loading && <span className="mini-spinner" aria-label="正在读取文档" />}
                  <button disabled={loading} onClick={() => openPreview(artifact)} type="button">◎ 预览</button>
                  <button aria-label={`另存为 ${artifact.name}`} disabled={loading} onClick={() => saveArtifact(artifact)} title="另存为…" type="button">⇩</button>
                </div>
              </article>
            );
          })}
          {nextCursor && <button className="agent-local-documents__more" disabled={isLoading} onClick={() => void load(nextCursor, false)} type="button">{isLoading ? "正在加载…" : "加载更多"}</button>}
        </div>
      )}
      <span className="agent-local-documents__mascot" aria-hidden="true">🦊</span>
      {preview && (
        <div className="agent-artifact-preview-backdrop" role="presentation">
          <section aria-label={`${preview.artifact.name} 文档预览`} aria-modal="true" className="agent-artifact-preview" role="dialog">
            <header>
              <div><h2>{preview.artifact.name}</h2><p>{formatArtifactSize(preview.artifact.size)} · Markdown 预览</p></div>
              <button aria-label="关闭文档预览" onClick={() => setPreview(null)} type="button">×</button>
            </header>
            <SafeMarkdownPreview markdown={preview.markdown} />
          </section>
        </div>
      )}
    </div>
  );
}

function formatArtifactSize(bytes: number) {
  if (bytes < 1_024) return `${bytes} B`;
  if (bytes < 1_024 * 1_024) return `${(bytes / 1_024).toFixed(bytes < 10 * 1_024 ? 1 : 0)} KB`;
  return `${(bytes / (1_024 * 1_024)).toFixed(1)} MB`;
}

function formatArtifactDate(unixMilliseconds: number) {
  return new Intl.DateTimeFormat("zh-CN", {
    year: "numeric", month: "short", day: "numeric", hour: "2-digit", minute: "2-digit",
  }).format(new Date(unixMilliseconds));
}

function AgentTeamCatalog({
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
