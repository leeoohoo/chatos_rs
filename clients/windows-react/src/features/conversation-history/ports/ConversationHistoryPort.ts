import type { ConversationHistorySnapshot } from "../domain/conversationHistory";

export interface ConversationHistoryPort {
  loadHistory(
    ownerUserId: string,
    conversationId: string,
    beforeOrdinal?: number,
  ): Promise<ConversationHistorySnapshot>;
}
