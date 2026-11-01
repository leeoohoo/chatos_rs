import type {
  HostDiagnosticsSnapshot,
  HostLifecycleSnapshot,
  MachineResourceSnapshot,
} from "../domain/hostLifecycle";

export interface HostLifecyclePort {
  snapshot(): Promise<HostLifecycleSnapshot>;
  diagnostics(): Promise<HostDiagnosticsSnapshot>;
  resourceSnapshot(): Promise<MachineResourceSnapshot>;
  start(ownerUserId: string): Promise<HostLifecycleSnapshot>;
  stop(): Promise<HostLifecycleSnapshot>;
}
