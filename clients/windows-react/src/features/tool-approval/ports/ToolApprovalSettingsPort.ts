import type {
  ToolApprovalMode,
  ToolApprovalSettings,
} from "../domain/toolApprovalSettings";

export interface ToolApprovalModeMutationRequest {
  ownerUserId: string;
  mode: ToolApprovalMode;
  riskAcknowledged: boolean;
}

export interface ToolApprovalModelMutationRequest {
  ownerUserId: string;
  selectedModelId: string | null;
  selectedThinkingLevel: string | null;
}

export interface ToolApprovalSettingsPort {
  loadSettings(ownerUserId: string): Promise<ToolApprovalSettings>;
  updateMode(request: ToolApprovalModeMutationRequest): Promise<ToolApprovalSettings>;
  updateModel(request: ToolApprovalModelMutationRequest): Promise<ToolApprovalSettings>;
}
