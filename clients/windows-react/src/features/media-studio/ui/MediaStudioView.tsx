import { lazy, Suspense, useEffect, useMemo, useState, type ReactNode } from "react";
import {
  canGenerateMediaStudioImage,
  canGenerateMediaStudioVideo,
  mediaStudioAssetPreviewUrl,
  mediaStudioDisplayItems,
  mediaStudioVideoCanvasState,
  supportedImageSizes,
  supportedVideoRatios,
  supportedVideoSizes,
  useMediaStudioStore,
  type MediaStudioDisplayItem,
} from "../application/mediaStudioStore";
import type {
  MediaStudioGeneratedAsset,
  MediaStudioHistoryAsset,
  MediaStudioHistoryItem,
} from "../domain/mediaStudio";
import type { MediaStudioPort } from "../ports/MediaStudioPort";
import type { StoryStudioPort } from "../../story-studio/ports/StoryStudioPort";
import "./mediaStudio.css";

const StoryStudioView = lazy(async () => {
  const module = await import("../../story-studio/ui/StoryStudioView");
  return { default: module.StoryStudioView };
});

interface MediaStudioViewProps {
  ownerUserId: string;
  port: MediaStudioPort;
  storyPort?: StoryStudioPort;
}

type StudioSection = "image" | "video" | "story" | "history";

export function MediaStudioView({ ownerUserId, port, storyPort }: MediaStudioViewProps) {
  const store = useMediaStudioStore();
  const [section, setSection] = useState<StudioSection>("image");
  const [historyPicker, setHistoryPicker] = useState<"image" | "video" | null>(null);
  const [preview, setPreview] = useState<{ url: string; alt: string } | null>(null);

  useEffect(() => {
    void store.activate(port, ownerUserId);
    return () => store.deactivate(port);
  }, [ownerUserId, port, store.activate, store.deactivate]);

  return (
    <section className="media-studio" aria-label="AI 创作">
      <StudioHeader section={section} storyAvailable={storyPort != null} onSelect={setSection} />
      {section === "image" ? (
        <ImageWorkspace
          onChooseHistory={() => setHistoryPicker("image")}
          onPreview={(url, alt) => setPreview({ url, alt })}
          port={port}
        />
      ) : section === "video" ? (
        <VideoWorkspace onChooseHistory={() => setHistoryPicker("video")} port={port} />
      ) : section === "story" ? (
        storyPort ? (
          <Suspense fallback={<div className="workspace-loading"><span className="mini-spinner" />正在载入剧情创作…</div>}>
            <StoryStudioView mediaPort={port} ownerUserId={ownerUserId} port={storyPort} />
          </Suspense>
        ) : null
      ) : (
        <HistoryWorkspace
          onPreview={(url, alt) => setPreview({ url, alt })}
          port={port}
        />
      )}
      {historyPicker && (
        <HistoryReferencePicker
          currentCount={historyPicker === "image" ? store.references.length : 0}
          history={store.history}
          maximumCount={historyPicker === "image" ? 8 : 1}
          mode={historyPicker}
          onDismiss={() => setHistoryPicker(null)}
          onSelect={(assets) => {
            const mode = historyPicker;
            setHistoryPicker(null);
            if (mode === "image") void store.addHistoryReferences(port, assets);
            else if (assets[0]) void store.addHistoryVideoInput(port, assets[0]);
          }}
        />
      )}
      {preview && (
        <ImagePreview
          alt={preview.alt}
          url={preview.url}
          onDismiss={() => setPreview(null)}
        />
      )}
    </section>
  );
}

function StudioHeader({
  section,
  storyAvailable,
  onSelect,
}: {
  section: StudioSection;
  storyAvailable: boolean;
  onSelect(section: StudioSection): void;
}) {
  return (
    <header className="media-studio__header">
      <div className="media-studio__identity">
        <span aria-hidden="true" className="media-studio__mark">✦</span>
        <span><strong>AI 创作</strong><small>独立于聊天的图片与视频工作台</small></span>
      </div>
      <div className="media-studio__segments" aria-label="创作类型" role="group">
        <button
          aria-pressed={section === "image"}
          className={section === "image" ? "is-active" : ""}
          onClick={() => onSelect("image")}
          type="button"
        ><span aria-hidden="true">▧</span>图片</button>
        <button
          aria-pressed={section === "video"}
          className={section === "video" ? "is-active" : ""}
          onClick={() => onSelect("video")}
          type="button"
        >
          <span aria-hidden="true">▶</span>视频
        </button>
        <button
          aria-pressed={section === "story"}
          className={section === "story" ? "is-active" : ""}
          disabled={!storyAvailable}
          onClick={() => onSelect("story")}
          title={storyAvailable ? undefined : "剧情模式不可用"}
          type="button"
        >
          <span aria-hidden="true">▤</span>剧情模式
        </button>
        <button
          aria-pressed={section === "history"}
          className={section === "history" ? "is-active" : ""}
          onClick={() => onSelect("history")}
          type="button"
        ><span aria-hidden="true">◷</span>记录</button>
      </div>
    </header>
  );
}

function ImageWorkspace({
  port,
  onChooseHistory,
  onPreview,
}: {
  port: MediaStudioPort;
  onChooseHistory(): void;
  onPreview(url: string, alt: string): void;
}) {
  return (
    <div className="media-studio__workspace">
      <CreationCanvas onPreview={onPreview} />
      <GenerationSettings onChooseHistory={onChooseHistory} port={port} />
    </div>
  );
}

function VideoWorkspace({
  port,
  onChooseHistory,
}: {
  port: MediaStudioPort;
  onChooseHistory(): void;
}) {
  return (
    <div className="media-studio__workspace media-video-workspace">
      <VideoCanvas port={port} />
      <VideoSettings onChooseHistory={onChooseHistory} port={port} />
    </div>
  );
}

function VideoCanvas({ port }: { port: MediaStudioPort }) {
  const state = useMediaStudioStore();
  const canvas = mediaStudioVideoCanvasState(state);
  const subtitle = canvas.kind === "progress"
    ? videoProgressText(canvas.status, canvas.percent)
    : canvas.kind === "failed"
      ? "生成失败"
      : canvas.kind === "result"
        ? `${canvas.item.modelName} · 已保存到本机`
        : "视频结果不会进入聊天记录";
  return (
    <section className="media-studio-card media-canvas" aria-label="视频画布">
      <header className="media-card-header">
        <div><h2>视频画布</h2><p>{subtitle}</p></div>
        {state.isGeneratingVideo && (
          <button
            className="media-stop-button"
            onClick={() => void state.cancelVideo(port)}
            type="button"
          >停止等待</button>
        )}
      </header>
      <div className="media-canvas__body media-video-canvas__body">
        {canvas.kind === "progress" ? (
          <VideoProgress
            hasHistory={state.videoHistory.length > 0}
            percent={canvas.percent}
            status={canvas.status}
          />
        ) : canvas.kind === "failed" ? (
          <VideoProgress hasHistory={state.videoHistory.length > 0} message={canvas.message} status="failed" />
        ) : canvas.kind === "result" ? (
          <VideoResult item={canvas.item} />
        ) : state.isLoadingVideoHistory ? (
          <StudioState icon={<span className="mini-spinner" />} title="正在读取视频记录…" />
        ) : (
          <div className="media-empty-canvas media-video-empty">
            <span aria-hidden="true" className="media-empty-canvas__icon">▶</span>
            <div><h2>描述一段有运动和镜头感的画面</h2><p>视频任务会在模型提供方异步生成，完成后自动下载到本机。</p></div>
          </div>
        )}
      </div>
      {state.videoHistoryErrorMessage && (
        <InlineNotice message={state.videoHistoryErrorMessage} tone="warning" />
      )}
    </section>
  );
}

function VideoProgress({
  status,
  percent = null,
  hasHistory,
  message,
}: {
  status: "submitting" | import("../domain/mediaStudio").MediaStudioVideoJobStatus;
  percent?: number | null;
  hasHistory: boolean;
  message?: string;
}) {
  return (
    <div className={`media-video-progress ${status === "failed" ? "is-failed" : ""}`}>
      <span aria-hidden="true" className="media-video-progress__icon">{status === "failed" ? "⚠" : "▶"}</span>
      <h2>{videoProgressText(status, percent)}</h2>
      {status !== "failed" && (percent == null
        ? <span className="media-video-progress__spinner mini-spinner" />
        : <progress max={100} value={percent} />)}
      <p>{message ?? (status === "failed"
        ? "请检查模型配置或稍后重试。"
        : "当前任务完成后会在这里显示新视频。")}</p>
      {hasHistory && <small>之前生成的视频仍保留在创作记录中。</small>}
    </div>
  );
}

function VideoResult({
  item,
  compact = false,
}: {
  item: import("../domain/mediaStudio").MediaStudioVideoHistoryItem;
  compact?: boolean;
}) {
  return (
    <article className={`media-video-result ${compact ? "is-compact" : ""}`}>
      {item.previewUrl ? (
        <video controls preload="metadata" src={item.previewUrl}>
          当前系统无法播放这个视频。
        </video>
      ) : <div className="media-result__unavailable">视频预览不可用</div>}
      <div className="media-result__prompt"><span>提示词</span><p>{item.prompt}</p></div>
    </article>
  );
}

function VideoSettings({
  port,
  onChooseHistory,
}: {
  port: MediaStudioPort;
  onChooseHistory(): void;
}) {
  const state = useMediaStudioStore();
  const disabled = state.isGeneratingVideo;
  const canGenerate = canGenerateMediaStudioVideo(state);
  return (
    <aside className="media-studio-card media-settings" aria-label="视频设置">
      <header className="media-settings__header">
        <h2>视频设置</h2>
        <button
          aria-label="刷新视频模型"
          disabled={state.isLoadingModels || disabled}
          onClick={() => void state.refreshModels(port)}
          title="刷新模型"
          type="button"
        >↻</button>
      </header>
      <div className="media-settings__scroll">
        <Field label="视频模型">
          {state.isLoadingModels && state.videoModels.length === 0 ? (
            <div className="media-model-state"><span className="mini-spinner" />正在读取模型…</div>
          ) : state.videoModels.length === 0 ? (
            <div className="media-model-empty"><strong>当前没有兼容视频模型</strong><span>请配置并启用 MiniMax-H3 后刷新。</span></div>
          ) : (
            <div className="media-model-select"><span aria-hidden="true">✦</span>
              <select
                aria-label="视频模型"
                disabled={disabled}
                onChange={(event) => state.setSelectedVideoModel(event.target.value)}
                value={state.selectedVideoModelId ?? ""}
              >
                {state.videoModels.map((model) => (
                  <option key={model.id} value={model.id}>{model.name} · {model.modelName}</option>
                ))}
              </select>
            </div>
          )}
        </Field>

        <VideoInputImageSelector disabled={disabled} onChooseHistory={onChooseHistory} port={port} />
        <VideoAudioSelector disabled={disabled} port={port} />

        <Field label="提示词">
          <textarea
            aria-label="视频提示词"
            disabled={disabled}
            maxLength={7_000}
            onChange={(event) => state.setVideoPrompt(event.target.value)}
            placeholder="描述主体运动、镜头运动、场景变化与氛围…"
            value={state.videoPrompt}
          />
        </Field>

        <div className="media-settings__row">
          <Field label="分辨率">
            <select
              aria-label="视频分辨率"
              disabled={disabled}
              onChange={(event) => state.setVideoSize(event.target.value as typeof state.videoSize)}
              value={state.videoSize}
            >{supportedVideoSizes.map((size) => <option key={size}>{size}</option>)}</select>
          </Field>
          <Field label="时长">
            <select
              aria-label="视频时长"
              disabled={disabled}
              onChange={(event) => state.setVideoSeconds(Number(event.target.value))}
              value={state.videoSeconds}
            >{Array.from({ length: 12 }, (_, index) => index + 4).map((seconds) => (
              <option key={seconds} value={seconds}>{seconds}s</option>
            ))}</select>
          </Field>
        </div>

        <Field label="画面比例">
          {state.videoInputImage ? (
            <div className="media-field-note">跟随参考图比例</div>
          ) : (
            <select
              aria-label="视频画面比例"
              disabled={disabled}
              onChange={(event) => state.setVideoRatio(event.target.value as typeof state.videoRatio)}
              value={state.videoRatio}
            >{supportedVideoRatios.map((ratio) => <option key={ratio}>{ratio}</option>)}</select>
          )}
        </Field>

        {state.videoInputErrorMessage && <InlineNotice message={state.videoInputErrorMessage} tone="warning" />}
        {state.videoGenerationErrorMessage && (
          <InlineNotice
            action={<button onClick={state.dismissVideoError} type="button">关闭</button>}
            message={state.videoGenerationErrorMessage}
            tone="warning"
          />
        )}
        <button
          className="media-generate-button"
          disabled={!canGenerate}
          onClick={() => void state.generateVideo(port)}
          type="button"
        >
          {state.isGeneratingVideo ? <span className="mini-spinner" /> : <span aria-hidden="true">▶</span>}
          {state.isGeneratingVideo ? "正在生成…" : state.videoInputImage ? "基于参考图生成视频" : "生成视频"}
        </button>
      </div>
    </aside>
  );
}

function VideoInputImageSelector({
  port,
  disabled,
  onChooseHistory,
}: {
  port: MediaStudioPort;
  disabled: boolean;
  onChooseHistory(): void;
}) {
  const state = useMediaStudioStore();
  const input = state.videoInputImage;
  return (
    <Field label="首帧参考图（可选）">
      <button
        className="media-history-reference-button"
        disabled={disabled || state.isImportingVideoInput || state.history.length === 0}
        onClick={onChooseHistory}
        type="button"
      ><span aria-hidden="true">▧</span>从创作记录选择</button>
      {input ? (
        <div className="media-video-input-card">
          {input.previewUrl ? <img alt="视频首帧参考图" src={input.previewUrl} /> : <span>▧</span>}
          <strong title={input.name}>{input.name}</strong>
          <button disabled={disabled} onClick={() => void state.pickVideoInputImage(port)} type="button">更换</button>
          <button aria-label="移除视频首帧参考图" disabled={disabled} onClick={() => void state.removeVideoInputImage(port)} type="button">×</button>
        </div>
      ) : (
        <button
          className="media-local-reference-button media-video-input-button"
          disabled={disabled || state.isImportingVideoInput}
          onClick={() => void state.pickVideoInputImage(port)}
          type="button"
        ><span aria-hidden="true">▧</span><strong>添加首帧参考图</strong><small>PNG、JPEG 或 WebP，最大 20 MB</small></button>
      )}
      {state.isImportingVideoInput && <div className="media-reference-loading"><span className="mini-spinner" />正在读取参考图…</div>}
    </Field>
  );
}

function VideoAudioSelector({ port, disabled }: { port: MediaStudioPort; disabled: boolean }) {
  const state = useMediaStudioStore();
  const audio = state.videoReferenceAudio;
  return (
    <Field label="参考音频（可选）">
      {audio ? (
        <div className="media-video-audio-card">
          <span aria-hidden="true">◉</span><strong title={audio.name}>{audio.name}</strong>
          <small>{formatBytes(audio.byteSize)}</small>
          <button disabled={disabled} onClick={() => void state.pickVideoReferenceAudio(port)} type="button">更换</button>
          <button aria-label="移除视频参考音频" disabled={disabled} onClick={() => void state.removeVideoReferenceAudio(port)} type="button">×</button>
        </div>
      ) : (
        <button
          className="media-local-reference-button media-video-audio-button"
          disabled={disabled || state.isImportingVideoAudio}
          onClick={() => void state.pickVideoReferenceAudio(port)}
          type="button"
        ><span aria-hidden="true">◉</span><strong>添加参考音频</strong><small>MP3、WAV、M4A 或 AAC，最大 20 MB</small></button>
      )}
      {state.isImportingVideoAudio && <div className="media-reference-loading"><span className="mini-spinner" />正在读取参考音频…</div>}
    </Field>
  );
}

function CreationCanvas({ onPreview }: { onPreview(url: string, alt: string): void }) {
  const state = useMediaStudioStore();
  const items = mediaStudioDisplayItems(state);
  const latest = items[0];
  const item = latest ? displayItem(latest) : null;

  return (
    <section className="media-studio-card media-canvas" aria-label="创作画布">
      <header className="media-card-header">
        <div>
          <h2>创作画布</h2>
          <p>{item ? `${item.modelName} · ${item.images.length} 张图片` : "结果会显示在这里，不会进入聊天记录"}</p>
        </div>
        {state.isGenerating ? (
          <span className="media-status-pill media-status-pill--working">
            <span className="mini-spinner" />正在生成
          </span>
        ) : item ? (
          <span className="media-status-pill media-status-pill--done"><span>●</span>已完成</span>
        ) : null}
      </header>
      <div className="media-canvas__body">
        {state.isLoadingHistory && !item ? (
          <StudioState icon={<span className="mini-spinner" />} title="正在读取创作记录…" />
        ) : item ? (
          <ResultItem item={latest!} onPreview={onPreview} />
        ) : (
          <EmptyCanvas />
        )}
      </div>
      {state.historyErrorMessage && (
        <InlineNotice message={state.historyErrorMessage} tone="warning" />
      )}
    </section>
  );
}

function EmptyCanvas() {
  const setPrompt = useMediaStudioStore((state) => state.setPrompt);
  const hasReference = useMediaStudioStore((state) => state.references.length > 0);
  const suggestions = [
    ["电影感人像", "电影感人像摄影，柔和侧光，细腻肤质，浅景深，真实自然"],
    ["产品海报", "极简产品广告海报，干净背景，工作室灯光，高级材质与留白"],
    ["概念场景", "宏大的未来城市概念场景，黄昏薄雾，电影级构图，丰富空间层次"],
  ] as const;
  return (
    <div className="media-empty-canvas">
      <span aria-hidden="true" className="media-empty-canvas__icon">▧</span>
      <div>
        <h2>描述你想看到的画面</h2>
        <p>{hasReference
          ? "参考图已经添加。请在右侧描述希望保留、修改或重新设计的内容。"
          : "从一个清晰的主体、场景或风格开始，也可以添加参考图进行图生图。"}</p>
      </div>
      <div className="media-suggestions">
        {suggestions.map(([title, prompt]) => (
          <button key={title} onClick={() => setPrompt(prompt)} type="button">{title}</button>
        ))}
      </div>
    </div>
  );
}

function ResultItem({
  item,
  onPreview,
  compact = false,
}: {
  item: MediaStudioDisplayItem;
  onPreview(url: string, alt: string): void;
  compact?: boolean;
}) {
  const value = displayItem(item);
  return (
    <article className={`media-result ${compact ? "media-result--compact" : ""}`}>
      <div className="media-result__images">
        {value.images.map((asset, index) => {
          const url = mediaStudioAssetPreviewUrl(asset);
          const alt = `${value.prompt || "生成图片"} ${index + 1}`;
          return url ? (
            <button key={asset.id} onClick={() => onPreview(url, alt)} type="button">
              <img alt={alt} loading="lazy" src={url} />
              <span aria-hidden="true">⌕</span>
            </button>
          ) : (
            <div className="media-result__unavailable" key={asset.id}>图片预览不可用</div>
          );
        })}
      </div>
      <div className="media-result__prompt">
        <span>提示词</span>
        <p>{value.prompt}</p>
      </div>
      {item.source === "generatedWithoutPersistence" && (
        <div className="media-result__unsaved">仅保留在当前页面，尚未写入本机创作记录</div>
      )}
    </article>
  );
}

function GenerationSettings({
  port,
  onChooseHistory,
}: {
  port: MediaStudioPort;
  onChooseHistory(): void;
}) {
  const state = useMediaStudioStore();
  const selectedModel = state.models.find((model) => model.id === state.selectedModelId);
  const canGenerate = canGenerateMediaStudioImage(state);

  return (
    <aside className="media-studio-card media-settings" aria-label="生成设置">
      <header className="media-settings__header">
        <h2>生成设置</h2>
        <button
          aria-label="刷新模型"
          disabled={state.isLoadingModels || state.isGenerating}
          onClick={() => void state.refreshModels(port)}
          title="刷新模型"
          type="button"
        >↻</button>
      </header>
      <div className="media-settings__scroll">
        <Field label="模型">
          {state.isLoadingModels && state.models.length === 0 ? (
            <div className="media-model-state"><span className="mini-spinner" />正在读取模型…</div>
          ) : state.models.length === 0 ? (
            <div className="media-model-empty">
              <strong>没有可用模型</strong>
              <span>{state.modelsErrorMessage ?? "请先配置一个全局启用且具有 API 密钥的模型。"}</span>
            </div>
          ) : (
            <div className="media-model-select">
              <span aria-hidden="true">✦</span>
              <select
                aria-label="图片模型"
                disabled={state.isGenerating}
                onChange={(event) => state.setSelectedModel(event.target.value)}
                value={state.selectedModelId ?? ""}
              >
                {state.models.map((model) => (
                  <option key={model.id} value={model.id}>{model.name} · {model.modelName}</option>
                ))}
              </select>
            </div>
          )}
          {selectedModel && !selectedModel.taskEnabled && (
            <small className="media-field-note">ⓘ 未用于任务，但仍可用于独立创作</small>
          )}
        </Field>

        <ReferenceSelector onChooseHistory={onChooseHistory} port={port} />

        <Field label="提示词">
          <textarea
            aria-label="提示词"
            disabled={state.isGenerating}
            maxLength={100_000}
            onChange={(event) => state.setPrompt(event.target.value)}
            placeholder="描述主体、场景、光线、构图与风格…"
            value={state.prompt}
          />
        </Field>

        <div className="media-settings__row">
          <Field label="尺寸">
            <select
              aria-label="图片尺寸"
              disabled={state.isGenerating}
              onChange={(event) => state.setSize(event.target.value)}
              value={state.size}
            >
              {supportedImageSizes.map((size) => (
                <option key={size} value={size}>{size === "auto" ? "自动" : size.replace("x", " × ")}</option>
              ))}
            </select>
          </Field>
          <Field label="数量">
            <div className="media-count-stepper">
              <button
                aria-label="减少图片数量"
                disabled={state.isGenerating || state.count <= 1}
                onClick={() => state.setCount(state.count - 1)}
                type="button"
              >−</button>
              <output aria-label="图片数量">{state.count}</output>
              <button
                aria-label="增加图片数量"
                disabled={state.isGenerating || state.count >= 4}
                onClick={() => state.setCount(state.count + 1)}
                type="button"
              >＋</button>
            </div>
          </Field>
        </div>

        {state.modelsErrorMessage && state.models.length > 0 && (
          <InlineNotice message={state.modelsErrorMessage} tone="warning" />
        )}
        {state.generationErrorMessage && (
          <InlineNotice
            action={<button onClick={state.dismissGenerationError} type="button">关闭</button>}
            message={state.generationErrorMessage}
            tone="warning"
          />
        )}
        <button
          className="media-generate-button"
          disabled={!canGenerate}
          onClick={() => void state.generate(port)}
          type="button"
        >
          {state.isGenerating ? <span className="mini-spinner" /> : <span aria-hidden="true">✦</span>}
          {state.isGenerating
            ? "正在生成…"
            : state.references.length > 0 ? "基于参考图生成" : "生成图片"}
        </button>
      </div>
    </aside>
  );
}

function ReferenceSelector({
  port,
  onChooseHistory,
}: {
  port: MediaStudioPort;
  onChooseHistory(): void;
}) {
  const state = useMediaStudioStore();
  const disabled = state.isGenerating || state.isImportingReferences;
  return (
    <Field
      label="参考图（可多选）"
      trailing={state.references.length > 0 ? <span className="media-reference-count">{state.references.length} / 8</span> : null}
    >
      <button
        className="media-history-reference-button"
        disabled={disabled || state.references.length >= 8 || state.history.length === 0}
        onClick={onChooseHistory}
        type="button"
      ><span aria-hidden="true">▧</span>从生成记录选择多张</button>
      {state.references.length > 0 ? (
        <div className="media-reference-panel">
          <div className="media-reference-grid">
            {state.references.map((reference, index) => (
              <div className="media-reference-card" key={reference.id}>
                {reference.previewUrl
                  ? <img alt={`参考图 ${index + 1}`} src={reference.previewUrl} />
                  : <span aria-hidden="true">▧</span>}
                <button
                  aria-label={`移除参考图 ${index + 1}`}
                  disabled={disabled}
                  onClick={() => void state.removeReference(port, reference.id)}
                  type="button"
                >×</button>
                <small>#{index + 1} · {reference.name}</small>
              </div>
            ))}
          </div>
          <button
            className="media-add-more"
            disabled={disabled || state.references.length >= 8}
            onClick={() => void state.pickReferences(port)}
            type="button"
          >＋ 继续添加</button>
        </div>
      ) : (
        <button
          className="media-local-reference-button"
          disabled={disabled}
          onClick={() => void state.pickReferences(port)}
          type="button"
        >
          <span aria-hidden="true">▧</span>
          <strong>添加一张或多张参考图</strong>
          <small>最多 8 张；每张最大 20 MB</small>
        </button>
      )}
      {state.isImportingReferences && (
        <div className="media-reference-loading"><span className="mini-spinner" />正在读取参考图…</div>
      )}
      {state.referenceErrorMessages.map((message, index) => (
        <InlineNotice key={`${message}-${index}`} message={message} tone="warning" />
      ))}
    </Field>
  );
}

function HistoryWorkspace({
  port,
  onPreview,
}: {
  port: MediaStudioPort;
  onPreview(url: string, alt: string): void;
}) {
  const state = useMediaStudioStore();
  const items = mediaStudioDisplayItems(state);
  const isLoading = state.isLoadingHistory || state.isLoadingVideoHistory;
  const isEmpty = items.length === 0 && state.videoHistory.length === 0;
  return (
    <div className="media-history-workspace">
      <header>
        <div><h2>创作记录</h2><p>当前账号保存在本机的图片和视频，以及本次尚未保存的图片结果。</p></div>
        <button
          disabled={isLoading}
          onClick={() => void Promise.all([state.refreshHistory(port), state.refreshVideoHistory(port)])}
          type="button"
        >
          {isLoading ? <span className="mini-spinner" /> : "↻"} 刷新
        </button>
      </header>
      {state.historyErrorMessage && <InlineNotice message={state.historyErrorMessage} tone="warning" />}
      {state.videoHistoryErrorMessage && <InlineNotice message={state.videoHistoryErrorMessage} tone="warning" />}
      {isEmpty && isLoading ? (
        <StudioState icon={<span className="mini-spinner" />} title="正在读取创作记录…" />
      ) : isEmpty ? (
        <StudioState icon="◷" title="还没有创作记录" description="生成的图片和视频会按账号保存在本机。" />
      ) : (
        <div className="media-history-list">
          {state.videoHistory.length > 0 && (
            <section className="media-history-group" aria-label="视频记录">
              <header><div><h3>视频</h3><p>异步生成完成并保存到本机的视频</p></div><span>{state.videoHistory.length} 个</span></header>
              <div className="media-video-history-grid">
                {state.videoHistory.map((item) => (
                  <section className="media-history-entry media-video-history-entry" key={item.id}>
                    <header><div><strong>{item.modelName}</strong><small>{formatDate(item.createdAtUnixMs)}</small></div><span>视频</span></header>
                    <VideoResult compact item={item} />
                  </section>
                ))}
              </div>
              {state.videoHistoryCursor && (
                <button
                  className="media-load-more"
                  disabled={state.isLoadingOlderVideoHistory}
                  onClick={() => void state.loadOlderVideoHistory(port)}
                  type="button"
                >{state.isLoadingOlderVideoHistory ? <><span className="mini-spinner" />正在读取…</> : "载入更早的视频"}</button>
              )}
            </section>
          )}
          {items.length > 0 && <div className="media-history-group__heading"><div><h3>图片</h3><p>生成图片与当前页面的临时结果</p></div><span>{items.length} 项</span></div>}
          {items.map((item) => {
            const value = displayItem(item);
            return (
              <section className="media-history-entry" key={`${item.source}-${value.id}`}>
                <header><div><strong>{value.modelName}</strong><small>{formatDate(value.createdAtUnixMs)}</small></div><span>{value.images.length} 张</span></header>
                <ResultItem compact item={item} onPreview={onPreview} />
              </section>
            );
          })}
          {state.historyCursor && (
            <button
              className="media-load-more"
              disabled={state.isLoadingOlderHistory}
              onClick={() => void state.loadOlderHistory(port)}
              type="button"
            >{state.isLoadingOlderHistory ? <><span className="mini-spinner" />正在读取…</> : "载入更早的记录"}</button>
          )}
        </div>
      )}
    </div>
  );
}

function HistoryReferencePicker({
  history,
  currentCount,
  maximumCount,
  mode,
  onDismiss,
  onSelect,
}: {
  history: MediaStudioHistoryItem[];
  currentCount: number;
  maximumCount: number;
  mode: "image" | "video";
  onDismiss(): void;
  onSelect(assets: Array<{ recordId: string; contentToken: string }>): void;
}) {
  useDismissOnEscape(onDismiss);
  const [selected, setSelected] = useState<string[]>([]);
  const maximum = Math.max(0, maximumCount - currentCount);
  const assets = useMemo(() => history.flatMap((item) => item.images.map((asset) => ({
    key: `${item.id}:${asset.contentToken}`,
    recordId: item.id,
    contentToken: asset.contentToken,
    previewUrl: asset.previewUrl,
    prompt: item.prompt,
  }))), [history]);
  return (
    <div className="media-sheet-backdrop" onMouseDown={(event) => { if (event.target === event.currentTarget) onDismiss(); }}>
      <section aria-label="从生成记录选择参考图" aria-modal="true" className="media-reference-sheet" role="dialog">
        <header>
          <div><h2>{mode === "video" ? "选择视频首帧" : "从生成记录选择"}</h2><p>最多还可以选择 {maximum} 张图片。</p></div>
          <button aria-label="关闭参考图选择" autoFocus onClick={onDismiss} type="button">×</button>
        </header>
        <div className="media-reference-sheet__grid">
          {assets.map((asset) => {
            const active = selected.includes(asset.key);
            return (
              <button
                aria-pressed={active}
                className={active ? "is-selected" : ""}
                disabled={!asset.previewUrl || (!active && selected.length >= maximum)}
                key={asset.key}
                onClick={() => setSelected((current) => active
                  ? current.filter((key) => key !== asset.key)
                  : [...current, asset.key])}
                type="button"
              >
                {asset.previewUrl ? <img alt={asset.prompt || "生成图片"} src={asset.previewUrl} /> : <span>预览不可用</span>}
                <i aria-hidden="true">{active ? "✓" : ""}</i>
              </button>
            );
          })}
        </div>
        <footer>
          <span>已选择 {selected.length} / {maximum}</span>
          <button className="is-secondary" onClick={onDismiss} type="button">取消</button>
          <button
            disabled={selected.length === 0}
            onClick={() => onSelect(assets.filter((asset) => selected.includes(asset.key)).map((asset) => ({
              recordId: asset.recordId,
              contentToken: asset.contentToken,
            })))}
            type="button"
          >{mode === "video" ? "设为首帧" : "添加参考图"}</button>
        </footer>
      </section>
    </div>
  );
}

function ImagePreview({
  url,
  alt,
  onDismiss,
}: {
  url: string;
  alt: string;
  onDismiss(): void;
}) {
  useDismissOnEscape(onDismiss);
  return (
    <div className="media-preview-backdrop" onMouseDown={(event) => { if (event.target === event.currentTarget) onDismiss(); }}>
      <section aria-label="图片预览" aria-modal="true" className="media-preview" role="dialog">
        <button aria-label="关闭图片预览" autoFocus onClick={onDismiss} type="button">×</button>
        <img alt={alt} src={url} />
      </section>
    </div>
  );
}

function Field({
  label,
  trailing,
  children,
}: {
  label: string;
  trailing?: ReactNode;
  children: ReactNode;
}) {
  return (
    <div className="media-field">
      <div className="media-field__label"><span>{label}</span>{trailing}</div>
      {children}
    </div>
  );
}

function InlineNotice({
  message,
  tone,
  action,
}: {
  message: string;
  tone: "warning";
  action?: ReactNode;
}) {
  return (
    <div className={`media-inline-notice media-inline-notice--${tone}`} role="status">
      <span aria-hidden="true">⚠</span><p>{message}</p>{action}
    </div>
  );
}

function StudioState({
  icon,
  title,
  description,
}: {
  icon: ReactNode;
  title: string;
  description?: string;
}) {
  return (
    <div className="media-studio-state">
      <span aria-hidden="true">{icon}</span><h2>{title}</h2>{description && <p>{description}</p>}
    </div>
  );
}

function displayItem(item: MediaStudioDisplayItem): {
  id: string;
  prompt: string;
  modelName: string;
  createdAtUnixMs: number;
  images: Array<MediaStudioHistoryAsset | MediaStudioGeneratedAsset>;
} {
  return item.source === "history" ? item.item : item;
}

function formatDate(value: number) {
  return new Intl.DateTimeFormat("zh-CN", {
    year: "numeric",
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  }).format(new Date(value));
}

function videoProgressText(
  status: "submitting" | import("../domain/mediaStudio").MediaStudioVideoJobStatus,
  percent: number | null,
) {
  const labels: Record<typeof status, string> = {
    submitting: "提交中",
    queued: "排队中",
    in_progress: "生成中",
    unknown: "等待状态更新",
    downloading: "下载中",
    saving: "保存中",
    failed: "生成失败",
    completed: "已完成",
    cancelled: "已取消",
  };
  const label = labels[status];
  return percent == null ? label : `${label} · ${Math.round(percent)}%`;
}

function formatBytes(value: number) {
  if (value < 1_024) return `${value} B`;
  if (value < 1_024 * 1_024) return `${Math.round(value / 1_024)} KB`;
  return `${(value / (1_024 * 1_024)).toFixed(1)} MB`;
}

function useDismissOnEscape(onDismiss: () => void) {
  useEffect(() => {
    const dismiss = (event: KeyboardEvent) => {
      if (event.key === "Escape") onDismiss();
    };
    window.addEventListener("keydown", dismiss);
    return () => window.removeEventListener("keydown", dismiss);
  }, [onDismiss]);
}
