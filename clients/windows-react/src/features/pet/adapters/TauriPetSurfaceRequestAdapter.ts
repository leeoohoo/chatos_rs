import { listen } from "@tauri-apps/api/event";
import type { PetSurfaceRequest, PetSurfaceRequestPort } from "../ports/PetSurfaceRequestPort";

export class TauriPetSurfaceRequestAdapter implements PetSurfaceRequestPort {
  async subscribe(listener: (request: PetSurfaceRequest) => void) {
    return listen<unknown>("pet-open-surface", (event) => {
      if (event.payload === "quick-chat" || event.payload === "notepad") listener(event.payload);
    });
  }
}
