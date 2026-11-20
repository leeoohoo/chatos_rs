import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useNotepadStore } from "../application/notepadStore";
import type { NotepadNoteDetail } from "../domain/notepad";
import type { NotepadPort } from "../ports/NotepadPort";
import { NotepadView } from "./NotepadView";

beforeEach(() => useNotepadStore.getState().reset());
afterEach(() => { cleanup(); vi.restoreAllMocks(); });

describe("NotepadView", () => {
  it("matches the macOS sheet empty state and exposes creation from the root menu", async () => {
    const adapter = port({ load: vi.fn(async () => ({ folders: [], notes: [] })) });
    render(<NotepadView hostReady onClose={vi.fn()} ownerUserId="alice" port={adapter} />);

    expect(await screen.findByRole("dialog", { name: "记事本" })).toBeVisible();
    expect(screen.getByText("当前目录：根目录")).toBeVisible();
    expect(screen.getByText("暂无笔记")).toBeVisible();
    expect(screen.getByText("选择一条笔记")).toBeVisible();
    expect(screen.getByRole("button", { name: "复制" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "导出" })).toBeDisabled();

    fireEvent.contextMenu(screen.getByRole("treeitem", { name: "根目录" }));
    expect(screen.getByRole("menuitem", { name: "新建子文件夹" })).toBeVisible();
    expect(screen.getByRole("menuitem", { name: "新建笔记" })).toBeVisible();
  });

  it("uses native copy and export actions and saves a dirty note before closing", async () => {
    const onClose = vi.fn();
    const initial = detail();
    const adapter = port({
      load: vi.fn(async () => ({ folders: [], notes: [initial.note] })),
      getNote: vi.fn(async () => initial),
      updateNote: vi.fn(async (_owner, draft) => ({
        note: { ...initial.note, title: draft.title, version: 2, updatedAtUnixMs: 30 },
        content: draft.content,
      })),
    });
    render(<NotepadView hostReady onClose={onClose} ownerUserId="alice" port={adapter} />);

    const title = await screen.findByRole("textbox", { name: "笔记标题" });
    fireEvent.click(screen.getByRole("button", { name: "复制" }));
    await waitFor(() => expect(adapter.copyText).toHaveBeenCalledWith("# 正文"));
    fireEvent.click(screen.getByRole("button", { name: "导出" }));
    await waitFor(() => expect(adapter.exportMarkdown).toHaveBeenCalledWith("alice", "方案", "# 正文"));

    fireEvent.change(title, { target: { value: "新方案" } });
    fireEvent.click(screen.getByRole("button", { name: "关闭" }));
    await waitFor(() => expect(adapter.updateNote).toHaveBeenCalledWith("alice", expect.objectContaining({ title: "新方案" })));
    await waitFor(() => expect(onClose).toHaveBeenCalledOnce());
  });
});

function port(overrides: Partial<NotepadPort> = {}): NotepadPort {
  return {
    load: vi.fn(async () => ({ folders: [], notes: [] })),
    createFolder: vi.fn(async (_owner, folder) => ({ folder, affectedNotes: 0 })),
    renameFolder: vi.fn(async (_owner, _from, to) => ({ folder: to, affectedNotes: 0 })),
    deleteFolder: vi.fn(async (_owner, folder) => ({ folder, affectedNotes: 0 })),
    createNote: vi.fn(async () => detail()),
    getNote: vi.fn(async () => detail()),
    updateNote: vi.fn(async () => detail(2)),
    deleteNote: vi.fn(async (_owner, noteId) => noteId),
    copyText: vi.fn(async () => undefined),
    exportMarkdown: vi.fn(async () => true),
    importImage: vi.fn(async () => null),
    ...overrides,
  };
}

function detail(version = 1): NotepadNoteDetail {
  return {
    note: {
      noteId: "note-1",
      title: "方案",
      folder: "",
      tags: ["重要"],
      version,
      createdAtUnixMs: 10,
      updatedAtUnixMs: 20 + version,
    },
    content: "# 正文",
  };
}
