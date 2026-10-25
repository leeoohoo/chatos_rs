import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from "react";
import type { AskUserPort } from "../../ask-user/ports/AskUserPort";
import type { ConversationAttachmentPort } from "../../conversation-attachments/ports/ConversationAttachmentPort";
import {
  formatAttachmentSize,
  mergeAttachmentDrafts,
  type AttachmentImportResult,
  type ConversationAttachmentDraft,
} from "../../conversation-attachments/domain/conversationAttachment";
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
  type AgentTeamAssetRevision,
  type AgentTeamRunSummary,
  type AgentTeamTodo,
  type AgentTeamTodoProgress,
  type AgentProjectDashboardSnapshot,
  type CreateAgentTeamInput,
  type PreparedAgentConversation,
  type AgentStaffingProposal,
} from "../domain/agentManagement";
import type { AgentManagementPort } from "../ports/AgentManagementPort";
import { AgentSkillManagementSheet } from "./AgentSkillManagementSheet";
import type { ChatOSLanguage } from "../../general-settings/domain/generalSettings";
import type { AgentArtifact } from "../domain/agentArtifact";
import { SafeMarkdownPreview } from "../../notepad/ui/SafeMarkdownPreview";
import {
  removeTrailingAgentTeamMentionQuery,
  resolveAgentTeamMentionIds,
  trailingAgentTeamMentionQuery,
  visibleAgentTeamMentionCandidates,
} from "../domain/agentTeamMention";
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
  contextLanguage = "zh-CN",
  onContextLanguageChange = async () => undefined,
  onOpenRequirementSurveys = () => undefined,
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
          <p>{dashboard?.summary ?? "项目经理会在收到任务状态、调研和审批变化后维护阶段判断、风险与下一步。"}</p>
          {!!dashboard?.nextSteps.length && <div className="agent-team-dashboard-next"><strong>下一步</strong>{dashboard.nextSteps.slice(0, 5).map((step, index) => <span key={`${index}-${step}`}>{index + 1}. {step}</span>)}</div>}
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

function todoStatusLabel(status: AgentTeamTodo["status"]) {
  return ({ pending: "等待", in_progress: "执行中", blocked: "项目经理协调中", completed: "已完成", cancelled: "已取消" })[status];
}

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
  const error = useAgentTeamTodoStore((state) => state.error);
  const activate = useAgentTeamTodoStore((state) => state.activate);
  const refresh = useAgentTeamTodoStore((state) => state.refresh);
  const reset = useAgentTeamTodoStore((state) => state.reset);
  const [expandedTodoId, setExpandedTodoId] = useState<string | null>(null);
  const [resultTodo, setResultTodo] = useState<AgentTeamTodo | null>(null);
  const [page, setPage] = useState(0);
  const cycling = useRef(false);
  const pageSize = 10;
  const supportsTodos = Boolean(
    port.listTeamTodos && port.dispatchNextTeamDelivery,
  );

  useEffect(() => {
    setExpandedTodoId(null);
    setResultTodo(null);
    setPage(0);
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

  useEffect(() => {
    const lastPage = Math.max(0, Math.ceil(todos.length / pageSize) - 1);
    if (page > lastPage) setPage(lastPage);
  }, [page, todos.length]);

  if (!supportsTodos) {
    return (
      <div className="agent-team-todos__state is-error" role="alert">
        <strong>当前版本尚未连接团队任务</strong><span>请更新本机客户端后重试。</span>
      </div>
    );
  }

  const visibleTodos = todos.slice(page * pageSize, (page + 1) * pageSize);
  const deliveredCount = todos.filter((todo) => todo.status === "completed"
    && todo.executionPlan.builtinCapabilities.includes("project_write")).length;
  const summaryOnlyCount = todos.filter((todo) => todo.status === "completed"
    && !todo.executionPlan.builtinCapabilities.includes("project_write")).length;
  const reviewCount = todos.filter((todo) => todo.status === "blocked").length;
  const waitingCount = todos.filter((todo) => todo.status === "pending").length;
  const firstVisible = todos.length === 0 ? 0 : page * pageSize + 1;
  const lastVisible = Math.min(todos.length, (page + 1) * pageSize);
  const pageCount = Math.max(1, Math.ceil(todos.length / pageSize));

  return (
    <div className="agent-team-todos">
      {error && (
        <div className="agent-team-conversation__error" role="alert">
          <span>{error}</span><button onClick={() => void refresh(port)} type="button">刷新</button>
        </div>
      )}
      {isLoading && todos.length === 0 ? (
        <div className="agent-team-todos__state"><span className="mini-spinner" />正在读取团队任务…</div>
      ) : todos.length === 0 ? (
        <div className="agent-team-todos__empty">
          <span aria-hidden="true">☷</span>
          <strong>还没有团队任务</strong>
          <p>项目经理创建的任务会显示在这里。</p>
        </div>
      ) : (
        <div className="agent-team-todos__scroll">
          <section aria-label="团队交付概览" className="agent-team-todo-overview">
            <div><strong>{deliveredCount}</strong><span>项目文件交付</span></div>
            <div><strong>{summaryOnlyCount}</strong><span>管理 / 检查完成</span></div>
            <div><strong>{reviewCount}</strong><span>需人工处理</span></div>
            <div><strong>{waitingCount}</strong><span>待执行</span></div>
            <p>“交付”按已完成且通过 Host 项目写入证据校验的任务统计；计划交付、授权工具和执行总结不会被当作实际项目产出。</p>
          </section>
          <div className="agent-team-todos__list">
            {visibleTodos.map((todo) => (
              <AgentTeamTodoCard
                expanded={expandedTodoId === todo.id}
                key={todo.id}
                onOpenResult={() => setResultTodo(todo)}
                onToggle={() => setExpandedTodoId((current) => current === todo.id ? null : todo.id)}
                ownerUserId={ownerUserId}
                port={port}
                teamId={detail.team.id}
                todo={todo}
              />
            ))}
          </div>
          <nav aria-label="团队任务分页" className="agent-team-todos__pagination">
            <span>{firstVisible}–{lastVisible} / {todos.length}</span>
            <div>
              <button aria-label="上一页任务" disabled={page === 0} onClick={() => setPage((value) => value - 1)} type="button">‹</button>
              <span>{page + 1} / {pageCount}</span>
              <button aria-label="下一页任务" disabled={page + 1 >= pageCount} onClick={() => setPage((value) => value + 1)} type="button">›</button>
            </div>
          </nav>
        </div>
      )}
      {resultTodo && <AgentTeamTodoResultDialog onClose={() => setResultTodo(null)} todo={resultTodo} />}
    </div>
  );
}

function AgentTeamTodoCard({
  expanded,
  onOpenResult,
  onToggle,
  ownerUserId,
  port,
  teamId,
  todo,
}: {
  expanded: boolean;
  onOpenResult: () => void;
  onToggle: () => void;
  ownerUserId: string;
  port: AgentManagementPort;
  teamId: string;
  todo: AgentTeamTodo;
}) {
  const [progress, setProgress] = useState<AgentTeamTodoProgress[]>([]);
  const [progressError, setProgressError] = useState<string | null>(null);

  useEffect(() => {
    if (!port.listTeamTodoProgress || todo.status === "pending") {
      setProgress([]);
      setProgressError(null);
      return undefined;
    }
    let cancelled = false;
    const load = async () => {
      try {
        const next = await port.listTeamTodoProgress!(ownerUserId, teamId, todo.id, 100);
        if (!cancelled) {
          setProgress(next);
          setProgressError(null);
        }
      } catch (failure) {
        if (!cancelled) setProgressError(failure instanceof Error ? failure.message : "执行记录暂时不可用。");
      }
    };
    void load();
    const timer = todo.status === "in_progress"
      ? window.setInterval(() => void load(), 2_000)
      : null;
    return () => {
      cancelled = true;
      if (timer != null) window.clearInterval(timer);
    };
  }, [ownerUserId, port, teamId, todo.id, todo.status, todo.updatedAtUnixMs]);

  const latest = progress.at(-1) ?? null;
  const toolCount = todo.executionPlan.builtinCapabilities.reduce(
    (count, capability) => count + teamTodoCapability(capability).tools.length,
    0,
  );
  const hasProjectFiles = todo.status === "completed"
    && todo.executionPlan.builtinCapabilities.includes("project_write");
  const statusColor = todo.status === "blocked" ? "warning" : todo.status;
  return (
    <article className={`agent-team-todo-card is-${statusColor}`}>
      <div className="agent-team-todo-card__identity">
        <span aria-hidden="true" className="agent-team-todo-card__avatar">{todo.agentName.trim().slice(0, 1) || "A"}</span>
        <div>
          <div className="agent-team-todo-card__title-row">
            <strong>{todo.title}</strong>
            <span className={`agent-team-todo-card__status is-${statusColor}`}>{todoStatusIcon(todo.status)} {todoStatusLabel(todo.status)}</span>
          </div>
          <small>●&nbsp; 负责人 · {todo.agentName}</small>
          <p>{collapsedTeamTodoText(todo.executionContract.objective, expanded ? Number.POSITIVE_INFINITY : 220)}</p>
        </div>
      </div>
      {todo.status === "blocked" && (
        <div className="agent-team-todo-card__notice">
          <span>⚠ {collapsedTeamTodoText(todo.blockedReason || "任务已阻塞，但未记录原因。", expanded ? Number.POSITIVE_INFINITY : 180)}</span>
          <strong>项目经理协调中</strong>
        </div>
      )}
      {latest && !expanded && (
        <section className={`agent-team-todo-card__latest is-${latest.kind}`} aria-label="最新执行记录">
          <div>
            <strong>{todoProgressStageLabel(latest)}</strong>
            <p>{latest.detail}</p>
          </div>
          <time>{formatTeamMessageTime(latest.createdAtUnixMs)}</time>
          {progress.length > 1 && <button onClick={onToggle} type="button">查看全部 {progress.length} 条</button>}
        </section>
      )}
      <div className="agent-team-todo-card__metadata">
        {todo.executionContract.expectedOutputs.length > 0 && <span>◇ 计划 {todo.executionContract.expectedOutputs.length} 项交付</span>}
        {todo.executionContract.acceptanceCriteria.length > 0 && <span>◉ {todo.executionContract.acceptanceCriteria.length} 项验收标准</span>}
        {toolCount > 0 && <span>⌘ 已授权 {toolCount} 个工具</span>}
        {todo.executionPlan.plugins.length > 0 && <span>◇ {todo.executionPlan.plugins.length} 个插件</span>}
        {progress.length > 0 && <span>⊙ 实际记录 {progress.length} 次</span>}
        {hasProjectFiles && <span className="is-delivered">▣ 已记录项目文件交付</span>}
        {!hasProjectFiles && todo.result && <span className="is-summary">▤ {todo.executionPlan.builtinCapabilities.includes("project_write") ? "有总结，未记录文件提交" : "管理 / 检查结果"}</span>}
        <button aria-expanded={expanded} onClick={onToggle} type="button">⌄ {expanded ? "收起" : "查看详情"}</button>
      </div>
      {expanded && (
        <div className="agent-team-todo-card__details">
          {todo.executionContract.scope && <TeamTodoDetailBlock icon="⌖" title="范围"><p>{todo.executionContract.scope}</p></TeamTodoDetailBlock>}
          <TeamTodoStringList icon="◇" title="交付物" values={todo.executionContract.expectedOutputs} />
          <TeamTodoStringList icon="✓" title="验收条件" values={todo.executionContract.acceptanceCriteria} />
          <TeamTodoStringList icon="●" title="约束" values={todo.executionContract.constraints} />
          <TeamTodoAuthorization plan={todo.executionPlan} />
          {latest && (
            <TeamTodoDetailBlock icon="↻" title="执行记录">
              <ol className="agent-team-todo-progress-list">
                {[...progress].reverse().map((item) => (
                  <li key={item.sequence}>
                    <div><strong>{todoProgressStageLabel(item)}</strong><time>{formatTeamMessageTime(item.createdAtUnixMs)}</time></div>
                    <p>{item.detail}</p>
                  </li>
                ))}
              </ol>
            </TeamTodoDetailBlock>
          )}
          {progressError && <div className="agent-team-todo-progress__error">{progressError}</div>}
          {todo.result && (
            <section className={`agent-team-todo-card__result ${hasProjectFiles ? "is-delivered" : ""}`}>
              <div><strong>{hasProjectFiles ? "项目交付结果" : "管理 / 检查结果"}</strong><p>{todo.result}</p></div>
              <button onClick={onOpenResult} type="button">查看完整结果</button>
            </section>
          )}
        </div>
      )}
    </article>
  );
}

function TeamTodoDetailBlock({
  children,
  icon,
  title,
}: {
  children: ReactNode;
  icon: string;
  title: string;
}) {
  return (
    <section className="agent-team-todo-detail-block">
      <strong><span aria-hidden="true">{icon}</span>{title}</strong>
      {children}
    </section>
  );
}

function TeamTodoStringList({ icon, title, values }: { icon: string; title: string; values: string[] }) {
  if (values.length === 0) return null;
  return (
    <TeamTodoDetailBlock icon={icon} title={title}>
      <ul>{values.map((value, index) => <li key={`${index}-${value}`}><span aria-hidden="true" />{value}</li>)}</ul>
    </TeamTodoDetailBlock>
  );
}

function TeamTodoAuthorization({ plan }: { plan: AgentTeamTodo["executionPlan"] }) {
  const capabilities = plan.builtinCapabilities.map(teamTodoCapability);
  return (
    <TeamTodoDetailBlock icon="◆" title="任务授权">
      <div className="agent-team-todo-authorization__mode">
        <strong>执行模式</strong><span>{plan.requiresExecution ? "独立执行环境" : "仅协调，无独立执行"}</span>
      </div>
      {capabilities.length === 0 ? (
        <p className="agent-team-todo-authorization__empty">未授权项目文件或终端工具。</p>
      ) : (
        <div className="agent-team-todo-authorization__list">
          <small>内置工具</small>
          {capabilities.map((capability) => (
            <section key={capability.key}>
              <header><strong>{capability.name}</strong><code>{capability.key}</code></header>
              <p>{capability.detail}</p>
              <code>{capability.tools.join("  ·  ")}</code>
            </section>
          ))}
        </div>
      )}
      <div className="agent-team-todo-authorization__plugins">
        <small>插件</small>
        {plan.plugins.length === 0 ? <p>未授权额外插件。</p> : plan.plugins.map((plugin) => (
          <section key={plugin.pluginId}>
            <strong>{plugin.displayName}</strong><code>{plugin.pluginId}</code>
            {plugin.reason && <span>授权理由：{plugin.reason}</span>}
          </section>
        ))}
      </div>
      <footer>
        <code>授权快照：{plan.selectionRevision}</code>
        {plan.selectedAtUnixMs > 0 && <time> · {formatTeamMessageTime(plan.selectedAtUnixMs)}</time>}
      </footer>
      <p className="agent-team-todo-authorization__note">这里展示任务创建时锁定的执行授权；Agent 职业与项目规则不属于本任务单独授权。</p>
    </TeamTodoDetailBlock>
  );
}

function AgentTeamTodoResultDialog({ onClose, todo }: { onClose: () => void; todo: AgentTeamTodo }) {
  return (
    <div className="agent-editor-backdrop" onMouseDown={onClose} role="presentation">
      <section aria-label="团队任务完整结果" aria-modal="true" className="agent-team-todo-result-dialog" onMouseDown={(event) => event.stopPropagation()} role="dialog">
        <header>
          <span aria-hidden="true" className="agent-team-todo-card__avatar">{todo.agentName.trim().slice(0, 1) || "A"}</span>
          <div><h2>{todo.title}</h2><p>●&nbsp; 负责人 · {todo.agentName}</p></div>
          <button onClick={onClose} type="button">完成</button>
        </header>
        <div>
          <h3>▤ 执行结果</h3>
          <SafeMarkdownPreview markdown={todo.result} />
        </div>
      </section>
    </div>
  );
}

function collapsedTeamTodoText(value: string, maximumCharacters: number) {
  const normalized = value.trim().split(/\s+/u).join(" ");
  if (!Number.isFinite(maximumCharacters) || normalized.length <= maximumCharacters) return normalized;
  return `${normalized.slice(0, maximumCharacters).trimEnd()}…`;
}

function todoStatusIcon(status: AgentTeamTodo["status"]) {
  return ({ pending: "◷", in_progress: "↻", blocked: "⚠", completed: "●", cancelled: "⊗" })[status];
}

function teamTodoCapability(capability: AgentTeamTodo["executionPlan"]["builtinCapabilities"][number]) {
  return {
    project_read: {
      key: capability,
      name: "项目读取",
      detail: "仅在当前任务绑定的项目目录内读取、列目录和搜索。",
      tools: ["project_read", "project_list", "project_search"],
    },
    project_write: {
      key: capability,
      name: "项目写入",
      detail: "通过暂存与提交会话修改当前项目；提交仍受运行时审批约束。",
      tools: ["project_write", "project_apply_patch", "project_commit"],
    },
    terminal: {
      key: capability,
      name: "项目终端",
      detail: "在当前任务绑定的项目目录内执行和管理命令进程。",
      tools: ["terminal_execute", "terminal_poll", "terminal_write", "terminal_kill"],
    },
  }[capability];
}

function todoProgressStageLabel(progress: AgentTeamTodoProgress) {
  if (progress.stage.trim()) return progress.stage;
  return {
    started: "开始执行",
    progress: "执行进展",
    blocked: "执行阻塞",
    completed: "执行完成",
    cancelled: "已取消",
  }[progress.kind];
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

function AgentTeamAssetsView({
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
  const [page, setPage] = useState(0);
  const [pageSize, setPageSize] = useState(20);
  const pageCount = Math.max(1, Math.ceil(assets.length / pageSize));
  const visibleAssets = assets.slice(page * pageSize, (page + 1) * pageSize);
  const profileNames = useMemo(
    () => new Map(profiles.map((profile) => [profile.id, profile.name])),
    [profiles],
  );
  const supportsAssets = Boolean(
    port.listTeamAssets && port.saveTeamAsset && port.archiveTeamAsset && port.listTeamAssetRevisions,
  );

  useEffect(() => {
    setExpandedIds([]);
    setEditorAsset(null);
    setHistoryAsset(null);
    setPage(0);
    if (supportsAssets) void activate(port, ownerUserId, detail.team.id);
    else reset();
    return reset;
  }, [activate, detail.team.id, ownerUserId, port, reset, supportsAssets]);

  useEffect(() => {
    if (page >= pageCount) setPage(pageCount - 1);
  }, [page, pageCount]);

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
            <span aria-hidden="true" className="agent-team-assets__empty-icon">▤</span>
            <strong>还没有共享资产</strong><span>在这里维护项目背景、进度、技术栈和架构决策。</span>
          </div>
        ) : visibleAssets.map((asset) => {
          const expanded = expandedIds.includes(asset.id);
          return (
            <article key={asset.id}>
              <header>
                <button
                  aria-expanded={expanded}
                  aria-label={asset.title}
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
                <details className="agent-team-asset-menu">
                  <summary aria-label={`${asset.title}操作`}>•••</summary>
                  <div role="menu">
                    <button onClick={(event) => {
                      event.currentTarget.closest("details")?.removeAttribute("open");
                      setEditorAsset(asset);
                    }} role="menuitem" type="button">编辑</button>
                    <button onClick={(event) => {
                      event.currentTarget.closest("details")?.removeAttribute("open");
                      setHistoryAsset(asset);
                      void loadRevisions(port, asset);
                    }} role="menuitem" type="button">版本历史</button>
                    <button
                      className="is-danger"
                      disabled={mutationAssetId != null}
                      onClick={(event) => {
                        event.currentTarget.closest("details")?.removeAttribute("open");
                        void archive(port, asset);
                      }}
                      role="menuitem"
                      type="button"
                    >归档</button>
                  </div>
                </details>
              </header>
              {expanded && <div className="agent-team-asset-markdown"><SafeMarkdownPreview markdown={asset.markdown} /></div>}
            </article>
          );
        })}
        {assets.length > 0 && (
          <Pagination
            onPageChange={setPage}
            onPageSizeChange={(size) => { setPageSize(size); setPage(0); }}
            page={page}
            pageCount={pageCount}
            pageSize={pageSize}
            total={assets.length}
          />
        )}
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
          profileNames={profileNames}
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
          <h2>{asset ? "编辑共享资产" : "新建共享资产"}</h2>
          <div>
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
          </div>
        </header>
        <div className="agent-team-asset-editor__body">
          <label><span>分类</span><select value={category} onChange={(event) => setCategory(event.target.value as AgentTeamAssetCategory)}>
            {Object.entries(teamAssetCategoryLabels).map(([value, label]) => <option key={value} value={value}>{label}</option>)}
          </select></label>
          <label><span>标题</span><input maxLength={240} onChange={(event) => setTitle(event.target.value)} value={title} /></label>
          <label><span>Markdown</span><textarea maxLength={128_000} onChange={(event) => setMarkdown(event.target.value)} value={markdown} /></label>
          <section><strong>预览</strong><SafeMarkdownPreview markdown={markdown} /></section>
        </div>
      </section>
    </div>
  );
}

function AgentTeamAssetHistory({
  asset,
  isLoading,
  profileNames,
  revisions,
  onClose,
}: {
  asset: AgentTeamAsset;
  isLoading: boolean;
  profileNames: Map<string, string>;
  revisions: AgentTeamAssetRevision[];
  onClose(): void;
}) {
  const [selectedRevision, setSelectedRevision] = useState<number | null>(null);
  const [page, setPage] = useState(0);
  const [pageSize, setPageSize] = useState(20);
  const pageCount = Math.max(1, Math.ceil(revisions.length / pageSize));
  const visibleRevisions = revisions.slice(page * pageSize, (page + 1) * pageSize);
  const selected = revisions.find((revision) => revision.revision === selectedRevision) ?? revisions[0];
  useEffect(() => {
    if (page >= pageCount) setPage(pageCount - 1);
  }, [page, pageCount]);
  useEffect(() => {
    if (selected && !visibleRevisions.some((revision) => revision.revision === selected.revision)) {
      setSelectedRevision(visibleRevisions[0]?.revision ?? null);
    }
  }, [selected, visibleRevisions]);
  return (
    <div className="agent-editor-backdrop" role="presentation" onMouseDown={onClose}>
      <section aria-label="共享资产版本历史" aria-modal="true" className="agent-team-asset-history" onMouseDown={(event) => event.stopPropagation()} role="dialog">
        <header><div><h2>版本历史</h2><p>{asset.title}</p></div><button onClick={onClose} type="button">完成</button></header>
        <div className="agent-team-asset-history__body">
          {isLoading && revisions.length === 0 ? <div className="agent-team-assets__state"><span className="mini-spinner" />正在读取版本历史…</div> : revisions.length === 0 ? (
            <div className="agent-team-assets__state"><strong>没有版本记录</strong><span>保存资产后，历史版本会显示在这里。</span></div>
          ) : (
            <>
              <div className="agent-team-asset-history__sidebar">
                <nav aria-label="共享资产版本">
                  {visibleRevisions.map((revision) => <button aria-pressed={selected?.revision === revision.revision} key={revision.revision} onClick={() => setSelectedRevision(revision.revision)} type="button"><strong>r{revision.revision}{revision.revision === asset.revision ? " · 当前" : ""}</strong><span>{revision.title}</span><small>{formatTeamMessageTime(revision.createdAtUnixMs)}</small></button>)}
                </nav>
                <Pagination
                  onPageChange={(nextPage) => { setPage(nextPage); setSelectedRevision(revisions[nextPage * pageSize]?.revision ?? null); }}
                  onPageSizeChange={(size) => { setPageSize(size); setPage(0); setSelectedRevision(revisions[0]?.revision ?? null); }}
                  page={page}
                  pageCount={pageCount}
                  pageSize={pageSize}
                  total={revisions.length}
                />
              </div>
              <section>
                <header>
                  <strong>{selected?.title}</strong>
                  <span>{selected && `r${selected.revision} · ${selected.editorAgentId ? profileNames.get(selected.editorAgentId) ?? "Agent" : "你"} · ${formatTeamMessageTime(selected.createdAtUnixMs)}`}</span>
                </header>
                {selected && <SafeMarkdownPreview markdown={selected.markdown} />}
              </section>
            </>
          )}
        </div>
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
          <AgentTeamRunLaneSummary lane="manager" run={visibleRuns.find((run) => run.lane === "manager") ?? null} onInspect={setSelectedRunId} />
          <AgentTeamRunLaneSummary lane="executor" run={visibleRuns.find((run) => run.lane === "executor") ?? null} onInspect={setSelectedRunId} />
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
          return (
            <article key={run.runId}>
              <button
                aria-label={`查看${profileNames.get(run.agentId) ?? run.agentId}的运行详情`}
                disabled={!runInspectorPort}
                onClick={() => setSelectedRunId(run.runId)}
                type="button"
              >
                <header>
                  <strong>{profileNames.get(run.agentId) ?? run.agentId}</strong>
                  <span className={`agent-team-runs__lane is-${run.lane}`}>{run.lane === "manager" ? "通讯" : "任务执行"}</span>
                  <em className={`is-${teamRunStatusClass(run.runStatus)}`}>{teamRunCheckpointStatusLabel(run.runStatus)}</em>
                  <b aria-hidden="true">›</b>
                </header>
                <p>{teamRunTriggerLabel(run.triggerKind)} · {metrics
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
          onClose={() => setSelectedRunId(null)}
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
  onInspect(runId: string): void;
}) {
  return (
    <button disabled={!run} onClick={() => { if (run) onInspect(run.runId); }} type="button">
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

function AgentTeamRunDetailSheet({
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
    if (!detail || ![
      "ready", "running", "queued", "model_ready", "model_running",
      "waiting_tool_result", "continuation_ready", "retry_scheduled",
    ].includes(detail.run.status.toLowerCase())) return undefined;
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
          <div><h2>Run 检查器</h2><p>{agentName} · {run.lane === "manager" ? "通讯" : "任务执行"}</p></div>
          <button onClick={onClose} type="button">完成</button>
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
                  <Row label="状态" value={teamRunCheckpointStatusLabel(run.runStatus)} />
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
                <h3>运行事件</h3>
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
        {detail?.run.status.toLowerCase() === "needs_review" && <footer>
          {detail?.run.status.toLowerCase() === "needs_review" && agentPort.retryTeamRun && (
            <button
              className="is-danger"
              disabled={isRetrying}
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
        </footer>}
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

function teamRunCheckpointStatusLabel(status: AgentTeamRunSummary["runStatus"]) {
  return {
    ready: "等待开始",
    running: "运行中",
    succeeded: "已完成",
    failed: "失败",
    cancelled: "已取消",
  }[status];
}

function teamRunStatusClass(status: AgentTeamRunSummary["runStatus"]) {
  return status === "succeeded" ? "completed" : status;
}

function teamRunMemoryThreadId(teamId: string, run: AgentTeamRunSummary) {
  return run.lane === "executor"
    ? `agent-team-executor-${run.deliveryId}`
    : `agent-team-${teamId}-${run.agentId}`;
}

function formatRunElapsed(createdAtUnixMs: number, updatedAtUnixMs: number) {
  return `${(Math.max(0, updatedAtUnixMs - createdAtUnixMs) / 1_000).toFixed(1)} 秒`;
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
  attachmentPort,
  detail,
  ownerUserId,
  port,
  profiles,
}: {
  attachmentPort?: ConversationAttachmentPort;
  detail: AgentTeamDetail;
  ownerUserId: string;
  port: AgentManagementPort;
  profiles: AgentProfileSummary[];
}) {
  const snapshot = useAgentTeamConversationStore((state) => state.snapshot);
  const isLoading = useAgentTeamConversationStore((state) => state.isLoading);
  const isLoadingOlder = useAgentTeamConversationStore((state) => state.isLoadingOlder);
  const isSending = useAgentTeamConversationStore((state) => state.isSending);
  const isCycling = useAgentTeamConversationStore((state) => state.isCycling);
  const error = useAgentTeamConversationStore((state) => state.error);
  const activate = useAgentTeamConversationStore((state) => state.activate);
  const refresh = useAgentTeamConversationStore((state) => state.refresh);
  const loadOlder = useAgentTeamConversationStore((state) => state.loadOlder);
  const send = useAgentTeamConversationStore((state) => state.send);
  const reset = useAgentTeamConversationStore((state) => state.reset);
  const [content, setContent] = useState("");
  const [mentionedAgentIds, setMentionedAgentIds] = useState<string[]>([]);
  const [attachments, setAttachments] = useState<ConversationAttachmentDraft[]>([]);
  const [attachmentError, setAttachmentError] = useState<string | null>(null);
  const [isPickingAttachments, setIsPickingAttachments] = useState(false);
  const [mentionMenuOpen, setMentionMenuOpen] = useState(false);
  const [mentionError, setMentionError] = useState<string | null>(null);
  const [mentionSuggestionsSuppressed, setMentionSuggestionsSuppressed] = useState(false);
  const [highlightedMentionIndex, setHighlightedMentionIndex] = useState(0);
  const transcriptRef = useRef<HTMLDivElement | null>(null);
  const followsLatestRef = useRef(true);
  const positionedTeamRef = useRef<string | null>(null);
  const previousLastMessageRef = useRef<string | null>(null);
  const attachmentScope = useMemo(() => ({
    ownerUserId,
    conversationId: `agent-team-compose-${detail.team.id}`,
    draftGeneration: 0,
  }), [detail.team.id, ownerUserId]);
  const attachmentsRef = useRef(attachments);
  attachmentsRef.current = attachments;
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
  const profilesById = useMemo(
    () => new Map(profiles.map((profile) => [profile.id, profile])),
    [profiles],
  );
  const mentionCandidates = useMemo(() => memberProfiles.flatMap(({ member, profile }) => profile ? [{
    id: member.agentId,
    name: profile.name,
    subtitle: member.role,
  }] : []), [memberProfiles]);
  const visibleMentionCandidates = useMemo(
    () => (mentionSuggestionsSuppressed ? [] : visibleAgentTeamMentionCandidates(content, mentionCandidates))
      .filter((candidate) => !mentionedAgentIds.includes(candidate.id)),
    [content, mentionCandidates, mentionedAgentIds, mentionSuggestionsSuppressed],
  );
  const resolvedMention = useMemo(
    () => resolveAgentTeamMentionIds(content, mentionedAgentIds, mentionCandidates),
    [content, mentionCandidates, mentionedAgentIds],
  );
  const visibleSnapshot = snapshot?.teamId === detail.team.id ? snapshot : null;
  const hasActiveDelivery = visibleSnapshot?.deliveries.some(
    (delivery) => delivery.status === "pending" || delivery.status === "running",
  ) ?? false;
  const canRoute = resolvedMention.ids.length > 0 || detail.team.defaultAgentId != null;
  const canSend = supportsConversation && (content.trim().length > 0 || attachments.length > 0) && canRoute
    && resolvedMention.error == null && !isSending && !isCycling;

  const acceptAttachmentResult = useCallback((result: AttachmentImportResult) => {
    if (result.scope.ownerUserId !== attachmentScope.ownerUserId
      || result.scope.conversationId !== attachmentScope.conversationId
      || result.scope.draftGeneration !== attachmentScope.draftGeneration) return;
    setAttachments((current) => mergeAttachmentDrafts(result.attachments, current));
    setAttachmentError(result.errors[0] ?? null);
  }, [attachmentScope]);

  useEffect(() => {
    if (!attachmentPort) return undefined;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void attachmentPort.activateScope(attachmentScope);
    void attachmentPort.onStaged(acceptAttachmentResult).then((listener) => {
      if (disposed) listener();
      else unlisten = listener;
    });
    return () => {
      disposed = true;
      unlisten?.();
      const ids = attachmentsRef.current.map((attachment) => attachment.id);
      if (ids.length > 0) void attachmentPort.discard(attachmentScope, ids);
      void attachmentPort.deactivateScope(attachmentScope);
    };
  }, [attachmentPort, attachmentScope]);

  const sendMessage = () => {
    if (resolvedMention.error) {
      setMentionError(resolvedMention.error);
      return;
    }
    followsLatestRef.current = true;
    void send(
      port,
      content,
      resolvedMention.ids,
      attachments.map((attachment) => attachment.id),
    ).then((succeeded) => {
    if (succeeded) {
      setContent("");
      setMentionedAgentIds([]);
      setAttachments([]);
      setAttachmentError(null);
      setMentionError(null);
    }
    });
  };

  const selectMention = (agentId: string) => {
    setMentionedAgentIds((current) => current.includes(agentId) ? current : [...current, agentId]);
    setContent((current) => removeTrailingAgentTeamMentionQuery(current));
    setMentionError(null);
    setMentionSuggestionsSuppressed(false);
    setMentionMenuOpen(false);
  };

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

  useEffect(() => {
    setHighlightedMentionIndex(0);
    setMentionSuggestionsSuppressed(false);
    if (trailingAgentTeamMentionQuery(content)) setMentionMenuOpen(false);
  }, [content]);

  useLayoutEffect(() => {
    const transcript = transcriptRef.current;
    if (!transcript || !visibleSnapshot || positionedTeamRef.current === detail.team.id) return;
    positionedTeamRef.current = detail.team.id;
    transcript.scrollTop = transcript.scrollHeight;
    followsLatestRef.current = true;
  }, [detail.team.id, visibleSnapshot]);

  const lastMessageId = visibleSnapshot?.messages.at(-1)?.id ?? null;
  useLayoutEffect(() => {
    const transcript = transcriptRef.current;
    if (!transcript || !lastMessageId || previousLastMessageRef.current === lastMessageId) return;
    const hadPrevious = previousLastMessageRef.current != null;
    previousLastMessageRef.current = lastMessageId;
    if (!hadPrevious || followsLatestRef.current) transcript.scrollTop = transcript.scrollHeight;
  }, [lastMessageId]);

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
      <div
        aria-live="polite"
        className="agent-team-conversation__messages"
        onScroll={(event) => {
          const target = event.currentTarget;
          followsLatestRef.current = target.scrollHeight - target.scrollTop - target.clientHeight <= 32;
        }}
        ref={transcriptRef}
      >
        {visibleSnapshot?.hasOlder && (
          <button
            className="agent-team-conversation__older"
            disabled={isLoadingOlder}
            onClick={() => {
              const transcript = transcriptRef.current;
              const beforeHeight = transcript?.scrollHeight ?? 0;
              const beforeTop = transcript?.scrollTop ?? 0;
              void loadOlder(port).then((loaded) => {
                if (!loaded) return;
                window.requestAnimationFrame(() => {
                  const current = transcriptRef.current;
                  if (current) current.scrollTop = beforeTop + current.scrollHeight - beforeHeight;
                });
              });
            }}
            type="button"
          >{isLoadingOlder ? "正在加载…" : "↻ 加载更早消息"}</button>
        )}
        {isLoading && !visibleSnapshot ? (
          <div className="agent-team-conversation__state"><span className="mini-spinner" />正在读取团队消息…</div>
        ) : visibleSnapshot?.messages.length ? visibleSnapshot.messages.map((message) => {
          const senderName = message.senderKind === "human"
            ? "你"
            : message.senderKind === "system"
              ? "系统"
              : profileNames.get(message.senderId) ?? message.senderId;
          const deliveries = deliveriesByMessage.get(message.id) ?? [];
          const senderProfile = message.senderKind === "agent" ? profilesById.get(message.senderId) : null;
          if (message.senderKind === "system") {
            return <div className="agent-team-message agent-team-message--system" key={message.id}>{message.content}</div>;
          }
          return (
            <div className={`agent-team-message-row agent-team-message-row--${message.senderKind}`} key={message.id}>
              {message.senderKind === "agent" && <TeamMessageAvatar name={senderName} profile={senderProfile} />}
              <div className="agent-team-message-column">
                <div className="agent-team-message__meta">
                  <strong>{senderName}</strong>
                  <time>{formatTeamMessageTime(message.createdAtUnixMs)}</time>
                </div>
                <div className={`agent-team-message agent-team-message--${message.senderKind}`}>
                  {message.content && <SafeMarkdownPreview markdown={message.content} />}
                  {message.attachments.length > 0 && (
                    <div className="agent-team-message__attachments" aria-label="消息附件">
                      {message.attachments.map((attachment) => (
                        <span key={attachment.id}>
                          <b>{attachment.kind === "image" ? "▧" : attachment.kind === "audio" ? "♪" : "↗"}</b>
                          {attachment.name}<small>{formatAttachmentSize(attachment.size)}</small>
                        </span>
                      ))}
                    </div>
                  )}
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
                </div>
              </div>
              {message.senderKind === "human" && <TeamMessageAvatar human name="你" />}
            </div>
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
        {hasActiveDelivery && <div className="agent-team-composer__activity"><span className="mini-spinner" />Agent 正在处理群聊…</div>}
        <div aria-label="已选择的 Agent" className="agent-team-composer__mentions">
          {mentionedAgentIds.map((agentId) => (
            <button
              aria-label={`移除 @${profileNames.get(agentId) ?? agentId}`}
              key={agentId}
              onClick={() => setMentionedAgentIds((current) => current.filter((id) => id !== agentId))}
              type="button"
            >@{profileNames.get(agentId) ?? agentId}<span aria-hidden="true">×</span></button>
          ))}
          {mentionedAgentIds.length === 0 && detail.team.defaultAgentId && (
            <span>默认发送给 {profileNames.get(detail.team.defaultAgentId) ?? "默认 Agent"}</span>
          )}
        </div>
        {attachments.length > 0 && (
          <div className="agent-team-composer__attachments" aria-label="待发送附件">
            {attachments.map((attachment) => (
              <span key={attachment.id}>
                <b>{attachment.name}</b><small>{formatAttachmentSize(attachment.byteSize)}</small>
                <button
                  aria-label={`移除附件：${attachment.name}`}
                  onClick={() => {
                    if (attachmentPort) void attachmentPort.discard(attachmentScope, [attachment.id]);
                    setAttachments((current) => current.filter((item) => item.id !== attachment.id));
                  }}
                  type="button"
                >×</button>
              </span>
            ))}
          </div>
        )}
        {(attachmentError || mentionError) && (
          <div className="agent-team-conversation__error" role="alert">{attachmentError ?? mentionError}</div>
        )}
        <div className="agent-team-composer__surface">
          <div className="agent-team-composer__tools">
            <div
              className="agent-team-mention-picker"
              onBlur={(event) => {
                if (!event.currentTarget.contains(event.relatedTarget)) setMentionMenuOpen(false);
              }}
              onKeyDown={(event) => {
                if (event.key === "Escape") setMentionMenuOpen(false);
              }}
            >
              <button
                aria-expanded={mentionMenuOpen}
                aria-label="选择要 @ 的 Agent"
                disabled={mentionCandidates.length === 0}
                onClick={() => setMentionMenuOpen((open) => !open)}
                type="button"
              >@</button>
              {mentionMenuOpen && (
                <div className="agent-team-mention-picker__menu" role="menu">
                  {mentionCandidates.map((candidate) => {
                    const selected = mentionedAgentIds.includes(candidate.id);
                    return (
                      <button
                        key={candidate.id}
                        aria-checked={selected}
                        onClick={() => {
                          setMentionedAgentIds((current) => selected
                            ? current.filter((id) => id !== candidate.id)
                            : [...current, candidate.id]);
                          setMentionError(null);
                        }}
                        role="menuitemcheckbox"
                        type="button"
                      ><span>{selected ? "✓" : "○"}</span><strong>{candidate.name}</strong><small>{candidate.subtitle}</small></button>
                    );
                  })}
                </div>
              )}
            </div>
            {attachmentPort && (
            <button
              aria-label="添加团队附件"
              disabled={isPickingAttachments || isSending}
              onClick={() => {
                setIsPickingAttachments(true);
                setAttachmentError(null);
                void attachmentPort.pickFiles(attachmentScope)
                  .then(acceptAttachmentResult)
                  .catch((failure: unknown) => setAttachmentError(
                    failure instanceof Error ? failure.message : "附件暂时无法添加。",
                  ))
                  .finally(() => setIsPickingAttachments(false));
              }}
              type="button"
            >📎</button>
            )}
          </div>
          <div className="agent-team-composer__input">
            {visibleMentionCandidates.length > 0 && (
              <div className="agent-team-mention-suggestions" role="listbox">
                {visibleMentionCandidates.map((candidate, index) => (
                  <button
                    aria-selected={index === highlightedMentionIndex}
                    key={candidate.id}
                    onMouseDown={(event) => event.preventDefault()}
                    onClick={() => selectMention(candidate.id)}
                    role="option"
                    type="button"
                  ><TeamMessageAvatar compact name={candidate.name} profile={profilesById.get(candidate.id)} /><span><strong>@{candidate.name}</strong><small>{candidate.subtitle}</small></span></button>
                ))}
              </div>
            )}
            <textarea
              aria-label="团队消息"
              disabled={isSending}
              maxLength={1_000_000}
              onChange={(event) => { setContent(event.target.value); setMentionError(null); }}
              onKeyDown={(event) => {
                if (visibleMentionCandidates.length > 0 && ["ArrowDown", "ArrowUp"].includes(event.key)) {
                  event.preventDefault();
                  setHighlightedMentionIndex((current) => (
                    current + (event.key === "ArrowDown" ? 1 : -1) + visibleMentionCandidates.length
                  ) % visibleMentionCandidates.length);
                  return;
                }
                if (event.key === "Escape" && visibleMentionCandidates.length > 0) {
                  event.preventDefault();
                  setMentionSuggestionsSuppressed(true);
                  return;
                }
                if (event.key === "Enter" && !event.shiftKey && !event.nativeEvent.isComposing) {
                  event.preventDefault();
                  if (visibleMentionCandidates.length > 0) selectMention(
                    visibleMentionCandidates[Math.min(highlightedMentionIndex, visibleMentionCandidates.length - 1)]!.id,
                  );
                  else if (canSend) sendMessage();
                }
              }}
              placeholder={canRoute
                ? "输入消息；不选择 @ 时交给默认 Agent，也可添加图片、文档和长文本…"
                : "请先选择一个 Agent"}
              rows={2}
              value={content}
            />
            <button
              aria-label="发送团队消息"
              className="primary-button"
              disabled={!canSend}
              onClick={sendMessage}
              type="button"
            >{isSending ? <span className="mini-spinner" /> : "↑"}</button>
          </div>
        </div>
        {!canRoute && <small>当前团队没有默认 Agent，请先选择一个接收者。</small>}
      </div>
    </div>
  );
}

function TeamMessageAvatar({
  compact = false,
  human = false,
  name,
  profile,
}: {
  compact?: boolean;
  human?: boolean;
  name: string;
  profile?: AgentProfileSummary | null;
}) {
  const className = `agent-team-message-avatar${compact ? " is-compact" : ""}${human ? " is-human" : ""}`;
  if (profile?.avatarPreviewUrl) return <img alt="" className={className} src={profile.avatarPreviewUrl} />;
  return <span aria-hidden="true" className={className}>{human ? "你" : name.trim().slice(0, 1) || "A"}</span>;
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

function AgentStaffingProposalPanel({
  ownerUserId,
  sourceRoomId,
  port,
  profiles,
  teams,
  onResolved,
}: {
  ownerUserId: string;
  sourceRoomId: string;
  port: AgentManagementPort;
  profiles: AgentProfileSummary[];
  teams: AgentTeamSummary[];
  onResolved(): Promise<void>;
}) {
  const [proposals, setProposals] = useState<AgentStaffingProposal[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [resolvingId, setResolvingId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const load = port.listStaffingProposals;
    if (!load) {
      setIsLoading(false);
      return;
    }
    let cancelled = false;
    let timer: number | undefined;
    const refreshProposals = async () => {
      try {
        const next = await load(ownerUserId, sourceRoomId);
        if (cancelled) return;
        if (next.some((proposal) => proposal.sourceRoomId !== sourceRoomId)) {
          throw new Error("Agent 人员提案来源会话不一致。");
        }
        setProposals(next);
        setError(null);
      } catch (failure) {
        if (!cancelled) {
          setError(failure instanceof Error ? failure.message : "Agent 人员提案暂时无法读取。");
        }
      } finally {
        if (!cancelled) {
          setIsLoading(false);
          timer = window.setTimeout(() => void refreshProposals(), 2_000);
        }
      }
    };
    setIsLoading(true);
    setProposals([]);
    setError(null);
    void refreshProposals();
    return () => {
      cancelled = true;
      if (timer != null) window.clearTimeout(timer);
    };
  }, [ownerUserId, port, sourceRoomId]);

  if (!port.listStaffingProposals || !port.resolveStaffingProposal) return null;
  if (!isLoading && proposals.length === 0 && !error) return null;
  const profileNames = new Map(profiles.map((profile) => [profile.id, profile.name]));
  const teamNames = new Map(teams.map((team) => [team.id, team.name]));
  const resolve = async (proposal: AgentStaffingProposal, approved: boolean) => {
    if (resolvingId) return;
    setResolvingId(proposal.id);
    setError(null);
    try {
      const result = await port.resolveStaffingProposal!(
        ownerUserId,
        sourceRoomId,
        proposal.id,
        proposal.kind,
        approved,
      );
      if (result.proposalId !== proposal.id
        || result.sourceRoomId !== sourceRoomId
        || result.status !== (approved ? "approved" : "rejected")) {
        throw new Error("Agent 人员提案审批结果不一致，请刷新后重试。");
      }
      setProposals((current) => current.filter((candidate) => candidate.id !== proposal.id));
      await onResolved();
    } catch (failure) {
      setError(failure instanceof Error ? failure.message : "Agent 人员提案暂时无法处理。");
    } finally {
      setResolvingId(null);
    }
  };

  return (
    <section aria-label="待审批人员提案" className="agent-staffing-proposals" aria-live="polite">
      <header>
        <div>
          <strong>待审批人员提案</strong>
          <small>Agent 只能提交建议；批准后原生事务才会真正变更成员，并自动回执给发起者。</small>
        </div>
        {isLoading && <span aria-label="正在读取人员提案" className="mini-spinner" />}
      </header>
      {error && <div className="agent-staffing-proposals__error" role="alert">{error}</div>}
      {proposals.map((proposal) => {
        const proposer = profileNames.get(proposal.proposerAgentId) ?? "Agent";
        const targetAgent = proposal.targetAgentId
          ? profileNames.get(proposal.targetAgentId) ?? "现有 Agent"
          : null;
        const targetTeam = proposal.targetTeamId
          ? teamNames.get(proposal.targetTeamId) ?? "项目团队"
          : null;
        const title = proposal.kind === "agentCreation"
          ? `创建 Agent：${proposal.name}`
          : proposal.kind === "existingAgentMembership"
            ? `邀请 ${targetAgent} 加入 ${targetTeam}`
            : `将 ${targetAgent} 移出当前团队`;
        return (
          <article key={proposal.id}>
            <div className="agent-staffing-proposals__content">
              <div className="agent-staffing-proposals__title">
                <span>{proposal.kind === "agentRemoval" ? "移出" : "人员"}</span>
                <div>
                  <strong>{title}</strong>
                  <small>{proposer} 提交 · {formatAgentActivityTime(proposal.createdAtUnixMs)}</small>
                </div>
              </div>
              {proposal.role && <p><b>团队角色</b>{proposal.role}</p>}
              {proposal.responsibility && <p><b>职责</b>{proposal.responsibility}</p>}
              {proposal.rationale && <p><b>理由</b>{proposal.rationale}</p>}
              {proposal.reason && <p><b>移出理由</b>{proposal.reason}</p>}
              {proposal.handoffPlan && <p><b>交接计划</b>{proposal.handoffPlan}</p>}
              {proposal.professionKey && (
                <p><b>职业</b>{agentProfessionLabels[proposal.professionKey]}</p>
              )}
              {proposal.thinkingLevel && <p><b>思考等级</b>{proposal.thinkingLevel}</p>}
              {proposal.rolePrompt && (
                <details>
                  <summary>查看完整角色提示词</summary>
                  <pre>{proposal.rolePrompt}</pre>
                </details>
              )}
            </div>
            <footer>
              <button
                disabled={resolvingId != null}
                onClick={() => void resolve(proposal, false)}
                type="button"
              >拒绝</button>
              <button
                className="primary-button"
                disabled={resolvingId != null}
                onClick={() => void resolve(proposal, true)}
                type="button"
              >{resolvingId === proposal.id ? "正在处理…" : "批准"}</button>
            </footer>
          </article>
        );
      })}
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
