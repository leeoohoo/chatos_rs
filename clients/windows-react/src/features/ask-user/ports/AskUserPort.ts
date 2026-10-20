import type { AskUserPrompt, AskUserSubmission } from "../domain/askUser";

export interface AskUserMutationRequest {
  ownerUserId: string;
  conversationId: string;
  promptId: string;
  submission?: AskUserSubmission;
}
export interface AskUserPort {
  loadPrompts(ownerUserId: string, conversationId: string): Promise<AskUserPrompt[]>;
  submitPrompt(request: AskUserMutationRequest & { submission: AskUserSubmission }): Promise<AskUserPrompt>;
  cancelPrompt(request: AskUserMutationRequest): Promise<AskUserPrompt>;
}
