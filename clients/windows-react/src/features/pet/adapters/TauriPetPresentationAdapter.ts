import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { petPresentationSchema, type PetPresentation } from "../domain/petPresentation";
import type { PetPresentationPort } from "../ports/PetPresentationPort";

export class TauriPetPresentationAdapter implements PetPresentationPort {
  async refresh(ownerUserId: string) {
    return petPresentationSchema.parse(await invoke("refresh_pet_presentation", { ownerUserId }));
  }

  async subscribe(listener: (presentation: PetPresentation) => void) {
    return listen<unknown>("pet-presentation", (event) => {
      const parsed = petPresentationSchema.safeParse(event.payload);
      if (parsed.success) listener(parsed.data);
    });
  }
}
