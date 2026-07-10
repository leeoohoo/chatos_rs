// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { RemoteConnection, Terminal } from '../../../types';
import type {
  RemoteConnectionDraftPayload,
  RemoteConnectionUpdatePayload,
} from '../../api/client/types';

export type RemoteConnectionMutationPayload = RemoteConnectionUpdatePayload;
export type RemoteConnectionCreatePayload = RemoteConnectionDraftPayload;

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
