import { create } from "zustand";
import { LOCAL_AGENT_PROTOCOL_VERSION } from "../domain/hostContract";
import {
  hostPhaseLabels,
  type HostDiagnosticEntry,
  type HostLifecycleSnapshot,
} from "../domain/hostLifecycle";
import type { HostLifecyclePort } from "../ports/HostLifecyclePort";

type PresentedSnapshot = HostLifecycleSnapshot & { label: string };

const initialSnapshot: PresentedSnapshot = {
  phase: "stopped",
  ready: false,
  protocolVersion: LOCAL_AGENT_PROTOCOL_VERSION,
  detail: "登录账号确定后才能启动本地服务。",
  retryable: false,
  label: hostPhaseLabels.stopped,
};

interface HostLifecycleState {
  snapshot: PresentedSnapshot;
  diagnostics: HostDiagnosticEntry[];
  requestId: number;
  refresh(port: HostLifecyclePort): Promise<void>;
}

function present(snapshot: HostLifecycleSnapshot): PresentedSnapshot {
  return { ...snapshot, label: hostPhaseLabels[snapshot.phase] };
}

export const useHostLifecycleStore = create<HostLifecycleState>((set, get) => ({
  snapshot: initialSnapshot,
  diagnostics: [],
  requestId: 0,
  async refresh(port) {
    const requestId = get().requestId + 1;
    set({ requestId });
    try {
      const snapshot = await port.snapshot();
      const diagnostics = await loadDiagnostics(port);
      if (get().requestId === requestId) {
        set({ snapshot: present(snapshot), diagnostics });
      }
    } catch (error) {
      const diagnostics = await loadDiagnostics(port);
      if (get().requestId !== requestId) return;
      set({
        diagnostics,
        snapshot: present({
          phase: "degraded",
          ready: false,
          protocolVersion: LOCAL_AGENT_PROTOCOL_VERSION,
          detail: error instanceof Error ? error.message : "无法读取本地服务状态。",
          retryable: true,
        }),
      });
    }
  },
}));

async function loadDiagnostics(port: HostLifecyclePort): Promise<HostDiagnosticEntry[]> {
  try {
    return (await port.diagnostics()).entries;
  } catch {
    return [];
  }
}
