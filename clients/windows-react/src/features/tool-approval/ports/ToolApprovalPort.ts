import type {
  ToolApproval,
  ToolApprovalDecision,
  ToolApprovalResolution,
} from "../domain/toolApproval";

export interface ToolApprovalMutationRequest {
  ownerUserId: string;
  invocationId: string;
  expectedVersion: number;
  decision: ToolApprovalDecision;
}

export interface ToolApprovalPort {
  listPending(ownerUserId: string): Promise<ToolApproval[]>;
  decide(request: ToolApprovalMutationRequest): Promise<ToolApprovalResolution>;
}
