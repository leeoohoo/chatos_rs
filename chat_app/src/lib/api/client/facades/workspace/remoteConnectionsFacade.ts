import * as workspaceApi from '../../workspace';
import type {
  DeleteSuccessResponse,
  FsMutationResponse,
  RemoteConnectionDraftPayload,
  RemoteConnectionResponse,
  RemoteConnectionTestResponse,
  RemoteConnectionUpdatePayload,
  RemoteSftpEntriesResponse,
  RemoteSftpTransferStatusResponse,
  SftpTransferStartPayload,
} from '../../types';
import type ApiClient from '../../../client';

export interface WorkspaceRemoteConnectionFacade {
  listRemoteConnections(userId?: string): Promise<RemoteConnectionResponse[]>;
  createRemoteConnection(data: RemoteConnectionDraftPayload): Promise<RemoteConnectionResponse>;
  getRemoteConnection(id: string): Promise<RemoteConnectionResponse>;
  updateRemoteConnection(id: string, data: RemoteConnectionUpdatePayload): Promise<RemoteConnectionResponse>;
  deleteRemoteConnection(id: string): Promise<DeleteSuccessResponse>;
  disconnectRemoteTerminal(id: string): Promise<DeleteSuccessResponse>;
  testRemoteConnectionDraft(
    data: RemoteConnectionDraftPayload,
    verificationCode?: string,
  ): Promise<RemoteConnectionTestResponse>;
  testRemoteConnection(id: string, verificationCode?: string): Promise<RemoteConnectionTestResponse>;
  listRemoteSftpEntries(
    connectionId: string,
    path?: string,
    verificationCode?: string,
  ): Promise<RemoteSftpEntriesResponse>;
  uploadRemoteSftpFile(
    connectionId: string,
    localPath: string,
    remotePath: string,
    verificationCode?: string,
  ): Promise<RemoteSftpTransferStatusResponse>;
  downloadRemoteSftpFile(
    connectionId: string,
    remotePath: string,
    localPath: string,
    verificationCode?: string,
  ): Promise<RemoteSftpTransferStatusResponse>;
  startRemoteSftpTransfer(
    connectionId: string,
    data: SftpTransferStartPayload,
    verificationCode?: string,
  ): Promise<RemoteSftpTransferStatusResponse>;
  getRemoteSftpTransferStatus(
    connectionId: string,
    transferId: string,
  ): Promise<RemoteSftpTransferStatusResponse>;
  cancelRemoteSftpTransfer(
    connectionId: string,
    transferId: string,
  ): Promise<RemoteSftpTransferStatusResponse>;
  createRemoteSftpDirectory(
    connectionId: string,
    parentPath: string,
    name: string,
    verificationCode?: string,
  ): Promise<FsMutationResponse>;
  renameRemoteSftpEntry(
    connectionId: string,
    fromPath: string,
    toPath: string,
    verificationCode?: string,
  ): Promise<FsMutationResponse>;
  deleteRemoteSftpEntry(
    connectionId: string,
    path: string,
    recursive?: boolean,
    verificationCode?: string,
  ): Promise<FsMutationResponse>;
}

export const workspaceRemoteConnectionFacade: WorkspaceRemoteConnectionFacade & ThisType<ApiClient> = {
  async listRemoteConnections(userId) {
    return workspaceApi.listRemoteConnections(this.getRequestFn(), userId);
  },
  async createRemoteConnection(data) {
    return workspaceApi.createRemoteConnection(this.getRequestFn(), data);
  },
  async getRemoteConnection(id) {
    return workspaceApi.getRemoteConnection(this.getRequestFn(), id);
  },
  async updateRemoteConnection(id, data) {
    return workspaceApi.updateRemoteConnection(this.getRequestFn(), id, data);
  },
  async deleteRemoteConnection(id) {
    return workspaceApi.deleteRemoteConnection(this.getRequestFn(), id);
  },
  async disconnectRemoteTerminal(id) {
    return workspaceApi.disconnectRemoteTerminal(this.getRequestFn(), id);
  },
  async testRemoteConnectionDraft(data, verificationCode) {
    return workspaceApi.testRemoteConnectionDraft(this.getRequestFn(), data, verificationCode);
  },
  async testRemoteConnection(id, verificationCode) {
    return workspaceApi.testRemoteConnection(this.getRequestFn(), id, verificationCode);
  },
  async listRemoteSftpEntries(connectionId, path, verificationCode) {
    return workspaceApi.listRemoteSftpEntries(
      this.getRequestFn(),
      connectionId,
      path,
      verificationCode,
    );
  },
  async uploadRemoteSftpFile(connectionId, localPath, remotePath, verificationCode) {
    return workspaceApi.uploadRemoteSftpFile(
      this.getRequestFn(),
      connectionId,
      localPath,
      remotePath,
      verificationCode,
    );
  },
  async downloadRemoteSftpFile(connectionId, remotePath, localPath, verificationCode) {
    return workspaceApi.downloadRemoteSftpFile(
      this.getRequestFn(),
      connectionId,
      remotePath,
      localPath,
      verificationCode,
    );
  },
  async startRemoteSftpTransfer(connectionId, data, verificationCode) {
    return workspaceApi.startRemoteSftpTransfer(
      this.getRequestFn(),
      connectionId,
      data,
      verificationCode,
    );
  },
  async getRemoteSftpTransferStatus(connectionId, transferId) {
    return workspaceApi.getRemoteSftpTransferStatus(this.getRequestFn(), connectionId, transferId);
  },
  async cancelRemoteSftpTransfer(connectionId, transferId) {
    return workspaceApi.cancelRemoteSftpTransfer(this.getRequestFn(), connectionId, transferId);
  },
  async createRemoteSftpDirectory(connectionId, parentPath, name, verificationCode) {
    return workspaceApi.createRemoteSftpDirectory(
      this.getRequestFn(),
      connectionId,
      parentPath,
      name,
      verificationCode,
    );
  },
  async renameRemoteSftpEntry(connectionId, fromPath, toPath, verificationCode) {
    return workspaceApi.renameRemoteSftpEntry(
      this.getRequestFn(),
      connectionId,
      fromPath,
      toPath,
      verificationCode,
    );
  },
  async deleteRemoteSftpEntry(connectionId, path, recursive = false, verificationCode) {
    return workspaceApi.deleteRemoteSftpEntry(
      this.getRequestFn(),
      connectionId,
      path,
      recursive,
      verificationCode,
    );
  },
};
