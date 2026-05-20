import { buildQuery } from '../shared';
import type {
  FsAppendGitignoreResponse,
  FsContentSearchResponse,
  FsDiscardGitChangesResponse,
  FsEntriesResponse,
  FsListEntriesOptions,
  FsMoveResponse,
  FsMutationResponse,
  FsOpenPathResponse,
  FsReadFileResponse,
} from '../types';
import type { ApiRequestFn } from './common';

export const listFsDirectories = (
  request: ApiRequestFn,
  path?: string,
  options?: FsListEntriesOptions,
): Promise<FsEntriesResponse> => {
  return request<FsEntriesResponse>(`/fs/list${buildQuery({ path, force_refresh: options?.forceRefresh })}`);
};

export const listFsEntries = (
  request: ApiRequestFn,
  path?: string,
  options?: FsListEntriesOptions,
): Promise<FsEntriesResponse> => {
  return request<FsEntriesResponse>(`/fs/entries${buildQuery({ path, force_refresh: options?.forceRefresh })}`);
};

export const searchFsEntries = (
  request: ApiRequestFn,
  path: string,
  query: string,
  limit?: number,
): Promise<FsEntriesResponse> => {
  return request<FsEntriesResponse>(`/fs/search${buildQuery({ path, q: query, limit })}`);
};

export const searchFsContent = (
  request: ApiRequestFn,
  path: string,
  query: string,
  options?: {
    limit?: number;
    caseSensitive?: boolean;
    wholeWord?: boolean;
  },
): Promise<FsContentSearchResponse> => {
  return request<FsContentSearchResponse>(`/fs/search-content${buildQuery({
    path,
    q: query,
    limit: options?.limit,
    case_sensitive: options?.caseSensitive,
    whole_word: options?.wholeWord,
  })}`);
};

export const readFsFile = (request: ApiRequestFn, path: string): Promise<FsReadFileResponse> => {
  return request<FsReadFileResponse>(`/fs/read${buildQuery({ path })}`);
};

export const createFsDirectory = (
  request: ApiRequestFn,
  parentPath: string,
  name: string,
): Promise<FsMutationResponse> => {
  return request<FsMutationResponse>('/fs/mkdir', {
    method: 'POST',
    body: JSON.stringify({
      parent_path: parentPath,
      name,
    }),
  });
};

export const createFsFile = (
  request: ApiRequestFn,
  parentPath: string,
  name: string,
  content = '',
): Promise<FsMutationResponse> => {
  return request<FsMutationResponse>('/fs/touch', {
    method: 'POST',
    body: JSON.stringify({
      parent_path: parentPath,
      name,
      content,
    }),
  });
};

export const deleteFsEntry = (
  request: ApiRequestFn,
  path: string,
  recursive = false,
): Promise<FsMutationResponse> => {
  return request<FsMutationResponse>('/fs/delete', {
    method: 'POST',
    body: JSON.stringify({
      path,
      recursive,
    }),
  });
};

export const moveFsEntry = (
  request: ApiRequestFn,
  sourcePath: string,
  targetParentPath: string,
  options?: { targetName?: string; replaceExisting?: boolean },
): Promise<FsMoveResponse> => {
  return request<FsMoveResponse>('/fs/move', {
    method: 'POST',
    body: JSON.stringify({
      source_path: sourcePath,
      target_parent_path: targetParentPath,
      target_name: options?.targetName,
      replace_existing: options?.replaceExisting,
    }),
  });
};

export const appendFsGitignore = (
  request: ApiRequestFn,
  path: string,
  mode: 'file' | 'folder' | 'extension',
): Promise<FsAppendGitignoreResponse> => {
  return request<FsAppendGitignoreResponse>('/fs/gitignore', {
    method: 'POST',
    body: JSON.stringify({
      path,
      mode,
    }),
  });
};

export const openFsPathExternally = (
  request: ApiRequestFn,
  path: string,
  mode: 'default' | 'reveal' | 'code',
): Promise<FsOpenPathResponse> => {
  return request<FsOpenPathResponse>('/fs/open', {
    method: 'POST',
    body: JSON.stringify({
      path,
      mode,
    }),
  });
};

export const discardFsGitChanges = (
  request: ApiRequestFn,
  path: string,
): Promise<FsDiscardGitChangesResponse> => {
  return request<FsDiscardGitChangesResponse>('/fs/discard-git-changes', {
    method: 'POST',
    body: JSON.stringify({
      path,
    }),
  });
};
