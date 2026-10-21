import { useEffect, useMemo, useState } from "react";
import type { MediaStudioPort } from "../../media-studio/ports/MediaStudioPort";
import { isStorySegmentReady, storyConfirmedImage } from "../domain/storyStudio";
import { useStoryStudioStore } from "../application/storyStudioStore";
import type { StoryStudioPort } from "../ports/StoryStudioPort";
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
        {project ? <ProjectWorkspace port={port} projectId={project.id} /> : (
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

function ProjectWorkspace({ port, projectId }: { port: StoryStudioPort; projectId: string }) {
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
            <button className="story-primary-button" disabled={!source.trim() || state.busy} onClick={() => void planOutline()} type="button">
              {state.busy ? <><span className="mini-spinner" />正在规划…</> : "生成剧情规划"}
            </button>
          </aside>
        </div>
      ) : <PlannedStoryWorkspace port={port} projectId={project.id} />}
    </div>
  );
}

function PlannedStoryWorkspace({ port, projectId }: { port: StoryStudioPort; projectId: string }) {
  const state = useStoryStudioStore();
  const project = state.projects.find((value) => value.id === projectId)!;
  const [selectedSegment, setSelectedSegment] = useState(project.segments[0]?.id ?? null);
  const segment = project.segments.find((value) => value.id === selectedSegment) ?? project.segments[0];
  const resources = [...project.characters, ...project.scenes, ...project.props];

  return (
    <div className="story-plan-workspace">
      <section className="story-resource-strip">
        <header><div><h3>角色与场景</h3><p>规划生成的素材事实源</p></div><button disabled={state.busy || project.segments.length === 0} onClick={() => void state.startPlanning(port, "refine", project.segments.map((value) => value.id), "")} type="button">细化全部镜头</button></header>
        <div>{resources.map((resource) => {
          const image = storyConfirmedImage(resource.media);
          const latest = resource.media.images.at(-1) ?? null;
          const hasUnconfirmedLatest = latest != null && latest.id !== resource.media.confirmedImageId;
          const target = `resource:${resource.id}`;
          const displayed = hasUnconfirmedLatest ? latest : image;
          return <article key={resource.id}>{displayed?.previewUrl ? <img alt={resource.name} src={displayed.previewUrl} /> : <span>◇</span>}<strong>{resource.name}{hasUnconfirmedLatest ? " · 待确认" : ""}</strong><small>{resource.imagePrompt || ("description" in resource ? resource.description : "等待素材描述")}</small>{hasUnconfirmedLatest && <button disabled={state.activeMediaTarget != null} onClick={() => void state.confirmLatestResources(port, [resource.id])} type="button">使用最新版本</button>}<button disabled={state.activeMediaTarget != null} onClick={() => void state.generateAssetImage(port, resource.id, "")} type="button">{state.activeMediaTarget === target ? "生成中…" : image ? "再生成" : "生成素材"}</button></article>;
        })}</div>
      </section>
      <div className="story-segment-layout">
        <nav aria-label="剧情分段">
          {project.segments.map((item, index) => <button className={item.id === segment?.id ? "is-selected" : ""} key={item.id} onClick={() => setSelectedSegment(item.id)} type="button"><span>{String(index + 1).padStart(2, "0")}</span><span><strong>{item.title}</strong><small>{item.seconds} 秒 · {segmentState(item)}</small></span></button>)}
        </nav>
        {segment && <SegmentInspector port={port} projectId={project.id} segmentId={segment.id} />}
      </div>
    </div>
  );
}

function SegmentInspector({ port, projectId, segmentId }: { port: StoryStudioPort; projectId: string; segmentId: string }) {
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
      </div>
      {(hasUnconfirmedFirst || hasUnconfirmedLast) && <button className="story-confirm-frames" disabled={state.activeMediaTarget != null} onClick={() => void state.confirmLatestFrames(port, segment.id, latestLast != null)} type="button">确认使用最新首尾帧</button>}
      {segment.detail && <section className="story-shot-list"><h4>镜头时间线</h4>{segment.detail.shots.map((shot) => <div key={`${shot.start}-${shot.end}`}><span>{shot.start}–{shot.end}s</span><p>{shot.prompt}</p></div>)}</section>}
      {segment.video?.previewUrl && <video className="story-video-preview" controls preload="metadata" src={segment.video.previewUrl} />}
      {segment.error && <p className="story-segment-error">{segment.error}</p>}
      <footer>
        <textarea maxLength={1_000} onChange={(event) => setUserIdeas(event.target.value)} placeholder="本次视频生成的补充要求（可选）" value={userIdeas} />
        <select onChange={(event) => setSize(event.target.value as "768P" | "2K")} value={size}><option>768P</option><option>2K</option></select>
        {segment.attempt && !segment.video ? <button className="story-primary-button" disabled={Boolean(progress)} onClick={() => void state.resumeVideo(port, segment.id)} type="button">{progress ? progressLabel(progress.status, progress.percent) : "恢复任务"}</button> : <button className="story-primary-button" disabled={!isStorySegmentReady(segment) || Boolean(progress)} onClick={() => void state.startVideo(port, segment.id, size, userIdeas)} type="button">{progress ? progressLabel(progress.status, progress.percent) : segment.video ? "已完成" : "生成视频"}</button>}
      </footer>
    </article>
  );
}

function FrameCard({ title, image, empty }: { title: string; image: string | null; empty: string }) {
  return <figure><figcaption>{title}</figcaption>{image ? <img alt={title} src={image} /> : <div><span>▧</span><small>{empty}</small></div>}</figure>;
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
