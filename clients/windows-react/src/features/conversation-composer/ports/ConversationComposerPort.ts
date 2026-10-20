import type {
  ConversationDraftSubmission,
  ConversationSendAck,
} from "../domain/conversationComposer";

export interface SendConversationDraftInput {
  ownerUserId: string;
  conversationId: string;
  guidanceTurnId: string | null;
  submission: ConversationDraftSubmission;
}

export interface ConversationComposerPort {
  sendDraft(input: SendConversationDraftInput): Promise<ConversationSendAck>;
}
