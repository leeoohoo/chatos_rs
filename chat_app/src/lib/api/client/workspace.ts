import { debugLog } from '@/lib/utils';

import { buildQuery } from './shared';

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
): Promise<any[]> => {
  const query = buildQuery({
    user_id: userId,
    project_id: projectId,
    limit: paging?.limit,
    offset: paging?.offset,
    include_archived: paging?.includeArchived === true ? true : undefined,
    include_archiving: paging?.includeArchiving === true ? true : undefined,
  });
  debugLog('🔍 getSessions API调用:', { userId, projectId, query });
  return request<any[]>(`/sessions${query}`);
};

export const createSession = (
  request: ApiRequestFn,
  data: { id: string; title: string; user_id: string; project_id?: string; metadata?: any }
): Promise<any> => {
  debugLog('🔍 createSession API调用:', data);
  return request<any>('/sessions', {
    method: 'POST',
    body: JSON.stringify(data),
  });
};

export const getSession = (request: ApiRequestFn, id: string): Promise<any> => {
  return request<any>(`/sessions/${id}`);
};

export const updateSession = (
  request: ApiRequestFn,
  id: string,
  data: { title?: string; description?: string; metadata?: any },
): Promise<any> => {
  return request<any>(`/sessions/${id}`, {
    method: 'PUT',
    body: JSON.stringify(data),
  });
};

export const deleteSession = (request: ApiRequestFn, id: string): Promise<any> => {
  return request<any>(`/sessions/${id}`, {
    method: 'DELETE',
  });
};

export const getContacts = (
  request: ApiRequestFn,
  userId?: string,
  paging?: ContactPaging,
): Promise<any[]> => {
  const query = buildQuery({
    user_id: userId,
    limit: paging?.limit,
    offset: paging?.offset,
  });
  return request<any[]>(`/contacts${query}`);
};

export const createContact = (
  request: ApiRequestFn,
  data: { agent_id: string; agent_name_snapshot?: string; user_id?: string },
): Promise<any> => {
  return request<any>('/contacts', {
    method: 'POST',
    body: JSON.stringify(data),
  });
};

export const deleteContact = (request: ApiRequestFn, contactId: string): Promise<any> => {
  return request<any>(`/contacts/${contactId}`, {
    method: 'DELETE',
  });
};

export const getContactProjectMemories = (
  request: ApiRequestFn,
  contactId: string,
  projectId: string,
  paging?: ContactPaging,
): Promise<any[]> => {
  const query = buildQuery({
    limit: paging?.limit,
    offset: paging?.offset,
  });
  return request<any[]>(
    `/contacts/${encodeURIComponent(contactId)}/project-memories/${encodeURIComponent(projectId)}${query}`,
  );
};

export const getContactProjects = (
  request: ApiRequestFn,
  contactId: string,
  paging?: ContactPaging,
): Promise<any[]> => {
  const query = buildQuery({
    limit: paging?.limit,
    offset: paging?.offset,
  });
  return request<any[]>(
    `/contacts/${encodeURIComponent(contactId)}/projects${query}`,
  );
};

export const getContactAgentRecalls = (
  request: ApiRequestFn,
  contactId: string,
  paging?: ContactPaging,
): Promise<any[]> => {
  const query = buildQuery({
    limit: paging?.limit,
    offset: paging?.offset,
  });
  return request<any[]>(
    `/contacts/${encodeURIComponent(contactId)}/agent-recalls${query}`,
  );
};

export const getSessionMessages = (
  request: ApiRequestFn,
  sessionId: string,
  params?: { limit?: number; offset?: number; compact?: boolean; strategy?: string }
): Promise<any[]> => {
  const query = buildQuery({
    limit: params?.limit,
    offset: params?.offset,
    compact: params?.compact,
    strategy: params?.strategy,
  });
  return request<any[]>(`/sessions/${sessionId}/messages${query}`);
};

export const getSessionTurnProcessMessages = (
  request: ApiRequestFn,
  sessionId: string,
  userMessageId: string
): Promise<any[]> => {
  return request<any[]>(`/sessions/${sessionId}/turns/${encodeURIComponent(userMessageId)}/process`);
};

export const getSessionTurnProcessMessagesByTurn = (
  request: ApiRequestFn,
  sessionId: string,
  turnId: string
): Promise<any[]> => {
  return request<any[]>(`/sessions/${sessionId}/turns/by-turn/${encodeURIComponent(turnId)}/process`);
};

export const listProjects = (request: ApiRequestFn, userId?: string): Promise<any[]> => {
  const query = buildQuery({ user_id: userId });
  return request<any[]>(`/projects${query}`);
};

export const createProject = (
  request: ApiRequestFn,
  data: { name: string; root_path: string; description?: string; user_id?: string }
): Promise<any> => {
  return request<any>('/projects', {
    method: 'POST',
    body: JSON.stringify(data),
  });
};

export const updateProject = (
  request: ApiRequestFn,
  id: string,
  data: { name?: string; root_path?: string; description?: string }
): Promise<any> => {
  return request<any>(`/projects/${id}`, {
    method: 'PUT',
    body: JSON.stringify(data),
  });
};

export const deleteProject = (request: ApiRequestFn, id: string): Promise<any> => {
  return request<any>(`/projects/${id}`, {
    method: 'DELETE',
  });
};

export const getProject = (request: ApiRequestFn, id: string): Promise<any> => {
  return request<any>(`/projects/${id}`);
};

export const listProjectContacts = (
  request: ApiRequestFn,
  projectId: string,
  paging?: ContactPaging,
): Promise<any[]> => {
  const query = buildQuery({
    limit: paging?.limit,
    offset: paging?.offset,
  });
  return request<any[]>(`/projects/${encodeURIComponent(projectId)}/contacts${query}`);
};

export const addProjectContact = (
  request: ApiRequestFn,
  projectId: string,
  data: { contact_id: string },
): Promise<any> => {
  return request<any>(`/projects/${encodeURIComponent(projectId)}/contacts`, {
    method: 'POST',
    body: JSON.stringify(data),
  });
};

export const removeProjectContact = (
  request: ApiRequestFn,
  projectId: string,
  contactId: string,
): Promise<any> => {
  return request<any>(`/projects/${encodeURIComponent(projectId)}/contacts/${encodeURIComponent(contactId)}`, {
    method: 'DELETE',
  });
};

export const listProjectChangeLogs = (
  request: ApiRequestFn,
  projectId: string,
  params?: { path?: string; limit?: number; offset?: number }
): Promise<any[]> => {
  const query = buildQuery({
    path: params?.path,
    limit: params?.limit,
    offset: params?.offset,
  });
  return request<any[]>(`/projects/${projectId}/changes${query}`);
};

export const getProjectChangeSummary = (request: ApiRequestFn, projectId: string): Promise<any> => {
  return request<any>(`/projects/${projectId}/changes/summary`);
};

export const confirmProjectChanges = (
  request: ApiRequestFn,
  projectId: string,
  payload: { mode?: 'all' | 'paths' | 'change_ids'; paths?: string[]; change_ids?: string[] }
): Promise<any> => {
  return request<any>(`/projects/${projectId}/changes/confirm`, {
    method: 'POST',
    body: JSON.stringify(payload),
  });
};

export const listTerminals = (request: ApiRequestFn, userId?: string): Promise<any[]> => {
  const query = buildQuery({ user_id: userId });
  return request<any[]>(`/terminals${query}`);
};

export const createTerminal = (
  request: ApiRequestFn,
  data: { name?: string; cwd: string; user_id?: string }
): Promise<any> => {
  return request<any>('/terminals', {
    method: 'POST',
    body: JSON.stringify(data),
  });
};

export const getTerminal = (request: ApiRequestFn, id: string): Promise<any> => {
  return request<any>(`/terminals/${id}`);
};

export const deleteTerminal = (request: ApiRequestFn, id: string): Promise<any> => {
  return request<any>(`/terminals/${id}`, {
    method: 'DELETE',
  });
};

export const listTerminalLogs = (
  request: ApiRequestFn,
  terminalId: string,
  params?: { limit?: number; offset?: number; before?: string }
): Promise<any[]> => {
  const query = buildQuery({
    limit: params?.limit,
    offset: params?.offset,
    before: params?.before,
  });
  return request<any[]>(`/terminals/${terminalId}/history${query}`);
};

export const listRemoteConnections = (request: ApiRequestFn, userId?: string): Promise<any[]> => {
  const query = buildQuery({ user_id: userId });
  return request<any[]>(`/remote-connections${query}`);
};

export const createRemoteConnection = (
  request: ApiRequestFn,
  data: RemoteConnectionPayload,
): Promise<any> => {
  return request<any>('/remote-connections', {
    method: 'POST',
    body: JSON.stringify(data),
  });
};

export const getRemoteConnection = (request: ApiRequestFn, id: string): Promise<any> => {
  return request<any>(`/remote-connections/${id}`);
};

export const updateRemoteConnection = (
  request: ApiRequestFn,
  id: string,
  data: Omit<RemoteConnectionPayload, 'host' | 'username'> & { host?: string; username?: string }
): Promise<any> => {
  return request<any>(`/remote-connections/${id}`, {
    method: 'PUT',
    body: JSON.stringify(data),
  });
};

export const deleteRemoteConnection = (request: ApiRequestFn, id: string): Promise<any> => {
  return request<any>(`/remote-connections/${id}`, {
    method: 'DELETE',
  });
};

export const disconnectRemoteTerminal = (request: ApiRequestFn, id: string): Promise<any> => {
  return request<any>(`/remote-connections/${id}/disconnect`, {
    method: 'POST',
  });
};

export const testRemoteConnectionDraft = (
  request: ApiRequestFn,
  data: RemoteConnectionPayload,
): Promise<any> => {
  return request<any>('/remote-connections/test', {
    method: 'POST',
    body: JSON.stringify(data),
  });
};

export const testRemoteConnection = (request: ApiRequestFn, id: string): Promise<any> => {
  return request<any>(`/remote-connections/${id}/test`, {
    method: 'POST',
  });
};

export const listRemoteSftpEntries = (
  request: ApiRequestFn,
  connectionId: string,
  path?: string
): Promise<any> => {
  return request<any>(`/remote-connections/${connectionId}/sftp/list${buildQuery({ path })}`);
};

export const uploadRemoteSftpFile = (
  request: ApiRequestFn,
  connectionId: string,
  localPath: string,
  remotePath: string
): Promise<any> => {
  return request<any>(`/remote-connections/${connectionId}/sftp/upload`, {
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
): Promise<any> => {
  return request<any>(`/remote-connections/${connectionId}/sftp/download`, {
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
): Promise<any> => {
  return request<any>(`/remote-connections/${connectionId}/sftp/transfer/start`, {
    method: 'POST',
    body: JSON.stringify(data),
  });
};

export const getRemoteSftpTransferStatus = (
  request: ApiRequestFn,
  connectionId: string,
  transferId: string
): Promise<any> => {
  return request<any>(`/remote-connections/${connectionId}/sftp/transfer/${encodeURIComponent(transferId)}`);
};

export const cancelRemoteSftpTransfer = (
  request: ApiRequestFn,
  connectionId: string,
  transferId: string
): Promise<any> => {
  return request<any>(`/remote-connections/${connectionId}/sftp/transfer/${encodeURIComponent(transferId)}/cancel`, {
    method: 'POST',
  });
};

export const createRemoteSftpDirectory = (
  request: ApiRequestFn,
  connectionId: string,
  parentPath: string,
  name: string
): Promise<any> => {
  return request<any>(`/remote-connections/${connectionId}/sftp/mkdir`, {
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
): Promise<any> => {
  return request<any>(`/remote-connections/${connectionId}/sftp/rename`, {
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
): Promise<any> => {
  return request<any>(`/remote-connections/${connectionId}/sftp/delete`, {
    method: 'POST',
    body: JSON.stringify({
      path,
      recursive,
    }),
  });
};

export const listFsDirectories = (request: ApiRequestFn, path?: string): Promise<any> => {
  return request<any>(`/fs/list${buildQuery({ path })}`);
};

export const listFsEntries = (request: ApiRequestFn, path?: string): Promise<any> => {
  return request<any>(`/fs/entries${buildQuery({ path })}`);
};

export const searchFsEntries = (
  request: ApiRequestFn,
  path: string,
  query: string,
  limit?: number
): Promise<any> => {
  return request<any>(`/fs/search${buildQuery({ path, q: query, limit })}`);
};

export const readFsFile = (request: ApiRequestFn, path: string): Promise<any> => {
  return request<any>(`/fs/read${buildQuery({ path })}`);
};

export const createFsDirectory = (
  request: ApiRequestFn,
  parentPath: string,
  name: string
): Promise<any> => {
  return request<any>('/fs/mkdir', {
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
): Promise<any> => {
  return request<any>('/fs/touch', {
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
): Promise<any> => {
  return request<any>('/fs/delete', {
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
): Promise<any> => {
  return request<any>('/fs/move', {
    method: 'POST',
    body: JSON.stringify({
      source_path: sourcePath,
      target_parent_path: targetParentPath,
      target_name: options?.targetName,
      replace_existing: options?.replaceExisting,
    }),
  });
};
