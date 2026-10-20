import { invoke } from "@tauri-apps/api/core";
import { accessControlSnapshotSchema } from "../domain/accessControl";
import type { AccessControlPort } from "../ports/AccessControlPort";

export class TauriAccessControlAdapter implements AccessControlPort {
  async load(ownerUserId: string) {
    return accessControlSnapshotSchema.parse(
      await invoke("load_access_control", { ownerUserId }),
    );
  }
}
