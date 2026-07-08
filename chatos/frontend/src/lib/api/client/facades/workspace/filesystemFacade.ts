// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import * as fsApi from '../../fs';
import * as workspaceApi from '../../workspace';
import type {
  FsAppendGitignoreResponse,
  FsContentSearchResponse,
  FsDiscardGitChangesResponse,
  FsEntriesResponse,
  FsListEntriesOptions,
  FsMoveOptions,
  FsMoveResponse,
  FsMutationResponse,
  FsOpenPathResponse,
  FsReadFileResponse,
} from '../../types';
import type ApiClient from '../../../client';

export interface WorkspaceFilesystemFacade {
  listFsDirectories(path?: string, options?: FsListEntriesOptions): Promise<FsEntriesResponse>;
  listFsEntries(path?: string, options?: FsListEntriesOptions): Promise<FsEntriesResponse>;
  searchFsEntries(path: string, query: string, limit?: number): Promise<FsEntriesResponse>;
  searchFsContent(
    path: string,
    query: string,
    options?: { limit?: number; caseSensitive?: boolean; wholeWord?: boolean },
  ): Promise<FsContentSearchResponse>;
  readFsFile(path: string): Promise<FsReadFileResponse>;
  createFsDirectory(parentPath: string, name: string): Promise<FsMutationResponse>;
  createFsFile(parentPath: string, name: string, content?: string): Promise<FsMutationResponse>;
  writeFsFile(path: string, content: string): Promise<FsMutationResponse>;
  deleteFsEntry(path: string, recursive?: boolean): Promise<FsMutationResponse>;
  moveFsEntry(sourcePath: string, targetParentPath: string, options?: FsMoveOptions): Promise<FsMoveResponse>;
  appendFsGitignore(path: string, mode: 'file' | 'folder' | 'extension'): Promise<FsAppendGitignoreResponse>;
  openFsPathExternally(path: string, mode: 'default' | 'reveal' | 'code'): Promise<FsOpenPathResponse>;
  discardFsGitChanges(path: string): Promise<FsDiscardGitChangesResponse>;
  downloadFsEntry(path: string): Promise<{ blob: Blob; filename: string; contentType: string }>;
}

export const workspaceFilesystemFacade: WorkspaceFilesystemFacade & ThisType<ApiClient> = {
  async listFsDirectories(path, options) {
    return workspaceApi.listFsDirectories(this.getRequestFn(), path, options);
  },
  async listFsEntries(path, options) {
    return workspaceApi.listFsEntries(this.getRequestFn(), path, options);
  },
  async searchFsEntries(path, query, limit) {
    return workspaceApi.searchFsEntries(this.getRequestFn(), path, query, limit);
  },
  async searchFsContent(path, query, options) {
    return workspaceApi.searchFsContent(this.getRequestFn(), path, query, options);
  },
  async readFsFile(path) {
    return workspaceApi.readFsFile(this.getRequestFn(), path);
  },
  async createFsDirectory(parentPath, name) {
    return workspaceApi.createFsDirectory(this.getRequestFn(), parentPath, name);
  },
  async createFsFile(parentPath, name, content = '') {
    return workspaceApi.createFsFile(this.getRequestFn(), parentPath, name, content);
  },
  async writeFsFile(path, content) {
    return workspaceApi.writeFsFile(this.getRequestFn(), path, content);
  },
  async deleteFsEntry(path, recursive = false) {
    return workspaceApi.deleteFsEntry(this.getRequestFn(), path, recursive);
  },
  async moveFsEntry(sourcePath, targetParentPath, options) {
    return workspaceApi.moveFsEntry(this.getRequestFn(), sourcePath, targetParentPath, options);
  },
  async appendFsGitignore(path, mode) {
    return workspaceApi.appendFsGitignore(this.getRequestFn(), path, mode);
  },
  async openFsPathExternally(path, mode) {
    return workspaceApi.openFsPathExternally(this.getRequestFn(), path, mode);
  },
  async discardFsGitChanges(path) {
    return workspaceApi.discardFsGitChanges(this.getRequestFn(), path);
  },
  async downloadFsEntry(path) {
    return fsApi.downloadFsEntry(this.getBinaryApiContext(), path);
  },
};
