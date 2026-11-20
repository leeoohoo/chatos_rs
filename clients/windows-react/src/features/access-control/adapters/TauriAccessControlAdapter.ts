import { invoke } from "@tauri-apps/api/core";
import { accessControlSnapshotSchema } from "../domain/accessControl";
import type { AccessControlPort } from "../ports/AccessControlPort";

export class TauriAccessControlAdapter implements AccessControlPort {
  async load(ownerUserId: string) {
    return accessControlSnapshotSchema.parse(
      await invoke("load_access_control", { ownerUserId }),
    );
  }

  async updateApproval(ownerUserId: string, approvalPolicy: "on_request" | "never", approvalReviewer: "user" | "auto_review", riskAcknowledged: boolean) {
    const mode = approvalPolicy === "never"
      ? "full_control"
      : approvalReviewer === "auto_review" ? "auto_approval" : "request_approval";
    await invoke("update_tool_approval_mode", {
      input: { ownerUserId, mode, riskAcknowledged },
    });
    return this.load(ownerUserId);
  }
}
