import { invoke } from "@tauri-apps/api/core";
import { hostLifecycleSnapshotSchema, type HostLifecycleSnapshot } from "../domain/hostLifecycle";
import type { HostLifecyclePort } from "../ports/HostLifecyclePort";

export class TauriHostLifecycleAdapter implements HostLifecyclePort {
  async snapshot(): Promise<HostLifecycleSnapshot> {
    return hostLifecycleSnapshotSchema.parse(
      await invoke("host_lifecycle_snapshot"),
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
