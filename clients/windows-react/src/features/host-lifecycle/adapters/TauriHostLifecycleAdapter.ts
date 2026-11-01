import { invoke } from "@tauri-apps/api/core";
import {
  hostDiagnosticsSnapshotSchema,
  hostLifecycleSnapshotSchema,
  machineResourceSnapshotSchema,
  type HostDiagnosticsSnapshot,
  type HostLifecycleSnapshot,
  type MachineResourceSnapshot,
} from "../domain/hostLifecycle";
import type { HostLifecyclePort } from "../ports/HostLifecyclePort";

export class TauriHostLifecycleAdapter implements HostLifecyclePort {
  async snapshot(): Promise<HostLifecycleSnapshot> {
    return hostLifecycleSnapshotSchema.parse(
      await invoke("host_lifecycle_snapshot"),
    );
  }

  async diagnostics(): Promise<HostDiagnosticsSnapshot> {
    return hostDiagnosticsSnapshotSchema.parse(await invoke("host_diagnostics"));
  }

  async resourceSnapshot(): Promise<MachineResourceSnapshot> {
    return machineResourceSnapshotSchema.parse(
      await invoke("machine_resource_snapshot"),
    );
  }

  async start(ownerUserId: string): Promise<HostLifecycleSnapshot> {
    return hostLifecycleSnapshotSchema.parse(
      await invoke("start_local_agent_host", { ownerUserId }),
    );
  }

  async stop(): Promise<HostLifecycleSnapshot> {
    return hostLifecycleSnapshotSchema.parse(
      await invoke("stop_local_agent_host"),
    );
  }
}
