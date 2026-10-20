import { invoke } from "@tauri-apps/api/core";
import { systemPermissionsSchema } from "../domain/systemPermissions";
import type { SystemPermissionsPort } from "../ports/SystemPermissionsPort";

export class TauriSystemPermissionsAdapter implements SystemPermissionsPort {
  async load(ownerUserId: string) {
    return systemPermissionsSchema.parse(
      await invoke("load_system_permissions", { ownerUserId }),
    );
  }

  async request(ownerUserId: string, permissionId: string) {
    return systemPermissionsSchema.parse(
      await invoke("request_system_permission", { ownerUserId, permissionId }),
    );
  }
}
