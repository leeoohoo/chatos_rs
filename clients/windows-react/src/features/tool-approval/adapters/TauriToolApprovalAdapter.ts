import { invoke } from "@tauri-apps/api/core";
import {
  toolApprovalResolutionSchema,
  toolApprovalsSchema,
} from "../domain/toolApproval";
import { toolApprovalSettingsSchema } from "../domain/toolApprovalSettings";
import type {
  ToolApprovalMutationRequest,
  ToolApprovalPort,
} from "../ports/ToolApprovalPort";
import type {
  ToolApprovalModeMutationRequest,
  ToolApprovalModelMutationRequest,
  ToolApprovalSettingsPort,
} from "../ports/ToolApprovalSettingsPort";

export class TauriToolApprovalAdapter implements ToolApprovalPort, ToolApprovalSettingsPort {
  async listPending(ownerUserId: string) {
    return toolApprovalsSchema.parse(
      await invoke("list_tool_approvals", { ownerUserId }),
    );
  }

  async decide(input: ToolApprovalMutationRequest) {
    return toolApprovalResolutionSchema.parse(
      await invoke("decide_tool_approval", { input }),
    );
  }

  async loadSettings(ownerUserId: string) {
    return toolApprovalSettingsSchema.parse(
      await invoke("load_tool_approval_settings", { ownerUserId }),
    );
  }

  async updateMode(input: ToolApprovalModeMutationRequest) {
    return toolApprovalSettingsSchema.parse(
      await invoke("update_tool_approval_mode", { input }),
    );
  }

  async updateModel(input: ToolApprovalModelMutationRequest) {
    return toolApprovalSettingsSchema.parse(
      await invoke("update_tool_approval_model", { input }),
    );
  }
}
