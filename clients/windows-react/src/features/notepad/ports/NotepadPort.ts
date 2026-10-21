import type {
  NotepadFolderMutation,
  NotepadNoteDetail,
  NotepadSnapshot,
} from "../domain/notepad";

export interface CreateNotepadNoteDraft {
  folder: string;
  title: string;
  content: string;
  tags: string[];
}

export interface UpdateNotepadNoteDraft extends CreateNotepadNoteDraft {
  noteId: string;
  expectedVersion: number;
}

export interface NotepadPort {
  load(ownerUserId: string, query: string | null): Promise<NotepadSnapshot>;
  createFolder(ownerUserId: string, folder: string): Promise<NotepadFolderMutation>;
  renameFolder(ownerUserId: string, from: string, to: string): Promise<NotepadFolderMutation>;
  deleteFolder(ownerUserId: string, folder: string): Promise<NotepadFolderMutation>;
  createNote(ownerUserId: string, draft: CreateNotepadNoteDraft): Promise<NotepadNoteDetail>;
  getNote(ownerUserId: string, noteId: string): Promise<NotepadNoteDetail>;
  updateNote(ownerUserId: string, draft: UpdateNotepadNoteDraft): Promise<NotepadNoteDetail>;
  deleteNote(ownerUserId: string, noteId: string, expectedVersion: number): Promise<string>;
}
