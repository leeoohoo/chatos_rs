import { useEffect, useMemo, useState, type ReactNode } from "react";
import {
  canGenerateMediaStudioImage,
  mediaStudioAssetPreviewUrl,
  mediaStudioDisplayItems,
  supportedImageSizes,
  useMediaStudioStore,
  type MediaStudioDisplayItem,
} from "../application/mediaStudioStore";
import type {
  MediaStudioGeneratedAsset,
  MediaStudioHistoryAsset,
  MediaStudioHistoryItem,
} from "../domain/mediaStudio";
import type { MediaStudioPort } from "../ports/MediaStudioPort";
import "./mediaStudio.css";

interface MediaStudioViewProps {
  ownerUserId: string;
  port: MediaStudioPort;
}

type StudioSection = "image" | "history";

export function MediaStudioView({ ownerUserId, port }: MediaStudioViewProps) {
  const store = useMediaStudioStore();
  const [section, setSection] = useState<StudioSection>("image");
  const [historyPickerOpen, setHistoryPickerOpen] = useState(false);
  const [preview, setPreview] = useState<{ url: string; alt: string } | null>(null);

  useEffect(() => {
    void store.activate(port, ownerUserId);
    return () => store.deactivate(port);
  }, [ownerUserId, port, store.activate, store.deactivate]);

  return (
    <section className="media-studio" aria-label="AI 创作">
      <StudioHeader section={section} onSelect={setSection} />
      {section === "image" ? (
        <ImageWorkspace
          onChooseHistory={() => setHistoryPickerOpen(true)}
          onPreview={(url, alt) => setPreview({ url, alt })}
          port={port}
        />
      ) : (
        <HistoryWorkspace
          onPreview={(url, alt) => setPreview({ url, alt })}
          port={port}
        />
      )}
      {historyPickerOpen && (
        <HistoryReferencePicker
          currentCount={store.references.length}
          history={store.history}
          onDismiss={() => setHistoryPickerOpen(false)}
          onSelect={(assets) => {
            setHistoryPickerOpen(false);
            void store.addHistoryReferences(port, assets);
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
  onSelect,
}: {
  section: StudioSection;
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
        <button disabled title="视频能力将在完成 macOS 准入后开放" type="button">
          <span aria-hidden="true">▶</span>视频
        </button>
        <button disabled title="剧情模式将在完成 macOS 准入后开放" type="button">
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
  return (
    <div className="media-history-workspace">
      <header>
        <div><h2>创作记录</h2><p>当前账号保存在本机的图片，以及本次尚未保存的生成结果。</p></div>
        <button disabled={state.isLoadingHistory} onClick={() => void state.refreshHistory(port)} type="button">
          {state.isLoadingHistory ? <span className="mini-spinner" /> : "↻"} 刷新
        </button>
      </header>
      {state.historyErrorMessage && <InlineNotice message={state.historyErrorMessage} tone="warning" />}
      {items.length === 0 && state.isLoadingHistory ? (
        <StudioState icon={<span className="mini-spinner" />} title="正在读取创作记录…" />
      ) : items.length === 0 ? (
        <StudioState icon="◷" title="还没有创作记录" description="生成的图片会按账号保存在本机。" />
      ) : (
        <div className="media-history-list">
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
  onDismiss,
  onSelect,
}: {
  history: MediaStudioHistoryItem[];
  currentCount: number;
  onDismiss(): void;
  onSelect(assets: Array<{ recordId: string; contentToken: string }>): void;
}) {
  useDismissOnEscape(onDismiss);
  const [selected, setSelected] = useState<string[]>([]);
  const maximum = Math.max(0, 8 - currentCount);
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
          <div><h2>从生成记录选择</h2><p>最多还可以选择 {maximum} 张图片。</p></div>
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
          >添加参考图</button>
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

function useDismissOnEscape(onDismiss: () => void) {
  useEffect(() => {
    const dismiss = (event: KeyboardEvent) => {
      if (event.key === "Escape") onDismiss();
    };
    window.addEventListener("keydown", dismiss);
    return () => window.removeEventListener("keydown", dismiss);
  }, [onDismiss]);
}
