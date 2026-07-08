// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { buildQuery } from '../shared';
import type {
  DeleteSuccessResponse,
  TerminalDispatchResponse,
  TerminalLogResponse,
  TerminalResponse,
} from '../types';
import type { ApiRequestFn } from './common';

export const listTerminals = (request: ApiRequestFn, userId?: string): Promise<TerminalResponse[]> => {
  const query = buildQuery({ user_id: userId });
  return request<TerminalResponse[]>(`/terminals${query}`);
};

export const createTerminal = (
  request: ApiRequestFn,
  data: { name?: string; cwd: string; user_id?: string },
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
  },
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
  params?: { limit?: number; offset?: number; before?: string },
): Promise<TerminalLogResponse[]> => {
  const query = buildQuery({
    limit: params?.limit,
    offset: params?.offset,
    before: params?.before,
  });
  return request<TerminalLogResponse[]>(`/terminals/${terminalId}/history${query}`);
};
