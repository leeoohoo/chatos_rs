export interface UiPromptHistoryItem {
  id: string;
  sessionId: string;
  conversationTurnId: string;
  kind: string;
  status: string;
  title: string;
  message: string;
  prompt: Record<string, unknown>;
  response: Record<string, unknown> | null;
  createdAt: string;
  updatedAt: string;
}
