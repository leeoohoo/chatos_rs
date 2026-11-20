import { useEffect, useMemo, useState } from "react";
import { useAgentTeamAssetStore } from "../application/agentTeamAssetStore";
import type {
  AgentProfileSummary,
  AgentTeamAsset,
  AgentTeamAssetCategory,
  AgentTeamAssetRevision,
  AgentTeamDetail,
} from "../domain/agentManagement";
import type { AgentManagementPort } from "../ports/AgentManagementPort";
import { SafeMarkdownPreview } from "../../notepad/ui/SafeMarkdownPreview";
import { formatTeamMessageTime, Pagination } from "./AgentManagementUiPrimitives";

const teamAssetCategoryLabels: Record<AgentTeamAssetCategory, string> = {
  overview: "项目概览",
  current_progress: "当前进度",
  tech_stack: "技术栈",
  architecture: "架构",
  conventions: "规范",
  decision: "决策",
  reference: "参考资料",
};


export function AgentTeamAssetsView({
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
  const reviews = useAgentTeamAssetStore((state) => state.reviews);
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
      {reviews.length > 0 && (
        <section aria-label="待审核共享资产建议" className="agent-team-asset-reviews">
          <header>
            <div><strong>待审核共享资产建议</strong><span>{reviews.length}</span></div>
            <p>项目经理 Agent 写入团队资产或给出事实理由明确拒绝后，建议才算处理完成。</p>
          </header>
          <div>
            {reviews.map((review) => (
              <article key={review.id}>
                <div className="agent-team-asset-reviews__title">
                  <strong>{review.suggestion.title}</strong>
                  <em>{teamAssetCategoryLabels[review.suggestion.category]}</em>
                </div>
                <p>{review.suggestion.rationale}</p>
                <small>来源任务：{review.todoTitle} · 提交 Agent：{review.sourceAgentName}</small>
                <details>
                  <summary>查看建议正文</summary>
                  <SafeMarkdownPreview markdown={review.suggestion.markdown} />
                </details>
              </article>
            ))}
          </div>
        </section>
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
