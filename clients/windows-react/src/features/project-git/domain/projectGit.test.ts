import { describe, expect, it } from "vitest";
import { gitSnapshot } from "../../../test/projectGitFixture";
import {
  parseProjectGitDiff,
  projectGitSnapshotSchema,
  visibleGitDecorations,
} from "./projectGit";

describe("project Git domain", () => {
  it("parses a safe projection and strips native-only absolute fields", () => {
    const snapshot = projectGitSnapshotSchema.parse({
      ...gitSnapshot(),
      repositoryRoot: "/Users/private/project",
      changes: [{
        ...gitSnapshot().changes[0],
        absolutePath: "/Users/private/project/README.md",
      }],
    });
    expect(snapshot).not.toHaveProperty("repositoryRoot");
    expect(snapshot.changes[0]).not.toHaveProperty("absolutePath");
  });

  it("rejects traversal and inconsistent derived change state", () => {
    expect(() => projectGitSnapshotSchema.parse({
      ...gitSnapshot(),
      changes: [{ ...gitSnapshot().changes[0], path: "../secret" }],
    })).toThrow();
    expect(() => projectGitSnapshotSchema.parse({
      ...gitSnapshot(),
      changes: [{ ...gitSnapshot().changes[0], hasStagedChanges: false }],
    })).toThrow();
  });

  it("parses diff line numbers and enforces the macOS 8000-line display limit", () => {
    const lines = parseProjectGitDiff("diff --git a/a b/a\n@@ -4,2 +7,2 @@\n old\n-old\n+new");
    expect(lines[2]).toMatchObject({ oldLine: 4, newLine: 7, kind: "context" });
    expect(lines[3]).toMatchObject({ oldLine: 5, newLine: null, kind: "deletion" });
    expect(lines[4]).toMatchObject({ oldLine: null, newLine: 8, kind: "addition" });
    expect(parseProjectGitDiff(Array.from({ length: 8_100 }, (_, index) => ` line ${index}`).join("\n"))).toHaveLength(8_000);
  });

  it("hides internal run decorations and normalizes visible refs", () => {
    expect(visibleGitDecorations(gitSnapshot().commits[0])).toEqual(["main", "origin/main"]);
  });
});
