import { describe, expect, it } from "vitest";
import { quickSearchAllowsFileLookup, quickSearchSnapshotSchema } from "./quickSearch";

describe("quickSearch domain", () => {
  it("applies the macOS scope rules for file search", () => {
    expect(quickSearchAllowsFileLookup("report")).toBe(true);
    expect(quickSearchAllowsFileLookup("/ report")).toBe(true);
    expect(quickSearchAllowsFileLookup("> report")).toBe(false);
    expect(quickSearchAllowsFileLookup("@ report")).toBe(false);
    expect(quickSearchAllowsFileLookup("r")).toBe(false);
  });

  it("rejects paths and unknown native fields", () => {
    expect(() => quickSearchSnapshotSchema.parse({
      presentationId: "11111111-1111-4111-8111-111111111111",
      requestId: 1,
      results: [{
        id: "22222222-2222-4222-8222-222222222222",
        kind: "file",
        title: "report.md",
        subtitle: "Documents",
        icon: "file",
        path: "C:\\Users\\owner\\Documents\\report.md",
      }],
      isFileSearchTimedOut: false,
      diagnostic: null,
    })).toThrow();
  });
});
