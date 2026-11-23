import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  petTranslationHistorySchema,
  petTranslationRecordSchema,
} from "../domain/petTranslation";
import type { PetTranslationPort } from "../ports/PetTranslationPort";
import { TauriConversationAttachmentAdapter } from "../../conversation-attachments/adapters/TauriConversationAttachmentAdapter";

export class TauriPetTranslationAdapter implements PetTranslationPort {
  private readonly attachments = new TauriConversationAttachmentAdapter();
  async list(ownerUserId: string) {
    return petTranslationHistorySchema.parse(
      await invoke("list_pet_translation_history", { ownerUserId }),
    );
  }

  async translate(input: Parameters<PetTranslationPort["translate"]>[0]) {
    return petTranslationRecordSchema.parse(
      await invoke("translate_pet_text", {
        input: {
          ownerUserId: input.ownerUserId,
          sourceText: input.sourceText,
          target: input.target,
          outputStyle: input.outputStyle,
          attachmentIds: input.attachmentIds,
        },
      }),
    );
  }

  async clear(ownerUserId: string) {
    return petTranslationHistorySchema.parse(
      await invoke("clear_pet_translation_history", { ownerUserId }),
    );
  }

  async pickAttachments(scope: Parameters<PetTranslationPort["pickAttachments"]>[0]) {
    return this.attachments.pickFiles(scope);
  }

  async importClipboard(scope: Parameters<PetTranslationPort["importClipboard"]>[0]) {
    return this.attachments.importClipboard(scope);
  }

  async discardAttachments(
    scope: Parameters<PetTranslationPort["discardAttachments"]>[0],
    attachmentIds: string[],
  ) {
    await this.attachments.discard(scope, attachmentIds);
  }

  async close() {
    await getCurrentWindow().close();
  }
}
