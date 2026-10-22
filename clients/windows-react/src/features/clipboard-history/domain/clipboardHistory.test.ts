import { describe, expect, it } from "vitest";
import { clipboardHistorySchema } from "./clipboardHistory";

describe("clipboard history projection", () => {
  it("accepts only bounded safe metadata and controlled image URLs", () => {
    const base = { language: "zh-CN", entries: [entry()] };
    expect(clipboardHistorySchema.safeParse(base).success).toBe(true);
    expect(clipboardHistorySchema.safeParse({ ...base, entries: [{ ...entry(), previewUrl: "file:///C:/private.png" }] }).success).toBe(false);
    expect(clipboardHistorySchema.safeParse({ ...base, entries: [{ ...entry(), payloadReference: "Payloads/private.png" }] }).success).toBe(false);
    expect(clipboardHistorySchema.safeParse({ ...base, entries: [{ ...entry(), kind: "text", previewUrl: entry().previewUrl }] }).success).toBe(false);
  });
});

function entry() {
  return {
    id: "11111111-1111-4111-8111-111111111111",
    kind: "image",
    createdAtUnixMs: 1,
    updatedAtUnixMs: 2,
    preview: null,
    byteCount: 128,
    isPinned: false,
    previewUrl: "http://chatos-clipboard.localhost/11111111-1111-4111-8111-111111111111",
  };
}
