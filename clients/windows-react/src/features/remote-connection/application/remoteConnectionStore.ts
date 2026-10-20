import { create } from "zustand";
import {
  sortRemoteConnections,
  type RemoteConnection,
} from "../domain/remoteConnection";
import type { RemoteConnectionPort } from "../ports/RemoteConnectionPort";

interface RemoteConnectionState {
  ownerUserId: string | null;
  hostAvailable: boolean;
  connections: RemoteConnection[];
  isLoading: boolean;
  errorMessage: string | null;
  generation: number;
  requestSequence: number;
  activeLoadRequest: number | null;
  activate(port: RemoteConnectionPort, ownerUserId: string): Promise<void>;
  suspend(ownerUserId: string): void;
  refresh(port: RemoteConnectionPort): Promise<void>;
  acceptSaved(ownerUserId: string, connection: RemoteConnection): void;
  acceptDeleted(ownerUserId: string, connectionId: string): void;
  dismissError(): void;
  reset(): void;
}

export const useRemoteConnectionStore = create<RemoteConnectionState>((set, get) => ({
  ownerUserId: null,
  hostAvailable: false,
  connections: [],
  isLoading: false,
  errorMessage: null,
  generation: 0,
  requestSequence: 0,
  activeLoadRequest: null,

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
        connections: [],
        isLoading: false,
        errorMessage: null,
        generation: current.generation + 1,
        requestSequence: current.requestSequence + 1,
        activeLoadRequest: null,
      });
    } else if (!current.hostAvailable) {
      set({
        hostAvailable: true,
        generation: current.generation + 1,
        requestSequence: current.requestSequence + 1,
        activeLoadRequest: null,
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
      generation: current.generation + 1,
      requestSequence: current.requestSequence + 1,
      activeLoadRequest: null,
    });
  },

  async refresh(port) {
    const current = get();
    if (!current.ownerUserId || !current.hostAvailable) return;
    const ownerUserId = current.ownerUserId;
    const generation = current.generation;
    const requestId = current.requestSequence + 1;
    set({
      requestSequence: requestId,
      activeLoadRequest: requestId,
      isLoading: current.connections.length === 0,
      errorMessage: null,
    });
    try {
      const connections = sortRemoteConnections(await port.list({ ownerUserId }));
      if (!acceptsLoad(get(), ownerUserId, generation, requestId)) return;
      set({ connections, isLoading: false, errorMessage: null, activeLoadRequest: null });
    } catch (error) {
      if (!acceptsLoad(get(), ownerUserId, generation, requestId)) return;
      set({
        isLoading: false,
        errorMessage: errorMessage(error),
        activeLoadRequest: null,
      });
    }
  },

  acceptSaved(ownerInput, connection) {
    const ownerUserId = ownerInput.trim();
    const current = get();
    if (!ownerUserId || current.ownerUserId !== ownerUserId || !current.hostAvailable) return;
    set({
      connections: sortRemoteConnections([
        ...current.connections.filter((candidate) => candidate.id !== connection.id),
        connection,
      ]),
      errorMessage: null,
    });
  },

  acceptDeleted(ownerInput, connectionInput) {
    const ownerUserId = ownerInput.trim();
    const connectionId = connectionInput.trim();
    const current = get();
    if (!ownerUserId || !connectionId
      || current.ownerUserId !== ownerUserId || !current.hostAvailable) return;
    set({
      connections: current.connections.filter((connection) => connection.id !== connectionId),
      errorMessage: null,
    });
  },

  dismissError() {
    set({ errorMessage: null });
  },

  reset() {
    set((state) => ({
      ownerUserId: null,
      hostAvailable: false,
      connections: [],
      isLoading: false,
      errorMessage: null,
      generation: state.generation + 1,
      requestSequence: state.requestSequence + 1,
      activeLoadRequest: null,
    }));
  },
}));

function acceptsLoad(
  state: RemoteConnectionState,
  ownerUserId: string,
  generation: number,
  requestId: number,
) {
  return state.ownerUserId === ownerUserId
    && state.hostAvailable
    && state.generation === generation
    && state.activeLoadRequest === requestId;
}

function errorMessage(error: unknown) {
  if (error instanceof Error && error.message.trim()) return error.message;
  if (typeof error === "string" && error.trim()) return error;
  return "远端连接列表暂时不可用。";
}
