import { describe, expect, it } from "vitest";
import { remoteDirectoryFixture } from "../../../test/remoteFileFixture";
import {
  formatRemoteFileSize,
  remoteDirectoryResultSchema,
  remoteLocalFileSelectionSchema,
  visibleRemoteEntries,
} from "./remoteFiles";

describe("remote file domain", () => {
  it("validates the bounded native listing and filters like the macOS workspace", () => {
    const listing = remoteDirectoryFixture();
    const parsed = remoteDirectoryResultSchema.parse({ status: "completed", listing });
    expect(parsed.status).toBe("completed");
    if (parsed.status !== "completed") throw new Error("expected completed listing");
    expect(parsed.listing.entries).toHaveLength(3);
    expect(visibleRemoteEntries(listing.entries, "APP", false).map((entry) => entry.name)).toEqual(["app.log"]);
    expect(visibleRemoteEntries(listing.entries, "", false).map((entry) => entry.name)).not.toContain(".env");
    expect(visibleRemoteEntries(listing.entries, "", true).map((entry) => entry.name)).toContain(".env");
  });

  it("accepts only opaque local selections and never an absolute path", () => {
    expect(remoteLocalFileSelectionSchema.parse({
      selectionId: "remote-file-selection-1",
      fileName: "app.zip",
      size: 2_048,
    })).not.toHaveProperty("path");
    expect(() => remoteLocalFileSelectionSchema.parse({
      selectionId: "remote-file-selection-1",
      fileName: "app.zip",
      size: 2_048,
      absolutePath: "C:\\Users\\alice\\app.zip",
    })).toThrow();
  });

  it("formats bounded transfer sizes for the presentation layer", () => {
    expect(formatRemoteFileSize(null)).toBe("—");
    expect(formatRemoteFileSize(1_536)).toBe("1.5 KB");
    expect(formatRemoteFileSize(2 * 1_024 ** 2)).toBe("2.0 MB");
  });
});
