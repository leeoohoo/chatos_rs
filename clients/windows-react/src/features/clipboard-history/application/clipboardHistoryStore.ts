import { create } from "zustand";
import type { ClipboardEntry } from "../domain/clipboardHistory";
import type { ClipboardHistoryPort } from "../ports/ClipboardHistoryPort";

interface ClipboardHistoryState {
  language: "zh-CN" | "en-US";
  entries: ClipboardEntry[];
  query: string;
  selectedIndex: number;
  isLoading: boolean;
  isMutating: boolean;
  error: string | null;
  requestId: number;
  load(port: ClipboardHistoryPort): Promise<void>;
  refresh(port: ClipboardHistoryPort): Promise<void>;
  setQuery(query: string): void;
  moveSelection(direction: -1 | 1): void;
  select(index: number): void;
  setPinned(port: ClipboardHistoryPort, entry: ClipboardEntry): Promise<void>;
  delete(port: ClipboardHistoryPort, entry: ClipboardEntry): Promise<void>;
  clear(port: ClipboardHistoryPort): Promise<void>;
  restore(port: ClipboardHistoryPort, entry: ClipboardEntry): Promise<boolean>;
  clearError(): void;
}

export const useClipboardHistoryStore = create<ClipboardHistoryState>((set, get) => ({
  language: "zh-CN", entries: [], query: "", selectedIndex: 0, isLoading: false, isMutating: false, error: null, requestId: 0,

  async load(port) {
    const requestId = get().requestId + 1;
    set({ query: "", selectedIndex: 0, isLoading: true, error: null, requestId });
    try {
      const history = await port.list();
      if (get().requestId !== requestId) return;
      set({ language: history.language, entries: history.entries, isLoading: false });
    } catch (failure) {
      if (get().requestId === requestId) set({ isLoading: false, error: errorMessage(failure) });
    }
  },

  async refresh(port) {
    const requestId = get().requestId;
    try {
      const history = await port.list();
      if (get().requestId !== requestId || get().isMutating) return;
      set((state) => ({ language: history.language, entries: history.entries, selectedIndex: boundedIndex(state.selectedIndex, filtered(history.entries, state.query).length) }));
    } catch {
      // Background refresh preserves the last trusted list.
    }
  },

  setQuery(query) { set((state) => ({ query, selectedIndex: boundedIndex(0, filtered(state.entries, query).length) })); },
  moveSelection(direction) {
    set((state) => {
      const count = filtered(state.entries, state.query).length;
      if (count === 0) return state;
      return { selectedIndex: (state.selectedIndex + direction + count) % count };
    });
  },
  select(selectedIndex) { set({ selectedIndex }); },

  async setPinned(port, entry) { await mutate(port, () => port.setPinned(entry.id, !entry.isPinned), set, get); },
  async delete(port, entry) { await mutate(port, () => port.delete(entry.id), set, get); },
  async clear(port) { await mutate(port, () => port.clear(), set, get); },
  async restore(port, entry) {
    set({ isMutating: true, error: null });
    try {
      await port.restore(entry.id);
      set({ isMutating: false });
      await port.close();
      return true;
    } catch (failure) {
      set({ isMutating: false, error: errorMessage(failure) });
      return false;
    }
  },
  clearError() { set({ error: null }); },
}));

export function selectFilteredEntries(state: Pick<ClipboardHistoryState, "entries" | "query">) {
  return filtered(state.entries, state.query);
}

async function mutate(
  _port: ClipboardHistoryPort,
  operation: () => Promise<{ language: "zh-CN" | "en-US"; entries: ClipboardEntry[] }>,
  set: (partial: Partial<ClipboardHistoryState> | ((state: ClipboardHistoryState) => Partial<ClipboardHistoryState>)) => void,
  get: () => ClipboardHistoryState,
) {
  if (get().isMutating) return;
  set({ isMutating: true, error: null });
  try {
    const history = await operation();
    set((state) => ({ language: history.language, entries: history.entries, isMutating: false, selectedIndex: boundedIndex(state.selectedIndex, filtered(history.entries, state.query).length) }));
  } catch (failure) {
    set({ isMutating: false, error: errorMessage(failure) });
  }
}

function filtered(entries: ClipboardEntry[], query: string) {
  const normalized = query.trim().toLocaleLowerCase();
  if (!normalized) return entries;
  return entries.filter((entry) => entry.kind.includes(normalized) || entry.preview?.toLocaleLowerCase().includes(normalized));
}

function boundedIndex(index: number, count: number) { return count === 0 ? 0 : Math.min(index, count - 1); }
function errorMessage(failure: unknown) { return failure instanceof Error ? failure.message : String(failure); }
