import type {
  AttachmentDragState,
  AttachmentImportResult,
  AttachmentScope,
  AttachmentTextPreview,
  ClipboardImportResult,
  ConversationAttachmentDraft,
} from "../domain/conversationAttachment";

export type UnlistenAttachmentEvent = () => void;

export interface ConversationAttachmentPort {
  activateScope(scope: AttachmentScope): Promise<void>;
  deactivateScope(scope: AttachmentScope): Promise<void>;
  pickFiles(scope: AttachmentScope): Promise<AttachmentImportResult>;
  importClipboard(scope: AttachmentScope): Promise<ClipboardImportResult>;
  discard(scope: AttachmentScope, attachmentIds: string[]): Promise<void>;
  previewText(scope: AttachmentScope, attachmentId: string): Promise<AttachmentTextPreview>;
  loadPersisted(
    scope: AttachmentScope,
    authorizedLocalRefs: string[],
  ): Promise<ConversationAttachmentDraft[]>;
  previewPersistedText(
    scope: AttachmentScope,
    authorizedLocalRef: string,
  ): Promise<AttachmentTextPreview>;
  onStaged(listener: (result: AttachmentImportResult) => void): Promise<UnlistenAttachmentEvent>;
  onDragState(listener: (state: AttachmentDragState) => void): Promise<UnlistenAttachmentEvent>;
}
