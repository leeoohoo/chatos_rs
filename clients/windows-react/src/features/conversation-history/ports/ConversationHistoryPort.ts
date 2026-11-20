import type { ConversationHistorySnapshot } from "../domain/conversationHistory";

export interface ConversationHistoryPort {
  loadHistory(
    ownerUserId: string,
    conversationId: string,
    beforeCursor?: string,
  ): Promise<ConversationHistorySnapshot>;
}
