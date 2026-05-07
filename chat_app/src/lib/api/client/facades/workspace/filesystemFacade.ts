import * as fsApi from '../../fs';
import * as workspaceApi from '../../workspace';
import type {
  FsContentSearchResponse,
  FsEntriesResponse,
  FsMoveOptions,
  FsMoveResponse,
  FsMutationResponse,
  FsReadFileResponse,
} from '../../types';
import type ApiClient from '../../../client';

export interface WorkspaceFilesystemFacade {
  listFsDirectories(path?: string): Promise<FsEntriesResponse>;
  listFsEntries(path?: string): Promise<FsEntriesResponse>;
  searchFsEntries(path: string, query: string, limit?: number): Promise<FsEntriesResponse>;
  searchFsContent(
    path: string,
    query: string,
    options?: { limit?: number; caseSensitive?: boolean; wholeWord?: boolean },
  ): Promise<FsContentSearchResponse>;
  readFsFile(path: string): Promise<FsReadFileResponse>;
  createFsDirectory(parentPath: string, name: string): Promise<FsMutationResponse>;
  createFsFile(parentPath: string, name: string, content?: string): Promise<FsMutationResponse>;
  deleteFsEntry(path: string, recursive?: boolean): Promise<FsMutationResponse>;
  moveFsEntry(sourcePath: string, targetParentPath: string, options?: FsMoveOptions): Promise<FsMoveResponse>;
  downloadFsEntry(path: string): Promise<{ blob: Blob; filename: string; contentType: string }>;
}

export const workspaceFilesystemFacade: WorkspaceFilesystemFacade & ThisType<ApiClient> = {
  async listFsDirectories(path) {
    return workspaceApi.listFsDirectories(this.getRequestFn(), path);
  },
  async listFsEntries(path) {
    return workspaceApi.listFsEntries(this.getRequestFn(), path);
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
  async deleteFsEntry(path, recursive = false) {
    return workspaceApi.deleteFsEntry(this.getRequestFn(), path, recursive);
  },
  async moveFsEntry(sourcePath, targetParentPath, options) {
    return workspaceApi.moveFsEntry(this.getRequestFn(), sourcePath, targetParentPath, options);
  },
  async downloadFsEntry(path) {
    return fsApi.downloadFsEntry(this.getBinaryApiContext(), path);
  },
};
