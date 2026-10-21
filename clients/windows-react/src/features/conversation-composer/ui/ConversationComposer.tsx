import { useEffect, useState, type ClipboardEvent, type KeyboardEvent } from "react";
import {
  formatAttachmentSize,
  shouldImportPaste,
  type ConversationAttachmentDraft,
} from "../../conversation-attachments/domain/conversationAttachment";
import type { ConversationAttachmentPort } from "../../conversation-attachments/ports/ConversationAttachmentPort";
import type { ConversationHistorySnapshot } from "../../conversation-history/domain/conversationHistory";
import { useConversationComposerStore } from "../application/conversationComposerStore";
import type { ConversationComposerPort } from "../ports/ConversationComposerPort";
import { ConversationRuntimeControls } from "../../conversation-runtime-settings/ui/ConversationRuntimeControls";
import type { ConversationRuntimeSettingsPort } from "../../conversation-runtime-settings/ports/ConversationRuntimeSettingsPort";
import {
  AVAILABLE_COMPOSER_CAPABILITIES,
  filterCapabilityCandidates,
  trailingCapabilityQuery,
  type ComposerCapabilityCandidate,
} from "../domain/conversationComposer";

export function ConversationComposer({
  ownerUserId,
  conversationId,
  history,
  port,
  attachmentPort,
  runtimeSettingsPort,
  availableCapabilities = AVAILABLE_COMPOSER_CAPABILITIES,
  showRuntimeControls = true,
  onAccepted,
}: {
  ownerUserId: string;
  conversationId: string;
  history: ConversationHistorySnapshot;
  port: ConversationComposerPort;
  attachmentPort: ConversationAttachmentPort;
  runtimeSettingsPort: ConversationRuntimeSettingsPort;
  availableCapabilities?: readonly ComposerCapabilityCandidate[];
  showRuntimeControls?: boolean;
  onAccepted(): Promise<void>;
}) {
  const draft = useConversationComposerStore((state) => state.draft);
  const selectedCapability = useConversationComposerStore(
    (state) => state.explicitCapabilitySelection,
  );
  const attachments = useConversationComposerStore((state) => state.attachments);
  const attachmentError = useConversationComposerStore((state) => state.attachmentError);
  const isImportingAttachments = useConversationComposerStore(
    (state) => state.isImportingAttachments,
  );
  const isAttachmentDropTargeted = useConversationComposerStore(
    (state) => state.isAttachmentDropTargeted,
  );
  const attachmentDraftGeneration = useConversationComposerStore(
    (state) => state.attachmentDraftGeneration,
  );
  const suggestionsSuppressed = useConversationComposerStore(
    (state) => state.capabilitySuggestionsSuppressed,
  );
  const highlightedCapabilityId = useConversationComposerStore(
    (state) => state.highlightedCapabilityId,
  );
  const isSending = useConversationComposerStore((state) => state.isSending);
  const error = useConversationComposerStore((state) => state.error);
  const setDraft = useConversationComposerStore((state) => state.setDraft);
  const setSelectedCapability = useConversationComposerStore(
    (state) => state.setExplicitCapabilitySelection,
  );
  const dismissCapabilitySuggestions = useConversationComposerStore(
    (state) => state.dismissCapabilitySuggestions,
  );
  const moveCapabilityHighlight = useConversationComposerStore(
    (state) => state.moveCapabilityHighlight,
  );
  const selectCapability = useConversationComposerStore((state) => state.selectCapability);
  const activateAttachmentScope = useConversationComposerStore(
    (state) => state.activateAttachmentScope,
  );
  const pickAttachments = useConversationComposerStore((state) => state.pickAttachments);
  const importClipboard = useConversationComposerStore((state) => state.importClipboard);
  const acceptAttachmentImport = useConversationComposerStore(
    (state) => state.acceptAttachmentImport,
  );
  const setAttachmentDragState = useConversationComposerStore(
    (state) => state.setAttachmentDragState,
  );
  const removeAttachment = useConversationComposerStore((state) => state.removeAttachment);
  const clearAttachmentError = useConversationComposerStore(
    (state) => state.clearAttachmentError,
  );
  const send = useConversationComposerStore((state) => state.send);
  const [previewedAttachment, setPreviewedAttachment] = useState<ConversationAttachmentDraft | null>(
    null,
  );
  const canSend = !isSending && (draft.trim().length > 0 || attachments.length > 0);
  const capabilityQuery = availableCapabilities.length === 0 || suggestionsSuppressed
    ? null
    : trailingCapabilityQuery(draft);
  const visibleCapabilities = capabilityQuery
    ? filterCapabilityCandidates(availableCapabilities, capabilityQuery).slice(0, 8)
    : [];

  useEffect(() => {
    const scope = {
      ownerUserId,
      conversationId,
      draftGeneration: attachmentDraftGeneration,
    };
    void activateAttachmentScope(attachmentPort, ownerUserId, conversationId);
    return () => { void attachmentPort.deactivateScope(scope); };
  }, [
    activateAttachmentScope,
    attachmentDraftGeneration,
    attachmentPort,
    conversationId,
    ownerUserId,
  ]);

  useEffect(() => {
    let disposed = false;
    let cleanup: (() => void)[] = [];
    void Promise.all([
      attachmentPort.onStaged((result) => acceptAttachmentImport(attachmentPort, result)),
      attachmentPort.onDragState(setAttachmentDragState),
    ]).then((listeners) => {
      if (disposed) listeners.forEach((listener) => listener());
      else cleanup = listeners;
    });
    return () => {
      disposed = true;
      cleanup.forEach((listener) => listener());
    };
  }, [acceptAttachmentImport, attachmentPort, setAttachmentDragState]);

  const submit = () => void send(
    port,
    ownerUserId,
    conversationId,
    history,
    attachmentPort,
    onAccepted,
  );
  const handlePaste = (event: ClipboardEvent<HTMLTextAreaElement>) => {
    const text = event.clipboardData.getData("text/plain");
    if (!shouldImportPaste([...event.clipboardData.types], text)) return;
    event.preventDefault();
    void importClipboard(attachmentPort, ownerUserId, conversationId).then((fallbackText) => {
      if (!fallbackText) return;
      const current = useConversationComposerStore.getState().draft;
      useConversationComposerStore.getState().setDraft(current + fallbackText);
    });
  };
  const handleKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (capabilityQuery) {
      if (event.key === "Escape") {
        event.preventDefault();
        dismissCapabilitySuggestions();
        return;
      }
      if (event.key === "ArrowUp" || event.key === "ArrowDown") {
        event.preventDefault();
        moveCapabilityHighlight(
          event.key === "ArrowDown" ? "down" : "up",
          availableCapabilities,
        );
        return;
      }
      if ((event.key === "Tab" || (event.key === "Enter" && !event.shiftKey))
          && !event.nativeEvent.isComposing) {
        event.preventDefault();
        const selected = visibleCapabilities.find(({ id }) => id === highlightedCapabilityId)
          ?? visibleCapabilities[0];
        if (selected) selectCapability(selected);
        return;
      }
    }
    if (event.key === "Enter" && !event.shiftKey && !event.nativeEvent.isComposing) {
      event.preventDefault();
      submit();
    }
  };

  return (
    <section
      className={isAttachmentDropTargeted
        ? "conversation-composer conversation-composer--drop-targeted"
        : "conversation-composer"}
      aria-label="消息输入"
    >
      {showRuntimeControls && (
        <ConversationRuntimeControls
          ownerUserId={ownerUserId}
          conversationId={conversationId}
          port={runtimeSettingsPort}
        />
      )}
      {selectedCapability ? (
        <div
          aria-label={`已选择能力：${selectedCapability.displayName}`}
          className="capability-chip"
        >
          <span aria-hidden="true" className="capability-chip__icon">✓</span>
          <span>{selectedCapability.displayName}</span>
          <button
            aria-label={`移除所选能力：${selectedCapability.displayName}`}
            onClick={() => setSelectedCapability(null)}
            type="button"
          >
            ×
          </button>
        </div>
      ) : null}
      {attachments.length ? (
        <div className="attachment-strip" aria-label="待发送附件">
          {attachments.map((attachment) => (
            <div className="attachment-chip" key={attachment.id}>
              <button
                className="attachment-chip__preview"
                onClick={() => setPreviewedAttachment(attachment)}
                type="button"
              >
                <AttachmentIcon attachment={attachment} />
                <span>
                  <strong>{attachment.name}</strong>
                  <small>{formatAttachmentSize(attachment.byteSize)}</small>
                </span>
              </button>
              <button
                aria-label={`移除附件：${attachment.name}`}
                className="attachment-chip__remove"
                onClick={() => void removeAttachment(attachmentPort, attachment.id)}
                type="button"
              >
                ×
              </button>
            </div>
          ))}
        </div>
      ) : null}
      {attachmentError ? (
        <div className="attachment-error" role="alert">
          <span aria-hidden="true">!</span>
          <p>{attachmentError}</p>
          <button aria-label="关闭附件错误" onClick={clearAttachmentError} type="button">×</button>
        </div>
      ) : null}
      {error && <p className="composer-error" role="alert">{error}</p>}
      <div className="composer-entry">
        {capabilityQuery ? (
          <div
            aria-label="能力建议"
            className="capability-suggestions"
            id="composer-capability-suggestions"
            role="listbox"
          >
            {visibleCapabilities.map((candidate) => {
              const highlighted = candidate.id === highlightedCapabilityId;
              return (
                <button
                  aria-selected={highlighted}
                  className={highlighted ? "capability-option capability-option--highlighted" : "capability-option"}
                  id={`capability-option-${candidate.id}`}
                  key={candidate.id}
                  onClick={() => selectCapability(candidate)}
                  role="option"
                  type="button"
                >
                  <span aria-hidden="true" className="capability-option__icon">✓</span>
                  <span className="capability-option__copy">
                    <strong>{candidate.selection.displayName}</strong>
                    <small>{candidate.subtitle}</small>
                  </span>
                  {highlighted ? <kbd aria-hidden="true">↩</kbd> : null}
                </button>
              );
            })}
            {visibleCapabilities.length === 0 ? (
              <p className="capability-empty">没有匹配的能力</p>
            ) : null}
          </div>
        ) : null}
        <div className="composer-input">
          <button
            aria-label="添加附件"
            className="composer-attachment"
            disabled={isSending}
            onClick={() => void pickAttachments(attachmentPort, ownerUserId, conversationId)}
            type="button"
          >
            {isImportingAttachments
              ? <span className="mini-spinner" aria-hidden="true" />
              : <span aria-hidden="true">＋</span>}
          </button>
          <textarea
            aria-activedescendant={capabilityQuery && highlightedCapabilityId
              ? `capability-option-${highlightedCapabilityId}`
              : undefined}
            aria-controls={capabilityQuery ? "composer-capability-suggestions" : undefined}
            aria-expanded={Boolean(capabilityQuery)}
            aria-label="消息"
            aria-haspopup="listbox"
            onChange={(event) => setDraft(event.target.value)}
            onKeyDown={handleKeyDown}
            onPaste={handlePaste}
            placeholder="输入消息…"
            rows={1}
            value={draft}
          />
          <button
            aria-label="发送"
            className="composer-send"
            disabled={!canSend}
            onClick={submit}
            type="button"
          >
            {isSending ? <span className="mini-spinner" /> : <span aria-hidden="true">↑</span>}
          </button>
        </div>
        {isAttachmentDropTargeted ? (
          <div className="attachment-drop-overlay" aria-live="polite">
            <span>松开即可添加附件</span>
          </div>
        ) : null}
      </div>
      {previewedAttachment ? (
        <AttachmentPreview
          attachment={previewedAttachment}
          attachmentPort={attachmentPort}
          conversationId={conversationId}
          onClose={() => setPreviewedAttachment(null)}
          ownerUserId={ownerUserId}
        />
      ) : null}
    </section>
  );
}

function AttachmentIcon({ attachment }: { attachment: ConversationAttachmentDraft }) {
  const symbol = attachment.kind === "image" ? "▧" : attachment.kind === "audio" ? "♪" : "▤";
  return <span aria-hidden="true" className={`attachment-icon attachment-icon--${attachment.kind}`}>{symbol}</span>;
}

export function AttachmentPreview({
  attachment,
  attachmentPort,
  ownerUserId,
  conversationId,
  onClose,
  authorizedLocalRef,
}: {
  attachment: ConversationAttachmentDraft;
  attachmentPort: ConversationAttachmentPort;
  ownerUserId: string;
  conversationId: string;
  onClose(): void;
  authorizedLocalRef?: string;
}) {
  const generation = useConversationComposerStore((state) => state.attachmentDraftGeneration);
  const [text, setText] = useState<string | null>(null);
  const [truncated, setTruncated] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (attachment.kind === "image" || !isTextLike(attachment.mimeType)) return;
    let disposed = false;
    const scope = {
      ownerUserId,
      conversationId,
      draftGeneration: generation,
    };
    const request = authorizedLocalRef
      ? attachmentPort.previewPersistedText(scope, authorizedLocalRef)
      : attachmentPort.previewText(scope, attachment.id);
    void request.then((preview) => {
      if (disposed) return;
      setText(preview.content);
      setTruncated(preview.truncated);
    }).catch((reason: unknown) => {
      if (!disposed) setError(errorMessage(reason));
    });
    return () => { disposed = true; };
  }, [attachment, attachmentPort, authorizedLocalRef, conversationId, generation, ownerUserId]);

  return (
    <div className="attachment-preview-backdrop" onMouseDown={onClose} role="presentation">
      <section
        aria-label={`附件预览：${attachment.name}`}
        aria-modal="true"
        className="attachment-preview"
        onMouseDown={(event) => event.stopPropagation()}
        role="dialog"
      >
        <header>
          <div>
            <strong>{attachment.name}</strong>
            <small>{attachment.mimeType} · {formatAttachmentSize(attachment.byteSize)}</small>
          </div>
          <button aria-label="关闭附件预览" onClick={onClose} type="button">×</button>
        </header>
        <div className="attachment-preview__content">
          {attachment.kind === "image" && attachment.previewUrl ? (
            <img alt={attachment.name} src={attachment.previewUrl} />
          ) : text != null ? (
            <pre>{text}{truncated ? "\n\n…预览已截断" : ""}</pre>
          ) : error ? (
            <p className="attachment-preview__message attachment-preview__message--error">{error}</p>
          ) : isTextLike(attachment.mimeType) ? (
            <span className="spinner" aria-label="正在读取附件预览" />
          ) : (
            <div className="attachment-preview__file">
              <AttachmentIcon attachment={attachment} />
              <p>该文件将在发送后由本地任务组件按授权引用读取。</p>
            </div>
          )}
        </div>
      </section>
    </div>
  );
}

function isTextLike(mimeType: string) {
  return mimeType.startsWith("text/")
    || ["application/json", "application/xml", "application/javascript", "application/x-yaml"]
      .includes(mimeType);
}

function errorMessage(error: unknown) {
  if (error instanceof Error && error.message.trim()) return error.message;
  if (typeof error === "string" && error.trim()) return error;
  return "无法预览附件。";
}
