import { describe, expect, it } from "vitest";
import { buildNotepadTree, normalizeNotepadFolder, parseNotepadTags } from "./notepad";

describe("notepad domain", () => {
  it("builds a deterministic folder-first tree with inferred ancestors", () => {
    const rows = buildNotepadTree(["工作/方案"], [
      note("root", "", 10),
      note("child", "工作/方案", 20),
    ]);

    expect(rows.map((row) => [row.kind, row.title, row.depth])).toEqual([
      ["folder", "工作", 0],
      ["folder", "方案", 1],
      ["note", "child", 2],
      ["note", "root", 0],
    ]);
  });

  it("normalizes folders and de-duplicates comma-separated tags", () => {
    expect(normalizeNotepadFolder(" 工作\\方案// ")).toBe("工作/方案");
    expect(parseNotepadTags("重要, 方案，重要\n待办")).toEqual(["重要", "方案", "待办"]);
  });
});

function note(noteId: string, folder: string, updatedAtUnixMs: number) {
  return {
    noteId, title: noteId, folder, tags: [], version: 1,
    createdAtUnixMs: 1, updatedAtUnixMs,
  };
}
