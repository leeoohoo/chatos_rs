import { useEffect, type ClipboardEvent } from "react";
import { usePetTranslationStore } from "../application/petTranslationStore";
import type { PetTranslationPort } from "../ports/PetTranslationPort";
import type { PetTranslationOutputStyle, PetTranslationRecord, PetTranslationTarget } from "../domain/petTranslation";
import type { AuthenticationPort } from "../../authentication/ports/AuthenticationPort";
import { formatAttachmentSize, type ConversationAttachmentDraft } from "../../conversation-attachments/domain/conversationAttachment";
import "./petTranslation.css";

export function PetTranslationSurface({ authentication, port }: { authentication: AuthenticationPort; port: PetTranslationPort }) {
  const state = usePetTranslationStore();
  const closeSurface = () => {
    const current = usePetTranslationStore.getState();
    const ids = current.attachments.map(({ id }) => id);
    const ownerUserId = current.session?.user.id;
    if (ownerUserId && ids.length) {
      void port.discardAttachments({ ownerUserId, conversationId: "pet-translation", draftGeneration: 0 }, ids)
        .catch(() => undefined)
        .finally(() => void port.close());
      return;
    }
    void port.close();
  };

  useEffect(() => {
    let active = true;
    void authentication.restoreSession().then((session) => {
      if (active) void usePetTranslationStore.getState().load(port, session);
    }).catch((failure) => {
      if (active) usePetTranslationStore.setState({ error: errorMessage(failure), isLoading: false });
    });
    return () => { active = false; };
  }, [port]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        closeSurface();
      }
      if ((event.ctrlKey || event.metaKey) && event.key === "Enter") {
        event.preventDefault();
        void usePetTranslationStore.getState().translate(port);
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [port]);

  const english = state.target === "english";
  const translateDisabled = state.isTranslating || state.isLoading || (!state.sourceText.trim() && !state.attachments.length) || !state.session;
  const handlePaste = (event: ClipboardEvent<HTMLTextAreaElement>) => {
    const hasImage = [...event.clipboardData.types].some((type) => type.startsWith("image/") || type === "Files");
    if (!hasImage) return;
    event.preventDefault();
    void state.importClipboard(port);
  };
  return (
    <main className="pet-translation-surface">
      <header className="pet-translation-titlebar" data-tauri-drag-region>
        <div className="pet-translation-titlebar__icon" aria-hidden="true">文</div>
        <div data-tauri-drag-region>
          <strong>{english ? "Quick Translate" : "快速翻译"}</strong>
          <span>{english ? "Translate text with your configured AI model" : "使用已配置的 AI 模型翻译文字"}</span>
        </div>
        <button aria-label={english ? "Close" : "关闭"} className="pet-translation-titlebar__close" onClick={closeSurface} type="button">×</button>
      </header>

      <div className="pet-translation-layout">
        <section className="pet-translation-editor" aria-label={english ? "Translator" : "翻译器"}>
          <div className="pet-translation-toolbar">
            <div className="pet-translation-segmented" role="group" aria-label={english ? "Target language" : "目标语言"}>
              <TargetButton active={state.target === "automatic"} label={english ? "Auto" : "自动"} onClick={() => state.setTarget("automatic")} />
              <TargetButton active={state.target === "simplifiedChinese"} label="中文" onClick={() => state.setTarget("simplifiedChinese")} />
              <TargetButton active={state.target === "english"} label="English" onClick={() => state.setTarget("english")} />
            </div>
            <div className="pet-translation-segmented" role="group" aria-label={english ? "Output style" : "输出样式"}>
              <StyleButton active={state.outputStyle === "translationOnly"} label={english ? "Translation" : "仅译文"} onClick={() => state.setOutputStyle("translationOnly")} />
              <StyleButton active={state.outputStyle === "bilingual"} label={english ? "Bilingual" : "双语"} onClick={() => state.setOutputStyle("bilingual")} />
            </div>
          </div>
          <div className="pet-translation-panels">
            <label className="pet-translation-panel">
              <span>{english ? "Source" : "原文"}</span>
              <textarea
                aria-label={english ? "Source text" : "原文"}
                autoFocus
                maxLength={20_000}
                onChange={(event) => state.setSourceText(event.target.value)}
                onPaste={handlePaste}
                placeholder={english ? "Paste text to translate…" : "粘贴或输入要翻译的内容…"}
                value={state.sourceText}
              />
              <small>{state.sourceText.length.toLocaleString()} / 20,000</small>
            </label>
            <div className="pet-translation-panel pet-translation-panel--result" aria-live="polite">
              <span>{english ? "Result" : "译文"}</span>
              {state.isTranslating ? <div className="pet-translation-loading"><span className="mini-spinner" />{english ? "Translating…" : "正在翻译…"}</div> : state.translatedText ? <p>{state.translatedText}</p> : <div className="pet-translation-empty">{english ? "Your translation will appear here." : "翻译结果会显示在这里。"}</div>}
              {state.translatedText && <button className="pet-translation-copy" onClick={() => void copyText(state.translatedText)} type="button">{english ? "Copy" : "复制"}</button>}
            </div>
          </div>
          <div className="pet-translation-attachments" aria-label={english ? "Image attachments" : "图片附件"}>
            <div className="pet-translation-attachments__header">
              <span>{english ? "Images" : "图片"} ({state.attachments.length}/10)</span>
              <button
                disabled={state.isTranslating || state.isImportingAttachments || state.attachments.length >= 10}
                onClick={() => void state.pickAttachments(port)}
                type="button"
              >{state.isImportingAttachments ? (english ? "Reading…" : "读取中…") : (english ? "Add image" : "添加图片")}</button>
            </div>
            {state.attachments.length > 0 ? (
              <div className="pet-translation-attachments__list">
                {state.attachments.map((attachment) => <AttachmentChip english={english} key={attachment.id} onRemove={() => void state.removeAttachment(port, attachment.id)} attachment={attachment} />)}
              </div>
            ) : <small>{english ? "Paste a screenshot or add PNG, JPEG, GIF, or WebP images." : "可直接粘贴截图，或添加 PNG、JPEG、GIF、WebP 图片。"}</small>}
          </div>
          {state.attachmentError && <div className="pet-translation-error" role="alert"><span aria-hidden="true">!</span><p>{state.attachmentError}</p><button aria-label={english ? "Dismiss attachment error" : "关闭附件错误"} onClick={() => usePetTranslationStore.setState({ attachmentError: null })} type="button">×</button></div>}
          {state.error && <div className="pet-translation-error" role="alert"><span aria-hidden="true">!</span><p>{state.error}</p><button aria-label={english ? "Dismiss error" : "关闭错误"} onClick={state.clearError} type="button">×</button></div>}
          <div className="pet-translation-actions">
            <span>{english ? "Ctrl + Enter to translate" : "Ctrl + Enter 翻译"}</span>
            <button className="pet-translation-submit" disabled={translateDisabled} onClick={() => void state.translate(port)} type="button">{state.isTranslating ? (english ? "Translating…" : "翻译中…") : (english ? "Translate" : "翻译")}</button>
          </div>
        </section>

        <aside className="pet-translation-history" aria-label={english ? "Translation history" : "翻译历史"}>
          <div className="pet-translation-history__header"><div><strong>{english ? "History" : "翻译历史"}</strong><span>{english ? "Saved on this device" : "仅保存在本机"}</span></div><button disabled={!state.records.length || state.isTranslating} onClick={() => void state.clear(port)} type="button">{english ? "Clear" : "清空"}</button></div>
          {state.isLoading ? <div className="pet-translation-history__empty"><span className="mini-spinner" />{english ? "Loading…" : "正在读取…"}</div> : state.records.length === 0 ? <div className="pet-translation-history__empty"><span aria-hidden="true">↔</span>{english ? "No translations yet" : "还没有翻译记录"}</div> : <div className="pet-translation-history__list">{state.records.map((record) => <HistoryRow english={english} key={record.id} onClick={() => state.selectRecord(record)} record={record} selected={record.id === state.selectedRecordId} />)}</div>}
        </aside>
      </div>
    </main>
  );
}

function AttachmentChip({ english, attachment, onRemove }: { english: boolean; attachment: ConversationAttachmentDraft; onRemove(): void }) {
  return <div className="pet-translation-attachment-chip"><span aria-hidden="true">▧</span><span><strong>{attachment.name}</strong><small>{formatAttachmentSize(attachment.byteSize)}</small></span><button aria-label={`${english ? "Remove" : "移除"} ${attachment.name}`} onClick={onRemove} type="button">×</button></div>;
}

function TargetButton({ active, label, onClick }: { active: boolean; label: string; onClick(): void }) {
  return <button aria-pressed={active} className={active ? "is-active" : ""} onClick={onClick} type="button">{label}</button>;
}

function StyleButton({ active, label, onClick }: { active: boolean; label: string; onClick(): void }) {
  return <button aria-pressed={active} className={active ? "is-active" : ""} onClick={onClick} type="button">{label}</button>;
}

function HistoryRow({ english, onClick, record, selected }: { english: boolean; onClick(): void; record: PetTranslationRecord; selected: boolean }) {
  const preview = record.sourceText.replace(/\s+/gu, " ").trim() || (english ? "Image translation" : "图片翻译");
  const date = new Date(record.createdAtUnixMs).toLocaleString(english ? "en-US" : "zh-CN", { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" });
  const direction = record.attachments.length
    ? "▧"
    : record.target === "automatic" ? (english ? "Auto" : "中英自动")
      : record.target === "english" ? "中→英" : "英→中";
  return <button className={`pet-translation-history__row ${selected ? "is-selected" : ""}`} onClick={onClick} type="button"><span className="pet-translation-history__arrow">{direction}</span><span><strong>{preview}</strong><small>{date} · {record.outputStyle === "bilingual" ? (english ? "Bilingual" : "双语") : (english ? "Translation" : "仅译文")}</small></span></button>;
}

async function copyText(value: string) {
  await navigator.clipboard.writeText(value);
}

function errorMessage(failure: unknown): string {
  if (failure instanceof Error && failure.message.trim()) return failure.message;
  if (typeof failure === "string" && failure.trim()) return failure;
  return "请求失败，请稍后重试。";
}
