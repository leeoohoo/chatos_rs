import type {
  PetTranslationHistory,
  PetTranslationOutputStyle,
  PetTranslationRecord,
  PetTranslationTarget,
} from "../domain/petTranslation";

export interface PetTranslationPort {
  list(ownerUserId: string): Promise<PetTranslationHistory>;
  translate(input: {
    ownerUserId: string;
    sourceText: string;
    target: PetTranslationTarget;
    outputStyle: PetTranslationOutputStyle;
  }): Promise<PetTranslationRecord>;
  clear(ownerUserId: string): Promise<PetTranslationHistory>;
  close(): Promise<void>;
}
