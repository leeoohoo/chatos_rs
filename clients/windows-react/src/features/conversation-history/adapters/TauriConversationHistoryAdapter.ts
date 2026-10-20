import { invoke } from "@tauri-apps/api/core";
import { conversationHistoryPageSchema } from "../../../contracts/conversationRunContract";
import type { ConversationHistoryPort } from "../ports/ConversationHistoryPort";

export class TauriConversationHistoryAdapter implements ConversationHistoryPort {
  async loadHistory(ownerUserId: string, conversationId: string, beforeOrdinal?: number) {
    return conversationHistoryPageSchema.parse(
      await invoke("load_conversation_history", {
        ownerUserId,
        conversationId,
        beforeOrdinal: beforeOrdinal ?? null,
      }),
    );
  }
}
