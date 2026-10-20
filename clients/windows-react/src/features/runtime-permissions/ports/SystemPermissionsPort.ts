import type { SystemPermissions } from "../domain/systemPermissions";

export interface SystemPermissionsPort {
  load(ownerUserId: string): Promise<SystemPermissions>;
  request(ownerUserId: string, permissionId: string): Promise<SystemPermissions>;
}
