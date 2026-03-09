export interface UiPromptHistoryItem {
  id: string;
  sessionId: string;
  conversationTurnId: string;
  kind: string;
  status: string;
  title: string;
  message: string;
  prompt: any;
  response: any;
  createdAt: string;
  updatedAt: string;
}
