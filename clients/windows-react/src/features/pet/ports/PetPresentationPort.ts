import type { PetPresentation } from "../domain/petPresentation";

export interface PetPresentationPort {
  refresh(ownerUserId: string): Promise<PetPresentation>;
  subscribe(listener: (presentation: PetPresentation) => void): Promise<() => void>;
  subscribeNotepadRequest(listener: () => void): Promise<() => void>;
  openTranslation(): Promise<void>;
  openNotepad(): Promise<void>;
}
