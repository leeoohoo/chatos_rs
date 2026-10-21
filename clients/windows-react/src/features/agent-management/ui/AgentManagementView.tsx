import { useEffect, useMemo, useState } from "react";
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
} from "../domain/agentManagement";
import type { AgentManagementPort } from "../ports/AgentManagementPort";
import { AgentSkillManagementSheet } from "./AgentSkillManagementSheet";
import "./agentManagement.css";

export function AgentManagementView({
  ownerUserId,
  port,
}: {
  ownerUserId: string;
  port: AgentManagementPort;
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
  const [showsSkillManagement, setShowsSkillManagement] = useState(false);
  const [editor, setEditor] = useState<{
    catalog: AgentProfileEditorCatalog;
    data: AgentProfileEditorData | null;
  } | null>(null);
  const pageCount = Math.max(1, Math.ceil(profiles.length / pageSize));
  const visibleProfiles = useMemo(
    () => profiles.slice(page * pageSize, page * pageSize + pageSize),
    [page, pageSize, profiles],
  );

  useEffect(() => {
    setEditor(null);
    setPreparingId(null);
    void activate(port, ownerUserId).then(() => loadEditorCatalog(port, false));
    return reset;
  }, [activate, loadEditorCatalog, ownerUserId, port, reset]);

  useEffect(() => {
    setPage((current) => Math.min(current, pageCount - 1));
  }, [pageCount]);

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
                  key={profile.id}
                  preparing={preparingId === profile.id}
                  profile={profile}
                  modelLabel={editorCatalog
                    ? editorCatalog.models.find((model) => model.id === profile.modelConfigId)?.displayName
                      ?? `${profile.modelConfigId}（不可用）`
                    : profile.modelConfigId}
                  onEdit={() => void openEditor(profile)}
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

function AgentCard({
  profile,
  modelLabel,
  preparing,
  onEdit,
}: {
  profile: AgentProfileSummary;
  modelLabel: string;
  preparing: boolean;
  onEdit(): void;
}) {
  return (
    <article className="agent-card">
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
        <button disabled title="私聊将在 Agent Conversation 准入后开放" type="button">私聊</button>
        <button disabled={preparing} onClick={onEdit} type="button">{preparing ? "准备中…" : "编辑"}</button>
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
