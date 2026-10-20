export const MAXIMUM_ATTACHMENT_COUNT = 20;
export const MAXIMUM_ATTACHMENT_BYTES = 20 * 1024 * 1024;
export const LONG_PASTE_CHARACTER_THRESHOLD = 4_000;
export const LONG_PASTE_BYTE_THRESHOLD = 8_000;

export type ConversationAttachmentKind = "image" | "file" | "audio";
export type ConversationAttachmentOrigin = "file" | "pasted_image" | "pasted_text";

export interface ConversationAttachmentDraft {
  id: string;
  name: string;
  mimeType: string;
  kind: ConversationAttachmentKind;
  origin: ConversationAttachmentOrigin;
  byteSize: number;
  previewUrl: string | null;
}

export interface AttachmentScope {
  ownerUserId: string;
  conversationId: string;
  draftGeneration: number;
}

export interface AttachmentImportResult {
  scope: AttachmentScope;
  attachments: ConversationAttachmentDraft[];
  errors: string[];
}

export interface AttachmentDragState {
  scope: AttachmentScope | null;
  targeted: boolean;
}

export type ClipboardImportResult =
  | { kind: "attachments"; text: null; result: AttachmentImportResult }
  | { kind: "text"; text: string; result: null }
  | { kind: "empty"; text: null; result: null };

export interface AttachmentTextPreview {
  content: string;
  truncated: boolean;
}

export function mergeAttachmentDrafts(
  failedOrIncoming: readonly ConversationAttachmentDraft[],
  current: readonly ConversationAttachmentDraft[],
): ConversationAttachmentDraft[] {
  const merged = new Map<string, ConversationAttachmentDraft>();
  for (const attachment of [...failedOrIncoming, ...current]) {
    if (!merged.has(attachment.id)) merged.set(attachment.id, attachment);
  }
  return [...merged.values()].slice(0, MAXIMUM_ATTACHMENT_COUNT);
}

export function formatAttachmentSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(bytes < 10 * 1024 ? 1 : 0)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function shouldImportPaste(types: readonly string[], text: string): boolean {
  return types.some((type) => type.startsWith("image/") || type === "Files")
    || Array.from(text).length >= LONG_PASTE_CHARACTER_THRESHOLD
    || new TextEncoder().encode(text).byteLength >= LONG_PASTE_BYTE_THRESHOLD;
}
