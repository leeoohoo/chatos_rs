import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  petTranslationHistorySchema,
  petTranslationRecordSchema,
} from "../domain/petTranslation";
import type { PetTranslationPort } from "../ports/PetTranslationPort";

export class TauriPetTranslationAdapter implements PetTranslationPort {
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
        },
      }),
    );
  }

  async clear(ownerUserId: string) {
    return petTranslationHistorySchema.parse(
      await invoke("clear_pet_translation_history", { ownerUserId }),
    );
  }

  async close() {
    await getCurrentWindow().close();
  }
}
