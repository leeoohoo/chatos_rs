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
  ownerUserId: string | null;
  requestId: number;
  refresh(port: HostLifecyclePort): Promise<void>;
  start(port: HostLifecyclePort, ownerUserId: string): Promise<void>;
  retry(port: HostLifecyclePort): Promise<void>;
  stop(port: HostLifecyclePort): Promise<void>;
}

function present(snapshot: HostLifecycleSnapshot): PresentedSnapshot {
  return { ...snapshot, label: hostPhaseLabels[snapshot.phase] };
}

export const useHostLifecycleStore = create<HostLifecycleState>((set, get) => ({
  snapshot: initialSnapshot,
  diagnostics: [],
  ownerUserId: null,
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
  async start(port, ownerUserId) {
    const normalizedOwner = ownerUserId.trim();
    const requestId = get().requestId + 1;
    if (!normalizedOwner) {
      set({
        ownerUserId: null,
        requestId,
        snapshot: failureSnapshot("登录账号无效，无法启动本地服务。"),
      });
      return;
    }
    set({
      ownerUserId: normalizedOwner,
      requestId,
      snapshot: present({
        phase: "launching",
        ready: false,
        protocolVersion: LOCAL_AGENT_PROTOCOL_VERSION,
        detail: "正在启动并验证本地服务。",
        retryable: false,
      }),
    });
    await finishStart(port, normalizedOwner, requestId, set, get);
  },
  async retry(port) {
    const ownerUserId = get().ownerUserId;
    if (!ownerUserId) {
      await get().refresh(port);
      return;
    }
    const requestId = get().requestId + 1;
    set({
      requestId,
      snapshot: present({
        phase: "restarting",
        ready: false,
        protocolVersion: LOCAL_AGENT_PROTOCOL_VERSION,
        detail: "正在重新启动并验证本地服务。",
        retryable: false,
      }),
    });
    await finishStart(port, ownerUserId, requestId, set, get);
  },
  async stop(port) {
    const requestId = get().requestId + 1;
    set({
      requestId,
      snapshot: present({
        phase: "stopped",
        ready: false,
        protocolVersion: LOCAL_AGENT_PROTOCOL_VERSION,
        detail: "正在停止本地服务。",
        retryable: false,
      }),
    });
    try {
      const snapshot = await port.stop();
      const diagnostics = await loadDiagnostics(port);
      if (get().requestId === requestId) {
        set({
          ownerUserId: null,
          snapshot: present(snapshot),
          diagnostics,
        });
      }
    } catch (error) {
      const diagnostics = await loadDiagnostics(port);
      if (get().requestId !== requestId) return;
      set({
        diagnostics,
        snapshot: failureSnapshot(errorMessage(error, "无法停止本地服务。")),
      });
    }
  },
}));

async function finishStart(
  port: HostLifecyclePort,
  ownerUserId: string,
  requestId: number,
  set: (partial: Partial<HostLifecycleState>) => void,
  get: () => HostLifecycleState,
): Promise<void> {
  try {
    const snapshot = await port.start(ownerUserId);
    const diagnostics = await loadDiagnostics(port);
    if (get().requestId === requestId && get().ownerUserId === ownerUserId) {
      set({ snapshot: present(snapshot), diagnostics });
    }
  } catch (error) {
    const diagnostics = await loadDiagnostics(port);
    if (get().requestId !== requestId || get().ownerUserId !== ownerUserId) return;
    set({
      diagnostics,
      snapshot: failureSnapshot(errorMessage(error, "无法启动本地服务。")),
    });
  }
}

function failureSnapshot(detail: string): PresentedSnapshot {
  return present({
    phase: "degraded",
    ready: false,
    protocolVersion: LOCAL_AGENT_PROTOCOL_VERSION,
    detail,
    retryable: true,
  });
}

function errorMessage(error: unknown, fallback: string): string {
  if (error instanceof Error && error.message.trim()) return error.message;
  if (typeof error === "string" && error.trim()) return error;
  return fallback;
}

async function loadDiagnostics(port: HostLifecyclePort): Promise<HostDiagnosticEntry[]> {
  try {
    return (await port.diagnostics()).entries;
  } catch {
    return [];
  }
}
