import { debugLog } from '@/lib/utils';

import { buildQuery } from './shared';
import type {
  ContactAgentRecallResponse,
  ContactBuiltinMcpGrantsResponse,
  ContactCreateResponse,
  ContactProjectLinkResponse,
  ContactProjectMemoryResponse,
  ContactResponse,
  DeleteSuccessResponse,
  FsEntriesResponse,
  FsMutationResponse,
  FsReadFileResponse,
  ProjectChangeLogResponse,
  ProjectChangeSummaryResponse,
  ProjectContactLinkResponse,
  ProjectResponse,
  ProjectRunCatalogResponse,
  ProjectRunExecuteResponse,
  RemoteConnectionResponse,
  RemoteConnectionTestResponse,
  RemoteSftpEntriesResponse,
  RemoteSftpTransferStatusResponse,
  SessionMessageResponse,
  SessionResponse,
  TurnRuntimeSnapshotLookupResponse,
  TerminalDispatchResponse,
  TerminalLogResponse,
  TerminalResponse,
} from './types';

export type ApiRequestFn = <T>(endpoint: string, options?: RequestInit) => Promise<T>;

export interface SessionPaging {
  limit?: number;
  offset?: number;
  includeArchived?: boolean;
  includeArchiving?: boolean;
}

export interface ContactPaging {
  limit?: number;
  offset?: number;
}

export interface RemoteConnectionPayload {
  name?: string;
  host: string;
  port?: number;
  username: string;
  auth_type?: 'private_key' | 'private_key_cert' | 'password';
  password?: string;
  private_key_path?: string;
  certificate_path?: string;
  default_remote_path?: string;
  host_key_policy?: 'strict' | 'accept_new';
  jump_enabled?: boolean;
  jump_host?: string;
  jump_port?: number;
  jump_username?: string;
  jump_private_key_path?: string;
  jump_password?: string;
  user_id?: string;
}

export const getSessions = (
  request: ApiRequestFn,
  userId?: string,
  projectId?: string,
  paging?: SessionPaging
): Promise<SessionResponse[]> => {
  const query = buildQuery({
    user_id: userId,
    project_id: projectId,
    limit: paging?.limit,
    offset: paging?.offset,
    include_archived: paging?.includeArchived === true ? true : undefined,
    include_archiving: paging?.includeArchiving === true ? true : undefined,
  });
  debugLog('🔍 getSessions API调用:', { userId, projectId, query });
  return request<SessionResponse[]>(`/sessions${query}`);
};

export const createSession = (
  request: ApiRequestFn,
  data: { id: string; title: string; user_id: string; project_id?: string; metadata?: any }
): Promise<SessionResponse> => {
  debugLog('🔍 createSession API调用:', data);
  return request<SessionResponse>('/sessions', {
    method: 'POST',
    body: JSON.stringify(data),
  });
};

export const getSession = (request: ApiRequestFn, id: string): Promise<SessionResponse> => {
  return request<SessionResponse>(`/sessions/${id}`);
};

export const updateSession = (
  request: ApiRequestFn,
  id: string,
  data: { title?: string; description?: string; metadata?: any },
): Promise<SessionResponse> => {
  return request<SessionResponse>(`/sessions/${id}`, {
    method: 'PUT',
    body: JSON.stringify(data),
  });
};

export const deleteSession = (request: ApiRequestFn, id: string): Promise<DeleteSuccessResponse> => {
  return request<DeleteSuccessResponse>(`/sessions/${id}`, {
    method: 'DELETE',
  });
};

export const getContacts = (
  request: ApiRequestFn,
  userId?: string,
  paging?: ContactPaging,
): Promise<ContactResponse[]> => {
  const query = buildQuery({
    user_id: userId,
    limit: paging?.limit,
    offset: paging?.offset,
  });
  return request<ContactResponse[]>(`/contacts${query}`);
};

export const createContact = (
  request: ApiRequestFn,
  data: { agent_id: string; agent_name_snapshot?: string; user_id?: string },
): Promise<ContactCreateResponse> => {
  return request<ContactCreateResponse>('/contacts', {
    method: 'POST',
    body: JSON.stringify(data),
  });
};

export const getContactBuiltinMcpGrants = (
  request: ApiRequestFn,
  contactId: string,
): Promise<ContactBuiltinMcpGrantsResponse> => {
  return request<ContactBuiltinMcpGrantsResponse>(
    `/contacts/${encodeURIComponent(contactId)}/builtin-mcp-grants`,
  );
};

export const updateContactBuiltinMcpGrants = (
  request: ApiRequestFn,
  contactId: string,
  data: { authorized_builtin_mcp_ids: string[] },
): Promise<ContactBuiltinMcpGrantsResponse> => {
  return request<ContactBuiltinMcpGrantsResponse>(
    `/contacts/${encodeURIComponent(contactId)}/builtin-mcp-grants`,
    {
      method: 'PATCH',
      body: JSON.stringify(data),
    },
  );
};

export const deleteContact = (
  request: ApiRequestFn,
  contactId: string,
): Promise<DeleteSuccessResponse> => {
  return request<DeleteSuccessResponse>(`/contacts/${contactId}`, {
    method: 'DELETE',
  });
};

export const getContactProjectMemories = (
  request: ApiRequestFn,
  contactId: string,
  projectId: string,
  paging?: ContactPaging,
): Promise<ContactProjectMemoryResponse[]> => {
  const query = buildQuery({
    limit: paging?.limit,
    offset: paging?.offset,
  });
  return request<ContactProjectMemoryResponse[]>(
    `/contacts/${encodeURIComponent(contactId)}/project-memories/${encodeURIComponent(projectId)}${query}`,
  );
};

export const getContactProjects = (
  request: ApiRequestFn,
  contactId: string,
  paging?: ContactPaging,
): Promise<ContactProjectLinkResponse[]> => {
  const query = buildQuery({
    limit: paging?.limit,
    offset: paging?.offset,
  });
  return request<ContactProjectLinkResponse[]>(
    `/contacts/${encodeURIComponent(contactId)}/projects${query}`,
  );
};

export const getContactAgentRecalls = (
  request: ApiRequestFn,
  contactId: string,
  paging?: ContactPaging,
): Promise<ContactAgentRecallResponse[]> => {
  const query = buildQuery({
    limit: paging?.limit,
    offset: paging?.offset,
  });
  return request<ContactAgentRecallResponse[]>(
    `/contacts/${encodeURIComponent(contactId)}/agent-recalls${query}`,
  );
};

export const getSessionMessages = (
  request: ApiRequestFn,
  sessionId: string,
  params?: { limit?: number; offset?: number; compact?: boolean; strategy?: string }
): Promise<SessionMessageResponse[]> => {
  const query = buildQuery({
    limit: params?.limit,
    offset: params?.offset,
    compact: params?.compact,
    strategy: params?.strategy,
  });
  return request<SessionMessageResponse[]>(`/sessions/${sessionId}/messages${query}`);
};

export const getSessionTurnProcessMessages = (
  request: ApiRequestFn,
  sessionId: string,
  userMessageId: string
): Promise<SessionMessageResponse[]> => {
  return request<SessionMessageResponse[]>(
    `/sessions/${sessionId}/turns/${encodeURIComponent(userMessageId)}/process`,
  );
};

export const getSessionTurnProcessMessagesByTurn = (
  request: ApiRequestFn,
  sessionId: string,
  turnId: string
): Promise<SessionMessageResponse[]> => {
  return request<SessionMessageResponse[]>(
    `/sessions/${sessionId}/turns/by-turn/${encodeURIComponent(turnId)}/process`,
  );
};

export const getSessionTurnRuntimeContextLatest = (
  request: ApiRequestFn,
  sessionId: string,
): Promise<TurnRuntimeSnapshotLookupResponse> => (
  request<TurnRuntimeSnapshotLookupResponse>(
    `/sessions/${sessionId}/turns/latest/runtime-context`,
  )
);

export const getSessionTurnRuntimeContextByTurn = (
  request: ApiRequestFn,
  sessionId: string,
  turnId: string,
): Promise<TurnRuntimeSnapshotLookupResponse> => (
  request<TurnRuntimeSnapshotLookupResponse>(
    `/sessions/${sessionId}/turns/by-turn/${encodeURIComponent(turnId)}/runtime-context`,
  )
);

export const listProjects = (request: ApiRequestFn, userId?: string): Promise<ProjectResponse[]> => {
  const query = buildQuery({ user_id: userId });
  return request<ProjectResponse[]>(`/projects${query}`);
};

export const createProject = (
  request: ApiRequestFn,
  data: { name: string; root_path: string; description?: string; user_id?: string }
): Promise<ProjectResponse> => {
  return request<ProjectResponse>('/projects', {
    method: 'POST',
    body: JSON.stringify(data),
  });
};

export const updateProject = (
  request: ApiRequestFn,
  id: string,
  data: { name?: string; root_path?: string; description?: string }
): Promise<ProjectResponse> => {
  return request<ProjectResponse>(`/projects/${id}`, {
    method: 'PUT',
    body: JSON.stringify(data),
  });
};

export const deleteProject = (request: ApiRequestFn, id: string): Promise<DeleteSuccessResponse> => {
  return request<DeleteSuccessResponse>(`/projects/${id}`, {
    method: 'DELETE',
  });
};

export const getProject = (request: ApiRequestFn, id: string): Promise<ProjectResponse> => {
  return request<ProjectResponse>(`/projects/${id}`);
};

export const analyzeProjectRun = (
  request: ApiRequestFn,
  projectId: string,
): Promise<ProjectRunCatalogResponse> => {
  return request<ProjectRunCatalogResponse>(`/projects/${encodeURIComponent(projectId)}/run/analyze`, {
    method: 'POST',
  });
};

export const getProjectRunCatalog = (
  request: ApiRequestFn,
  projectId: string,
): Promise<ProjectRunCatalogResponse> => {
  return request<ProjectRunCatalogResponse>(`/projects/${encodeURIComponent(projectId)}/run/catalog`);
};

export const executeProjectRun = (
  request: ApiRequestFn,
  projectId: string,
  data: {
    target_id?: string;
    cwd?: string;
    command?: string;
    create_if_missing?: boolean;
  },
): Promise<ProjectRunExecuteResponse> => {
  return request<ProjectRunExecuteResponse>(`/projects/${encodeURIComponent(projectId)}/run/execute`, {
    method: 'POST',
    body: JSON.stringify(data),
  });
};

export const setProjectRunDefault = (
  request: ApiRequestFn,
  projectId: string,
  targetId: string,
): Promise<ProjectRunCatalogResponse> => {
  return request<ProjectRunCatalogResponse>(`/projects/${encodeURIComponent(projectId)}/run/default`, {
    method: 'POST',
    body: JSON.stringify({ target_id: targetId }),
  });
};

export const listProjectContacts = (
  request: ApiRequestFn,
  projectId: string,
  paging?: ContactPaging,
): Promise<ProjectContactLinkResponse[]> => {
  const query = buildQuery({
    limit: paging?.limit,
    offset: paging?.offset,
  });
  return request<ProjectContactLinkResponse[]>(`/projects/${encodeURIComponent(projectId)}/contacts${query}`);
};

export const addProjectContact = (
  request: ApiRequestFn,
  projectId: string,
  data: { contact_id: string },
): Promise<ProjectContactLinkResponse> => {
  return request<ProjectContactLinkResponse>(`/projects/${encodeURIComponent(projectId)}/contacts`, {
    method: 'POST',
    body: JSON.stringify(data),
  });
};

export const removeProjectContact = (
  request: ApiRequestFn,
  projectId: string,
  contactId: string,
): Promise<DeleteSuccessResponse> => {
  return request<DeleteSuccessResponse>(`/projects/${encodeURIComponent(projectId)}/contacts/${encodeURIComponent(contactId)}`, {
    method: 'DELETE',
  });
};

export const listProjectChangeLogs = (
  request: ApiRequestFn,
  projectId: string,
  params?: { path?: string; limit?: number; offset?: number }
): Promise<ProjectChangeLogResponse[]> => {
  const query = buildQuery({
    path: params?.path,
    limit: params?.limit,
    offset: params?.offset,
  });
  return request<ProjectChangeLogResponse[]>(`/projects/${projectId}/changes${query}`);
};

export const getProjectChangeSummary = (
  request: ApiRequestFn,
  projectId: string,
): Promise<ProjectChangeSummaryResponse> => {
  return request<ProjectChangeSummaryResponse>(`/projects/${projectId}/changes/summary`);
};

export const confirmProjectChanges = (
  request: ApiRequestFn,
  projectId: string,
  payload: { mode?: 'all' | 'paths' | 'change_ids'; paths?: string[]; change_ids?: string[] }
): Promise<DeleteSuccessResponse> => {
  return request<DeleteSuccessResponse>(`/projects/${projectId}/changes/confirm`, {
    method: 'POST',
    body: JSON.stringify(payload),
  });
};

export const listTerminals = (request: ApiRequestFn, userId?: string): Promise<TerminalResponse[]> => {
  const query = buildQuery({ user_id: userId });
  return request<TerminalResponse[]>(`/terminals${query}`);
};

export const createTerminal = (
  request: ApiRequestFn,
  data: { name?: string; cwd: string; user_id?: string }
): Promise<TerminalResponse> => {
  return request<TerminalResponse>('/terminals', {
    method: 'POST',
    body: JSON.stringify(data),
  });
};

export const dispatchTerminalCommand = (
  request: ApiRequestFn,
  data: {
    cwd: string;
    command: string;
    user_id?: string;
    project_id?: string;
    create_if_missing?: boolean;
  }
): Promise<TerminalDispatchResponse> => {
  return request<TerminalDispatchResponse>('/terminals/dispatch-command', {
    method: 'POST',
    body: JSON.stringify(data),
  });
};

export const getTerminal = (request: ApiRequestFn, id: string): Promise<TerminalResponse> => {
  return request<TerminalResponse>(`/terminals/${id}`);
};

export const interruptTerminal = (
  request: ApiRequestFn,
  id: string,
  data?: { reason?: string },
): Promise<TerminalDispatchResponse> => {
  return request<TerminalDispatchResponse>(`/terminals/${encodeURIComponent(id)}/interrupt`, {
    method: 'POST',
    body: JSON.stringify(data || {}),
  });
};

export const deleteTerminal = (request: ApiRequestFn, id: string): Promise<DeleteSuccessResponse> => {
  return request<DeleteSuccessResponse>(`/terminals/${id}`, {
    method: 'DELETE',
  });
};

export const listTerminalLogs = (
  request: ApiRequestFn,
  terminalId: string,
  params?: { limit?: number; offset?: number; before?: string }
): Promise<TerminalLogResponse[]> => {
  const query = buildQuery({
    limit: params?.limit,
    offset: params?.offset,
    before: params?.before,
  });
  return request<TerminalLogResponse[]>(`/terminals/${terminalId}/history${query}`);
};

export const listRemoteConnections = (
  request: ApiRequestFn,
  userId?: string,
): Promise<RemoteConnectionResponse[]> => {
  const query = buildQuery({ user_id: userId });
  return request<RemoteConnectionResponse[]>(`/remote-connections${query}`);
};

export const createRemoteConnection = (
  request: ApiRequestFn,
  data: RemoteConnectionPayload,
): Promise<RemoteConnectionResponse> => {
  return request<RemoteConnectionResponse>('/remote-connections', {
    method: 'POST',
    body: JSON.stringify(data),
  });
};

export const getRemoteConnection = (
  request: ApiRequestFn,
  id: string,
): Promise<RemoteConnectionResponse> => {
  return request<RemoteConnectionResponse>(`/remote-connections/${id}`);
};

export const updateRemoteConnection = (
  request: ApiRequestFn,
  id: string,
  data: Omit<RemoteConnectionPayload, 'host' | 'username'> & { host?: string; username?: string }
): Promise<RemoteConnectionResponse> => {
  return request<RemoteConnectionResponse>(`/remote-connections/${id}`, {
    method: 'PUT',
    body: JSON.stringify(data),
  });
};

export const deleteRemoteConnection = (request: ApiRequestFn, id: string): Promise<DeleteSuccessResponse> => {
  return request<DeleteSuccessResponse>(`/remote-connections/${id}`, {
    method: 'DELETE',
  });
};

export const disconnectRemoteTerminal = (
  request: ApiRequestFn,
  id: string,
): Promise<DeleteSuccessResponse> => {
  return request<DeleteSuccessResponse>(`/remote-connections/${id}/disconnect`, {
    method: 'POST',
  });
};

export const testRemoteConnectionDraft = (
  request: ApiRequestFn,
  data: RemoteConnectionPayload,
): Promise<RemoteConnectionTestResponse> => {
  return request<RemoteConnectionTestResponse>('/remote-connections/test', {
    method: 'POST',
    body: JSON.stringify(data),
  });
};

export const testRemoteConnection = (
  request: ApiRequestFn,
  id: string,
): Promise<RemoteConnectionTestResponse> => {
  return request<RemoteConnectionTestResponse>(`/remote-connections/${id}/test`, {
    method: 'POST',
  });
};

export const listRemoteSftpEntries = (
  request: ApiRequestFn,
  connectionId: string,
  path?: string
): Promise<RemoteSftpEntriesResponse> => {
  return request<RemoteSftpEntriesResponse>(
    `/remote-connections/${connectionId}/sftp/list${buildQuery({ path })}`,
  );
};

export const uploadRemoteSftpFile = (
  request: ApiRequestFn,
  connectionId: string,
  localPath: string,
  remotePath: string
): Promise<RemoteSftpTransferStatusResponse> => {
  return request<RemoteSftpTransferStatusResponse>(`/remote-connections/${connectionId}/sftp/upload`, {
    method: 'POST',
    body: JSON.stringify({
      local_path: localPath,
      remote_path: remotePath,
    }),
  });
};

export const downloadRemoteSftpFile = (
  request: ApiRequestFn,
  connectionId: string,
  remotePath: string,
  localPath: string
): Promise<RemoteSftpTransferStatusResponse> => {
  return request<RemoteSftpTransferStatusResponse>(`/remote-connections/${connectionId}/sftp/download`, {
    method: 'POST',
    body: JSON.stringify({
      remote_path: remotePath,
      local_path: localPath,
    }),
  });
};

export const startRemoteSftpTransfer = (
  request: ApiRequestFn,
  connectionId: string,
  data: {
    direction: 'upload' | 'download';
    local_path: string;
    remote_path: string;
  },
): Promise<RemoteSftpTransferStatusResponse> => {
  return request<RemoteSftpTransferStatusResponse>(`/remote-connections/${connectionId}/sftp/transfer/start`, {
    method: 'POST',
    body: JSON.stringify(data),
  });
};

export const getRemoteSftpTransferStatus = (
  request: ApiRequestFn,
  connectionId: string,
  transferId: string
): Promise<RemoteSftpTransferStatusResponse> => {
  return request<RemoteSftpTransferStatusResponse>(
    `/remote-connections/${connectionId}/sftp/transfer/${encodeURIComponent(transferId)}`,
  );
};

export const cancelRemoteSftpTransfer = (
  request: ApiRequestFn,
  connectionId: string,
  transferId: string
): Promise<RemoteSftpTransferStatusResponse> => {
  return request<RemoteSftpTransferStatusResponse>(
    `/remote-connections/${connectionId}/sftp/transfer/${encodeURIComponent(transferId)}/cancel`,
    {
    method: 'POST',
    },
  );
};

export const createRemoteSftpDirectory = (
  request: ApiRequestFn,
  connectionId: string,
  parentPath: string,
  name: string
): Promise<FsMutationResponse> => {
  return request<FsMutationResponse>(`/remote-connections/${connectionId}/sftp/mkdir`, {
    method: 'POST',
    body: JSON.stringify({
      parent_path: parentPath,
      name,
    }),
  });
};

export const renameRemoteSftpEntry = (
  request: ApiRequestFn,
  connectionId: string,
  fromPath: string,
  toPath: string
): Promise<FsMutationResponse> => {
  return request<FsMutationResponse>(`/remote-connections/${connectionId}/sftp/rename`, {
    method: 'POST',
    body: JSON.stringify({
      from_path: fromPath,
      to_path: toPath,
    }),
  });
};

export const deleteRemoteSftpEntry = (
  request: ApiRequestFn,
  connectionId: string,
  path: string,
  recursive = false
): Promise<FsMutationResponse> => {
  return request<FsMutationResponse>(`/remote-connections/${connectionId}/sftp/delete`, {
    method: 'POST',
    body: JSON.stringify({
      path,
      recursive,
    }),
  });
};

export const listFsDirectories = (request: ApiRequestFn, path?: string): Promise<FsEntriesResponse> => {
  return request<FsEntriesResponse>(`/fs/list${buildQuery({ path })}`);
};

export const listFsEntries = (request: ApiRequestFn, path?: string): Promise<FsEntriesResponse> => {
  return request<FsEntriesResponse>(`/fs/entries${buildQuery({ path })}`);
};

export const searchFsEntries = (
  request: ApiRequestFn,
  path: string,
  query: string,
  limit?: number
): Promise<FsEntriesResponse> => {
  return request<FsEntriesResponse>(`/fs/search${buildQuery({ path, q: query, limit })}`);
};

export const readFsFile = (request: ApiRequestFn, path: string): Promise<FsReadFileResponse> => {
  return request<FsReadFileResponse>(`/fs/read${buildQuery({ path })}`);
};

export const createFsDirectory = (
  request: ApiRequestFn,
  parentPath: string,
  name: string
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
  content = ''
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
  recursive = false
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
  options?: { targetName?: string; replaceExisting?: boolean }
): Promise<FsMutationResponse> => {
  return request<FsMutationResponse>('/fs/move', {
    method: 'POST',
    body: JSON.stringify({
      source_path: sourcePath,
      target_parent_path: targetParentPath,
      target_name: options?.targetName,
      replace_existing: options?.replaceExisting,
    }),
  });
};
