import type { HostLifecycleSnapshot } from "../domain/hostLifecycle";

export interface HostLifecyclePort {
  snapshot(): Promise<HostLifecycleSnapshot>;
  start(ownerUserId: string): Promise<HostLifecycleSnapshot>;
  stop(): Promise<HostLifecycleSnapshot>;
}
