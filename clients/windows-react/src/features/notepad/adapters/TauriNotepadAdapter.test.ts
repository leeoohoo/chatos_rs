import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { TauriNotepadAdapter } from "./TauriNotepadAdapter";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

describe("TauriNotepadAdapter", () => {
  beforeEach(() => vi.mocked(invoke).mockReset());

  it("uses owner-scoped typed commands and exact expected versions", async () => {
    const adapter = new TauriNotepadAdapter();
    vi.mocked(invoke).mockResolvedValueOnce({ folders: ["工作"], notes: [note()] });
    await adapter.load("owner-1", "方案");
    expect(invoke).toHaveBeenLastCalledWith("load_notepad", {
      input: { ownerUserId: "owner-1", query: "方案" },
    });

    vi.mocked(invoke).mockResolvedValueOnce({ note: note({ version: 2 }), content: "更新" });
    await adapter.updateNote("owner-1", {
      noteId: "note-1", expectedVersion: 1, title: "方案", folder: "工作",
      tags: ["重要"], content: "更新",
    });
    expect(invoke).toHaveBeenLastCalledWith("update_notepad_note", {
      input: {
        ownerUserId: "owner-1", noteId: "note-1", expectedVersion: 1,
        title: "方案", folder: "工作", tags: ["重要"], content: "更新",
      },
    });
  });

  it("rejects native responses that expose owner, file paths, or unknown fields", async () => {
    const adapter = new TauriNotepadAdapter();
    vi.mocked(invoke).mockResolvedValueOnce({
      folders: [],
      notes: [{ ...note(), ownerUserId: "owner-1", file: "C:\\private\\note.md" }],
    });
    await expect(adapter.load("owner-1", null)).rejects.toThrow();
  });

  it("imports images through native picker without sending browser bytes", async () => {
    const adapter = new TauriNotepadAdapter();
    vi.mocked(invoke).mockResolvedValueOnce({
      noteId: "note-1", expectedVersion: 3, name: "screen.png", mimeType: "image/png",
      size: 4, sha256: "a".repeat(64), markdown: "![screen](<data:image/png;base64,iVBORw==>)",
    });
    await adapter.importImage("owner-1", "note-1", 3, "picker");
    expect(invoke).toHaveBeenLastCalledWith("import_notepad_image", {
      input: { ownerUserId: "owner-1", noteId: "note-1", expectedVersion: 3, source: "picker" },
    });
  });

  it("uses native clipboard and save dialog for note actions", async () => {
    const adapter = new TauriNotepadAdapter();
    vi.mocked(invoke).mockResolvedValueOnce(undefined);
    await adapter.copyText("# 方案");
    expect(invoke).toHaveBeenLastCalledWith("copy_notepad_text", {
      input: { text: "# 方案" },
    });

    vi.mocked(invoke).mockResolvedValueOnce(true);
    await expect(adapter.exportMarkdown("owner-1", "方案", "# 方案")).resolves.toBe(true);
    expect(invoke).toHaveBeenLastCalledWith("export_notepad_markdown", {
      input: { ownerUserId: "owner-1", title: "方案", content: "# 方案" },
    });
  });
});

function note(overrides: Record<string, unknown> = {}) {
  return {
    noteId: "note-1", title: "方案", folder: "工作", tags: ["重要"], version: 1,
    createdAtUnixMs: 10, updatedAtUnixMs: 20, ...overrides,
  };
}
