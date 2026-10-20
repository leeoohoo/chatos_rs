import { invoke } from "@tauri-apps/api/core";
import {
  toolApprovalResolutionSchema,
  toolApprovalsSchema,
} from "../domain/toolApproval";
import type {
  ToolApprovalMutationRequest,
  ToolApprovalPort,
} from "../ports/ToolApprovalPort";

export class TauriToolApprovalAdapter implements ToolApprovalPort {
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
}
