import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { MediaStudioPort } from "../../media-studio/ports/MediaStudioPort";
import { isStorySegmentReady, storyConfirmedImage } from "../domain/storyStudio";
import type { StoryMediaBatchKind, StoryMediaBatchPreparation, StoryMediaBatchSnapshot, StoryPlanningRun, StoryProject } from "../domain/storyStudio";
import { useStoryStudioStore } from "../application/storyStudioStore";
import type { StoryPlanningProgress } from "../application/storyStudioStore";
import type { StoryImageImportTarget, StoryStudioPort } from "../ports/StoryStudioPort";
import "./storyStudio.css";

interface StoryStudioViewProps {
  ownerUserId: string;
  mediaPort: MediaStudioPort;
  port: StoryStudioPort;
}

const EMPTY_STORY_BATCHES: StoryMediaBatchSnapshot[] = [];
const EMPTY_PLANNING_RUNS: StoryPlanningRun[] = [];

export function StoryStudioView({ ownerUserId, mediaPort, port }: StoryStudioViewProps) {
  const state = useStoryStudioStore();
  const [showCreate, setShowCreate] = useState(false);
  const [settingsProjectId, setSettingsProjectId] = useState<string | null>(null);

  useEffect(() => {
    void state.activate(port, mediaPort, ownerUserId);
    return () => state.deactivate(port);
  }, [mediaPort, ownerUserId, port, state.activate, state.deactivate]);

  const project = state.projects.find((value) => value.id === state.selectedProjectId) ?? null;
  return (
    <section className="story-studio" aria-label="剧情模式">
      <aside className="story-studio__projects">
        <header>
          <span><strong>剧情项目</strong><small>{state.projects.length} 个项目</small></span>
          <button aria-label="新建剧情项目" onClick={() => setShowCreate(true)} type="button">＋</button>
        </header>
        <div className="story-studio__project-list">
          {state.loading && state.projects.length === 0 ? (
            <div className="story-studio__loading"><span className="mini-spinner" />正在读取项目…</div>
          ) : state.projects.length === 0 ? (
            <div className="story-studio__empty-list"><span>▤</span><p>从一段原文开始制作完整剧情视频。</p></div>
          ) : state.projects.map((item) => (
            <button
              className={item.id === state.selectedProjectId ? "is-selected" : ""}
              key={item.id}
              onClick={() => void state.selectProject(port, item.id)}
              type="button"
            >
              <span aria-hidden="true">▤</span>
              <span><strong>{item.title}</strong><small>{projectStage(item)}</small></span>
            </button>
          ))}
        </div>
        {state.unreadableCount > 0 && <p className="story-studio__warning">{state.unreadableCount} 个损坏项目已隔离</p>}
      </aside>
      <main className="story-studio__main">
        {state.error && (
          <div className="story-studio__error" role="alert">
            <span>{state.error}</span>
            <button onClick={state.clearError} type="button">关闭</button>
          </div>
        )}
        {project ? <ProjectWorkspace mediaPort={mediaPort} onOpenSettings={() => setSettingsProjectId(project.id)} port={port} projectId={project.id} /> : (
          <div className="story-studio__welcome">
            <span aria-hidden="true">✦</span>
            <h2>把故事变成可控的镜头</h2>
            <p>剧情规划、角色场景素材、首尾帧和分段视频都保存在本机项目中。</p>
            <button onClick={() => setShowCreate(true)} type="button">新建剧情项目</button>
          </div>
        )}
      </main>
      {showCreate && <StoryProjectSheet onClose={() => setShowCreate(false)} port={port} />}
      {settingsProjectId && <StoryProjectSheet onClose={() => setSettingsProjectId(null)} port={port} projectId={settingsProjectId} />}
    </section>
  );
}

function ProjectWorkspace({ mediaPort, onOpenSettings, port, projectId }: { mediaPort: MediaStudioPort; onOpenSettings(): void; port: StoryStudioPort; projectId: string }) {
  const state = useStoryStudioStore();
  const project = state.projects.find((value) => value.id === projectId)!;
  const [source, setSource] = useState(project.source);
  const [style, setStyle] = useState(project.style);
  const [ratio, setRatio] = useState(project.ratio);
  const [ideas, setIdeas] = useState("");

  useEffect(() => {
    setSource(project.source);
    setStyle(project.style);
    setRatio(project.ratio);
  }, [project.id, project.ratio, project.source, project.style]);

  const sourceLocked = project.segments.length > 0;
  const dirty = source !== project.source || style !== project.style || ratio !== project.ratio;
  const planning = state.planningProgress[project.id] ?? null;
  const isPlanning = state.busy && planningIsActive(planning);
  const recoverable = !state.busy
    ? recoverablePlanningRun(state.planningRuns[project.id] ?? [], project.baseFingerprint)
    : null;
  const save = () => state.saveSource(port, { source, style, ratio });
  const planOutline = async () => {
    if (dirty) await useStoryStudioStore.getState().saveSource(port, { source, style, ratio });
    if (useStoryStudioStore.getState().error) return;
    await useStoryStudioStore.getState().startPlanning(port, "outline", [], ideas);
  };

  return (
    <div className="story-project">
      <header className="story-project__header">
        <div><p>STORY PROJECT</p><h2>{project.title}</h2><span>{project.description || "未填写项目说明"}</span></div>
        <div className="story-project__summary">
          <span><strong>{project.characters.length + project.scenes.length + project.props.length}</strong>素材</span>
          <span><strong>{project.segments.length}</strong>分段</span>
          <span><strong>{project.segments.filter((segment) => segment.video).length}</strong>成片</span>
          <button aria-label="剧情设置" disabled={state.busy} onClick={onOpenSettings} type="button">设置</button>
        </div>
      </header>
      <StoryPlanningHistoryPanel port={port} projectId={project.id} />
      {recoverable && <PlanningRecoveryBanner port={port} run={recoverable} />}
      {project.segments.length === 0 ? (
        <div className="story-source-workspace">
          <section className="story-source-card">
            <header><div><h3>故事原文</h3><p>保存后由规划 Agent 按 macOS 同一套规则拆分素材与镜头。</p></div><span>{source.length.toLocaleString()} / 80,000</span></header>
            <textarea
              aria-label="故事原文"
              disabled={state.busy || sourceLocked}
              maxLength={80_000}
              onChange={(event) => setSource(event.target.value)}
              placeholder="粘贴小说、脚本、分镜大纲或创意描述…"
              value={source}
            />
          </section>
          <aside className="story-source-settings">
            <h3>画面设定</h3>
            <label>整体风格<textarea disabled={state.busy} maxLength={2_000} onChange={(event) => setStyle(event.target.value)} value={style} /></label>
            <label>画面比例<select disabled={state.busy} onChange={(event) => setRatio(event.target.value)} value={ratio}>
              {["16:9", "9:16", "1:1", "4:3", "3:4", "21:9"].map((value) => <option key={value}>{value}</option>)}
            </select></label>
            <label>本次补充要求<textarea disabled={state.busy} maxLength={2_000} onChange={(event) => setIdeas(event.target.value)} placeholder="例如：保持克制的悬疑节奏" value={ideas} /></label>
            <button className="story-secondary-button" disabled={!dirty || state.busy} onClick={() => void save()} type="button">保存原文</button>
            {planning && <PlanningStatus progress={planning} />}
            <button className="story-primary-button" disabled={!source.trim() || state.busy} onClick={() => void planOutline()} type="button">
              {isPlanning ? <><span className="mini-spinner" />{planningActionLabel(planning)}</> : "生成剧情规划"}
            </button>
          </aside>
        </div>
      ) : <PlannedStoryWorkspace mediaPort={mediaPort} port={port} projectId={project.id} />}
    </div>
  );
}

function PlannedStoryWorkspace({ mediaPort, port, projectId }: { mediaPort: MediaStudioPort; port: StoryStudioPort; projectId: string }) {
  const state = useStoryStudioStore();
  const project = state.projects.find((value) => value.id === projectId)!;
  const [selectedSegment, setSelectedSegment] = useState<string | null>(project.segments[0]?.id ?? null);
  const segment = project.segments.find((value) => value.id === selectedSegment) ?? project.segments[0];
  const resources = [...project.characters, ...project.scenes, ...project.props];
  const planning = state.planningProgress[project.id] ?? null;
  const isRefining = state.busy && planning?.stage === "refine" && planningIsActive(planning);
  const [historyTarget, setHistoryTarget] = useState<StoryImageImportTarget | null>(null);
  const [retryTarget, setRetryTarget] = useState<StoryImageImportTarget | null>(null);
  const [showBatchPreview, setShowBatchPreview] = useState(false);
  const [deleteSegmentId, setDeleteSegmentId] = useState<string | null>(null);
  const hasUnresolvedJobs = resources.some((value) => value.media.generationAttemptId != null)
    || project.segments.some((value) => value.attempt != null
      || value.firstFrames.generationAttemptId != null
      || value.lastFrames.generationAttemptId != null);
  const structureLocked = state.busy
    || state.activeMediaTarget != null
    || hasUnresolvedJobs
    || project.segments.some((value) => value.video != null);
  const selectedIndex = segment
    ? project.segments.findIndex((value) => value.id === segment.id)
    : -1;
  const mutateStructure = async (
    operation: "add" | "delete" | "move",
    segmentId?: string,
    offset?: -1 | 1,
  ) => {
    const saved = await state.updateSegmentStructure(port, { operation, segmentId, offset });
    if (!saved) return;
    if (operation === "add") setSelectedSegment(saved.segments.at(-1)?.id ?? null);
    else if (operation === "delete") setSelectedSegment(saved.segments[0]?.id ?? null);
  };

  return (
    <div className="story-plan-workspace">
      {planning && <PlanningStatus progress={planning} />}
      <StoryMediaBatchPanel port={port} projectId={project.id} />
      <section className="story-resource-strip">
        <header><div><h3>角色与场景</h3><p>规划生成的素材事实源</p></div><div className="story-resource-strip__actions"><button disabled={state.busy} onClick={() => setShowBatchPreview(true)} type="button">批量制作</button><button disabled={state.busy || project.segments.length === 0} onClick={() => void state.startPlanning(port, "refine", project.segments.map((value) => value.id), "")} type="button">{isRefining ? planningActionLabel(planning) : "细化全部镜头"}</button></div></header>
        <div>{resources.map((resource) => {
          const image = storyConfirmedImage(resource.media);
          const latest = resource.media.images.at(-1) ?? null;
          const hasUnconfirmedLatest = latest != null && latest.id !== resource.media.confirmedImageId;
          const target = `resource:${resource.id}`;
          const displayed = hasUnconfirmedLatest ? latest : image;
          const importTarget = `import:resource:${resource.id}`;
          return <article key={resource.id}>{displayed?.previewUrl ? <img alt={resource.name} src={displayed.previewUrl} /> : <span>◇</span>}<strong>{resource.name}{hasUnconfirmedLatest ? " · 待确认" : ""}</strong><small>{resource.media.generationAttemptId ? "上次图片提交状态待核对" : resource.imagePrompt || ("description" in resource ? resource.description : "等待素材描述")}</small>{hasUnconfirmedLatest && <button disabled={state.activeMediaTarget != null} onClick={() => void state.confirmLatestResources(port, [resource.id])} type="button">使用最新版本</button>}<button disabled={state.activeMediaTarget != null || resource.media.generationAttemptId != null} onClick={() => void state.importLocalImage(port, { kind: "resource", resourceId: resource.id })} type="button">{state.activeMediaTarget === importTarget ? "正在导入…" : "导入本机图片"}</button><button disabled={state.activeMediaTarget != null || resource.media.generationAttemptId != null} onClick={() => setHistoryTarget({ kind: "resource", resourceId: resource.id })} type="button">从创作历史选择</button>{resource.media.generationAttemptId && <button className="is-warning" disabled={state.activeMediaTarget != null} onClick={() => setRetryTarget({ kind: "resource", resourceId: resource.id })} type="button">核对后允许重试…</button>}<button disabled={state.activeMediaTarget != null || resource.media.generationAttemptId != null} onClick={() => void state.generateAssetImage(port, resource.id, "")} type="button">{state.activeMediaTarget === target ? "生成中…" : image ? "再生成" : "生成素材"}</button></article>;
        })}</div>
      </section>
      <div className="story-segment-layout">
        <nav aria-label="剧情分段">
          <header><span><strong>全剧时间线</strong><small>{project.segments.length} 个分段</small></span><button aria-label="添加剧情分段" disabled={structureLocked || !project.source.trim()} onClick={() => void mutateStructure("add")} type="button">＋</button></header>
          <div>{project.segments.map((item, index) => <button className={item.id === segment?.id ? "is-selected" : ""} key={item.id} onClick={() => setSelectedSegment(item.id)} type="button"><span>{String(index + 1).padStart(2, "0")}</span><span><strong>{item.title}</strong><small>{item.seconds} 秒 · {segmentState(item)}</small></span></button>)}</div>
          {segment && <footer><button aria-label="上移分段" disabled={structureLocked || selectedIndex <= 0} onClick={() => void mutateStructure("move", segment.id, -1)} type="button">↑</button><button aria-label="下移分段" disabled={structureLocked || selectedIndex < 0 || selectedIndex + 1 >= project.segments.length} onClick={() => void mutateStructure("move", segment.id, 1)} type="button">↓</button><button aria-label="删除分段" className="is-destructive" disabled={structureLocked} onClick={() => setDeleteSegmentId(segment.id)} type="button">删除</button></footer>}
        </nav>
        {segment && <SegmentInspector onPickHistory={setHistoryTarget} onRetryImage={setRetryTarget} port={port} projectId={project.id} segmentId={segment.id} />}
      </div>
      {historyTarget && <StoryImageHistorySheet mediaPort={mediaPort} onClose={() => setHistoryTarget(null)} port={port} target={historyTarget} />}
      {retryTarget && <StoryImageRetrySheet onClose={() => setRetryTarget(null)} port={port} target={retryTarget} />}
      {showBatchPreview && <StoryMediaBatchPreviewSheet onClose={() => setShowBatchPreview(false)} port={port} project={project} />}
      {deleteSegmentId && <div className="story-sheet-backdrop" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget) setDeleteSegmentId(null); }}><section aria-modal="true" className="story-sheet" role="dialog"><header><div><span>!</span><div><h2>删除这个分段？</h2><p>删除后会重新检查全剧衔接。</p></div></div><button aria-label="关闭" onClick={() => setDeleteSegmentId(null)} type="button">×</button></header><div className="story-sheet__body"><p className="story-retry-warning">其他分段的详细镜头计划需要重新细化；已有素材文件仍会保留。</p></div><footer><button onClick={() => setDeleteSegmentId(null)} type="button">取消</button><button className="story-retry-confirm" disabled={state.busy} onClick={() => { const id = deleteSegmentId; setDeleteSegmentId(null); void mutateStructure("delete", id); }} type="button">删除分段</button></footer></section></div>}
    </div>
  );
}

function StoryMediaBatchPreviewSheet({ port, project, onClose }: {
  port: StoryStudioPort;
  project: StoryProject;
  onClose(): void;
}) {
  const ownerUserId = useStoryStudioStore((state) => state.ownerUserId);
  const presentationId = useStoryStudioStore((state) => state.presentationId);
  const busy = useStoryStudioStore((state) => state.busy);
  const startMediaBatch = useStoryStudioStore((state) => state.startMediaBatch);
  const [kind, setKind] = useState<StoryMediaBatchKind>("pipeline");
  const [eligible, setEligible] = useState<string[]>([]);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [preview, setPreview] = useState<StoryMediaBatchPreparation | null>(null);
  const [userIdeas, setUserIdeas] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const requestSequence = useRef(0);

  const requestPreview = useCallback(async (
    nextKind: StoryMediaBatchKind,
    targetIds?: string[],
  ) => {
    if (!ownerUserId || !presentationId || !project.baseFingerprint) return;
    if (targetIds?.length === 0) {
      requestSequence.current += 1;
      setSelected(new Set());
      setPreview(null);
      setError(null);
      setLoading(false);
      return;
    }
    const sequence = ++requestSequence.current;
    setLoading(true);
    setError(null);
    try {
      const value = await port.previewMediaBatch({
        ownerUserId,
        presentationId,
        projectId: project.id,
        baseFingerprint: project.baseFingerprint,
        kind: nextKind,
        targetIds,
        userIdeas,
      });
      if (requestSequence.current !== sequence) return;
      setEligible(value.eligibleTargetIds);
      setSelected(new Set(value.preparation?.requestedTargetIds ?? targetIds ?? value.eligibleTargetIds));
      setPreview(value.preparation);
      setLoading(false);
      setError(value.validationError);
    } catch (reason) {
      if (requestSequence.current !== sequence) return;
      setPreview(null);
      setLoading(false);
      setError(storyViewErrorMessage(reason, "批量制作计划暂时无法预览。"));
    }
  }, [ownerUserId, port, presentationId, project.baseFingerprint, project.id, userIdeas]);

  useEffect(() => {
    setEligible([]);
    setSelected(new Set());
    setPreview(null);
    void requestPreview(kind);
  }, [kind, requestPreview]);

  const choose = (targetId: string, checked: boolean) => {
    const next = new Set(selected);
    if (checked) next.add(targetId);
    else next.delete(targetId);
    setSelected(next);
    void requestPreview(kind, eligible.filter((id) => next.has(id)));
  };
  const resources = [...project.characters, ...project.scenes, ...project.props];
  const counts = preview?.steps.reduce<Record<string, number>>((result, step) => {
    result[step.kind] = (result[step.kind] ?? 0) + 1;
    return result;
  }, {}) ?? {};
  const videoSeconds = preview?.steps
    .filter((step) => step.kind === "videos")
    .reduce((total, step) => total + (project.segments.find((item) => item.id === step.targetId)?.seconds ?? 0), 0) ?? 0;
  const start = async () => {
    if (!preview) return;
    if (await startMediaBatch(port, preview, userIdeas)) onClose();
  };

  return (
    <div className="story-sheet-backdrop" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget) onClose(); }}>
      <section aria-modal="true" className="story-sheet story-batch-sheet" role="dialog">
        <header><div><span>▦</span><div><h2>按已保存描述批量制作</h2><p>预览由原生层按 macOS 的同一套规则生成；此步骤不会提交 AI 任务。</p></div></div><button aria-label="关闭" onClick={onClose} type="button">×</button></header>
        <div className="story-batch-sheet__body">
          <div className="story-batch-kinds" role="tablist" aria-label="批量制作环节">
            {BATCH_KINDS.map((item) => <button aria-selected={kind === item.id} className={kind === item.id ? "is-selected" : ""} key={item.id} onClick={() => setKind(item.id)} role="tab" type="button">{item.label}</button>)}
          </div>
          <label>统一补充要求<textarea maxLength={2_000} onChange={(event) => setUserIdeas(event.target.value)} placeholder="例如：整体更温暖、减少快速运镜，或突出某个角色" value={userIdeas} /></label>
          <div className="story-batch-target-heading"><span>只列出当前可执行、尚未完成的目标</span><div><button disabled={eligible.length === 0} onClick={() => void requestPreview(kind, eligible)} type="button">全选</button><button disabled={selected.size === 0} onClick={() => void requestPreview(kind, [])} type="button">清空</button></div></div>
          <div className="story-batch-targets">
            {loading && eligible.length === 0 ? <div className="story-studio__loading"><span className="mini-spinner" />正在核对可执行目标…</div> : eligible.length === 0 ? <p>没有就绪目标，请先完成文字计划或确认参考图片。</p> : eligible.map((targetId) => {
              const target = batchCandidate(project, kind, targetId);
              return <label key={targetId}><input checked={selected.has(targetId)} disabled={loading} onChange={(event) => choose(targetId, event.target.checked)} type="checkbox" /><span><strong>{target.name}</strong><small>{target.detail}</small></span></label>;
            })}
          </div>
          {kind === "pipeline" && <p className="story-batch-note">执行顺序：缺少的关联素材 → 每段首帧 → 需要的尾帧 → 所选视频。已有确认版本保持不变；已有未确认图片必须先由你选定。</p>}
          {preview && <section className="story-batch-summary"><header><strong>原生执行计划已验证</strong><span>{preview.executionMode === "directVideos" ? "独立顺序视频队列" : "可恢复持久化批次"}</span></header><div><span><strong>{preview.requestedTargetIds.length}</strong>目标</span><span><strong>{counts.assets ?? 0}</strong>素材图</span><span><strong>{(counts.frames ?? 0) + (counts.lastFrames ?? 0)}</strong>首尾帧</span><span><strong>{counts.videos ?? 0}</strong>视频</span><span><strong>{videoSeconds}</strong>视频秒数</span></div><ol>{preview.steps.map((step, index) => <li key={step.id}><span>{index + 1}</span>{batchStepLabel(step.kind)} · {batchTargetName(project, step.targetId)}</li>)}</ol></section>}
          {error && <p className="story-batch-error" role="alert">{error}</p>}
        </div>
        <footer><small>{preview?.executionMode === "directVideos" ? "分段视频必须进入独立顺序队列，不会混入持久化图片批次。" : "创建后会持久化为可恢复队列；本步骤只登记计划，不会伪造完成状态。"}</small><div><button onClick={onClose} type="button">取消</button>{preview && <button className="story-primary-button" disabled={busy || loading} onClick={() => void start()} type="button">{busy ? (preview.executionMode === "directVideos" ? "正在顺序生成…" : "正在创建队列…") : (preview.executionMode === "directVideos" ? "确认并顺序生成" : "确认并创建队列")}</button>}</div></footer>
      </section>
    </div>
  );
}

function StoryMediaBatchPanel({ port, projectId }: { port: StoryStudioPort; projectId: string }) {
  const storedBatches = useStoryStudioStore((state) => state.mediaBatches[projectId]);
  const batches = storedBatches ?? EMPTY_STORY_BATCHES;
  const unreadable = useStoryStudioStore((state) => state.mediaBatchHistoryUnreadable[projectId] ?? 0);
  const nextCursor = useStoryStudioStore((state) => state.mediaBatchHistoryNextCursor[projectId] ?? null);
  const busy = useStoryStudioStore((state) => state.busy);
  const abandon = useStoryStudioStore((state) => state.abandonMediaBatch);
  const resume = useStoryStudioStore((state) => state.resumeMediaBatch);
  const loadMore = useStoryStudioStore((state) => state.loadMoreMediaBatchHistory);
  const [showHistory, setShowHistory] = useState(false);
  const [loadingMore, setLoadingMore] = useState(false);
  const [confirmAbandon, setConfirmAbandon] = useState<StoryMediaBatchSnapshot | null>(null);
  const latest = batches[0];
  if (!latest && unreadable === 0) return null;
  const complete = latest?.steps.filter((step) => step.completed).length ?? 0;
  const unfinished = latest && !["completed", "abandoned"].includes(latest.status);
  const requestMore = async () => {
    setLoadingMore(true);
    await loadMore(port, projectId);
    setLoadingMore(false);
  };
  return <>
    <section className="story-batch-panel">
      <span aria-hidden="true">{latest?.status === "completed" ? "✓" : "▦"}</span>
      <div>
        <header><strong>制作队列</strong>{latest && <small>{batchStatusLabel(latest.status)}</small>}</header>
        {latest ? <><progress max={Math.max(latest.steps.length, 1)} value={complete} /><p>已完成 {complete} / {latest.steps.length} 个步骤 · {batchKindLabel(latest.kind)}</p>{latest.error && <em>{latest.error}</em>}{latest.events.length > 0 && <ol className="story-batch-events">{latest.events.slice(-3).reverse().map((event) => <li key={event.id}>{event.detail}</li>)}</ol>}</> : <p>没有可读取的制作记录。</p>}
        {unreadable > 0 && <em>{unreadable} 条损坏记录已隔离</em>}
      </div>
      <div className="story-batch-actions">
        <button onClick={() => setShowHistory(true)} type="button">查看历史</button>
        {unfinished && <button disabled={busy} onClick={() => void resume(port, projectId, latest.id)} type="button">{busy && latest.status === "running" ? "执行中…" : "恢复批次"}</button>}
        {unfinished && <button className="story-danger-button" disabled={busy} onClick={() => setConfirmAbandon(latest)} type="button">核对后结束</button>}
      </div>
    </section>
    {showHistory && <div className="story-sheet-backdrop" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget) setShowHistory(false); }}>
      <section aria-modal="true" className="story-sheet story-batch-history-sheet" role="dialog">
        <header><div><span>▦</span><div><h2>制作历史</h2><p>查看每个原生批次的检查点、最近事件与恢复状态。</p></div></div><button aria-label="关闭" onClick={() => setShowHistory(false)} type="button">×</button></header>
        <div className="story-batch-history-list">
          {batches.map((batch) => {
            const completed = batch.steps.filter((step) => step.completed).length;
            const canContinue = !["completed", "abandoned"].includes(batch.status);
            return <article key={batch.id}>
              <header><div><strong>{batchKindLabel(batch.kind)}</strong><small>{new Date(batch.consentAtUnixMs).toLocaleString()}</small></div><span>{batchStatusLabel(batch.status)}</span></header>
              <progress max={Math.max(batch.steps.length, 1)} value={completed} />
              <p>已完成 {completed} / {batch.steps.length} 个步骤</p>
              {batch.error && <em>{batch.error}</em>}
              <ol>{batch.events.slice(-50).reverse().map((event) => <li key={event.id}><time>{new Date(event.dateUnixMs).toLocaleTimeString()}</time><span>{event.detail}</span></li>)}</ol>
              {canContinue && <footer><button disabled={busy} onClick={() => void resume(port, projectId, batch.id)} type="button">恢复批次</button><button className="story-danger-button" disabled={busy} onClick={() => setConfirmAbandon(batch)} type="button">核对后结束</button></footer>}
            </article>;
          })}
          {unreadable > 0 && <p className="story-batch-history-warning">{unreadable} 条损坏记录已安全隔离，未用于恢复。</p>}
        </div>
        <footer><button disabled={!nextCursor || loadingMore} onClick={() => void requestMore()} type="button">{loadingMore ? "正在读取…" : nextCursor ? "加载更早记录" : "没有更早记录"}</button><button onClick={() => setShowHistory(false)} type="button">完成</button></footer>
      </section>
    </div>}
    {confirmAbandon && <div className="story-sheet-backdrop" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget) setConfirmAbandon(null); }}><section aria-modal="true" className="story-sheet" role="dialog"><header><div><span>!</span><div><h2>已核实原生成任务？</h2><p>结束旧批次前必须核对服务商记录，避免重复提交和重复计费。</p></div></div><button aria-label="关闭" onClick={() => setConfirmAbandon(null)} type="button">×</button></header><div className="story-sheet__body"><p className="story-retry-warning">已有图片、视频和任务记录会保留。图片提交结果不明时，请先确认失败或未创建；结束后重新生成可能再次计费。</p></div><footer><button onClick={() => setConfirmAbandon(null)} type="button">取消</button><button className="story-retry-confirm" disabled={busy} onClick={() => { const batch = confirmAbandon; setConfirmAbandon(null); void abandon(port, projectId, batch.id); }} type="button">已核对，结束旧批次</button></footer></section></div>}
  </>;
}

function batchStatusLabel(status: ReturnType<typeof useStoryStudioStore.getState>["mediaBatches"][string][number]["status"]) {
  if (status === "completed") return "已完成";
  if (status === "abandoned") return "已结束";
  if (status === "needsReview") return "需要核对";
  if (status === "running") return "执行中";
  if (status === "ready") return "已创建";
  return "已暂停";
}

function batchKindLabel(kind: Exclude<StoryMediaBatchKind, "videos">) {
  if (kind === "assets") return "素材图片";
  if (kind === "frames") return "分段首帧";
  if (kind === "lastFrames") return "分段尾帧";
  return "一键全流程";
}

const BATCH_KINDS: { id: StoryMediaBatchKind; label: string }[] = [
  { id: "pipeline", label: "一键全流程" },
  { id: "assets", label: "素材图片" },
  { id: "frames", label: "分段首帧" },
  { id: "lastFrames", label: "分段尾帧" },
  { id: "videos", label: "分段视频" },
];

function batchStepLabel(kind: Exclude<StoryMediaBatchKind, "pipeline">) {
  if (kind === "assets") return "素材图片";
  if (kind === "frames") return "分段首帧";
  if (kind === "lastFrames") return "分段尾帧";
  return "分段视频";
}

function batchTargetName(project: StoryProject, targetId: string) {
  return [...project.characters, ...project.scenes, ...project.props]
    .find((item) => item.id === targetId)?.name
    ?? project.segments.find((item) => item.id === targetId)?.title
    ?? targetId;
}

function batchCandidate(project: StoryProject, kind: StoryMediaBatchKind, targetId: string) {
  if (kind === "assets") {
    const resource = [...project.characters, ...project.scenes, ...project.props]
      .find((item) => item.id === targetId);
    return {
      name: resource?.name ?? targetId,
      detail: resource?.imagePrompt || "已保存的素材描述",
    };
  }
  const segment = project.segments.find((item) => item.id === targetId);
  return {
    name: segment?.title ?? targetId,
    detail: segment?.synopsis || "已保存的分段描述",
  };
}

function storyViewErrorMessage(error: unknown, fallback: string) {
  return error instanceof Error && error.message.trim() ? error.message : fallback;
}

function SegmentInspector({ port, projectId, segmentId, onPickHistory, onRetryImage }: { port: StoryStudioPort; projectId: string; segmentId: string; onPickHistory(target: StoryImageImportTarget): void; onRetryImage(target: StoryImageImportTarget): void }) {
  const state = useStoryStudioStore();
  const project = state.projects.find((value) => value.id === projectId)!;
  const segment = project.segments.find((value) => value.id === segmentId)!;
  const first = storyConfirmedImage(segment.firstFrames);
  const last = storyConfirmedImage(segment.lastFrames);
  const latestFirst = segment.firstFrames.images.at(-1) ?? null;
  const latestLast = segment.lastFrames.images.at(-1) ?? null;
  const hasUnconfirmedFirst = latestFirst != null && latestFirst.id !== segment.firstFrames.confirmedImageId;
  const hasUnconfirmedLast = latestLast != null && latestLast.id !== segment.lastFrames.confirmedImageId;
  const progress = state.videoProgress[segment.id];
  const segmentIndex = project.segments.findIndex((value) => value.id === segment.id);
  const previousVideoAvailable = segmentIndex > 0 && project.segments[segmentIndex - 1]?.video != null;
  const referenceResourceIds = [
    ...segment.characterIds,
    ...segment.sceneIds,
    ...segment.propIds,
  ].filter((id) => {
    const resource = [...project.characters, ...project.scenes, ...project.props]
      .find((value) => value.id === id);
    return resource?.media.confirmedImageId != null;
  });
  const [userIdeas, setUserIdeas] = useState("");
  const [size, setSize] = useState<"768P" | "2K">("768P");
  const [showVideoRetry, setShowVideoRetry] = useState(false);
  return (
    <article className="story-segment-inspector">
      <header><div><span>{segment.kind === "transition" ? "转场片段" : "剧情片段"}</span><h3>{segment.title}</h3><p>{segment.synopsis}</p></div><strong>{segment.seconds}s</strong></header>
      <div className="story-frame-pair">
        <FrameCard title={hasUnconfirmedFirst ? "首帧 · 待确认" : "首帧"} image={(hasUnconfirmedFirst ? latestFirst : first)?.previewUrl ?? null} empty="等待确认首帧" />
        <span aria-hidden="true">→</span>
        <FrameCard title={hasUnconfirmedLast ? "尾帧 · 待确认" : "尾帧"} image={(hasUnconfirmedLast ? latestLast : last)?.previewUrl ?? null} empty="可选尾帧" />
      </div>
      <div className="story-frame-actions">
        <button disabled={!segment.detail || referenceResourceIds.length === 0 || state.activeMediaTarget != null || segment.attempt != null} onClick={() => void state.generateFrameImage(port, segment.id, "first", referenceResourceIds, userIdeas)} type="button">{state.activeMediaTarget === `frame:${segment.id}:first` ? "正在生成首帧…" : first ? "重新生成首帧" : "生成首帧"}</button>
        <button disabled={!segment.detail || referenceResourceIds.length === 0 || state.activeMediaTarget != null || segment.attempt != null} onClick={() => void state.generateFrameImage(port, segment.id, "last", referenceResourceIds, userIdeas)} type="button">{state.activeMediaTarget === `frame:${segment.id}:last` ? "正在生成尾帧…" : last ? "重新生成尾帧" : "生成尾帧"}</button>
        <button disabled={state.activeMediaTarget != null || segment.attempt != null} onClick={() => void state.importLocalImage(port, { kind: "frame", segmentId: segment.id, frameRole: "first" })} type="button">{state.activeMediaTarget === `import:frame:${segment.id}:first` ? "正在导入首帧…" : "导入本机首帧"}</button>
        <button disabled={state.activeMediaTarget != null || segment.attempt != null} onClick={() => void state.importLocalImage(port, { kind: "frame", segmentId: segment.id, frameRole: "last" })} type="button">{state.activeMediaTarget === `import:frame:${segment.id}:last` ? "正在导入尾帧…" : "导入本机尾帧"}</button>
        <button disabled={state.activeMediaTarget != null || segment.attempt != null} onClick={() => onPickHistory({ kind: "frame", segmentId: segment.id, frameRole: "first" })} type="button">从历史选择首帧</button>
        <button disabled={state.activeMediaTarget != null || segment.attempt != null} onClick={() => onPickHistory({ kind: "frame", segmentId: segment.id, frameRole: "last" })} type="button">从历史选择尾帧</button>
        {segment.firstFrames.generationAttemptId && <button className="is-warning" disabled={state.activeMediaTarget != null} onClick={() => onRetryImage({ kind: "frame", segmentId: segment.id, frameRole: "first" })} type="button">核对首帧任务后重试…</button>}
        {segment.lastFrames.generationAttemptId && <button className="is-warning" disabled={state.activeMediaTarget != null} onClick={() => onRetryImage({ kind: "frame", segmentId: segment.id, frameRole: "last" })} type="button">核对尾帧任务后重试…</button>}
      </div>
      {(hasUnconfirmedFirst || hasUnconfirmedLast) && <button className="story-confirm-frames" disabled={state.activeMediaTarget != null} onClick={() => void state.confirmLatestFrames(port, segment.id, latestLast != null)} type="button">确认使用最新首尾帧</button>}
      {segment.detail && <section className="story-shot-list"><h4>镜头时间线</h4>{segment.detail.shots.map((shot) => <div key={`${shot.start}-${shot.end}`}><span>{shot.start}–{shot.end}s</span><p>{shot.prompt}</p></div>)}</section>}
      {segment.video?.previewUrl && <video className="story-video-preview" controls preload="metadata" src={segment.video.previewUrl} />}
      {segment.video && <div className="story-video-redo"><p>重做会先把当前成片保留到创作历史，不会覆盖本机文件。</p><div><button disabled={state.busy} onClick={() => void state.archiveCompletedVideo(port, segment.id, false)} type="button">从首尾帧重做</button><button disabled={state.busy} onClick={() => void state.archiveCompletedVideo(port, segment.id, true)} type="button">按原视频修改</button></div></div>}
      {segment.error && <p className="story-segment-error">{segment.error}</p>}
      <footer>
        <textarea maxLength={1_000} onChange={(event) => setUserIdeas(event.target.value)} placeholder="本次视频生成的补充要求（可选）" value={userIdeas} />
        <select aria-label="分段时长" disabled={segment.attempt != null || segment.video != null || state.busy} onChange={(event) => void state.updateSegmentDuration(port, segment.id, Number(event.target.value))} value={segment.seconds}>{Array.from(new Set([...(project.models.supportedVideoDurations ?? Array.from({ length: 14 }, (_, index) => index + 2)), segment.seconds])).sort((left, right) => left - right).map((seconds) => <option key={seconds} value={seconds}>{seconds} 秒</option>)}</select>
        <select aria-label="视频引导方式" disabled={segment.attempt != null || segment.video != null || state.busy} onChange={(event) => void state.updateVideoGuidance(port, segment.id, event.target.value as "first_frame" | "first_and_last_frames" | "previous_video" | "source_video")} value={segment.videoGuidanceMode}><option value="first_frame">首帧</option><option disabled={last == null} value="first_and_last_frames">首尾帧</option><option disabled={!previousVideoAvailable} value="previous_video">续写上一段</option><option disabled={segment.archivedVideos.length === 0} value="source_video">修改原视频</option></select>
        <select onChange={(event) => setSize(event.target.value as "768P" | "2K")} value={size}><option>768P</option><option>2K</option></select>
        {segment.attempt && !segment.video ? segment.attempt.resumable ? <button className="story-primary-button" disabled={Boolean(progress)} onClick={() => void state.resumeVideo(port, segment.id)} type="button">{progress ? progressLabel(progress.status, progress.percent) : "查询原任务 / 恢复下载"}</button> : <button className="story-video-review-button" disabled={state.busy} onClick={() => setShowVideoRetry(true)} type="button">核对后允许重试…</button> : <button className="story-primary-button" disabled={!isStorySegmentReady(segment) || Boolean(progress)} onClick={() => void state.startVideo(port, segment.id, size, userIdeas)} type="button">{progress ? progressLabel(progress.status, progress.percent) : segment.video ? "已完成" : "生成视频"}</button>}
      </footer>
      {showVideoRetry && <StoryVideoRetrySheet onClose={() => setShowVideoRetry(false)} port={port} segmentId={segment.id} />}
    </article>
  );
}

function FrameCard({ title, image, empty }: { title: string; image: string | null; empty: string }) {
  return <figure><figcaption>{title}</figcaption>{image ? <img alt={title} src={image} /> : <div><span>▧</span><small>{empty}</small></div>}</figure>;
}

function PlanningStatus({ progress }: { progress: StoryPlanningProgress }) {
  const terminal = ["paused", "completed", "failed", "needs_review", "limit_reached"].includes(progress.status);
  return (
    <section className={`story-planning-status story-planning-status--${progress.status}`} aria-live="polite">
      <span aria-hidden="true">{terminal ? "●" : <span className="mini-spinner" />}</span>
      <div>
        <strong>{planningStatusLabel(progress)}</strong>
        <small>{progress.stage === "outline" ? "剧情拆解" : "镜头细化"} · 已调用模型 {progress.modelCalls} 次</small>
        {progress.stopReason && <p>{progress.stopReason}</p>}
      </div>
    </section>
  );
}

function StoryImageHistorySheet({ mediaPort, port, target, onClose }: { mediaPort: MediaStudioPort; port: StoryStudioPort; target: StoryImageImportTarget; onClose(): void }) {
  const state = useStoryStudioStore();
  useEffect(() => {
    void state.loadStoryImageHistory(mediaPort);
  }, [mediaPort, state.loadStoryImageHistory]);
  const select = async (recordId: string, contentToken: string) => {
    await useStoryStudioStore.getState().importHistoryImage(port, target, recordId, contentToken);
    if (!useStoryStudioStore.getState().error) onClose();
  };
  return (
    <div className="story-sheet-backdrop" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget) onClose(); }}>
      <section aria-modal="true" className="story-sheet story-history-sheet" role="dialog">
        <header><div><span>▧</span><div><h2>从 AI 创作历史选择</h2><p>图片会复制到当前剧情项目并立即确认使用。</p></div></div><button aria-label="关闭" onClick={onClose} type="button">×</button></header>
        <div className="story-history-grid">
          {state.storyImageHistoryLoading ? <div className="story-studio__loading"><span className="mini-spinner" />正在读取创作历史…</div> : state.storyImageHistory.length === 0 ? <p>还没有可用的图片创作记录。</p> : state.storyImageHistory.flatMap((item) => item.images.map((asset) => (
            <button disabled={state.activeMediaTarget != null} key={`${item.id}:${asset.id}`} onClick={() => void select(item.id, asset.contentToken)} type="button">
              {asset.previewUrl ? <img alt={asset.revisedPrompt || item.prompt} src={asset.previewUrl} /> : <span>图片</span>}
              <small>{asset.revisedPrompt || item.prompt}</small>
            </button>
          )))}
        </div>
      </section>
    </div>
  );
}

function StoryImageRetrySheet({ port, target, onClose }: { port: StoryStudioPort; target: StoryImageImportTarget; onClose(): void }) {
  const state = useStoryStudioStore();
  const confirm = async () => {
    await state.allowImageRetry(port, target);
    if (!useStoryStudioStore.getState().error) onClose();
  };
  return <div className="story-sheet-backdrop" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget) onClose(); }}><section aria-modal="true" className="story-sheet" role="dialog"><header><div><span>!</span><div><h2>确认已核对图片任务？</h2><p>这是防止重复提交和重复计费的安全步骤。</p></div></div><button aria-label="关闭" onClick={onClose} type="button">×</button></header><div className="story-sheet__body"><p className="story-retry-warning">只有确认服务商没有生成可用结果后才应解锁。下一次点击生成会重新提交，并可能再次计费。</p></div><footer><button onClick={onClose} type="button">取消</button><button className="story-retry-confirm" disabled={state.activeMediaTarget != null} onClick={() => void confirm()} type="button">确认未生成，允许重试</button></footer></section></div>;
}

function StoryVideoRetrySheet({ port, segmentId, onClose }: { port: StoryStudioPort; segmentId: string; onClose(): void }) {
  const state = useStoryStudioStore();
  const confirm = async () => {
    await state.allowVideoRetry(port, segmentId);
    if (!useStoryStudioStore.getState().error) onClose();
  };
  return <div className="story-sheet-backdrop" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget) onClose(); }}><section aria-modal="true" className="story-sheet" role="dialog"><header><div><span>!</span><div><h2>确认已核对原视频任务？</h2><p>本操作不会立即向服务商重新提交。</p></div></div><button aria-label="关闭" onClick={onClose} type="button">×</button></header><div className="story-sheet__body"><p className="story-retry-warning">只有确认原任务失败或未创建时才应重试。后续再次生成可能扣费，旧任务记录会保留。</p></div><footer><button onClick={onClose} type="button">取消</button><button className="story-retry-confirm" disabled={state.busy} onClick={() => void confirm()} type="button">已核对，允许重新生成</button></footer></section></div>;
}

function StoryPlanningHistoryPanel({ port, projectId }: { port: StoryStudioPort; projectId: string }) {
  const storedRuns = useStoryStudioStore((state) => state.planningRuns[projectId]);
  const runs = storedRuns ?? EMPTY_PLANNING_RUNS;
  const unreadable = useStoryStudioStore((state) => state.planningHistoryUnreadable[projectId] ?? 0);
  const nextCursor = useStoryStudioStore((state) => state.planningHistoryNextCursor[projectId] ?? null);
  const busy = useStoryStudioStore((state) => state.busy);
  const resume = useStoryStudioStore((state) => state.resumePlanning);
  const loadMore = useStoryStudioStore((state) => state.loadMorePlanningHistory);
  const [showHistory, setShowHistory] = useState(false);
  const [loadingMore, setLoadingMore] = useState(false);
  const [confirmResume, setConfirmResume] = useState<StoryPlanningRun | null>(null);
  const latest = runs[0];
  if (!latest && unreadable === 0) return null;
  const requestMore = async () => {
    setLoadingMore(true);
    await loadMore(port, projectId);
    setLoadingMore(false);
  };
  const chooseRun = (run: StoryPlanningRun) => {
    setShowHistory(false);
    setConfirmResume(run);
  };
  return <>
    <section className="story-planning-history-panel">
      <span aria-hidden="true">◷</span>
      <div>
        <header><strong>规划草稿与运行记录</strong>{latest && <small>{planningRunStatusLabel(latest)}</small>}</header>
        {latest ? <p>{planningStageLabel(latest.stage)} · {latest.draft.characters.length + latest.draft.scenes.length + latest.draft.props.length} 个素材 · {latest.draft.segments.length} 个分段 · {latest.modelCalls} 次模型调用</p> : <p>没有可读取的规划记录。</p>}
        {unreadable > 0 && <em>{unreadable} 条损坏记录已隔离</em>}
      </div>
      <button onClick={() => setShowHistory(true)} type="button">查看规划历史</button>
    </section>
    {showHistory && <div className="story-sheet-backdrop" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget) setShowHistory(false); }}>
      <section aria-modal="true" className="story-sheet story-planning-history-sheet" role="dialog">
        <header><div><span>◷</span><div><h2>规划草稿与运行记录</h2><p>草稿逐步保存在本机；完整校验通过后才会应用，不会覆盖已修改的项目。</p></div></div><button aria-label="关闭" onClick={() => setShowHistory(false)} type="button">×</button></header>
        <div className="story-planning-history-list">
          {runs.map((run) => {
            const canResume = !run.applied && run.abandonedAtUnixMs == null;
            const duration = run.draft.segments.reduce((total, segment) => total + segment.seconds, 0);
            return <article key={run.id}>
              <header><div><strong>{planningStageLabel(run.stage)}</strong><small>{new Date(run.updatedAtUnixMs).toLocaleString()}</small></div><span>{planningRunStatusLabel(run)}</span></header>
              <div className="story-planning-history-metrics"><span><strong>{run.draft.characters.length + run.draft.scenes.length + run.draft.props.length}</strong>素材</span><span><strong>{run.draft.segments.length}</strong>分段</span><span><strong>{duration}</strong>规划秒数</span><span><strong>{run.modelCalls}</strong>模型调用</span></div>
              {run.stopReason && <em>{run.stopReason}</em>}
              <details><summary>人物文字画像（{run.draft.characters.length}）</summary><div className="story-planning-history-details">{run.draft.characters.length === 0 ? <p>本次草稿没有人物。</p> : run.draft.characters.map((character) => <section key={character.id}><strong>{character.name}{character.profile.isProtagonist ? " · 主角" : ""}</strong><PlanningTextRows rows={[["剧情身份", character.profile.roleInStory], ["外貌", character.profile.appearance], ["性格", character.profile.personality], ["动机", character.profile.motivation], ["人物关系", character.profile.relationships], ["服装", character.profile.costume], ["一致性约束", character.profile.consistencyNotes]]} /></section>)}</div></details>
              <details><summary>场景文字画像（{run.draft.scenes.length}）</summary><div className="story-planning-history-details">{run.draft.scenes.length === 0 ? <p>本次草稿没有场景。</p> : run.draft.scenes.map((scene) => <section key={scene.id}><strong>{scene.name}</strong><PlanningTextRows rows={[["剧情作用", scene.profile.roleInStory], ["场景设定", scene.profile.setting], ["空间布局", scene.profile.spatialLayout], ["光线与色彩", scene.profile.lightingAndPalette], ["关键元素", scene.profile.keyElements], ["氛围", scene.profile.atmosphere], ["一致性约束", scene.profile.consistencyNotes]]} /></section>)}</div></details>
              <details><summary>道具文字画像（{run.draft.props.length}）</summary><div className="story-planning-history-details">{run.draft.props.length === 0 ? <p>本次草稿没有道具。</p> : run.draft.props.map((prop) => <section key={prop.id}><strong>{prop.name}</strong><PlanningTextRows rows={[["描述", prop.description], ["画面提示", prop.imagePrompt]]} /></section>)}</div></details>
              <details><summary>分段草稿（{run.draft.segments.length}）</summary><div className="story-planning-history-details">{run.draft.segments.length === 0 ? <p>本次草稿还没有分段。</p> : run.draft.segments.map((segment) => <section key={segment.id}><strong>{segment.title} · {segment.kind === "transition" ? "转场" : "剧情"} · {segment.seconds} 秒</strong><p>{segment.synopsis}</p>{segment.detail && <PlanningTextRows rows={[["首帧", segment.detail.firstFramePrompt], ["尾帧", segment.detail.lastFramePrompt ?? ""], ["衔接进入", segment.detail.continuityIn], ["衔接离开", segment.detail.continuityOut], ["声音", segment.detail.audio], ["约束", segment.detail.constraints]]} />}</section>)}</div></details>
              <details><summary>最近 {Math.min(run.events.length, 100)} 条动作（完整记录保存在本机）</summary><ol>{run.events.slice(-100).reverse().map((event) => <li key={event.id}><time>{new Date(event.dateUnixMs).toLocaleString()}</time><span>{event.detail}</span></li>)}</ol></details>
              {canResume && <footer><button disabled={busy} onClick={() => chooseRun(run)} type="button">{run.status === "completed" ? "应用这一条记录" : "恢复这一条记录"}</button></footer>}
            </article>;
          })}
          {unreadable > 0 && <p className="story-planning-history-warning">{unreadable} 条损坏记录已安全隔离，原文件保留且未用于恢复。</p>}
        </div>
        <footer><button disabled={!nextCursor || loadingMore} onClick={() => void requestMore()} type="button">{loadingMore ? "正在读取…" : nextCursor ? "加载更早记录" : "没有更早记录"}</button><button onClick={() => setShowHistory(false)} type="button">完成</button></footer>
      </section>
    </div>}
    {confirmResume && <div className="story-sheet-backdrop" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget) setConfirmResume(null); }}><section aria-modal="true" className="story-sheet" role="dialog"><header><div><span>◷</span><div><h2>{confirmResume.status === "completed" ? "应用这份规划草稿？" : "继续这次规划？"}</h2><p>已经完成并保存的内容不会重复生成。</p></div></div><button aria-label="关闭" onClick={() => setConfirmResume(null)} type="button">×</button></header><div className="story-sheet__body"><p className="story-planning-confirmation">原生层会先核对项目指纹；项目已被修改时不会覆盖现有内容。规划完整校验通过后才会应用。</p></div><footer><button onClick={() => setConfirmResume(null)} type="button">取消</button><button className="story-primary-button" disabled={busy} onClick={() => { const run = confirmResume; setConfirmResume(null); void resume(port, projectId, run.id); }} type="button">确认继续</button></footer></section></div>}
  </>;
}

function PlanningTextRows({ rows }: { rows: [string, string][] }) {
  const visible = rows.filter(([, value]) => value.trim());
  if (visible.length === 0) return null;
  return <dl>{visible.map(([label, value]) => <div key={label}><dt>{label}</dt><dd>{value}</dd></div>)}</dl>;
}

function planningStageLabel(stage: StoryPlanningRun["stage"]) {
  return stage === "outline" ? "剧情拆解" : "镜头细化";
}

function planningRunStatusLabel(run: StoryPlanningRun) {
  if (run.applied) return "规划完成 · 已应用";
  if (run.abandonedAtUnixMs != null) return "中断草稿 · 已放弃";
  if (run.status === "completed") return "草稿已完成 · 待应用";
  if (run.status === "running") return "正在分步规划 · 草稿自动保存";
  if (run.status === "needs_review") return "需要核对后恢复";
  if (run.status === "limit_reached") return "达到运行限制 · 可恢复";
  if (run.status === "failed") return "规划失败 · 可查看";
  return "已暂停 / 中断 · 可恢复";
}

function PlanningRecoveryBanner({ port, run }: {
  port: StoryStudioPort;
  run: ReturnType<typeof useStoryStudioStore.getState>["planningRuns"][string][number];
}) {
  const state = useStoryStudioStore();
  const completed = run.status === "completed";
  return (
    <section className="story-planning-recovery">
      <span aria-hidden="true">◷</span>
      <div>
        <strong>{completed ? "规划草稿已完成，等待应用" : "上次规划已中断，草稿已安全保存"}</strong>
        <p>{run.stage === "outline" ? "剧情拆解" : "镜头细化"} · {run.draft.characters.length + run.draft.scenes.length + run.draft.props.length} 个素材 · {run.draft.segments.length} 个分段 · 已调用模型 {run.modelCalls} 次</p>
        {run.stopReason && <small>{run.stopReason}</small>}
      </div>
      <button className="is-destructive" disabled={state.busy} onClick={() => void state.abandonPlanning(port, run.projectId, run.id)} type="button">放弃草稿</button>
      <button disabled={state.busy} onClick={() => void state.resumePlanning(port, run.projectId, run.id)} type="button">{completed ? "应用草稿" : "继续规划"}</button>
    </section>
  );
}

function recoverablePlanningRun(
  runs: ReturnType<typeof useStoryStudioStore.getState>["planningRuns"][string],
  baseFingerprint: string | undefined,
) {
  if (!baseFingerprint) return null;
  return runs.find((run) => !run.applied
    && run.abandonedAtUnixMs == null
    && run.baseProjectFingerprint === baseFingerprint) ?? null;
}

function planningActionLabel(progress: StoryPlanningProgress | null) {
  if (!progress) return "正在启动规划…";
  if (progress.status === "ready") return "正在准备规划…";
  return progress.stage === "outline" ? "正在拆解剧情…" : "正在细化镜头…";
}

function planningIsActive(progress: StoryPlanningProgress | null) {
  return progress?.status === "ready" || progress?.status === "running";
}

function planningStatusLabel(progress: StoryPlanningProgress) {
  switch (progress.status) {
    case "ready": return "正在准备规划";
    case "running": return "规划 Agent 正在工作";
    case "paused": return "规划已暂停";
    case "completed": return "规划已经完成";
    case "failed": return "规划执行失败";
    case "needs_review": return "规划需要检查";
    case "limit_reached": return "规划已达到运行上限";
  }
}

function StoryProjectSheet({ port, projectId, onClose }: { port: StoryStudioPort; projectId?: string; onClose(): void }) {
  const state = useStoryStudioStore();
  const project = projectId ? state.projects.find((value) => value.id === projectId) ?? null : null;
  const [title, setTitle] = useState(project?.title ?? "");
  const [description, setDescription] = useState(project?.description ?? "");
  const choices = useMemo(() => modelChoices(state.models), [state.models]);
  const [textModelId, setTextModelId] = useState(project?.models.textModelId ?? "");
  const [imageModelId, setImageModelId] = useState(project?.models.imageModelId ?? "");
  const [videoModelId, setVideoModelId] = useState(project?.models.videoModelId ?? "");
  const text = textModelId || choices.text[0]?.id || "";
  const image = imageModelId || choices.image[0]?.id || "";
  const video = videoModelId || choices.video[0]?.id || "";
  const hasUnresolvedImageJobs = project != null && [
    ...project.characters.map((value) => value.media.generationAttemptId),
    ...project.scenes.map((value) => value.media.generationAttemptId),
    ...project.props.map((value) => value.media.generationAttemptId),
    ...project.segments.flatMap((value) => [
      value.firstFrames.generationAttemptId,
      value.lastFrames.generationAttemptId,
    ]),
  ].some(Boolean);
  const hasUnresolvedVideoJobs = project?.segments.some((value) => value.attempt != null) ?? false;
  const unchanged = project != null
    && title === project.title
    && description === project.description
    && text === project.models.textModelId
    && image === project.models.imageModelId
    && video === project.models.videoModelId;
  const submit = async () => {
    if (project) {
      if (await state.updateProjectSettings(port, {
        title,
        description,
        textModelId: text,
        imageModelId: image,
        videoModelId: video,
      })) onClose();
      return;
    }
    await state.createProject(port, {
      title,
      description,
      textModelId: text,
      imageModelId: image,
      videoModelId: video,
    });
    if (!useStoryStudioStore.getState().error) onClose();
  };
  const titleId = project ? "story-settings-title" : "story-create-title";
  const descriptionId = project ? "story-settings-description" : "story-create-description";
  return <div className="story-sheet-backdrop" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget && !state.busy) onClose(); }}><section aria-modal="true" className="story-sheet" role="dialog"><header><div><span>{project ? "⌘" : "✦"}</span><div><h2>{project ? "剧情设置" : "新建剧情项目"}</h2><p>{project ? "修改项目信息和模型组合；不会启动 AI 或产生费用。" : "模型和素材均按当前账号隔离保存在本机。"}</p></div></div><button aria-label="关闭" disabled={state.busy} onClick={onClose} type="button">×</button></header><div className="story-sheet__body"><label htmlFor={titleId}>项目名称<input autoFocus id={titleId} maxLength={120} onChange={(event) => setTitle(event.target.value)} placeholder="例如：雨夜来客" value={title} /></label><label htmlFor={descriptionId}>项目说明<textarea id={descriptionId} maxLength={4_000} onChange={(event) => setDescription(event.target.value)} placeholder="可选" value={description} /></label><ModelSelect label="文本规划模型" models={choices.text} value={text} onChange={setTextModelId} /><ModelSelect disabled={hasUnresolvedImageJobs} label="图片生成模型" models={choices.image} value={image} onChange={setImageModelId} /><ModelSelect disabled={hasUnresolvedVideoJobs} label="视频生成模型" models={choices.video} value={video} onChange={setVideoModelId} />{(hasUnresolvedImageJobs || hasUnresolvedVideoJobs) && <p className="story-settings-note">上次提交结果尚未核对，对应模型暂时不能修改。</p>}</div><footer><button disabled={state.busy} onClick={onClose} type="button">取消</button><button className="story-primary-button" disabled={!title.trim() || !text || !image || !video || state.busy || unchanged} onClick={() => void submit()} type="button">{state.busy ? (project ? "正在保存…" : "正在创建…") : (project ? "保存设置" : "创建项目")}</button></footer></section></div>;
}

function ModelSelect({ disabled = false, label, models, value, onChange }: { disabled?: boolean; label: string; models: ReturnType<typeof modelChoices>["text"]; value: string; onChange(value: string): void }) {
  const selectedUnavailable = value && !models.some((model) => model.id === value);
  return <label>{label}<select disabled={disabled} onChange={(event) => onChange(event.target.value)} value={value}>{models.length === 0 && !selectedUnavailable && <option value="">没有可用模型</option>}{selectedUnavailable && <option value={value}>原模型不可用，请重新选择</option>}{models.map((model) => <option key={model.id} value={model.id}>{model.name} · {model.modelName}</option>)}</select></label>;
}

function modelChoices(models: ReturnType<typeof useStoryStudioStore.getState>["models"]) {
  const enabled = models.filter((model) => model.enabled && model.hasApiKey);
  const video = enabled.filter((model) => model.modelName.toLowerCase() === "minimax-h3");
  const image = enabled.filter((model) => /image|flux|seedream|dall-e/iu.test(model.modelName));
  const excluded = new Set([...video, ...image].map((model) => model.id));
  const text = enabled.filter((model) => !excluded.has(model.id));
  return { text: text.length ? text : enabled, image: image.length ? image : enabled, video: video.length ? video : enabled };
}

function projectStage(project: ReturnType<typeof useStoryStudioStore.getState>["projects"][number]) {
  if (project.segments.some((segment) => segment.video)) return "视频制作中";
  if (project.segments.length) return `${project.segments.length} 个分段`;
  return project.source.trim() ? "等待规划" : "等待原文";
}

function segmentState(segment: ReturnType<typeof useStoryStudioStore.getState>["projects"][number]["segments"][number]) {
  if (segment.video) return "已完成";
  if (segment.attempt) return "生成中";
  if (isStorySegmentReady(segment)) return "可生成";
  if (segment.detail) return "等待首帧";
  return "等待细化";
}

function progressLabel(status: string, percent: number | null) {
  const label = status === "submitting" ? "正在提交" : status === "checking" ? "正在查询" : "正在生成";
  return percent == null ? label : `${label} ${Math.round(percent)}%`;
}
