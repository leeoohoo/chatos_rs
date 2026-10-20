import type { AccessControlSnapshot } from "../domain/accessControl";

export interface AccessControlPort {
  load(ownerUserId: string): Promise<AccessControlSnapshot>;
}
