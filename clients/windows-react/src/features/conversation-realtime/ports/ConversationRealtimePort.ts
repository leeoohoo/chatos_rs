import type { ConversationRealtimeCursor } from "../domain/conversationRealtime";

export interface ConversationRealtimePollInput {
  ownerUserId: string;
  conversationId: string;
  afterCursor: number | null;
  observedConversationVersion: number;
}

export interface ConversationRealtimePort {
  poll(input: ConversationRealtimePollInput): Promise<ConversationRealtimeCursor>;
}
