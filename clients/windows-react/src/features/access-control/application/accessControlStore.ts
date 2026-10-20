import { create } from "zustand";
import type { AccessControlSnapshot } from "../domain/accessControl";
import type { AccessControlPort } from "../ports/AccessControlPort";

interface AccessControlState {
  ownerUserId: string | null;
  hostAvailable: boolean;
  snapshot: AccessControlSnapshot | null;
  isLoading: boolean;
  error: string | null;
  requestSequence: number;
  activeRequestId: number;
  activate(port: AccessControlPort, ownerUserId: string): Promise<void>;
  suspend(ownerUserId: string): void;
  refresh(port: AccessControlPort): Promise<void>;
  reset(): void;
}

export const useAccessControlStore = create<AccessControlState>((set, get) => ({
  ownerUserId: null,
  hostAvailable: false,
  snapshot: null,
  isLoading: false,
  error: null,
  requestSequence: 0,
  activeRequestId: 0,

  async activate(port, ownerInput) {
    const ownerUserId = ownerInput.trim();
    if (!ownerUserId) {
      get().reset();
      return;
    }
    const current = get();
    if (current.ownerUserId !== ownerUserId) {
      set({
        ownerUserId,
        hostAvailable: true,
        snapshot: null,
        isLoading: false,
        error: null,
        requestSequence: current.requestSequence + 1,
        activeRequestId: 0,
      });
    } else if (!current.hostAvailable) {
      set({
        hostAvailable: true,
        requestSequence: current.requestSequence + 1,
        activeRequestId: 0,
      });
    }
    await get().refresh(port);
  },

  suspend(ownerInput) {
    const ownerUserId = ownerInput.trim();
    const current = get();
    if (!ownerUserId || current.ownerUserId !== ownerUserId || !current.hostAvailable) return;
    set({
      hostAvailable: false,
      isLoading: false,
      requestSequence: current.requestSequence + 1,
      activeRequestId: 0,
    });
  },

  async refresh(port) {
    const current = get();
    if (!current.ownerUserId || !current.hostAvailable) return;
    const ownerUserId = current.ownerUserId;
    const requestId = current.requestSequence + 1;
    set({
      requestSequence: requestId,
      activeRequestId: requestId,
      isLoading: current.snapshot == null,
      error: null,
    });
    try {
      const snapshot = await port.load(ownerUserId);
      if (!accepts(get(), ownerUserId, requestId)) return;
      set({ snapshot, isLoading: false, error: null, activeRequestId: 0 });
    } catch (error) {
      if (!accepts(get(), ownerUserId, requestId)) return;
      set({
        isLoading: false,
        error: errorMessage(error),
        activeRequestId: 0,
      });
    }
  },

  reset() {
    set((state) => ({
      ownerUserId: null,
      hostAvailable: false,
      snapshot: null,
      isLoading: false,
      error: null,
      requestSequence: state.requestSequence + 1,
      activeRequestId: 0,
    }));
  },
}));

function accepts(state: AccessControlState, ownerUserId: string, requestId: number) {
  return state.ownerUserId === ownerUserId
    && state.hostAvailable
    && state.activeRequestId === requestId;
}

function errorMessage(error: unknown) {
  if (error instanceof Error && error.message.trim()) return error.message;
  if (typeof error === "string" && error.trim()) return error;
  return "访问控制状态暂时不可用。";
}
