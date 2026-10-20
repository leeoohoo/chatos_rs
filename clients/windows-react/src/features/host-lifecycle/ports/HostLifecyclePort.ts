import type {
  HostDiagnosticsSnapshot,
  HostLifecycleSnapshot,
} from "../domain/hostLifecycle";

export interface HostLifecyclePort {
  snapshot(): Promise<HostLifecycleSnapshot>;
  diagnostics(): Promise<HostDiagnosticsSnapshot>;
  start(ownerUserId: string): Promise<HostLifecycleSnapshot>;
  stop(): Promise<HostLifecycleSnapshot>;
}
