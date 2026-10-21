import { useEffect, useMemo, useState } from "react";
import type { MediaStudioPort } from "../../media-studio/ports/MediaStudioPort";
import { isStorySegmentReady, storyConfirmedImage } from "../domain/storyStudio";
import { useStoryStudioStore } from "../application/storyStudioStore";
import type { StoryPlanningProgress } from "../application/storyStudioStore";
import type { StoryImageImportTarget, StoryStudioPort } from "../ports/StoryStudioPort";
import "./storyStudio.css";

interface StoryStudioViewProps {
  ownerUserId: string;
  mediaPort: MediaStudioPort;
  port: StoryStudioPort;
}

export function StoryStudioView({ ownerUserId, mediaPort, port }: StoryStudioViewProps) {
  const state = useStoryStudioStore();
  const [showCreate, setShowCreate] = useState(false);

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
        {project ? <ProjectWorkspace mediaPort={mediaPort} port={port} projectId={project.id} /> : (
          <div className="story-studio__welcome">
            <span aria-hidden="true">✦</span>
            <h2>把故事变成可控的镜头</h2>
            <p>剧情规划、角色场景素材、首尾帧和分段视频都保存在本机项目中。</p>
            <button onClick={() => setShowCreate(true)} type="button">新建剧情项目</button>
          </div>
        )}
      </main>
      {showCreate && <CreateStorySheet onClose={() => setShowCreate(false)} port={port} />}
    </section>
  );
}

function ProjectWorkspace({ mediaPort, port, projectId }: { mediaPort: MediaStudioPort; port: StoryStudioPort; projectId: string }) {
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
        </div>
      </header>
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
  const [selectedSegment, setSelectedSegment] = useState(project.segments[0]?.id ?? null);
  const segment = project.segments.find((value) => value.id === selectedSegment) ?? project.segments[0];
  const resources = [...project.characters, ...project.scenes, ...project.props];
  const planning = state.planningProgress[project.id] ?? null;
  const isRefining = state.busy && planning?.stage === "refine" && planningIsActive(planning);
  const [historyTarget, setHistoryTarget] = useState<StoryImageImportTarget | null>(null);
  const [retryTarget, setRetryTarget] = useState<StoryImageImportTarget | null>(null);

  return (
    <div className="story-plan-workspace">
      {planning && <PlanningStatus progress={planning} />}
      <section className="story-resource-strip">
        <header><div><h3>角色与场景</h3><p>规划生成的素材事实源</p></div><button disabled={state.busy || project.segments.length === 0} onClick={() => void state.startPlanning(port, "refine", project.segments.map((value) => value.id), "")} type="button">{isRefining ? planningActionLabel(planning) : "细化全部镜头"}</button></header>
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
          {project.segments.map((item, index) => <button className={item.id === segment?.id ? "is-selected" : ""} key={item.id} onClick={() => setSelectedSegment(item.id)} type="button"><span>{String(index + 1).padStart(2, "0")}</span><span><strong>{item.title}</strong><small>{item.seconds} 秒 · {segmentState(item)}</small></span></button>)}
        </nav>
        {segment && <SegmentInspector onPickHistory={setHistoryTarget} onRetryImage={setRetryTarget} port={port} projectId={project.id} segmentId={segment.id} />}
      </div>
      {historyTarget && <StoryImageHistorySheet mediaPort={mediaPort} onClose={() => setHistoryTarget(null)} port={port} target={historyTarget} />}
      {retryTarget && <StoryImageRetrySheet onClose={() => setRetryTarget(null)} port={port} target={retryTarget} />}
    </div>
  );
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
        {segment.attempt && !segment.video ? <button className="story-primary-button" disabled={Boolean(progress)} onClick={() => void state.resumeVideo(port, segment.id)} type="button">{progress ? progressLabel(progress.status, progress.percent) : "恢复任务"}</button> : <button className="story-primary-button" disabled={!isStorySegmentReady(segment) || Boolean(progress)} onClick={() => void state.startVideo(port, segment.id, size, userIdeas)} type="button">{progress ? progressLabel(progress.status, progress.percent) : segment.video ? "已完成" : "生成视频"}</button>}
      </footer>
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

function PlanningRecoveryBanner({ port, run }: {
  port: StoryStudioPort;
  run: ReturnType<typeof useStoryStudioStore.getState>["planningRuns"][string][number];
}) {
  const state = useStoryStudioStore();
  return (
    <section className="story-planning-recovery">
      <span aria-hidden="true">◷</span>
      <div>
        <strong>上次规划已中断，草稿已安全保存</strong>
        <p>{run.stage === "outline" ? "剧情拆解" : "镜头细化"} · {run.draft.characters.length + run.draft.scenes.length + run.draft.props.length} 个素材 · {run.draft.segments.length} 个分段 · 已调用模型 {run.modelCalls} 次</p>
        {run.stopReason && <small>{run.stopReason}</small>}
      </div>
      <button className="is-destructive" disabled={state.busy} onClick={() => void state.abandonPlanning(port, run.projectId, run.id)} type="button">放弃草稿</button>
      <button disabled={state.busy} onClick={() => void state.resumePlanning(port, run.projectId, run.id)} type="button">继续规划</button>
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

function CreateStorySheet({ port, onClose }: { port: StoryStudioPort; onClose(): void }) {
  const state = useStoryStudioStore();
  const [title, setTitle] = useState("");
  const [description, setDescription] = useState("");
  const choices = useMemo(() => modelChoices(state.models), [state.models]);
  const [textModelId, setTextModelId] = useState("");
  const [imageModelId, setImageModelId] = useState("");
  const [videoModelId, setVideoModelId] = useState("");
  const text = textModelId || choices.text[0]?.id || "";
  const image = imageModelId || choices.image[0]?.id || "";
  const video = videoModelId || choices.video[0]?.id || "";
  const submit = async () => {
    await state.createProject(port, { title, description, textModelId: text, imageModelId: image, videoModelId: video });
    if (!useStoryStudioStore.getState().error) onClose();
  };
  return <div className="story-sheet-backdrop" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget) onClose(); }}><section aria-modal="true" className="story-sheet" role="dialog"><header><div><span>✦</span><div><h2>新建剧情项目</h2><p>模型和素材均按当前账号隔离保存在本机。</p></div></div><button aria-label="关闭" onClick={onClose} type="button">×</button></header><div className="story-sheet__body"><label>项目名称<input autoFocus maxLength={120} onChange={(event) => setTitle(event.target.value)} placeholder="例如：雨夜来客" value={title} /></label><label>项目说明<textarea maxLength={4_000} onChange={(event) => setDescription(event.target.value)} placeholder="可选" value={description} /></label><ModelSelect label="文本规划模型" models={choices.text} value={text} onChange={setTextModelId} /><ModelSelect label="图片生成模型" models={choices.image} value={image} onChange={setImageModelId} /><ModelSelect label="视频生成模型" models={choices.video} value={video} onChange={setVideoModelId} /></div><footer><button onClick={onClose} type="button">取消</button><button className="story-primary-button" disabled={!title.trim() || !text || !image || !video || state.busy} onClick={() => void submit()} type="button">{state.busy ? "正在创建…" : "创建项目"}</button></footer></section></div>;
}

function ModelSelect({ label, models, value, onChange }: { label: string; models: ReturnType<typeof modelChoices>["text"]; value: string; onChange(value: string): void }) {
  return <label>{label}<select onChange={(event) => onChange(event.target.value)} value={value}>{models.length === 0 && <option value="">没有可用模型</option>}{models.map((model) => <option key={model.id} value={model.id}>{model.name} · {model.modelName}</option>)}</select></label>;
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
