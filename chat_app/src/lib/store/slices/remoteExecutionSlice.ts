// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { RemoteConnection, Terminal } from '../../../types';

export interface RemoteConnectionMutationPayload {
  name?: string;
  host?: string;
  port?: number;
  username?: string;
  auth_type?: 'private_key' | 'private_key_cert' | 'password';
  password?: string;
  private_key_path?: string;
  certificate_path?: string;
  default_remote_path?: string;
  host_key_policy?: 'strict' | 'accept_new';
  jump_enabled?: boolean;
  jump_connection_id?: string;
  jump_host?: string;
  jump_port?: number;
  jump_username?: string;
  jump_private_key_path?: string;
  jump_certificate_path?: string;
  jump_password?: string;
}

export interface RemoteConnectionCreatePayload extends RemoteConnectionMutationPayload {
  host: string;
  username: string;
}

export interface RemoteExecutionSliceState {
  terminals: Terminal[];
  currentTerminalId: string | null;
  currentTerminal: Terminal | null;
  remoteConnections: RemoteConnection[];
  currentRemoteConnectionId: string | null;
  currentRemoteConnection: RemoteConnection | null;
}

export const remoteExecutionInitialState: RemoteExecutionSliceState = {
  terminals: [],
  currentTerminalId: null,
  currentTerminal: null,
  remoteConnections: [],
  currentRemoteConnectionId: null,
  currentRemoteConnection: null,
};

export interface RemoteExecutionSliceActions {
  loadTerminals: (options?: { force?: boolean }) => Promise<Terminal[]>;
  createTerminal: (cwd: string, name?: string) => Promise<Terminal>;
  deleteTerminal: (terminalId: string) => Promise<void>;
  selectTerminal: (terminalId: string) => Promise<void>;
  markTerminalsStale: (options?: { userId?: string | null; terminalId?: string | null }) => void;
  removeTerminalLocally: (terminalId: string) => void;
  applyRealtimeTerminalSnapshot: (terminal: Terminal | unknown) => Terminal | null;
  refreshTerminalById: (terminalId: string) => Promise<Terminal | null>;

  loadRemoteConnections: (options?: { force?: boolean }) => Promise<RemoteConnection[]>;
  createRemoteConnection: (payload: RemoteConnectionCreatePayload) => Promise<RemoteConnection>;
  updateRemoteConnection: (
    connectionId: string,
    payload: RemoteConnectionMutationPayload,
  ) => Promise<RemoteConnection | null>;
  deleteRemoteConnection: (connectionId: string) => Promise<void>;
  selectRemoteConnection: (
    connectionId: string | null,
    options?: { activatePanel?: boolean },
  ) => Promise<void>;
  openRemoteSftp: (connectionId: string) => Promise<void>;
  markRemoteConnectionsStale: (options?: { userId?: string | null; connectionId?: string | null }) => void;
  removeRemoteConnectionLocally: (connectionId: string) => void;
  applyRealtimeRemoteConnectionSnapshot: (
    connection: RemoteConnection | unknown,
  ) => RemoteConnection | null;
  refreshRemoteConnectionById: (connectionId: string) => Promise<RemoteConnection | null>;
}
