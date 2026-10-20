import { describe, expect, it } from "vitest";
import { flattenDirectoryTree, projectDirectoryListingSchema } from "./projectDirectory";

describe("project directory domain", () => {
  it("accepts only direct portable relative children", () => {
    expect(projectDirectoryListingSchema.parse(listing("", [entry("Sources", true)])).entries).toHaveLength(1);
    expect(() => projectDirectoryListingSchema.parse(listing("", [entry("/secret", false)]))).toThrow();
    expect(() => projectDirectoryListingSchema.parse(listing("Sources", [entry("Other/file", false)]))).toThrow();
  });

  it("flattens only expanded directory listings", () => {
    const root = listing("", [entry("Sources", true), entry("README.md", false)]);
    const sources = listing("Sources", [{ ...entry("Sources/App.ts", false), name: "App.ts" }]);
    const collapsed = flattenDirectoryTree({ "": root, Sources: sources }, new Set(), new Set());
    expect(collapsed.map((item) => item.entry.path)).toEqual(["Sources", "README.md"]);
    const expanded = flattenDirectoryTree({ "": root, Sources: sources }, new Set(["Sources"]), new Set());
    expect(expanded.map((item) => item.entry.path)).toEqual(["Sources", "Sources/App.ts", "README.md"]);
  });
});

function listing(path: string, entries: ReturnType<typeof entry>[]) {
  return {
    projectId: "project-1",
    path,
    parentPath: path ? "" : null,
    isWritable: true,
    entries,
    isTruncated: false,
  };
}

function entry(path: string, isDirectory: boolean) {
  return {
    name: path.split("/").at(-1) ?? path,
    path,
    isDirectory,
    isWritable: true,
    size: isDirectory ? null : 10,
    modifiedAtUnixMs: 1,
  };
}
