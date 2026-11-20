import { beforeEach, describe, expect, it, vi } from "vitest";
import type { NotepadNoteDetail, NotepadSnapshot } from "../domain/notepad";
import type { NotepadPort } from "../ports/NotepadPort";
import { useNotepadStore } from "./notepadStore";

describe("notepad store", () => {
  beforeEach(() => useNotepadStore.getState().reset());

  it("lets the latest note selection win when an earlier request finishes later", async () => {
    let releaseSlow!: (detail: NotepadNoteDetail) => void;
    const slow = new Promise<NotepadNoteDetail>((resolve) => { releaseSlow = resolve; });
    const port = stubPort({
      getNote: vi.fn(async (_owner, id) => {
        if (id === "b") return slow;
        return detail(id);
      }),
    });
    await useNotepadStore.getState().activate(port, "owner-1");

    const pending = useNotepadStore.getState().selectNote(port, "b");
    await useNotepadStore.getState().selectNote(port, "c");
    releaseSlow(detail("b"));
    await pending;

    expect(useNotepadStore.getState().selectedNoteId).toBe("c");
    expect(useNotepadStore.getState().draft.content).toBe("content-c");
  });

  it("re-reads Host truth after a version conflict", async () => {
    let fetchCount = 0;
    const getNote = vi.fn(async (_owner: string, id: string) => {
      fetchCount += 1;
      return fetchCount === 1 ? detail(id) : detail(id, 2, "server truth");
    });
    const port = stubPort({
      getNote,
      updateNote: vi.fn(async () => { throw new Error("笔记已经变化"); }),
    });
    await useNotepadStore.getState().activate(port, "owner-1");
    useNotepadStore.getState().updateDraft({ content: "local edit" });

    await expect(useNotepadStore.getState().save(port)).resolves.toBe(false);
    expect(getNote).toHaveBeenCalledTimes(2);
    expect(useNotepadStore.getState().draft.content).toBe("server truth");
    const selectedId = useNotepadStore.getState().selectedNoteId;
    expect(useNotepadStore.getState().notes.find((note) => note.noteId === selectedId)?.version).toBe(2);
    expect(useNotepadStore.getState().error).toContain("已经变化");
  });

  it("rejects owner and page teardown late results", async () => {
    let release!: (snapshot: NotepadSnapshot) => void;
    const port = stubPort({
      load: vi.fn(async () => new Promise<NotepadSnapshot>((resolve) => { release = resolve; })),
    });
    const pending = useNotepadStore.getState().activate(port, "owner-1");
    useNotepadStore.getState().reset();
    release(snapshot());
    await pending;

    expect(useNotepadStore.getState().ownerUserId).toBeNull();
    expect(useNotepadStore.getState().notes).toEqual([]);
  });

  it("repairs selection after recursively deleting its folder", async () => {
    const port = stubPort({
      load: vi.fn()
        .mockResolvedValueOnce(snapshot())
        .mockResolvedValueOnce({ folders: [], notes: [] }),
    });
    await useNotepadStore.getState().activate(port, "owner-1");
    useNotepadStore.getState().selectFolder("工作");

    await expect(useNotepadStore.getState().deleteFolder(port, "工作")).resolves.toBe(true);
    expect(useNotepadStore.getState().selectedFolder).toBe("");
    expect(useNotepadStore.getState().selectedNoteId).toBeNull();
  });

  it("inserts a native-imported image at the editor selection and version-saves it", async () => {
    const updateNote = vi.fn(async (_owner, draft) => detail(draft.noteId, draft.expectedVersion + 1, draft.content));
    const port = stubPort({
      importImage: vi.fn(async (_owner: string, noteId: string, expectedVersion: number) => ({
        noteId, expectedVersion, name: "screen.png", mimeType: "image/png" as const, size: 4,
        sha256: "a".repeat(64), markdown: "![screen](<data:image/png;base64,iVBORw==>)",
      })),
      updateNote,
    });
    await useNotepadStore.getState().activate(port, "owner-1");
    const current = useNotepadStore.getState().draft.content;

    await expect(useNotepadStore.getState().insertImage(port, "picker", current.length)).resolves.toBe(true);
    expect(updateNote).toHaveBeenCalledTimes(1);
    expect(useNotepadStore.getState().draft.content).toContain("data:image/png;base64,iVBORw==");
    expect(useNotepadStore.getState().error).toBeNull();
  });
});

function stubPort(overrides: Partial<NotepadPort> = {}): NotepadPort {
  const defaults: NotepadPort = {
    load: vi.fn(async () => snapshot()),
    createFolder: vi.fn(async (_owner, folder) => ({ folder, affectedNotes: 0 })),
    renameFolder: vi.fn(async (_owner, _from, to) => ({ folder: to, affectedNotes: 1 })),
    deleteFolder: vi.fn(async (_owner, folder) => ({ folder, affectedNotes: 1 })),
    createNote: vi.fn(async () => detail("a")),
    getNote: vi.fn(async (_owner, id) => detail(id)),
    updateNote: vi.fn(async (_owner, draft) => detail(draft.noteId, draft.expectedVersion + 1, draft.content)),
    deleteNote: vi.fn(async (_owner, noteId) => noteId),
    copyText: vi.fn(async () => undefined),
    exportMarkdown: vi.fn(async () => true),
    importImage: vi.fn(async () => null),
  };
  return { ...defaults, ...overrides };
}

function snapshot(): NotepadSnapshot {
  return { folders: ["工作"], notes: ["a", "b", "c"].map((id, index) => detail(id, 1, `content-${id}`, index + 1).note) };
}

function detail(id: string, version = 1, content = `content-${id}`, updatedAtUnixMs = version): NotepadNoteDetail {
  return {
    note: {
      noteId: id, title: id.toUpperCase(), folder: "工作", tags: [], version,
      createdAtUnixMs: 1, updatedAtUnixMs,
    },
    content,
  };
}
