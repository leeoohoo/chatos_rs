import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { z } from "zod";
import type { ConversationAttachmentPort } from "../ports/ConversationAttachmentPort";

const scopeSchema = z.object({
  ownerUserId: z.string().min(1),
  conversationId: z.string().min(1),
  draftGeneration: z.number().int().nonnegative(),
});

const attachmentSchema = z.object({
  id: z.string().min(1),
  name: z.string().min(1),
  mimeType: z.string().min(1),
  kind: z.enum(["image", "file", "audio"]),
  origin: z.enum(["file", "pasted_image", "pasted_text"]),
  byteSize: z.number().int().positive(),
  previewUrl: z.string().min(1).nullable(),
});

const importResultSchema = z.object({
  scope: scopeSchema,
  attachments: z.array(attachmentSchema),
  errors: z.array(z.string()),
});

const clipboardResultSchema = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("attachments"), text: z.null(), result: importResultSchema }),
  z.object({ kind: z.literal("text"), text: z.string(), result: z.null() }),
  z.object({ kind: z.literal("empty"), text: z.null(), result: z.null() }),
]);

const dragStateSchema = z.object({
  scope: scopeSchema.nullable(),
  targeted: z.boolean(),
});

const textPreviewSchema = z.object({
  content: z.string(),
  truncated: z.boolean(),
});

export class TauriConversationAttachmentAdapter implements ConversationAttachmentPort {
  async activateScope(scope: Parameters<ConversationAttachmentPort["activateScope"]>[0]) {
    await invoke("activate_conversation_attachment_scope", { input: scopeSchema.parse(scope) });
  }

  async deactivateScope(scope: Parameters<ConversationAttachmentPort["deactivateScope"]>[0]) {
    await invoke("deactivate_conversation_attachment_scope", { input: scopeSchema.parse(scope) });
  }

  async pickFiles(scope: Parameters<ConversationAttachmentPort["pickFiles"]>[0]) {
    return importResultSchema.parse(await invoke("pick_conversation_attachments", { input: scope }));
  }

  async importClipboard(scope: Parameters<ConversationAttachmentPort["importClipboard"]>[0]) {
    return clipboardResultSchema.parse(await invoke("import_conversation_clipboard", { input: scope }));
  }

  async discard(
    scope: Parameters<ConversationAttachmentPort["discard"]>[0],
    attachmentIds: string[],
  ) {
    await invoke("discard_conversation_attachments", { input: { ...scope, attachmentIds } });
  }

  async previewText(
    scope: Parameters<ConversationAttachmentPort["previewText"]>[0],
    attachmentId: string,
  ) {
    return textPreviewSchema.parse(await invoke("preview_conversation_attachment_text", {
      input: { ...scope, attachmentId },
    }));
  }

  async loadPersisted(
    scope: Parameters<ConversationAttachmentPort["loadPersisted"]>[0],
    authorizedLocalRefs: string[],
  ) {
    return z.array(attachmentSchema).parse(await invoke(
      "load_persisted_conversation_attachments",
      { input: { ...scope, authorizedLocalRefs } },
    ));
  }

  async previewPersistedText(
    scope: Parameters<ConversationAttachmentPort["previewPersistedText"]>[0],
    authorizedLocalRef: string,
  ) {
    return textPreviewSchema.parse(await invoke(
      "preview_persisted_conversation_attachment_text",
      { input: { ...scope, authorizedLocalRef } },
    ));
  }

  async onStaged(listener: Parameters<ConversationAttachmentPort["onStaged"]>[0]) {
    return listen("conversation-attachments-staged", (event) => {
      listener(importResultSchema.parse(event.payload));
    });
  }

  async onDragState(listener: Parameters<ConversationAttachmentPort["onDragState"]>[0]) {
    return listen("conversation-attachment-drag-state", (event) => {
      listener(dragStateSchema.parse(event.payload));
    });
  }
}
