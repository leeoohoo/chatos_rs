import type { RemoteConnection } from '../../../types';
import type ApiClient from '../../api/client';
import { normalizeRemoteConnection } from '../helpers/remoteConnections';

interface CreateRemoteConnectionPayload {
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
}

interface UpdateRemoteConnectionPayload {
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
  jump_host?: string;
  jump_port?: number;
  jump_username?: string;
  jump_private_key_path?: string;
  jump_password?: string;
}

interface Deps {
  set: any;
  get: any;
  client: ApiClient;
  getUserIdParam: () => string;
}

export function createRemoteConnectionActions({ set, get, client, getUserIdParam }: Deps) {
  return {
    loadRemoteConnections: async () => {
      try {
        const uid = getUserIdParam();
        const list = await client.listRemoteConnections(uid);
        const formatted = Array.isArray(list) ? list.map(normalizeRemoteConnection) : [];
        set((state: any) => {
          state.remoteConnections = formatted;
          if (!state.currentRemoteConnectionId) {
            const lastId = localStorage.getItem(`lastRemoteConnectionId_${uid}`);
            if (lastId) {
              const matched = formatted.find((item: RemoteConnection) => item.id === lastId);
              if (matched) {
                state.currentRemoteConnectionId = matched.id;
                state.currentRemoteConnection = matched;
              }
            }
          } else {
            const matched = formatted.find((item: RemoteConnection) => item.id === state.currentRemoteConnectionId);
            if (matched) {
              state.currentRemoteConnection = matched;
            }
          }
        });
        return formatted;
      } catch (error) {
        console.error('Failed to load remote connections:', error);
        set((state: any) => {
          state.error = error instanceof Error ? error.message : 'Failed to load remote connections';
        });
        return [];
      }
    },

    createRemoteConnection: async (payload: CreateRemoteConnectionPayload) => {
      const uid = getUserIdParam();
      const created = await client.createRemoteConnection({
        ...payload,
        user_id: uid,
      });
      const connection = normalizeRemoteConnection(created);
      set((state: any) => {
        state.remoteConnections.unshift(connection);
        state.currentRemoteConnectionId = connection.id;
        state.currentRemoteConnection = connection;
        state.activePanel = 'remote_terminal';
      });
      localStorage.setItem(`lastRemoteConnectionId_${uid}`, connection.id);
      return connection;
    },

    updateRemoteConnection: async (connectionId: string, payload: UpdateRemoteConnectionPayload) => {
      try {
        const updated = await client.updateRemoteConnection(connectionId, payload);
        const connection = normalizeRemoteConnection(updated);
        set((state: any) => {
          const index = state.remoteConnections.findIndex((item: RemoteConnection) => item.id === connectionId);
          if (index !== -1) {
            state.remoteConnections[index] = connection;
          }
          if (state.currentRemoteConnectionId === connectionId) {
            state.currentRemoteConnection = connection;
          }
        });
        return connection;
      } catch (error) {
        console.error('Failed to update remote connection:', error);
        set((state: any) => {
          state.error = error instanceof Error ? error.message : 'Failed to update remote connection';
        });
        return null;
      }
    },

    deleteRemoteConnection: async (connectionId: string) => {
      try {
        await client.deleteRemoteConnection(connectionId);
        set((state: any) => {
          state.remoteConnections = state.remoteConnections.filter((item: RemoteConnection) => item.id !== connectionId);
          if (state.currentRemoteConnectionId === connectionId) {
            state.currentRemoteConnectionId = null;
            state.currentRemoteConnection = null;
            if (state.activePanel === 'remote_terminal' || state.activePanel === 'remote_sftp') {
              state.activePanel = 'chat';
            }
          }
        });
      } catch (error) {
        console.error('Failed to delete remote connection:', error);
        set((state: any) => {
          state.error = error instanceof Error ? error.message : 'Failed to delete remote connection';
        });
      }
    },

    selectRemoteConnection: async (connectionId: string) => {
      try {
        let connection = get().remoteConnections.find((item: RemoteConnection) => item.id === connectionId) || null;
        if (!connection) {
          const fetched = await client.getRemoteConnection(connectionId);
          connection = normalizeRemoteConnection(fetched);
        }
        const uid = getUserIdParam();
        set((state: any) => {
          state.currentRemoteConnectionId = connectionId;
          state.currentRemoteConnection = connection;
          state.activePanel = 'remote_terminal';
        });
        localStorage.setItem(`lastRemoteConnectionId_${uid}`, connectionId);
      } catch (error) {
        console.error('Failed to select remote connection:', error);
        set((state: any) => {
          state.error = error instanceof Error ? error.message : 'Failed to select remote connection';
        });
      }
    },

    openRemoteSftp: async (connectionId: string) => {
      try {
        let connection = get().remoteConnections.find((item: RemoteConnection) => item.id === connectionId) || null;
        if (!connection) {
          const fetched = await client.getRemoteConnection(connectionId);
          connection = normalizeRemoteConnection(fetched);
        }
        const uid = getUserIdParam();
        set((state: any) => {
          state.currentRemoteConnectionId = connectionId;
          state.currentRemoteConnection = connection;
          state.activePanel = 'remote_sftp';
        });
        localStorage.setItem(`lastRemoteConnectionId_${uid}`, connectionId);
      } catch (error) {
        console.error('Failed to open remote sftp:', error);
        set((state: any) => {
          state.error = error instanceof Error ? error.message : 'Failed to open remote sftp';
        });
      }
    },
  };
}
