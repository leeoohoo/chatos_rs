// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

export interface FsEntryResponse {
  name?: string;
  path?: string;
  is_dir?: boolean;
  isDir?: boolean;
  writable?: boolean;
  size?: number | null;
  modified_at?: string | null;
  modifiedAt?: string | null;
}

export interface FsEntriesResponse {
  path?: string | null;
  parent?: string | null;
  writable?: boolean;
  entries?: FsEntryResponse[];
  roots?: FsEntryResponse[];
  truncated?: boolean;
}

export interface FsListEntriesOptions {
  forceRefresh?: boolean;
}

export interface FsReadFileResponse {
  path?: string;
  name?: string;
  size?: number;
  content_type?: string;
  contentType?: string;
  is_binary?: boolean;
  isBinary?: boolean;
  writable?: boolean | null;
  modified_at?: string | null;
  modifiedAt?: string | null;
  content?: string;
}

export interface FsContentSearchEntryResponse {
  path?: string;
  relative_path?: string;
  relativePath?: string;
  line?: number;
  column?: number;
  text?: string;
}

export interface FsContentSearchResponse {
  path?: string | null;
  query?: string | null;
  entries?: FsContentSearchEntryResponse[];
  truncated?: boolean;
  visited_dirs?: number;
  visitedDirs?: number;
}

export interface FsMutationResponse {
  success?: boolean;
  path?: string;
  name?: string;
  message?: string;
}

export interface FsMoveResponse extends FsMutationResponse {
  to_path?: string;
  toPath?: string;
}

export interface FsMoveOptions {
  targetName?: string;
  replaceExisting?: boolean;
}

export interface FsAppendGitignoreResponse extends FsMutationResponse {
  pattern?: string;
  created?: boolean;
  appended?: boolean;
}

export interface FsOpenPathResponse extends FsMutationResponse {
  mode?: string;
}

export interface FsDiscardGitChangesResponse extends FsMutationResponse {
  stdout?: string | null;
  stderr?: string | null;
}
