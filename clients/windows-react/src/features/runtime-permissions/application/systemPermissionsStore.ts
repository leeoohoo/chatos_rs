import { create } from "zustand";
import { isPermissionReady, type SystemPermissions } from "../domain/systemPermissions";
import type { SystemPermissionsPort } from "../ports/SystemPermissionsPort";

interface SystemPermissionsState {
  ownerUserId: string | null;
  hostAvailable: boolean;
  snapshot: SystemPermissions | null;
  isLoading: boolean;
  requestingPermissionId: string | null;
  loadError: string | null;
  mutationError: string | null;
  requestSequence: number;
  activeLoadId: number;
  activeRequestId: number;
  activate(port: SystemPermissionsPort, ownerUserId: string): Promise<void>;
  suspend(ownerUserId: string): void;
  refresh(port: SystemPermissionsPort): Promise<void>;
  request(port: SystemPermissionsPort, permissionId: string): Promise<boolean>;
  clearMutationError(): void;
  reset(): void;
}

export const useSystemPermissionsStore = create<SystemPermissionsState>((set, get) => ({
  ownerUserId: null,
  hostAvailable: false,
  snapshot: null,
  isLoading: false,
  requestingPermissionId: null,
  loadError: null,
  mutationError: null,
  requestSequence: 0,
  activeLoadId: 0,
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
        requestingPermissionId: null,
        loadError: null,
        mutationError: null,
        requestSequence: current.requestSequence + 1,
        activeLoadId: 0,
        activeRequestId: 0,
      });
    } else if (!current.hostAvailable) {
      set({
        hostAvailable: true,
        requestSequence: current.requestSequence + 1,
        activeLoadId: 0,
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
      requestingPermissionId: null,
      requestSequence: current.requestSequence + 1,
      activeLoadId: 0,
      activeRequestId: 0,
    });
  },

  async refresh(port) {
    const current = get();
    if (!current.ownerUserId || !current.hostAvailable || current.requestingPermissionId) return;
    const ownerUserId = current.ownerUserId;
    const requestId = current.requestSequence + 1;
    set({
      requestSequence: requestId,
      activeLoadId: requestId,
      isLoading: current.snapshot == null,
      loadError: null,
    });
    try {
      const snapshot = await port.load(ownerUserId);
      if (!acceptsLoad(get(), ownerUserId, requestId)) return;
      set({ snapshot, isLoading: false, loadError: null, activeLoadId: 0 });
    } catch (error) {
      if (!acceptsLoad(get(), ownerUserId, requestId)) return;
      set({
        isLoading: false,
        loadError: errorMessage(error, "系统权限状态暂时不可用。"),
        activeLoadId: 0,
      });
    }
  },

  async request(port, permissionInput) {
    const permissionId = permissionInput.trim();
    const current = get();
    const permission = current.snapshot?.items.find((item) => item.id === permissionId);
    if (!current.ownerUserId
      || !current.hostAvailable
      || current.requestingPermissionId
      || !permission
      || !permission.canRequest
      || isPermissionReady(permission.status)) return false;
    const ownerUserId = current.ownerUserId;
    const requestId = current.requestSequence + 1;
    set({
      requestSequence: requestId,
      activeLoadId: 0,
      activeRequestId: requestId,
      isLoading: false,
      requestingPermissionId: permissionId,
      mutationError: null,
    });
    try {
      const snapshot = await port.request(ownerUserId, permissionId);
      if (!acceptsRequest(get(), ownerUserId, permissionId, requestId)) return false;
      set({
        snapshot,
        requestingPermissionId: null,
        mutationError: null,
        activeRequestId: 0,
      });
      return true;
    } catch (error) {
      if (!acceptsRequest(get(), ownerUserId, permissionId, requestId)) return false;
      set({
        requestingPermissionId: null,
        mutationError: errorMessage(error, "无法打开系统授权引导。"),
        activeRequestId: 0,
      });
      return false;
    }
  },

  clearMutationError() {
    set({ mutationError: null });
  },

  reset() {
    set((state) => ({
      ownerUserId: null,
      hostAvailable: false,
      snapshot: null,
      isLoading: false,
      requestingPermissionId: null,
      loadError: null,
      mutationError: null,
      requestSequence: state.requestSequence + 1,
      activeLoadId: 0,
      activeRequestId: 0,
    }));
  },
}));

function acceptsLoad(state: SystemPermissionsState, ownerUserId: string, requestId: number) {
  return state.ownerUserId === ownerUserId
    && state.hostAvailable
    && state.activeLoadId === requestId;
}

function acceptsRequest(
  state: SystemPermissionsState,
  ownerUserId: string,
  permissionId: string,
  requestId: number,
) {
  return state.ownerUserId === ownerUserId
    && state.hostAvailable
    && state.requestingPermissionId === permissionId
    && state.activeRequestId === requestId;
}

function errorMessage(error: unknown, fallback: string) {
  if (error instanceof Error && error.message.trim()) return error.message;
  if (typeof error === "string" && error.trim()) return error;
  return fallback;
}
