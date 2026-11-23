import type {
  PetTranslationHistory,
  PetTranslationOutputStyle,
  PetTranslationRecord,
  PetTranslationTarget,
} from "../domain/petTranslation";
import type {
  AttachmentImportResult,
  AttachmentScope,
  ClipboardImportResult,
} from "../../conversation-attachments/domain/conversationAttachment";

export interface PetTranslationPort {
  list(ownerUserId: string): Promise<PetTranslationHistory>;
  translate(input: {
    ownerUserId: string;
    sourceText: string;
    target: PetTranslationTarget;
    outputStyle: PetTranslationOutputStyle;
    attachmentIds: string[];
  }): Promise<PetTranslationRecord>;
  clear(ownerUserId: string): Promise<PetTranslationHistory>;
  pickAttachments(scope: AttachmentScope): Promise<AttachmentImportResult>;
  importClipboard(scope: AttachmentScope): Promise<ClipboardImportResult>;
  discardAttachments(scope: AttachmentScope, attachmentIds: string[]): Promise<void>;
  close(): Promise<void>;
}
