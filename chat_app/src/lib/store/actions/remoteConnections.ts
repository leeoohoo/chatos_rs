// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { RemoteConnection } from '../../../types';
import type ApiClient from '../../api/client';
import { ApiRequestError } from '../../api/client/shared';
import { normalizeRemoteConnection } from '../helpers/remoteConnections';
import { mergeSessionRuntimeIntoMetadata } from '../helpers/sessionRuntime';
import type { ChatStoreDraft, ChatStoreGet, ChatStoreSet } from '../types';

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
  jump_connection_id?: string;
  jump_host?: string;
  jump_port?: number;
  jump_username?: string;
  jump_private_key_path?: string;
  jump_certificate_path?: string;
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
  jump_connection_id?: string;
  jump_host?: string;
  jump_port?: number;
  jump_username?: string;
  jump_private_key_path?: string;
  jump_certificate_path?: string;
  jump_password?: string;
}

interface Deps {
  set: ChatStoreSet;
  get: ChatStoreGet;
  client: ApiClient;
  getUserIdParam: () => string;
}

interface LoadRemoteConnectionsOptions {
  force?: boolean;
}

interface RemoteConnectionsListCacheEntry {
  connections: RemoteConnection[];
  stale: boolean;
}

interface RemoteConnectionsDetailCacheEntry {
  connection: RemoteConnection;
  stale: boolean;
}

interface RemoteConnectionsClientCacheState {
  listCache: Map<string, RemoteConnectionsListCacheEntry>;
  listInflight: Map<string, Promise<RemoteConnection[]>>;
  detailCache: Map<string, RemoteConnectionsDetailCacheEntry>;
  detailInflight: Map<string, Promise<RemoteConnection>>;
}

const remoteConnectionsClientCaches = new WeakMap<ApiClient, RemoteConnectionsClientCacheState>();

const normalizeUserId = (userId: string): string => String(userId || '').trim();

const buildRemoteConnectionsListCacheKey = (userId: string): string => normalizeUserId(userId);

const normalizeConnectionId = (connectionId: string): string => String(connectionId || '').trim();

const getOrCreateClientCacheState = (apiClient: ApiClient): RemoteConnectionsClientCacheState => {
  const existing = remoteConnectionsClientCaches.get(apiClient);
  if (existing) {
    return existing;
  }

  const next: RemoteConnectionsClientCacheState = {
    listCache: new Map(),
    listInflight: new Map(),
    detailCache: new Map(),
    detailInflight: new Map(),
  };
  remoteConnectionsClientCaches.set(apiClient, next);
  return next;
};

const upsertRemoteConnection = (
  connections: RemoteConnection[],
  connection: RemoteConnection,
): RemoteConnection[] => {
  const index = connections.findIndex((item) => item.id === connection.id);
  if (index === -1) {
    return [connection, ...connections];
  }
  const next = [...connections];
  next[index] = connection;
  return next;
};

const removeRemoteConnection = (
  connections: RemoteConnection[],
  connectionId: string,
): RemoteConnection[] => {
  return connections.filter((item) => item.id !== connectionId);
};

const markRemoteConnectionCachesStale = (
  apiClient: ApiClient,
  options?: { userId?: string | null; connectionId?: string | null },
) => {
  const cacheState = getOrCreateClientCacheState(apiClient);
  const normalizedUserId = normalizeUserId(String(options?.userId || ''));
  const normalizedConnectionId = normalizeConnectionId(String(options?.connectionId || ''));

  if (normalizedUserId) {
    const cached = cacheState.listCache.get(buildRemoteConnectionsListCacheKey(normalizedUserId));
    if (cached) {
      cacheState.listCache.set(buildRemoteConnectionsListCacheKey(normalizedUserId), {
        ...cached,
        stale: true,
      });
    }
  } else {
    cacheState.listCache.forEach((entry, key) => {
      cacheState.listCache.set(key, {
        ...entry,
        stale: true,
      });
    });
  }

  if (normalizedConnectionId) {
    const cached = cacheState.detailCache.get(normalizedConnectionId);
    if (cached) {
      cacheState.detailCache.set(normalizedConnectionId, {
        ...cached,
        stale: true,
      });
    }
  }
};

export function createRemoteConnectionActions({ set, get, client, getUserIdParam }: Deps) {
  const syncRemoteConnectionDetailCache = (connection: RemoteConnection) => {
    const normalizedConnectionId = normalizeConnectionId(connection.id);
    if (!normalizedConnectionId) {
      return;
    }
    const cacheState = getOrCreateClientCacheState(client);
    cacheState.detailCache.set(normalizedConnectionId, {
      connection,
      stale: false,
    });
  };

  const syncRemoteConnectionListCaches = (
    updater: (connections: RemoteConnection[]) => RemoteConnection[],
  ) => {
    const cacheState = getOrCreateClientCacheState(client);
    cacheState.listCache.forEach((entry, key) => {
      cacheState.listCache.set(key, {
        connections: updater(entry.connections),
        stale: false,
      });
    });
  };

  const syncLoadedRemoteConnections = (userId: string, connections: RemoteConnection[]) => {
    const cacheState = getOrCreateClientCacheState(client);
    cacheState.listCache.set(buildRemoteConnectionsListCacheKey(userId), {
      connections,
      stale: false,
    });
    connections.forEach((connection) => {
      syncRemoteConnectionDetailCache(connection);
    });
  };

  const upsertRemoteConnectionCaches = (connection: RemoteConnection) => {
    syncRemoteConnectionDetailCache(connection);
    syncRemoteConnectionListCaches((connections) => upsertRemoteConnection(connections, connection));
  };

  const removeRemoteConnectionCaches = (connectionId: string) => {
    const normalizedConnectionId = normalizeConnectionId(connectionId);
    if (!normalizedConnectionId) {
      return;
    }
    const cacheState = getOrCreateClientCacheState(client);
    cacheState.detailCache.delete(normalizedConnectionId);
    cacheState.detailInflight.delete(normalizedConnectionId);
    syncRemoteConnectionListCaches((connections) => removeRemoteConnection(connections, normalizedConnectionId));
  };

  const loadRemoteConnectionDetail = async (
    connectionId: string,
    options?: { force?: boolean },
  ): Promise<RemoteConnection> => {
    const normalizedConnectionId = normalizeConnectionId(connectionId);
    if (!normalizedConnectionId) {
      throw new Error('remote connection id is required');
    }

    const cacheState = getOrCreateClientCacheState(client);
    const cached = cacheState.detailCache.get(normalizedConnectionId);
    if (!options?.force && cached && !cached.stale) {
      return cached.connection;
    }

    let inflight = cacheState.detailInflight.get(normalizedConnectionId);
    if (!inflight) {
      inflight = client.getRemoteConnection(normalizedConnectionId)
        .then((payload) => normalizeRemoteConnection(payload))
        .then((connection) => {
          syncRemoteConnectionDetailCache(connection);
          syncRemoteConnectionListCaches((connections) => upsertRemoteConnection(connections, connection));
          return connection;
        })
        .finally(() => {
          cacheState.detailInflight.delete(normalizedConnectionId);
        });
      cacheState.detailInflight.set(normalizedConnectionId, inflight);
    }

    return inflight;
  };

  const persistRemoteConnectionToCurrentSession = (connectionId: string | null) => {
    const state = get();
    const currentSessionId = typeof state?.currentSessionId === 'string'
      ? state.currentSessionId.trim()
      : '';
    if (!currentSessionId) {
      return;
    }
    const currentSession = state?.currentSession;
    const metadata = mergeSessionRuntimeIntoMetadata(currentSession?.metadata, {
      remoteConnectionId: connectionId,
    });
    set((draft: ChatStoreDraft) => {
      const sessionIndex = draft.sessions.findIndex((item) => item?.id === currentSessionId);
      if (sessionIndex >= 0) {
        draft.sessions[sessionIndex].metadata = metadata;
      }
      if (draft.currentSession && draft.currentSession.id === currentSessionId) {
        draft.currentSession.metadata = metadata;
      }
    });
    void client.updateSession(currentSessionId, { metadata }).catch(() => {});
  };

  return {
    applyRealtimeRemoteConnectionSnapshot: (connectionPayload: RemoteConnection | unknown) => {
      const connection = normalizeRemoteConnection(connectionPayload);
      const normalizedConnectionId = normalizeConnectionId(connection?.id || '');
      if (!normalizedConnectionId) {
        return null;
      }
      upsertRemoteConnectionCaches(connection);
      set((state: ChatStoreDraft) => {
        state.remoteConnections = upsertRemoteConnection(state.remoteConnections, connection);
        if (state.currentRemoteConnectionId === normalizedConnectionId) {
          state.currentRemoteConnection = connection;
        }
      });
      return connection;
    },

    loadRemoteConnections: async (options?: LoadRemoteConnectionsOptions) => {
      try {
        const uid = getUserIdParam();
        const cacheKey = buildRemoteConnectionsListCacheKey(uid);
        const cacheState = getOrCreateClientCacheState(client);
        const cached = cacheState.listCache.get(cacheKey);
        if (!options?.force && cached && !cached.stale) {
          const formatted = cached.connections;
          set((state: ChatStoreDraft) => {
            state.remoteConnections = formatted;
            if (state.currentRemoteConnectionId) {
              const matched = formatted.find((item: RemoteConnection) => item.id === state.currentRemoteConnectionId);
              if (matched) {
                state.currentRemoteConnection = matched;
              } else {
                state.currentRemoteConnectionId = null;
                state.currentRemoteConnection = null;
              }
            } else {
              state.currentRemoteConnection = null;
            }
          });
          return formatted;
        }

        let inflight = cacheState.listInflight.get(cacheKey);
        if (!inflight) {
          inflight = client.listRemoteConnections(uid)
            .then((list) => {
              const formatted = Array.isArray(list) ? list.map(normalizeRemoteConnection) : [];
              syncLoadedRemoteConnections(uid, formatted);
              return formatted;
            })
            .finally(() => {
              cacheState.listInflight.delete(cacheKey);
            });
          cacheState.listInflight.set(cacheKey, inflight);
        }

        const formatted = await inflight;
        set((state: ChatStoreDraft) => {
          state.remoteConnections = formatted;
          if (state.currentRemoteConnectionId) {
            const matched = formatted.find((item: RemoteConnection) => item.id === state.currentRemoteConnectionId);
            if (matched) {
              state.currentRemoteConnection = matched;
            } else {
              state.currentRemoteConnectionId = null;
              state.currentRemoteConnection = null;
            }
          } else {
            state.currentRemoteConnection = null;
          }
        });
        return formatted;
      } catch (error) {
        console.error('Failed to load remote connections:', error);
        set((state: ChatStoreDraft) => {
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
      upsertRemoteConnectionCaches(connection);
      set((state: ChatStoreDraft) => {
        state.remoteConnections = upsertRemoteConnection(state.remoteConnections, connection);
        state.currentRemoteConnectionId = connection.id;
        state.currentRemoteConnection = connection;
        state.activePanel = 'remote_terminal';
      });
      localStorage.setItem(`lastRemoteConnectionId_${uid}`, connection.id);
      persistRemoteConnectionToCurrentSession(connection.id);
      return connection;
    },

    updateRemoteConnection: async (connectionId: string, payload: UpdateRemoteConnectionPayload) => {
      try {
        const updated = await client.updateRemoteConnection(connectionId, payload);
        const connection = normalizeRemoteConnection(updated);
        upsertRemoteConnectionCaches(connection);
        set((state: ChatStoreDraft) => {
          state.remoteConnections = upsertRemoteConnection(state.remoteConnections, connection);
          if (state.currentRemoteConnectionId === connectionId) {
            state.currentRemoteConnection = connection;
          }
        });
        return connection;
      } catch (error) {
        console.error('Failed to update remote connection:', error);
        set((state: ChatStoreDraft) => {
          state.error = error instanceof Error ? error.message : 'Failed to update remote connection';
        });
        return null;
      }
    },

    deleteRemoteConnection: async (connectionId: string) => {
      try {
        const shouldClearSessionRuntime = get().currentRemoteConnectionId === connectionId;
        await client.deleteRemoteConnection(connectionId);
        removeRemoteConnectionCaches(connectionId);
        set((state: ChatStoreDraft) => {
          state.remoteConnections = removeRemoteConnection(state.remoteConnections, connectionId);
          if (state.currentRemoteConnectionId === connectionId) {
            state.currentRemoteConnectionId = null;
            state.currentRemoteConnection = null;
            if (state.activePanel === 'remote_terminal' || state.activePanel === 'remote_sftp') {
              state.activePanel = 'chat';
            }
          }
        });
        if (shouldClearSessionRuntime) {
          persistRemoteConnectionToCurrentSession(null);
        }
      } catch (error) {
        console.error('Failed to delete remote connection:', error);
        set((state: ChatStoreDraft) => {
          state.error = error instanceof Error ? error.message : 'Failed to delete remote connection';
        });
      }
    },

    selectRemoteConnection: async (
      connectionId: string | null,
      options?: { activatePanel?: boolean },
    ) => {
      try {
        const normalizedConnectionId = normalizeConnectionId(connectionId || '');
        const activatePanel = options?.activatePanel !== false;
        const uid = getUserIdParam();
        if (!normalizedConnectionId) {
          set((state: ChatStoreDraft) => {
            state.currentRemoteConnectionId = null;
            state.currentRemoteConnection = null;
            if (state.activePanel === 'remote_terminal' || state.activePanel === 'remote_sftp') {
              state.activePanel = 'chat';
            }
          });
          localStorage.removeItem(`lastRemoteConnectionId_${uid}`);
          persistRemoteConnectionToCurrentSession(null);
          return;
        }
        let connection = get().remoteConnections.find((item: RemoteConnection) => item.id === normalizedConnectionId) || null;
        if (!connection) {
          connection = await loadRemoteConnectionDetail(normalizedConnectionId);
        }
        set((state: ChatStoreDraft) => {
          state.remoteConnections = upsertRemoteConnection(state.remoteConnections, connection);
          state.currentRemoteConnectionId = normalizedConnectionId;
          state.currentRemoteConnection = connection;
          if (activatePanel) {
            state.activePanel = 'remote_terminal';
          }
        });
        localStorage.setItem(`lastRemoteConnectionId_${uid}`, normalizedConnectionId);
        persistRemoteConnectionToCurrentSession(normalizedConnectionId);
      } catch (error) {
        console.error('Failed to select remote connection:', error);
        set((state: ChatStoreDraft) => {
          state.error = error instanceof Error ? error.message : 'Failed to select remote connection';
        });
      }
    },

    openRemoteSftp: async (connectionId: string) => {
      try {
        const normalizedConnectionId = normalizeConnectionId(connectionId);
        let connection = get().remoteConnections.find((item: RemoteConnection) => item.id === normalizedConnectionId) || null;
        if (!connection) {
          connection = await loadRemoteConnectionDetail(normalizedConnectionId);
        }
        const uid = getUserIdParam();
        set((state: ChatStoreDraft) => {
          state.remoteConnections = upsertRemoteConnection(state.remoteConnections, connection);
          state.currentRemoteConnectionId = normalizedConnectionId;
          state.currentRemoteConnection = connection;
          state.activePanel = 'remote_sftp';
        });
        localStorage.setItem(`lastRemoteConnectionId_${uid}`, normalizedConnectionId);
        persistRemoteConnectionToCurrentSession(normalizedConnectionId);
      } catch (error) {
        console.error('Failed to open remote sftp:', error);
        set((state: ChatStoreDraft) => {
          state.error = error instanceof Error ? error.message : 'Failed to open remote sftp';
        });
      }
    },

    markRemoteConnectionsStale: (options?: { userId?: string | null; connectionId?: string | null }) => {
      markRemoteConnectionCachesStale(client, options);
    },

    removeRemoteConnectionLocally: (connectionId: string) => {
      const normalizedConnectionId = normalizeConnectionId(connectionId);
      if (!normalizedConnectionId) {
        return;
      }
      const shouldClearSessionRuntime = get().currentRemoteConnectionId === normalizedConnectionId;
      removeRemoteConnectionCaches(normalizedConnectionId);
      set((state: ChatStoreDraft) => {
        state.remoteConnections = removeRemoteConnection(state.remoteConnections, normalizedConnectionId);
        if (state.currentRemoteConnectionId === normalizedConnectionId) {
          state.currentRemoteConnectionId = null;
          state.currentRemoteConnection = null;
          if (state.activePanel === 'remote_terminal' || state.activePanel === 'remote_sftp') {
            state.activePanel = 'chat';
          }
        }
      });
      if (shouldClearSessionRuntime) {
        persistRemoteConnectionToCurrentSession(null);
      }
    },

    refreshRemoteConnectionById: async (connectionId: string) => {
      try {
        const normalizedConnectionId = normalizeConnectionId(connectionId);
        if (!normalizedConnectionId) {
          return null;
        }
        const connection = await loadRemoteConnectionDetail(normalizedConnectionId, { force: true });
        set((state: ChatStoreDraft) => {
          state.remoteConnections = upsertRemoteConnection(state.remoteConnections, connection);
          if (state.currentRemoteConnectionId === normalizedConnectionId) {
            state.currentRemoteConnection = connection;
          }
        });
        return connection;
      } catch (error) {
        if (error instanceof ApiRequestError && error.status === 404) {
          const shouldClearSessionRuntime = get().currentRemoteConnectionId === connectionId;
          removeRemoteConnectionCaches(connectionId);
          set((state: ChatStoreDraft) => {
            state.remoteConnections = removeRemoteConnection(state.remoteConnections, connectionId);
            if (state.currentRemoteConnectionId === connectionId) {
              state.currentRemoteConnectionId = null;
              state.currentRemoteConnection = null;
              if (state.activePanel === 'remote_terminal' || state.activePanel === 'remote_sftp') {
                state.activePanel = 'chat';
              }
            }
          });
          if (shouldClearSessionRuntime) {
            persistRemoteConnectionToCurrentSession(null);
          }
          return null;
        }
        console.error('Failed to refresh remote connection detail:', error);
        return null;
      }
    },
  };
}
