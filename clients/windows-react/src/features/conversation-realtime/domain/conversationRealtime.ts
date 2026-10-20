export interface ConversationRealtimeCursor {
  nextCursor: number;
  conversationVersion: number;
  affected: boolean;
}

export interface ConversationRealtimeSnapshot {
  cursor: number | null;
  conversationVersion: number;
  error: string | null;
}
