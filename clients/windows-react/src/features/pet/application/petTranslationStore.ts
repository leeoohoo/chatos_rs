import { create } from "zustand";
import type { AuthSession } from "../../authentication/domain/authentication";
import type { PetTranslationPort } from "../ports/PetTranslationPort";
import type {
  AttachmentImportResult,
  AttachmentScope,
  ConversationAttachmentDraft,
} from "../../conversation-attachments/domain/conversationAttachment";
import type {
  PetTranslationOutputStyle,
  PetTranslationRecord,
  PetTranslationTarget,
} from "../domain/petTranslation";

interface PetTranslationState {
  session: AuthSession | null;
  loadRequestId: number;
  records: PetTranslationRecord[];
  sourceText: string;
  translatedText: string;
  target: PetTranslationTarget;
  outputStyle: PetTranslationOutputStyle;
  isLoading: boolean;
  isTranslating: boolean;
  error: string | null;
  selectedRecordId: string | null;
  attachments: ConversationAttachmentDraft[];
  attachmentError: string | null;
  isImportingAttachments: boolean;
  setSourceText(value: string): void;
  setTarget(value: PetTranslationTarget): void;
  setOutputStyle(value: PetTranslationOutputStyle): void;
  load(port: PetTranslationPort, session: AuthSession | null): Promise<void>;
  translate(port: PetTranslationPort): Promise<void>;
  pickAttachments(port: PetTranslationPort): Promise<void>;
  importClipboard(port: PetTranslationPort): Promise<void>;
  removeAttachment(port: PetTranslationPort, attachmentId: string): Promise<void>;
  selectRecord(record: PetTranslationRecord): void;
  clear(port: PetTranslationPort): Promise<void>;
  clearError(): void;
}

export const usePetTranslationStore = create<PetTranslationState>((set, get) => ({
  session: null,
  loadRequestId: 0,
  records: [],
  sourceText: "",
  translatedText: "",
  target: "simplifiedChinese",
  outputStyle: "translationOnly",
  isLoading: false,
  isTranslating: false,
  error: null,
  selectedRecordId: null,
  attachments: [],
  attachmentError: null,
  isImportingAttachments: false,

  setSourceText(sourceText) { set({ sourceText, error: null }); },
  setTarget(target) { set({ target }); },
  setOutputStyle(outputStyle) { set({ outputStyle }); },

  async load(port, session) {
    const previous = get();
    const requestId = previous.loadRequestId + 1;
    const previousOwnerUserId = previous.session?.user.id ?? null;
    const nextOwnerUserId = session?.user.id ?? null;
    const ownerChanged = previousOwnerUserId !== nextOwnerUserId;
    let attachmentCleanupError: string | null = null;
    if (ownerChanged && previousOwnerUserId && previous.attachments.length > 0) {
      try {
        await port.discardAttachments(
          scopeFor(previousOwnerUserId),
          previous.attachments.map(({ id }) => id),
        );
      } catch {
        attachmentCleanupError = "旧账号的图片草稿清理失败，已从当前窗口移除。";
      }
    }
    set({
      session,
      loadRequestId: requestId,
      isLoading: true,
      error: null,
      attachmentError: attachmentCleanupError,
      ...(ownerChanged ? {
        records: [],
        sourceText: "",
        translatedText: "",
        selectedRecordId: null,
        attachments: [],
      } : {}),
    });
    if (!session) {
      if (get().loadRequestId === requestId) {
        set({ isLoading: false, error: "请先登录 ChatOS。" });
      }
      return;
    }
    try {
      const history = await port.list(session.user.id);
      if (get().loadRequestId !== requestId) return;
      set({ records: history.records, isLoading: false });
    } catch (failure) {
      if (get().loadRequestId !== requestId) return;
      set({ isLoading: false, error: errorMessage(failure) });
    }
  },

  async translate(port) {
    const state = get();
    const sourceText = state.sourceText.trim();
    if (!state.session) {
      set({ error: "请先登录 ChatOS。" });
      return;
    }
    if (!sourceText && state.attachments.length === 0) {
      set({ error: "请输入要翻译的内容或添加图片。" });
      return;
    }
    if (state.isTranslating) return;
    set({ isTranslating: true, error: null });
    try {
      const record = await port.translate({
        ownerUserId: state.session.user.id,
        sourceText,
        target: state.target,
        outputStyle: state.outputStyle,
        attachmentIds: state.attachments.map(({ id }) => id),
      });
      set((current) => ({
        records: [record, ...current.records.filter((item) => item.id !== record.id)],
        translatedText: record.translatedText,
        selectedRecordId: record.id,
        attachments: [],
        attachmentError: null,
        isTranslating: false,
      }));
    } catch (failure) {
      set({ isTranslating: false, error: errorMessage(failure) });
    }
  },

  selectRecord(record) {
    set({
      selectedRecordId: record.id,
      sourceText: record.sourceText,
      translatedText: record.translatedText,
      target: record.target,
      outputStyle: record.outputStyle,
      error: null,
    });
  },

  async clear(port) {
    const session = get().session;
    if (!session || get().isTranslating) return;
    set({ error: null });
    try {
      const history = await port.clear(session.user.id);
      set({ records: history.records, selectedRecordId: null });
    } catch (failure) {
      set({ error: errorMessage(failure) });
    }
  },

  async pickAttachments(port) {
    const session = get().session;
    if (!session || get().isTranslating || get().isImportingAttachments) return;
    set({ isImportingAttachments: true, attachmentError: null });
    try {
      const result = await port.pickAttachments(scopeFor(session.user.id));
      await acceptAttachmentImport(port, result, session.user.id);
    } catch (failure) {
      set({ attachmentError: errorMessage(failure) });
    } finally {
      set({ isImportingAttachments: false });
    }
  },

  async importClipboard(port) {
    const session = get().session;
    if (!session || get().isTranslating || get().isImportingAttachments) return;
    set({ isImportingAttachments: true, attachmentError: null });
    try {
      const result = await port.importClipboard(scopeFor(session.user.id));
      if (result.kind === "text") {
        set((current) => ({ sourceText: current.sourceText + (result.text ?? "") }));
      } else if (result.kind === "attachments" && result.result) {
        await acceptAttachmentImport(port, result.result, session.user.id);
      }
    } catch (failure) {
      set({ attachmentError: errorMessage(failure) });
    } finally {
      set({ isImportingAttachments: false });
    }
  },

  async removeAttachment(port, attachmentId) {
    const session = get().session;
    if (!session) return;
    try {
      await port.discardAttachments(scopeFor(session.user.id), [attachmentId]);
      set((current) => ({
        attachments: current.attachments.filter(({ id }) => id !== attachmentId),
        attachmentError: current.attachments.length <= 1 ? null : current.attachmentError,
      }));
    } catch (failure) {
      set({ attachmentError: errorMessage(failure) });
    }
  },

  clearError() { set({ error: null }); },
}));

const MAXIMUM_TRANSLATION_ATTACHMENTS = 10;

function scopeFor(ownerUserId: string): AttachmentScope {
  return { ownerUserId, conversationId: "pet-translation", draftGeneration: 0 };
}

async function acceptAttachmentImport(
  port: PetTranslationPort,
  result: AttachmentImportResult,
  ownerUserId: string,
) {
  const accepted = result.attachments.filter(({ kind, mimeType }) => (
    kind === "image"
      && ["image/png", "image/jpeg", "image/gif", "image/webp"].includes(mimeType.toLowerCase())
  ));
  const rejected = result.attachments.filter((attachment) => !accepted.includes(attachment));
  const current = usePetTranslationStore.getState().attachments;
  const available = Math.max(0, MAXIMUM_TRANSLATION_ATTACHMENTS - current.length);
  const overflow = accepted.slice(available);
  const retained = accepted.slice(0, available);
  const discardIds = [...rejected, ...overflow].map(({ id }) => id);
  if (discardIds.length) {
    await port.discardAttachments(scopeFor(ownerUserId), discardIds);
  }
  const messages = [
    ...result.errors,
    ...(rejected.length ? ["快速翻译仅支持 PNG、JPEG、GIF 或 WebP 图片。"] : []),
    ...(overflow.length ? [`快速翻译最多添加 ${MAXIMUM_TRANSLATION_ATTACHMENTS} 张图片。`] : []),
  ];
  usePetTranslationStore.setState((state) => ({
    attachments: [...state.attachments, ...retained],
    attachmentError: messages.length ? messages.join("；") : null,
  }));
}

function errorMessage(failure: unknown): string {
  if (failure instanceof Error && failure.message.trim()) return failure.message;
  if (typeof failure === "string" && failure.trim()) return failure;
  return "请求失败，请稍后重试。";
}
