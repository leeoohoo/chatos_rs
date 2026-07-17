// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type {
  CreateLocalConnectorDirectoryRequest,
  CreateLocalConnectorDirectoryResponse,
  LocalConnectorDeviceResponse,
  LocalConnectorDirectoryListResponse,
  LocalConnectorWorkspaceResponse,
} from '../client/types';
import {
  createLocalRuntimeDirectory,
  listLocalRuntimeDevices,
  listLocalRuntimeDirectory,
  listLocalRuntimeWorkspaces,
} from './connectorResources';
import {
  appendLocalFsGitignore,
  createLocalFsDirectory,
  createLocalFsFile,
  deleteLocalFsEntry,
  discardLocalFsGitChanges,
  downloadLocalFsEntry,
  listLocalFsEntries,
  moveLocalFsEntry,
  openLocalFsPath,
  readLocalFsFile,
  searchLocalFsContent,
  searchLocalFsEntries,
  writeLocalFsFile,
} from './filesystem';
import {
  checkoutLocalGit,
  commitLocalGit,
  compareLocalGitBranch,
  createLocalGitBranch,
  discardLocalGitPaths,
  fetchLocalGit,
  getLocalGitBranches,
  getLocalGitClientInfo,
  getLocalGitDiff,
  getLocalGitStatus,
  getLocalGitSummary,
  mergeLocalGit,
  pullLocalGit,
  pushLocalGit,
  stageLocalGitPaths,
  unstageLocalGitPaths,
} from './git';

export class LocalRuntimeResourceClient {
  getGitClientInfo() {
    return getLocalGitClientInfo();
  }

  getGitSummary(root: string) {
    return getLocalGitSummary(root);
  }

  getGitBranches(root: string) {
    return getLocalGitBranches(root);
  }

  getGitStatus(root: string) {
    return getLocalGitStatus(root);
  }

  compareGitBranch(root: string, target: string) {
    return compareLocalGitBranch(root, target);
  }

  getGitDiff(data: { root: string; path: string; target?: string; staged?: boolean }) {
    return getLocalGitDiff(data);
  }

  fetchGit(data: { root: string; remote?: string }) {
    return fetchLocalGit(data);
  }

  pullGit(data: { root: string; mode?: string }) {
    return pullLocalGit(data);
  }

  pushGit(data: { root: string; remote?: string; branch?: string; setUpstream?: boolean }) {
    return pushLocalGit(data);
  }

  checkoutGit(data: {
    root: string;
    branch?: string;
    remoteBranch?: string;
    createTracking?: boolean;
  }) {
    return checkoutLocalGit(data);
  }

  createGitBranch(data: { root: string; name: string; startPoint?: string; checkout?: boolean }) {
    return createLocalGitBranch(data);
  }

  mergeGit(data: { root: string; branch: string; mode?: string }) {
    return mergeLocalGit(data);
  }

  stageGitPaths(data: { root: string; paths: string[] }) {
    return stageLocalGitPaths(data);
  }

  unstageGitPaths(data: { root: string; paths: string[] }) {
    return unstageLocalGitPaths(data);
  }

  discardGitPaths(data: { root: string; paths: string[] }) {
    return discardLocalGitPaths(data);
  }

  commitGit(data: { root: string; message: string; paths?: string[] }) {
    return commitLocalGit(data);
  }

  listFsEntries(path: string) {
    return listLocalFsEntries(path);
  }

  searchFsEntries(path: string, query: string, limit?: number) {
    return searchLocalFsEntries(path, query, limit);
  }

  searchFsContent(
    path: string,
    query: string,
    options?: { limit?: number; caseSensitive?: boolean; wholeWord?: boolean },
  ) {
    return searchLocalFsContent(path, query, options);
  }

  readFsFile(path: string) {
    return readLocalFsFile(path);
  }

  createFsDirectory(parentPath: string, name: string) {
    return createLocalFsDirectory(parentPath, name);
  }

  createFsFile(parentPath: string, name: string, content: string) {
    return createLocalFsFile(parentPath, name, content);
  }

  writeFsFile(path: string, content: string) {
    return writeLocalFsFile(path, content);
  }

  deleteFsEntry(path: string, recursive: boolean) {
    return deleteLocalFsEntry(path, recursive);
  }

  moveFsEntry(
    sourcePath: string,
    targetParentPath: string,
    options?: import('../client/types').FsMoveOptions,
  ) {
    return moveLocalFsEntry(sourcePath, targetParentPath, options);
  }

  appendFsGitignore(path: string, mode: 'file' | 'folder' | 'extension') {
    return appendLocalFsGitignore(path, mode);
  }

  openFsPath(path: string, mode: 'default' | 'reveal' | 'code') {
    return openLocalFsPath(path, mode);
  }

  discardFsGitChanges(path: string) {
    return discardLocalFsGitChanges(path);
  }

  downloadFsEntry(path: string) {
    return downloadLocalFsEntry(path);
  }

  async listConnectorDevices(): Promise<LocalConnectorDeviceResponse[]> {
    return listLocalRuntimeDevices();
  }

  async listConnectorWorkspaces(): Promise<LocalConnectorWorkspaceResponse[]> {
    return listLocalRuntimeWorkspaces();
  }

  async listConnectorDirectory(
    workspaceId: string,
    path?: string,
  ): Promise<LocalConnectorDirectoryListResponse> {
    return listLocalRuntimeDirectory(workspaceId, path);
  }

  async createConnectorDirectory(
    data: CreateLocalConnectorDirectoryRequest,
  ): Promise<CreateLocalConnectorDirectoryResponse> {
    return createLocalRuntimeDirectory(data);
  }
}
