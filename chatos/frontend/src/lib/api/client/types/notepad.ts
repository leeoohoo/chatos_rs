// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

export interface NotepadListOptions {
  folder?: string;
  recursive?: boolean;
  tags?: string[];
  match?: 'all' | 'any';
  query?: string;
  limit?: number;
}

export interface NotepadCreatePayload {
  folder?: string;
  title?: string;
  content?: string;
  tags?: string[];
}

export interface NotepadUpdatePayload {
  title?: string;
  content?: string;
  folder?: string;
  tags?: string[];
}

export interface NotepadSearchOptions {
  query: string;
  folder?: string;
  recursive?: boolean;
  tags?: string[];
  match?: 'all' | 'any';
  include_content?: boolean;
  limit?: number;
}

export interface NotepadFolderMutationResponse {
  ok?: boolean;
  folder?: string;
  from?: string;
  to?: string;
  moved_notes?: number;
  deleted_notes?: number;
}

export interface NotepadNoteResponse {
  id: string;
  title: string;
  folder: string;
  tags: string[];
  created_at: string;
  updated_at: string;
  file: string;
}

export interface NotepadInitResponse {
  ok?: boolean;
  [key: string]: unknown;
}

export interface NotepadFoldersResponse {
  ok?: boolean;
  folders?: string[];
}

export interface NotepadNotesResponse {
  ok?: boolean;
  notes?: NotepadNoteResponse[];
}

export interface NotepadNoteDetailResponse {
  ok?: boolean;
  note?: NotepadNoteResponse | null;
  content?: string;
}

export interface NotepadDeleteNoteResponse {
  ok?: boolean;
  id?: string;
}

export interface NotepadTagResponse {
  tag: string;
  count: number;
}

export interface NotepadTagsResponse {
  ok?: boolean;
  tags?: NotepadTagResponse[];
}
