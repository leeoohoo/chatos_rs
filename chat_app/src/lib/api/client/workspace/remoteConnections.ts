// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { buildQuery } from '../shared';
import type {
  DeleteSuccessResponse,
  FsMutationResponse,
  RemoteConnectionResponse,
  RemoteConnectionTestResponse,
  RemoteSftpEntriesResponse,
  RemoteSftpTransferStatusResponse,
} from '../types';
import type { ApiRequestFn, RemoteConnectionPayload } from './common';
import { buildRemoteVerificationHeaders } from './common';

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
  data: Omit<RemoteConnectionPayload, 'host' | 'username'> & { host?: string; username?: string },
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
  verificationCode?: string,
): Promise<RemoteConnectionTestResponse> => {
  return request<RemoteConnectionTestResponse>('/remote-connections/test', {
    method: 'POST',
    headers: buildRemoteVerificationHeaders(verificationCode),
    body: JSON.stringify(data),
  });
};

export const testRemoteConnection = (
  request: ApiRequestFn,
  id: string,
  verificationCode?: string,
): Promise<RemoteConnectionTestResponse> => {
  return request<RemoteConnectionTestResponse>(`/remote-connections/${id}/test`, {
    method: 'POST',
    headers: buildRemoteVerificationHeaders(verificationCode),
  });
};

export const listRemoteSftpEntries = (
  request: ApiRequestFn,
  connectionId: string,
  path?: string,
  verificationCode?: string,
): Promise<RemoteSftpEntriesResponse> => {
  return request<RemoteSftpEntriesResponse>(
    `/remote-connections/${connectionId}/sftp/list${buildQuery({ path })}`,
    { headers: buildRemoteVerificationHeaders(verificationCode) },
  );
};

export const uploadRemoteSftpFile = (
  request: ApiRequestFn,
  connectionId: string,
  localPath: string,
  remotePath: string,
  verificationCode?: string,
): Promise<RemoteSftpTransferStatusResponse> => {
  return request<RemoteSftpTransferStatusResponse>(`/remote-connections/${connectionId}/sftp/upload`, {
    method: 'POST',
    headers: buildRemoteVerificationHeaders(verificationCode),
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
  localPath: string,
  verificationCode?: string,
): Promise<RemoteSftpTransferStatusResponse> => {
  return request<RemoteSftpTransferStatusResponse>(`/remote-connections/${connectionId}/sftp/download`, {
    method: 'POST',
    headers: buildRemoteVerificationHeaders(verificationCode),
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
  verificationCode?: string,
): Promise<RemoteSftpTransferStatusResponse> => {
  return request<RemoteSftpTransferStatusResponse>(`/remote-connections/${connectionId}/sftp/transfer/start`, {
    method: 'POST',
    headers: buildRemoteVerificationHeaders(verificationCode),
    body: JSON.stringify(data),
  });
};

export const getRemoteSftpTransferStatus = (
  request: ApiRequestFn,
  connectionId: string,
  transferId: string,
): Promise<RemoteSftpTransferStatusResponse> => {
  return request<RemoteSftpTransferStatusResponse>(
    `/remote-connections/${connectionId}/sftp/transfer/${encodeURIComponent(transferId)}`,
  );
};

export const cancelRemoteSftpTransfer = (
  request: ApiRequestFn,
  connectionId: string,
  transferId: string,
): Promise<RemoteSftpTransferStatusResponse> => {
  return request<RemoteSftpTransferStatusResponse>(
    `/remote-connections/${connectionId}/sftp/transfer/${encodeURIComponent(transferId)}/cancel`,
    { method: 'POST' },
  );
};

export const createRemoteSftpDirectory = (
  request: ApiRequestFn,
  connectionId: string,
  parentPath: string,
  name: string,
  verificationCode?: string,
): Promise<FsMutationResponse> => {
  return request<FsMutationResponse>(`/remote-connections/${connectionId}/sftp/mkdir`, {
    method: 'POST',
    headers: buildRemoteVerificationHeaders(verificationCode),
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
  toPath: string,
  verificationCode?: string,
): Promise<FsMutationResponse> => {
  return request<FsMutationResponse>(`/remote-connections/${connectionId}/sftp/rename`, {
    method: 'POST',
    headers: buildRemoteVerificationHeaders(verificationCode),
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
  recursive = false,
  verificationCode?: string,
): Promise<FsMutationResponse> => {
  return request<FsMutationResponse>(`/remote-connections/${connectionId}/sftp/delete`, {
    method: 'POST',
    headers: buildRemoteVerificationHeaders(verificationCode),
    body: JSON.stringify({
      path,
      recursive,
    }),
  });
};
