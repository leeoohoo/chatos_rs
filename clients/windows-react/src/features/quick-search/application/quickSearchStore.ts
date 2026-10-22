import { create } from "zustand";
import type { QuickSearchResult } from "../domain/quickSearch";
import type { QuickSearchPort } from "../ports/QuickSearchPort";

interface QuickSearchState {
  presentationId: string | null;
  language: "zh-CN" | "en-US";
  query: string;
  results: QuickSearchResult[];
  selectedIndex: number;
  isLoading: boolean;
  isSearchingFiles: boolean;
  isExecuting: boolean;
  diagnostic: string | null;
  error: string | null;
  lifecycleId: number;
  requestId: number;
  open(port: QuickSearchPort): Promise<void>;
  setQuery(query: string): void;
  search(port: QuickSearchPort, includeFiles: boolean): Promise<void>;
  moveSelection(direction: -1 | 1): void;
  select(index: number): void;
  execute(port: QuickSearchPort, result: QuickSearchResult): Promise<boolean>;
  close(port: QuickSearchPort): Promise<void>;
  clearError(): void;
}

export const useQuickSearchStore = create<QuickSearchState>((set, get) => ({
  presentationId: null,
  language: "zh-CN",
  query: "",
  results: [],
  selectedIndex: 0,
  isLoading: false,
  isSearchingFiles: false,
  isExecuting: false,
  diagnostic: null,
  error: null,
  lifecycleId: 0,
  requestId: 0,

  async open(port) {
    const lifecycleId = get().lifecycleId + 1;
    set({
      presentationId: null,
      query: "",
      results: [],
      selectedIndex: 0,
      isLoading: true,
      isSearchingFiles: false,
      isExecuting: false,
      diagnostic: null,
      error: null,
      lifecycleId,
      requestId: 0,
    });
    try {
      const bootstrap = await port.bootstrap();
      if (get().lifecycleId !== lifecycleId) return;
      set({
        presentationId: bootstrap.presentationId,
        language: bootstrap.language,
        isLoading: false,
      });
    } catch (failure) {
      if (get().lifecycleId === lifecycleId) {
        set({ isLoading: false, error: errorMessage(failure) });
      }
    }
  },

  setQuery(query) {
    set({ query: Array.from(query).slice(0, 256).join(""), selectedIndex: 0, diagnostic: null, error: null });
  },

  async search(port, includeFiles) {
    const state = get();
    if (!state.presentationId) return;
    const presentationId = state.presentationId;
    const query = state.query;
    const requestId = state.requestId + 1;
    const lifecycleId = state.lifecycleId;
    set({
      requestId,
      isLoading: state.results.length === 0 && !includeFiles,
      isSearchingFiles: includeFiles,
      error: null,
    });
    try {
      const snapshot = await port.search(presentationId, query, requestId, includeFiles);
      const current = get();
      if (
        current.lifecycleId !== lifecycleId
        || current.requestId !== requestId
        || current.presentationId !== snapshot.presentationId
      ) return;
      set({
        results: snapshot.results,
        selectedIndex: boundedIndex(current.selectedIndex, snapshot.results.length),
        isLoading: false,
        isSearchingFiles: false,
        diagnostic: snapshot.diagnostic ?? null,
      });
    } catch (failure) {
      const current = get();
      if (current.lifecycleId !== lifecycleId || current.requestId !== requestId) return;
      set({ isLoading: false, isSearchingFiles: false, error: errorMessage(failure) });
    }
  },

  moveSelection(direction) {
    set((state) => {
      if (state.results.length === 0) return state;
      return {
        selectedIndex: (state.selectedIndex + direction + state.results.length) % state.results.length,
      };
    });
  },

  select(selectedIndex) {
    if (selectedIndex >= 0 && selectedIndex < get().results.length) set({ selectedIndex });
  },

  async execute(port, result) {
    const { presentationId, isExecuting } = get();
    if (!presentationId || isExecuting || !get().results.some((candidate) => candidate.id === result.id)) {
      return false;
    }
    set({ isExecuting: true, error: null });
    try {
      await port.execute(presentationId, result.id);
      set({ isExecuting: false });
      return true;
    } catch (failure) {
      set({ isExecuting: false, error: errorMessage(failure) });
      return false;
    }
  },

  async close(port) {
    const lifecycleId = get().lifecycleId + 1;
    set({ lifecycleId, presentationId: null, isLoading: false, isSearchingFiles: false, isExecuting: false });
    try {
      await port.close();
    } catch (failure) {
      if (get().lifecycleId === lifecycleId) set({ error: errorMessage(failure) });
    }
  },

  clearError() { set({ error: null }); },
}));

function boundedIndex(index: number, count: number) {
  return count === 0 ? 0 : Math.min(index, count - 1);
}

function errorMessage(failure: unknown) {
  if (failure instanceof Error && failure.message.trim()) return failure.message;
  if (typeof failure === "string" && failure.trim()) return failure;
  return "快捷搜索暂时不可用。";
}
