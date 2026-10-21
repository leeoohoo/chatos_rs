import { useEffect, useMemo, useState } from "react";
import type { AskUserPort } from "../../ask-user/ports/AskUserPort";
import type { ConversationAttachmentPort } from "../../conversation-attachments/ports/ConversationAttachmentPort";
import type { ConversationComposerPort } from "../../conversation-composer/ports/ConversationComposerPort";
import type { ConversationHistoryPort } from "../../conversation-history/ports/ConversationHistoryPort";
import { ConversationHistoryView } from "../../conversation-history/ui/ConversationHistoryView";
import type { ConversationRealtimePort } from "../../conversation-realtime/ports/ConversationRealtimePort";
import type { ConversationRuntimeSettingsPort } from "../../conversation-runtime-settings/ports/ConversationRuntimeSettingsPort";
import type { RunInspectorPort } from "../../run-inspector/ports/RunInspectorPort";
import { useAgentManagementStore } from "../application/agentManagementStore";
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
}: {
  ownerUserId: string;
  port: AgentManagementPort;
  historyPort?: ConversationHistoryPort;
  attachmentPort?: ConversationAttachmentPort;
  realtimePort?: ConversationRealtimePort;
  runtimeSettingsPort?: ConversationRuntimeSettingsPort;
  runInspectorPort?: RunInspectorPort;
  askUserPort?: AskUserPort;
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
  const [page, setPage] = useState(0);
  const [pageSize, setPageSize] = useState(20);
  const [preparingId, setPreparingId] = useState<string | null>(null);
  const [preparingDirectId, setPreparingDirectId] = useState<string | null>(null);
  const [directConversation, setDirectConversation] = useState<PreparedAgentConversation | null>(null);
  const [directError, setDirectError] = useState<string | null>(null);
  const [activityRefreshSequence, setActivityRefreshSequence] = useState(0);
  const [selectedAgentId, setSelectedAgentId] = useState<string | null>(null);
  const [showsSkillManagement, setShowsSkillManagement] = useState(false);
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
    void activate(port, ownerUserId).then(() => loadEditorCatalog(port, false));
    return reset;
  }, [activate, loadEditorCatalog, ownerUserId, port, reset]);

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
            <p>管理 Agent、模型和权限。</p>
          </div>
          <div className="agent-management__actions">
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
          </div>
        </header>

        {error && !editor && (
          <div className="agent-management__error" role="alert">
            <span>{error}</span>
            <button onClick={() => void refresh(port)} type="button">重试</button>
          </div>
        )}

        {directError && (
          <div className="agent-management__error" role="alert">
            <span>{directError}</span>
            <button onClick={() => setDirectError(null)} type="button">关闭</button>
          </div>
        )}

        {isLoading && profiles.length === 0 ? (
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
