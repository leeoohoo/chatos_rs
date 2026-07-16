// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type {
  FsAppendGitignoreResponse,
  FsContentSearchResponse,
  FsDiscardGitChangesResponse,
  FsEntriesResponse,
  FsMoveOptions,
  FsMoveResponse,
  FsMutationResponse,
  FsOpenPathResponse,
  FsReadFileResponse,
} from '../client/types';
import { requestLocalRuntime } from './bridge';

const query = (values: Record<string, string | number | boolean | undefined>): string => {
  const params = new URLSearchParams();
  Object.entries(values).forEach(([key, value]) => {
    if (value !== undefined) params.set(key, String(value));
  });
  const suffix = params.toString();
  return suffix ? `?${suffix}` : '';
};

const post = <T>(endpoint: string, body: unknown): Promise<T> => requestLocalRuntime<T>(endpoint, {
  method: 'POST',
  body: JSON.stringify(body),
});

export const listLocalFsEntries = (path: string): Promise<FsEntriesResponse> =>
  requestLocalRuntime(`/api/local/runtime/fs/entries${query({ path })}`);

export const searchLocalFsEntries = (
  path: string,
  searchQuery: string,
  limit?: number,
): Promise<FsEntriesResponse> => requestLocalRuntime(
  `/api/local/runtime/fs/search${query({ path, q: searchQuery, limit })}`,
);

export const searchLocalFsContent = (
  path: string,
  searchQuery: string,
  options?: { limit?: number; caseSensitive?: boolean; wholeWord?: boolean },
): Promise<FsContentSearchResponse> => requestLocalRuntime(
  `/api/local/runtime/fs/search-content${query({
    path,
    q: searchQuery,
    limit: options?.limit,
    case_sensitive: options?.caseSensitive,
    whole_word: options?.wholeWord,
  })}`,
);

export const readLocalFsFile = (path: string): Promise<FsReadFileResponse> =>
  requestLocalRuntime(`/api/local/runtime/fs/read${query({ path })}`);

export const createLocalFsDirectory = (parentPath: string, name: string): Promise<FsMutationResponse> =>
  post('/api/local/runtime/fs/mkdir', { parent_path: parentPath, name });

export const createLocalFsFile = (
  parentPath: string,
  name: string,
  content: string,
): Promise<FsMutationResponse> => post('/api/local/runtime/fs/touch', {
  parent_path: parentPath,
  name,
  content,
});

export const writeLocalFsFile = (path: string, content: string): Promise<FsMutationResponse> =>
  post('/api/local/runtime/fs/write', { path, content });

export const deleteLocalFsEntry = (path: string, recursive: boolean): Promise<FsMutationResponse> =>
  post('/api/local/runtime/fs/delete', { path, recursive });

export const moveLocalFsEntry = (
  sourcePath: string,
  targetParentPath: string,
  options?: FsMoveOptions,
): Promise<FsMoveResponse> => post('/api/local/runtime/fs/move', {
  source_path: sourcePath,
  target_parent_path: targetParentPath,
  target_name: options?.targetName,
  replace_existing: options?.replaceExisting,
});

export const appendLocalFsGitignore = (
  path: string,
  mode: 'file' | 'folder' | 'extension',
): Promise<FsAppendGitignoreResponse> => post('/api/local/runtime/fs/gitignore', { path, mode });

export const openLocalFsPath = (
  path: string,
  mode: 'default' | 'reveal' | 'code',
): Promise<FsOpenPathResponse> => post('/api/local/runtime/fs/open', { path, mode });

export const discardLocalFsGitChanges = (path: string): Promise<FsDiscardGitChangesResponse> =>
  post('/api/local/runtime/fs/discard-git-changes', { path });

export const downloadLocalFsEntry = async (
  path: string,
): Promise<{ blob: Blob; filename: string; contentType: string }> => {
  const response = await requestLocalRuntime<{
    filename?: string;
    content_type?: string;
    data_base64?: string;
  }>(`/api/local/runtime/fs/download${query({ path })}`);
  const binary = atob(response.data_base64 || '');
  const bytes = Uint8Array.from(binary, (character) => character.charCodeAt(0));
  const contentType = response.content_type || 'application/octet-stream';
  return {
    blob: new Blob([bytes], { type: contentType }),
    filename: response.filename || 'download',
    contentType,
  };
};
