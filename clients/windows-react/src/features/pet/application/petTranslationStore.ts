import { create } from "zustand";
import type { AuthSession } from "../../authentication/domain/authentication";
import type { PetTranslationPort } from "../ports/PetTranslationPort";
import type {
  PetTranslationOutputStyle,
  PetTranslationRecord,
  PetTranslationTarget,
} from "../domain/petTranslation";

interface PetTranslationState {
  session: AuthSession | null;
  records: PetTranslationRecord[];
  sourceText: string;
  translatedText: string;
  target: PetTranslationTarget;
  outputStyle: PetTranslationOutputStyle;
  isLoading: boolean;
  isTranslating: boolean;
  error: string | null;
  selectedRecordId: string | null;
  setSourceText(value: string): void;
  setTarget(value: PetTranslationTarget): void;
  setOutputStyle(value: PetTranslationOutputStyle): void;
  load(port: PetTranslationPort, session: AuthSession | null): Promise<void>;
  translate(port: PetTranslationPort): Promise<void>;
  selectRecord(record: PetTranslationRecord): void;
  clear(port: PetTranslationPort): Promise<void>;
  clearError(): void;
}

export const usePetTranslationStore = create<PetTranslationState>((set, get) => ({
  session: null,
  records: [],
  sourceText: "",
  translatedText: "",
  target: "simplifiedChinese",
  outputStyle: "translationOnly",
  isLoading: false,
  isTranslating: false,
  error: null,
  selectedRecordId: null,

  setSourceText(sourceText) { set({ sourceText, error: null }); },
  setTarget(target) { set({ target }); },
  setOutputStyle(outputStyle) { set({ outputStyle }); },

  async load(port, session) {
    set({ session, isLoading: true, error: null });
    if (!session) {
      set({ isLoading: false, error: "请先登录 ChatOS。" });
      return;
    }
    try {
      const history = await port.list(session.user.id);
      set({ records: history.records, isLoading: false });
    } catch (failure) {
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
    if (!sourceText) {
      set({ error: "请输入要翻译的内容。" });
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
      });
      set((current) => ({
        records: [record, ...current.records.filter((item) => item.id !== record.id)],
        translatedText: record.translatedText,
        selectedRecordId: record.id,
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

  clearError() { set({ error: null }); },
}));

function errorMessage(failure: unknown): string {
  if (failure instanceof Error && failure.message.trim()) return failure.message;
  if (typeof failure === "string" && failure.trim()) return failure;
  return "请求失败，请稍后重试。";
}
