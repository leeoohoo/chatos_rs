import { create } from "zustand";
import type { DeviceGatewaySnapshot } from "../domain/deviceGateway";
import type { DeviceGatewayPort } from "../ports/DeviceGatewayPort";

interface DeviceGatewayState {
  ownerUserId: string | null;
  snapshot: DeviceGatewaySnapshot | null;
  isLoading: boolean;
  error: string | null;
  requestId: number;
  load(port: DeviceGatewayPort, ownerUserId: string): Promise<void>;
  reset(): void;
}

export const useDeviceGatewayStore = create<DeviceGatewayState>((set, get) => ({
  ownerUserId: null,
  snapshot: null,
  isLoading: false,
  error: null,
  requestId: 0,

  async load(port, ownerInput) {
    const ownerUserId = ownerInput.trim();
    const requestId = get().requestId + 1;
    if (!ownerUserId) {
      get().reset();
      return;
    }
    set({ ownerUserId, isLoading: true, error: null, requestId });
    try {
      const snapshot = await port.load(ownerUserId);
      if (get().ownerUserId !== ownerUserId || get().requestId !== requestId) return;
      set({ snapshot, isLoading: false });
    } catch (failure) {
      if (get().ownerUserId !== ownerUserId || get().requestId !== requestId) return;
      set({ isLoading: false, error: failure instanceof Error ? failure.message : String(failure) });
    }
  },

  reset() {
    set((state) => ({
      ownerUserId: null,
      snapshot: null,
      isLoading: false,
      error: null,
      requestId: state.requestId + 1,
    }));
  },
}));
