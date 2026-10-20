import { describe, expect, it } from "vitest";
import {
  formatAttachmentSize,
  mergeAttachmentDrafts,
  shouldImportPaste,
  type ConversationAttachmentDraft,
} from "./conversationAttachment";

const attachment = (id: string): ConversationAttachmentDraft => ({
  id,
  name: `${id}.txt`,
  mimeType: "text/plain",
  kind: "file",
  origin: "file",
  byteSize: 10,
  previewUrl: null,
});

describe("conversation attachment domain", () => {
  it("restores failed attachments first without duplicates", () => {
    expect(mergeAttachmentDrafts(
      [attachment("failed"), attachment("duplicate")],
      [attachment("duplicate"), attachment("current")],
    ).map(({ id }) => id)).toEqual(["failed", "duplicate", "current"]);
  });

  it("classifies native attachment paste thresholds", () => {
    expect(shouldImportPaste(["image/png"], "")).toBe(true);
    expect(shouldImportPaste(["text/plain"], "short")).toBe(false);
    expect(shouldImportPaste(["text/plain"], "x".repeat(4_000))).toBe(true);
    expect(shouldImportPaste(["text/plain"], "中".repeat(3_000))).toBe(true);
  });

  it("formats bounded attachment sizes", () => {
    expect(formatAttachmentSize(12)).toBe("12 B");
    expect(formatAttachmentSize(1536)).toBe("1.5 KB");
    expect(formatAttachmentSize(2 * 1024 * 1024)).toBe("2.0 MB");
  });
});
