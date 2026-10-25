import { invoke } from "@tauri-apps/api/core";
import { z } from "zod";
import {
  notepadFolderMutationSchema,
  notepadImageInsertionSchema,
  notepadNoteDetailSchema,
  notepadSnapshotSchema,
} from "../domain/notepad";
import type {
  CreateNotepadNoteDraft,
  NotepadPort,
  UpdateNotepadNoteDraft,
} from "../ports/NotepadPort";

export class TauriNotepadAdapter implements NotepadPort {
  async load(ownerUserId: string, query: string | null) {
    return notepadSnapshotSchema.parse(await invoke("load_notepad", {
      input: { ownerUserId, query },
    }));
  }

  async createFolder(ownerUserId: string, folder: string) {
    return notepadFolderMutationSchema.parse(await invoke("create_notepad_folder", {
      input: { ownerUserId, folder },
    }));
  }

  async renameFolder(ownerUserId: string, from: string, to: string) {
    return notepadFolderMutationSchema.parse(await invoke("rename_notepad_folder", {
      input: { ownerUserId, from, to },
    }));
  }

  async deleteFolder(ownerUserId: string, folder: string) {
    return notepadFolderMutationSchema.parse(await invoke("delete_notepad_folder", {
      input: { ownerUserId, folder, recursive: true },
    }));
  }

  async createNote(ownerUserId: string, draft: CreateNotepadNoteDraft) {
    return notepadNoteDetailSchema.parse(await invoke("create_notepad_note", {
      input: { ownerUserId, ...draft },
    }));
  }

  async getNote(ownerUserId: string, noteId: string) {
    return notepadNoteDetailSchema.parse(await invoke("get_notepad_note", {
      input: { ownerUserId, noteId },
    }));
  }

  async updateNote(ownerUserId: string, draft: UpdateNotepadNoteDraft) {
    return notepadNoteDetailSchema.parse(await invoke("update_notepad_note", {
      input: { ownerUserId, ...draft },
    }));
  }

  async deleteNote(ownerUserId: string, noteId: string, expectedVersion: number) {
    return z.string().parse(await invoke("delete_notepad_note", {
      input: { ownerUserId, noteId, expectedVersion },
    }));
  }

  async copyText(text: string) {
    await invoke("copy_notepad_text", { input: { text } });
  }

  async exportMarkdown(ownerUserId: string, title: string, content: string) {
    return z.boolean().parse(await invoke("export_notepad_markdown", {
      input: { ownerUserId, title, content },
    }));
  }

  async importImage(ownerUserId: string, noteId: string, expectedVersion: number, source: "picker" | "clipboard") {
    return notepadImageInsertionSchema.nullable().parse(await invoke("import_notepad_image", {
      input: { ownerUserId, noteId, expectedVersion, source },
    }));
  }
}
