import { z } from "zod";

const folderSchema = z.string().max(1_000).refine(
  (value) => !value.split("/").some((part) => part === "." || part === ".."),
  "记事本文件夹无效。",
);

export const notepadNoteSchema = z.object({
  noteId: z.string().trim().min(1).max(1_024),
  title: z.string().max(1_000),
  folder: folderSchema,
  tags: z.array(z.string().trim().min(1).max(128)).max(64),
  version: z.number().int().positive(),
  createdAtUnixMs: z.number().int().nonnegative(),
  updatedAtUnixMs: z.number().int().nonnegative(),
}).strict().superRefine((note, context) => {
  if (note.updatedAtUnixMs < note.createdAtUnixMs) {
    context.addIssue({ code: "custom", message: "笔记更新时间不能早于创建时间。", path: ["updatedAtUnixMs"] });
  }
  if (new Set(note.tags).size !== note.tags.length) {
    context.addIssue({ code: "custom", message: "笔记标签不能重复。", path: ["tags"] });
  }
});

export const notepadNoteDetailSchema = z.object({
  note: notepadNoteSchema,
  content: z.string().max(512 * 1_024),
}).strict();

export const notepadSnapshotSchema = z.object({
  folders: z.array(folderSchema).max(5_000),
  notes: z.array(notepadNoteSchema).max(500),
}).strict().superRefine((snapshot, context) => {
  if (new Set(snapshot.folders).size !== snapshot.folders.length) {
    context.addIssue({ code: "custom", message: "记事本文件夹不能重复。", path: ["folders"] });
  }
  if (new Set(snapshot.notes.map((note) => note.noteId)).size !== snapshot.notes.length) {
    context.addIssue({ code: "custom", message: "笔记标识不能重复。", path: ["notes"] });
  }
});

export const notepadFolderMutationSchema = z.object({
  folder: folderSchema,
  affectedNotes: z.number().int().nonnegative(),
}).strict();

export const notepadImageInsertionSchema = z.object({
  noteId: z.string().trim().min(1).max(1_024),
  expectedVersion: z.number().int().positive(),
  name: z.string().trim().min(1).max(512),
  mimeType: z.enum(["image/png", "image/jpeg", "image/webp", "image/gif"]),
  size: z.number().int().positive().max(512 * 1_024),
  sha256: z.string().regex(/^[a-f0-9]{64}$/u),
  markdown: z.string().min(1).max(512 * 1_024),
}).strict().superRefine((image, context) => {
  const prefix = `data:${image.mimeType};base64,`;
  if (!image.markdown.includes(prefix)) {
    context.addIssue({ code: "custom", message: "笔记图片正文与媒体类型不一致。", path: ["markdown"] });
  }
});

export type NotepadNote = z.infer<typeof notepadNoteSchema>;
export type NotepadNoteDetail = z.infer<typeof notepadNoteDetailSchema>;
export type NotepadSnapshot = z.infer<typeof notepadSnapshotSchema>;
export type NotepadFolderMutation = z.infer<typeof notepadFolderMutationSchema>;
export type NotepadImageInsertion = z.infer<typeof notepadImageInsertionSchema>;
export type NotepadImageSource = "picker" | "clipboard";
export type NotepadEditorMode = "edit" | "preview" | "split";

export interface NotepadDraft {
  title: string;
  folder: string;
  tags: string[];
  content: string;
}

export interface NotepadTreeRow {
  id: string;
  kind: "folder" | "note";
  depth: number;
  title: string;
  folder: string;
  note: NotepadNote | null;
}

export function normalizeNotepadFolder(value: string): string {
  return value
    .trim()
    .replaceAll("\\", "/")
    .split("/")
    .map((part) => part.trim())
    .filter((part) => part.length > 0 && part !== "." && part !== "..")
    .join("/");
}

export function parseNotepadTags(value: string): string[] {
  return [...new Set(value
    .split(/[,，\n]/u)
    .map((tag) => tag.trim())
    .filter(Boolean))].slice(0, 64);
}

export function compareNotepadNotes(left: NotepadNote, right: NotepadNote): number {
  return right.updatedAtUnixMs - left.updatedAtUnixMs
    || left.title.localeCompare(right.title, undefined, { sensitivity: "base" })
    || left.noteId.localeCompare(right.noteId);
}

export function buildNotepadTree(folders: string[], notes: NotepadNote[]): NotepadTreeRow[] {
  const allFolders = new Set<string>();
  for (const rawFolder of folders) addFolderAncestors(allFolders, rawFolder);
  for (const note of notes) addFolderAncestors(allFolders, note.folder);
  const childFolders = new Map<string, string[]>();
  for (const folder of allFolders) {
    const parent = parentFolder(folder);
    childFolders.set(parent, [...(childFolders.get(parent) ?? []), folder]);
  }
  for (const values of childFolders.values()) {
    values.sort((left, right) => leafName(left).localeCompare(leafName(right), undefined, { sensitivity: "base" }));
  }
  const notesByFolder = new Map<string, NotepadNote[]>();
  for (const note of [...notes].sort(compareNotepadNotes)) {
    const folder = normalizeNotepadFolder(note.folder);
    notesByFolder.set(folder, [...(notesByFolder.get(folder) ?? []), note]);
  }
  const rows: NotepadTreeRow[] = [];
  const append = (parent: string, depth: number) => {
    for (const folder of childFolders.get(parent) ?? []) {
      rows.push({ id: `folder:${folder}`, kind: "folder", depth, title: leafName(folder), folder, note: null });
      append(folder, depth + 1);
    }
    for (const note of notesByFolder.get(parent) ?? []) {
      rows.push({
        id: `note:${note.noteId}`,
        kind: "note",
        depth,
        title: note.title.trim() || "未命名笔记",
        folder: parent,
        note,
      });
    }
  };
  append("", 0);
  return rows;
}

function addFolderAncestors(target: Set<string>, rawFolder: string) {
  const parts = normalizeNotepadFolder(rawFolder).split("/").filter(Boolean);
  for (let index = 1; index <= parts.length; index += 1) {
    target.add(parts.slice(0, index).join("/"));
  }
}

function parentFolder(folder: string) {
  const parts = folder.split("/");
  return parts.slice(0, -1).join("/");
}

function leafName(folder: string) {
  return folder.split("/").at(-1) ?? folder;
}
