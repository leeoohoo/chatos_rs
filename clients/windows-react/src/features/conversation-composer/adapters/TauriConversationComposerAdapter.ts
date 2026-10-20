import { invoke } from "@tauri-apps/api/core";
import { z } from "zod";
import type { ConversationComposerPort } from "../ports/ConversationComposerPort";

const sendAckSchema = z.object({
  deliveryKind: z.enum(["guidance", "newTurn"]),
  turnId: z.string().min(1),
  userMessageId: z.string().min(1),
});

export class TauriConversationComposerAdapter implements ConversationComposerPort {
  async sendDraft(input: Parameters<ConversationComposerPort["sendDraft"]>[0]) {
    return sendAckSchema.parse(await invoke("send_conversation_draft", { input }));
  }
}
