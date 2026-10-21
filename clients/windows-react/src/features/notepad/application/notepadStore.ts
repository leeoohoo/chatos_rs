import { create } from "zustand";
import {
  compareNotepadNotes,
  normalizeNotepadFolder,
  type NotepadDraft,
  type NotepadEditorMode,
  type NotepadNote,
  type NotepadNoteDetail,
} from "../domain/notepad";
import type { NotepadPort } from "../ports/NotepadPort";

const emptyDraft: NotepadDraft = { title: "", folder: "", tags: [], content: "" };

interface NotepadState {
  ownerUserId: string | null;
  folders: string[];
  notes: NotepadNote[];
  selectedFolder: string;
  selectedNoteId: string | null;
  draft: NotepadDraft;
  savedDraft: NotepadDraft;
  searchQuery: string;
  editorMode: NotepadEditorMode;
  isLoading: boolean;
  isLoadingNote: boolean;
  isSaving: boolean;
  isMutating: boolean;
  error: string | null;
  epoch: number;
  requestSerial: number;
  listRequestId: number;
  noteRequestId: number;
  draftRevision: number;
  activate(port: NotepadPort, ownerUserId: string): Promise<void>;
  refresh(port: NotepadPort, showsProgress?: boolean, retainedError?: string | null): Promise<boolean>;
  syncExternalChanges(port: NotepadPort): Promise<void>;
  setSearchQuery(query: string): void;
  selectFolder(folder: string): void;
  selectNote(port: NotepadPort, noteId: string, force?: boolean): Promise<boolean>;
  updateDraft(patch: Partial<NotepadDraft>): void;
  setEditorMode(mode: NotepadEditorMode): void;
  save(port: NotepadPort): Promise<boolean>;
  createFolder(port: NotepadPort, folder: string): Promise<boolean>;
  renameFolder(port: NotepadPort, from: string, to: string): Promise<boolean>;
  deleteFolder(port: NotepadPort, folder: string): Promise<boolean>;
  createNote(port: NotepadPort, title: string, folder?: string): Promise<boolean>;
  deleteSelectedNote(port: NotepadPort): Promise<boolean>;
  clearError(): void;
  reset(): void;
}

export const useNotepadStore = create<NotepadState>((set, get) => ({
  ownerUserId: null,
  folders: [],
  notes: [],
  selectedFolder: "",
  selectedNoteId: null,
  draft: emptyDraft,
  savedDraft: emptyDraft,
  searchQuery: "",
  editorMode: "preview",
  isLoading: false,
  isLoadingNote: false,
  isSaving: false,
  isMutating: false,
  error: null,
  epoch: 0,
  requestSerial: 0,
  listRequestId: 0,
  noteRequestId: 0,
  draftRevision: 0,

  async activate(port, ownerInput) {
    const ownerUserId = ownerInput.trim();
    if (!ownerUserId) {
      get().reset();
      return;
    }
    if (get().ownerUserId !== ownerUserId) {
      set((state) => ({
        ...initialContent(),
        ownerUserId,
        epoch: state.epoch + 1,
        requestSerial: state.requestSerial,
      }));
    }
    await get().refresh(port);
  },

  async refresh(port, showsProgress = true, retainedError = null) {
    const current = get();
    if (!current.ownerUserId || current.listRequestId !== 0 || current.isMutating) return false;
    const ownerUserId = current.ownerUserId;
    const epoch = current.epoch;
    const requestId = current.requestSerial + 1;
    set({
      requestSerial: requestId,
      listRequestId: requestId,
      isLoading: showsProgress && current.notes.length === 0,
      error: retainedError,
    });
    try {
      const snapshot = await port.load(ownerUserId, current.searchQuery.trim() || null);
      const latest = get();
      if (!accepts(latest, ownerUserId, epoch) || latest.listRequestId !== requestId) return false;
      const notes = [...snapshot.notes].sort(compareNotepadNotes);
      const selectedStillExists = latest.selectedNoteId == null
        || notes.some((note) => note.noteId === latest.selectedNoteId);
      set({
        folders: normalizeFolders(snapshot.folders),
        notes,
        isLoading: false,
        listRequestId: 0,
        error: retainedError,
        ...(!selectedStillExists ? clearedSelection(latest.selectedFolder) : {}),
      });
      if (get().selectedNoteId == null && notes[0]) {
        await get().selectNote(port, notes[0].noteId);
      }
      return true;
    } catch (failure) {
      const latest = get();
      if (!accepts(latest, ownerUserId, epoch) || latest.listRequestId !== requestId) return false;
      set({ isLoading: false, listRequestId: 0, error: retainedError ?? errorMessage(failure) });
      return false;
    }
  },

  async syncExternalChanges(port) {
    const current = get();
    if (!current.ownerUserId || isDirty(current) || current.isSaving || current.isMutating
      || current.isLoading || current.isLoadingNote) return;
    const selectedNoteId = current.selectedNoteId;
    const previousVersion = current.notes.find((note) => note.noteId === selectedNoteId)?.version;
    if (!(await get().refresh(port, false))) return;
    const latest = get();
    if (!selectedNoteId || latest.selectedNoteId !== selectedNoteId || isDirty(latest)) return;
    const nextVersion = latest.notes.find((note) => note.noteId === selectedNoteId)?.version;
    if (nextVersion != null && nextVersion !== previousVersion) {
      await latest.selectNote(port, selectedNoteId, true);
    }
  },

  setSearchQuery(searchQuery) {
    set({ searchQuery });
  },

  selectFolder(folder) {
    set({ selectedFolder: normalizeNotepadFolder(folder) });
  },

  async selectNote(port, noteIdInput, force = false) {
    const noteId = noteIdInput.trim();
    let current = get();
    if (!current.ownerUserId || !noteId) return false;
    if (!force && current.selectedNoteId === noteId) {
      if (current.noteRequestId !== 0) {
        const requestId = current.requestSerial + 1;
        set({ requestSerial: requestId, noteRequestId: 0, isLoadingNote: false });
      }
      return true;
    }
    if (isDirty(current) && !(await current.save(port))) return false;
    current = get();
    if (!current.ownerUserId) return false;
    const ownerUserId = current.ownerUserId;
    const epoch = current.epoch;
    const requestId = current.requestSerial + 1;
    set({ requestSerial: requestId, noteRequestId: requestId, isLoadingNote: true, error: null });
    try {
      const detail = await port.getNote(ownerUserId, noteId);
      const latest = get();
      if (!accepts(latest, ownerUserId, epoch) || latest.noteRequestId !== requestId) return false;
      if (detail.note.noteId !== noteId) throw new Error("Local Agent Host 返回了不匹配的笔记详情。");
      set({
        notes: upsertNote(latest.notes, detail.note),
        selectedNoteId: noteId,
        selectedFolder: detail.note.folder,
        draft: detailDraft(detail),
        savedDraft: detailDraft(detail),
        draftRevision: latest.draftRevision + 1,
        noteRequestId: 0,
        isLoadingNote: false,
        error: null,
      });
      return true;
    } catch (failure) {
      const latest = get();
      if (!accepts(latest, ownerUserId, epoch) || latest.noteRequestId !== requestId) return false;
      set({ noteRequestId: 0, isLoadingNote: false, error: errorMessage(failure) });
      return false;
    }
  },

  updateDraft(patch) {
    set((state) => ({ draft: { ...state.draft, ...patch }, draftRevision: state.draftRevision + 1 }));
  },

  setEditorMode(editorMode) {
    set({ editorMode });
  },

  async save(port) {
    const current = get();
    const note = current.notes.find((candidate) => candidate.noteId === current.selectedNoteId);
    if (!current.ownerUserId || !current.selectedNoteId || !note) return true;
    if (!isDirty(current)) return true;
    if (current.isSaving || current.isMutating) return false;
    const ownerUserId = current.ownerUserId;
    const noteId = current.selectedNoteId;
    const epoch = current.epoch;
    const revision = current.draftRevision;
    const submitted = normalizeDraft(current.draft);
    set({ isSaving: true, error: null });
    try {
      const detail = await port.updateNote(ownerUserId, {
        noteId,
        expectedVersion: note.version,
        ...submitted,
      });
      const latest = get();
      if (!accepts(latest, ownerUserId, epoch) || latest.selectedNoteId !== noteId) return false;
      if (detail.note.noteId !== noteId || detail.note.version <= note.version) {
        throw new Error("Local Agent Host 返回了不匹配的笔记保存结果。");
      }
      const nextSaved = { ...submitted, title: detail.note.title, folder: detail.note.folder, tags: detail.note.tags };
      set({
        notes: upsertNote(latest.notes, detail.note),
        savedDraft: nextSaved,
        draft: latest.draftRevision === revision ? nextSaved : latest.draft,
        selectedFolder: latest.draftRevision === revision ? detail.note.folder : latest.selectedFolder,
        draftRevision: latest.draftRevision === revision ? latest.draftRevision + 1 : latest.draftRevision,
        isSaving: false,
      });
      return true;
    } catch (failure) {
      const latest = get();
      if (!accepts(latest, ownerUserId, epoch) || latest.selectedNoteId !== noteId) return false;
      const message = errorMessage(failure);
      set({ isSaving: false, error: message });
      await reloadTruth(port, ownerUserId, noteId, epoch, message, set, get);
      return false;
    }
  },

  async createFolder(port, folderInput) {
    const current = get();
    const folder = normalizeNotepadFolder(folderInput);
    if (!current.ownerUserId || !folder || current.isMutating) return false;
    return runMutation(port, set, get, async (ownerUserId) => {
      const result = await port.createFolder(ownerUserId, folder);
      if (result.folder !== folder) throw new Error("文件夹创建结果不一致。");
      set({ selectedFolder: folder });
    });
  },

  async renameFolder(port, fromInput, toInput) {
    const current = get();
    const from = normalizeNotepadFolder(fromInput);
    const to = normalizeNotepadFolder(toInput);
    if (!current.ownerUserId || !from || !to || current.isMutating) return false;
    return runMutation(port, set, get, async (ownerUserId) => {
      const result = await port.renameFolder(ownerUserId, from, to);
      if (result.folder !== to) throw new Error("文件夹重命名结果不一致。");
      const latest = get();
      const selectedFolder = replaceFolderPrefix(latest.selectedFolder, from, to);
      set({ selectedFolder });
    });
  },

  async deleteFolder(port, folderInput) {
    const current = get();
    const folder = normalizeNotepadFolder(folderInput);
    if (!current.ownerUserId || !folder || current.isMutating) return false;
    return runMutation(port, set, get, async (ownerUserId) => {
      const result = await port.deleteFolder(ownerUserId, folder);
      if (result.folder !== folder) throw new Error("文件夹删除结果不一致。");
      const latest = get();
      if (inFolder(latest.selectedFolder, folder)) set({ selectedFolder: "" });
    });
  },

  async createNote(port, titleInput, folderInput) {
    const current = get();
    if (!current.ownerUserId || current.isMutating) return false;
    const ownerUserId = current.ownerUserId;
    const epoch = current.epoch;
    const folder = normalizeNotepadFolder(folderInput ?? current.selectedFolder);
    const title = titleInput.trim();
    set({ isMutating: true, error: null });
    try {
      const detail = await port.createNote(ownerUserId, { folder, title, content: "", tags: [] });
      const latest = get();
      if (!accepts(latest, ownerUserId, epoch)) return false;
      set({
        notes: upsertNote(latest.notes, detail.note),
        selectedNoteId: detail.note.noteId,
        selectedFolder: detail.note.folder,
        draft: detailDraft(detail),
        savedDraft: detailDraft(detail),
        draftRevision: latest.draftRevision + 1,
        isMutating: false,
      });
      await get().refresh(port, false);
      return true;
    } catch (failure) {
      if (accepts(get(), ownerUserId, epoch)) set({ isMutating: false, error: errorMessage(failure) });
      return false;
    }
  },

  async deleteSelectedNote(port) {
    const current = get();
    const note = current.notes.find((candidate) => candidate.noteId === current.selectedNoteId);
    if (!current.ownerUserId || !note || current.isMutating) return false;
    const ownerUserId = current.ownerUserId;
    const epoch = current.epoch;
    set({ isMutating: true, error: null });
    try {
      const deletedId = await port.deleteNote(ownerUserId, note.noteId, note.version);
      const latest = get();
      if (!accepts(latest, ownerUserId, epoch)) return false;
      if (deletedId !== note.noteId) throw new Error("笔记删除结果不一致。");
      set({ ...clearedSelection(latest.selectedFolder), isMutating: false });
      await get().refresh(port, false);
      return true;
    } catch (failure) {
      if (accepts(get(), ownerUserId, epoch)) {
        const message = errorMessage(failure);
        set({ isMutating: false, error: message });
        await get().refresh(port, false, message);
      }
      return false;
    }
  },

  clearError() {
    set({ error: null });
  },

  reset() {
    set((state) => ({
      ...initialContent(),
      ownerUserId: null,
      epoch: state.epoch + 1,
      requestSerial: state.requestSerial,
    }));
  },
}));

export function isNotepadDirty(state: Pick<NotepadState, "selectedNoteId" | "draft" | "savedDraft">) {
  return isDirty(state);
}

function initialContent() {
  return {
    folders: [],
    notes: [],
    selectedFolder: "",
    selectedNoteId: null,
    draft: emptyDraft,
    savedDraft: emptyDraft,
    searchQuery: "",
    editorMode: "preview" as NotepadEditorMode,
    isLoading: false,
    isLoadingNote: false,
    isSaving: false,
    isMutating: false,
    error: null,
    listRequestId: 0,
    noteRequestId: 0,
    draftRevision: 0,
  };
}

function accepts(state: NotepadState, ownerUserId: string, epoch: number) {
  return state.ownerUserId === ownerUserId && state.epoch === epoch;
}

function isDirty(state: Pick<NotepadState, "selectedNoteId" | "draft" | "savedDraft">) {
  return state.selectedNoteId != null && !sameDraft(state.draft, state.savedDraft);
}

function sameDraft(left: NotepadDraft, right: NotepadDraft) {
  return left.title === right.title && left.folder === right.folder
    && left.content === right.content && left.tags.join("\0") === right.tags.join("\0");
}

function normalizeDraft(draft: NotepadDraft): NotepadDraft {
  return {
    title: draft.title.trim(),
    folder: normalizeNotepadFolder(draft.folder),
    tags: [...new Set(draft.tags.map((tag) => tag.trim()).filter(Boolean))].slice(0, 64),
    content: draft.content,
  };
}

function detailDraft(detail: NotepadNoteDetail): NotepadDraft {
  return {
    title: detail.note.title,
    folder: detail.note.folder,
    tags: detail.note.tags,
    content: detail.content,
  };
}

function normalizeFolders(folders: string[]) {
  return [...new Set(folders.map(normalizeNotepadFolder).filter(Boolean))]
    .sort((left, right) => left.localeCompare(right, undefined, { sensitivity: "base" }));
}

function upsertNote(notes: NotepadNote[], note: NotepadNote) {
  return [...notes.filter((candidate) => candidate.noteId !== note.noteId), note].sort(compareNotepadNotes);
}

function clearedSelection(selectedFolder: string) {
  return {
    selectedNoteId: null,
    selectedFolder,
    draft: emptyDraft,
    savedDraft: emptyDraft,
    draftRevision: 0,
    noteRequestId: 0,
    isLoadingNote: false,
    isSaving: false,
  };
}

async function reloadTruth(
  port: NotepadPort,
  ownerUserId: string,
  noteId: string,
  epoch: number,
  retainedError: string,
  set: (patch: Partial<NotepadState>) => void,
  get: () => NotepadState,
) {
  try {
    const detail = await port.getNote(ownerUserId, noteId);
    const latest = get();
    if (!accepts(latest, ownerUserId, epoch) || latest.selectedNoteId !== noteId
      || detail.note.noteId !== noteId) return;
    set({
      notes: upsertNote(latest.notes, detail.note),
      draft: detailDraft(detail),
      savedDraft: detailDraft(detail),
      selectedFolder: detail.note.folder,
      draftRevision: latest.draftRevision + 1,
      error: retainedError,
    });
  } catch {
    // Keep the original mutation error; a manual refresh can recover if the note vanished.
  }
}

async function runMutation(
  port: NotepadPort,
  set: (patch: Partial<NotepadState>) => void,
  get: () => NotepadState,
  operation: (ownerUserId: string) => Promise<void>,
) {
  const current = get();
  if (!current.ownerUserId) return false;
  const ownerUserId = current.ownerUserId;
  const epoch = current.epoch;
  set({ isMutating: true, error: null });
  try {
    await operation(ownerUserId);
    if (!accepts(get(), ownerUserId, epoch)) return false;
    set({ isMutating: false });
    await get().refresh(port, false);
    return true;
  } catch (failure) {
    if (accepts(get(), ownerUserId, epoch)) {
      const message = errorMessage(failure);
      set({ isMutating: false, error: message });
      await get().refresh(port, false, message);
    }
    return false;
  }
}

function replaceFolderPrefix(folder: string, from: string, to: string) {
  if (folder === from) return to;
  return folder.startsWith(`${from}/`) ? `${to}${folder.slice(from.length)}` : folder;
}

function inFolder(value: string, folder: string) {
  return value === folder || value.startsWith(`${folder}/`);
}

function errorMessage(failure: unknown) {
  return failure instanceof Error ? failure.message : String(failure);
}
