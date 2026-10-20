import { invoke } from "@tauri-apps/api/core";
import { z } from "zod";
import type { ConversationRealtimePort } from "../ports/ConversationRealtimePort";

const cursorSchema = z.object({
  nextCursor: z.number().int().nonnegative(),
  conversationVersion: z.number().int().positive(),
  affected: z.boolean(),
});

export class TauriConversationRealtimeAdapter implements ConversationRealtimePort {
  async poll(input: Parameters<ConversationRealtimePort["poll"]>[0]) {
    return cursorSchema.parse(await invoke("poll_conversation_realtime", { input }));
  }
}
