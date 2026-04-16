import * as fsApi from '../fs';
import * as messagesApi from '../messages';
import * as workspaceApi from '../workspace';
import type {
  ContactAgentRecallResponse,
  ContactCreateResponse,
  ContactProjectLinkResponse,
  ContactProjectMemoryResponse,
  ContactResponse,
  DeleteSuccessResponse,
  FsEntriesResponse,
  FsMoveOptions,
  FsMutationResponse,
  FsReadFileResponse,
  MessageCreatePayload,
  PagingOptions,
  ProjectChangeLogResponse,
  ProjectChangeSummaryResponse,
  ProjectContactLinkResponse,
  ProjectResponse,
  ProjectRunCatalogResponse,
  ProjectRunExecuteResponse,
  RemoteConnectionDraftPayload,
  RemoteConnectionResponse,
  RemoteConnectionTestResponse,
  RemoteConnectionUpdatePayload,
  RemoteSftpEntriesResponse,
  RemoteSftpTransferStatusResponse,
  SessionMessageResponse,
  SessionPagingOptions,
  SessionResponse,
  SessionUpdatePayload,
  SessionUpsertPayload,
  SftpTransferStartPayload,
  TerminalDispatchResponse,
  TerminalLogResponse,
  TerminalResponse,
  TurnRuntimeSnapshotLookupResponse,
} from '../types';
import type ApiClient from '../../client';

export interface WorkspaceFacade {
  getSessions(
    userId?: string,
    projectId?: string,
    paging?: SessionPagingOptions,
  ): Promise<SessionResponse[]>;
  createSession(data: SessionUpsertPayload): Promise<SessionResponse>;
  getSession(id: string): Promise<SessionResponse>;
  updateSession(id: string, data: SessionUpdatePayload): Promise<SessionResponse>;
  deleteSession(id: string): Promise<DeleteSuccessResponse>;
  getContacts(userId?: string, paging?: PagingOptions): Promise<ContactResponse[]>;
  createContact(data: { agent_id: string; agent_name_snapshot?: string; user_id?: string }): Promise<ContactCreateResponse>;
  deleteContact(contactId: string): Promise<DeleteSuccessResponse>;
  getContactProjectMemories(
    contactId: string,
    projectId: string,
    paging?: PagingOptions,
  ): Promise<ContactProjectMemoryResponse[]>;
  getContactProjects(contactId: string, paging?: PagingOptions): Promise<ContactProjectLinkResponse[]>;
  getContactAgentRecalls(contactId: string, paging?: PagingOptions): Promise<ContactAgentRecallResponse[]>;
  getConversationMessages(
    conversationId: string,
    params?: { limit?: number; offset?: number; compact?: boolean; strategy?: string },
  ): Promise<SessionMessageResponse[]>;
  getConversationTurnProcessMessages(conversationId: string, userMessageId: string): Promise<SessionMessageResponse[]>;
  getConversationTurnProcessMessagesByTurn(conversationId: string, turnId: string): Promise<SessionMessageResponse[]>;
  getConversationLatestTurnRuntimeContext(conversationId: string): Promise<TurnRuntimeSnapshotLookupResponse>;
  getConversationTurnRuntimeContextByTurn(
    conversationId: string,
    turnId: string,
  ): Promise<TurnRuntimeSnapshotLookupResponse>;
  listProjects(userId?: string): Promise<ProjectResponse[]>;
  createProject(data: { name: string; root_path: string; description?: string; user_id?: string }): Promise<ProjectResponse>;
  updateProject(id: string, data: { name?: string; root_path?: string; description?: string }): Promise<ProjectResponse>;
  deleteProject(id: string): Promise<DeleteSuccessResponse>;
  getProject(id: string): Promise<ProjectResponse>;
  analyzeProjectRun(projectId: string): Promise<ProjectRunCatalogResponse>;
  getProjectRunCatalog(projectId: string): Promise<ProjectRunCatalogResponse>;
  executeProjectRun(
    projectId: string,
    data: { target_id?: string; cwd?: string; command?: string; create_if_missing?: boolean },
  ): Promise<ProjectRunExecuteResponse>;
  setProjectRunDefault(projectId: string, targetId: string): Promise<ProjectRunCatalogResponse>;
  listProjectContacts(projectId: string, paging?: PagingOptions): Promise<ProjectContactLinkResponse[]>;
  addProjectContact(projectId: string, data: { contact_id: string }): Promise<ProjectContactLinkResponse>;
  removeProjectContact(projectId: string, contactId: string): Promise<DeleteSuccessResponse>;
  listProjectChangeLogs(
    projectId: string,
    params?: { path?: string; limit?: number; offset?: number },
  ): Promise<ProjectChangeLogResponse[]>;
  getProjectChangeSummary(projectId: string): Promise<ProjectChangeSummaryResponse>;
  confirmProjectChanges(
    projectId: string,
    payload: { mode?: 'all' | 'paths' | 'change_ids'; paths?: string[]; change_ids?: string[] },
  ): Promise<DeleteSuccessResponse>;
  listTerminals(userId?: string): Promise<TerminalResponse[]>;
  createTerminal(data: { name?: string; cwd: string; user_id?: string }): Promise<TerminalResponse>;
  dispatchTerminalCommand(data: {
    cwd: string;
    command: string;
    user_id?: string;
    project_id?: string;
    create_if_missing?: boolean;
  }): Promise<TerminalDispatchResponse>;
  getTerminal(id: string): Promise<TerminalResponse>;
  interruptTerminal(id: string, data?: { reason?: string }): Promise<TerminalDispatchResponse>;
  deleteTerminal(id: string): Promise<DeleteSuccessResponse>;
  listTerminalLogs(
    terminalId: string,
    params?: { limit?: number; offset?: number; before?: string },
  ): Promise<TerminalLogResponse[]>;
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
  listFsDirectories(path?: string): Promise<FsEntriesResponse>;
  listFsEntries(path?: string): Promise<FsEntriesResponse>;
  searchFsEntries(path: string, query: string, limit?: number): Promise<FsEntriesResponse>;
  readFsFile(path: string): Promise<FsReadFileResponse>;
  createFsDirectory(parentPath: string, name: string): Promise<FsMutationResponse>;
  createFsFile(parentPath: string, name: string, content?: string): Promise<FsMutationResponse>;
  deleteFsEntry(path: string, recursive?: boolean): Promise<FsMutationResponse>;
  moveFsEntry(sourcePath: string, targetParentPath: string, options?: FsMoveOptions): Promise<FsMutationResponse>;
  downloadFsEntry(path: string): Promise<{ blob: Blob; filename: string; contentType: string }>;
  createMessage(data: MessageCreatePayload): Promise<SessionMessageResponse>;
}

export const workspaceFacade: WorkspaceFacade & ThisType<ApiClient> = {
  async getSessions(userId, projectId, paging) {
    return workspaceApi.getSessions(this.getRequestFn(), userId, projectId, paging);
  },
  async createSession(data) {
    return workspaceApi.createSession(this.getRequestFn(), data);
  },
  async getSession(id) {
    return workspaceApi.getSession(this.getRequestFn(), id);
  },
  async updateSession(id, data) {
    return workspaceApi.updateSession(this.getRequestFn(), id, data);
  },
  async deleteSession(id) {
    return workspaceApi.deleteSession(this.getRequestFn(), id);
  },
  async getContacts(userId, paging) {
    return workspaceApi.getContacts(this.getRequestFn(), userId, paging);
  },
  async createContact(data) {
    return workspaceApi.createContact(this.getRequestFn(), data);
  },
  async deleteContact(contactId) {
    return workspaceApi.deleteContact(this.getRequestFn(), contactId);
  },
  async getContactProjectMemories(contactId, projectId, paging) {
    return workspaceApi.getContactProjectMemories(this.getRequestFn(), contactId, projectId, paging);
  },
  async getContactProjects(contactId, paging) {
    return workspaceApi.getContactProjects(this.getRequestFn(), contactId, paging);
  },
  async getContactAgentRecalls(contactId, paging) {
    return workspaceApi.getContactAgentRecalls(this.getRequestFn(), contactId, paging);
  },
  async getConversationMessages(conversationId, params) {
    return workspaceApi.getConversationMessages(this.getRequestFn(), conversationId, params);
  },
  async getConversationTurnProcessMessages(conversationId, userMessageId) {
    return workspaceApi.getConversationTurnProcessMessages(this.getRequestFn(), conversationId, userMessageId);
  },
  async getConversationTurnProcessMessagesByTurn(conversationId, turnId) {
    return workspaceApi.getConversationTurnProcessMessagesByTurn(this.getRequestFn(), conversationId, turnId);
  },
  async getConversationLatestTurnRuntimeContext(conversationId) {
    return workspaceApi.getConversationLatestTurnRuntimeContext(this.getRequestFn(), conversationId);
  },
  async getConversationTurnRuntimeContextByTurn(conversationId, turnId) {
    return workspaceApi.getConversationTurnRuntimeContextByTurn(this.getRequestFn(), conversationId, turnId);
  },
  async listProjects(userId) {
    return workspaceApi.listProjects(this.getRequestFn(), userId);
  },
  async createProject(data) {
    return workspaceApi.createProject(this.getRequestFn(), data);
  },
  async updateProject(id, data) {
    return workspaceApi.updateProject(this.getRequestFn(), id, data);
  },
  async deleteProject(id) {
    return workspaceApi.deleteProject(this.getRequestFn(), id);
  },
  async getProject(id) {
    return workspaceApi.getProject(this.getRequestFn(), id);
  },
  async analyzeProjectRun(projectId) {
    return workspaceApi.analyzeProjectRun(this.getRequestFn(), projectId);
  },
  async getProjectRunCatalog(projectId) {
    return workspaceApi.getProjectRunCatalog(this.getRequestFn(), projectId);
  },
  async executeProjectRun(projectId, data) {
    return workspaceApi.executeProjectRun(this.getRequestFn(), projectId, data);
  },
  async setProjectRunDefault(projectId, targetId) {
    return workspaceApi.setProjectRunDefault(this.getRequestFn(), projectId, targetId);
  },
  async listProjectContacts(projectId, paging) {
    return workspaceApi.listProjectContacts(this.getRequestFn(), projectId, paging);
  },
  async addProjectContact(projectId, data) {
    return workspaceApi.addProjectContact(this.getRequestFn(), projectId, data);
  },
  async removeProjectContact(projectId, contactId) {
    return workspaceApi.removeProjectContact(this.getRequestFn(), projectId, contactId);
  },
  async listProjectChangeLogs(projectId, params) {
    return workspaceApi.listProjectChangeLogs(this.getRequestFn(), projectId, params);
  },
  async getProjectChangeSummary(projectId) {
    return workspaceApi.getProjectChangeSummary(this.getRequestFn(), projectId);
  },
  async confirmProjectChanges(projectId, payload) {
    return workspaceApi.confirmProjectChanges(this.getRequestFn(), projectId, payload);
  },
  async listTerminals(userId) {
    return workspaceApi.listTerminals(this.getRequestFn(), userId);
  },
  async createTerminal(data) {
    return workspaceApi.createTerminal(this.getRequestFn(), data);
  },
  async dispatchTerminalCommand(data) {
    return workspaceApi.dispatchTerminalCommand(this.getRequestFn(), data);
  },
  async getTerminal(id) {
    return workspaceApi.getTerminal(this.getRequestFn(), id);
  },
  async interruptTerminal(id, data) {
    return workspaceApi.interruptTerminal(this.getRequestFn(), id, data);
  },
  async deleteTerminal(id) {
    return workspaceApi.deleteTerminal(this.getRequestFn(), id);
  },
  async listTerminalLogs(terminalId, params) {
    return workspaceApi.listTerminalLogs(this.getRequestFn(), terminalId, params);
  },
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
  async listFsDirectories(path) {
    return workspaceApi.listFsDirectories(this.getRequestFn(), path);
  },
  async listFsEntries(path) {
    return workspaceApi.listFsEntries(this.getRequestFn(), path);
  },
  async searchFsEntries(path, query, limit) {
    return workspaceApi.searchFsEntries(this.getRequestFn(), path, query, limit);
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
  async createMessage(data) {
    return messagesApi.createMessage(this.getRequestFn(), data);
  },
};
