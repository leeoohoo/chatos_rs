import React, { createContext, useContext, useEffect, useMemo, useRef, useState } from 'react';

import { useApiClientContext } from '../api/ApiClientContext';
import { RealtimeClient } from './client';
import {
  getRealtimeConnectionStateSnapshot,
  setRealtimeConnectionStateSnapshot,
} from './state';
import type {
  RealtimeConnectionState,
  RealtimeDebugEventRecord,
  RealtimeDebugSnapshot,
  RealtimeEventEnvelope,
  RealtimeTopic,
} from './types';

type RealtimeDebugWindow = Window & typeof globalThis & {
  __CHATOS_REALTIME_DEBUG__?: {
    snapshot: RealtimeDebugSnapshot;
    getSnapshot: () => RealtimeDebugSnapshot;
    getTopics: () => RealtimeTopic[];
    getConnectionState: () => RealtimeConnectionState;
    getRecentEvents: () => RealtimeDebugEventRecord[];
  };
};

interface RealtimeStateContextValue {
  connectionState: RealtimeConnectionState;
  debugSnapshot: RealtimeDebugSnapshot;
}

interface RealtimeContextValue extends RealtimeStateContextValue {
  client: RealtimeClient;
}

const RealtimeClientContext = createContext<RealtimeClient | null>(null);
const RealtimeConnectionStateContext = createContext<RealtimeConnectionState | null>(null);
const RealtimeDebugSnapshotContext = createContext<RealtimeDebugSnapshot | null>(null);

interface RealtimeProviderProps {
  children: React.ReactNode;
  accessToken?: string | null;
}

export const RealtimeProvider: React.FC<RealtimeProviderProps> = ({
  children,
  accessToken,
}) => {
  const apiClient = useApiClientContext();
  const clientRef = useRef<RealtimeClient | null>(null);
  if (!clientRef.current) {
    clientRef.current = new RealtimeClient(
      apiClient.getBaseUrl(),
      () => apiClient.issueWebSocketTicket(),
    );
  }

  const client = clientRef.current;
  const [connectionState, setConnectionState] = useState<RealtimeConnectionState>(
    getRealtimeConnectionStateSnapshot(),
  );
  const [debugSnapshot, setDebugSnapshot] = useState<RealtimeDebugSnapshot>(
    client.getDebugSnapshot(),
  );

  useEffect(() => {
    client.setBaseUrl(apiClient.getBaseUrl());
  }, [apiClient, client]);

  useEffect(() => {
    client.setWebSocketTicketFactory(() => apiClient.issueWebSocketTicket());
  }, [apiClient, client]);

  useEffect(() => {
    client.setAccessToken(accessToken);
  }, [accessToken, client]);

  useEffect(() => {
    const unsubscribe = client.subscribeState((state) => {
      setRealtimeConnectionStateSnapshot(state);
      setConnectionState(state);
    });
    return () => {
      unsubscribe();
    };
  }, [client]);

  useEffect(() => {
    const unsubscribe = client.subscribeDebug(setDebugSnapshot);
    return () => {
      unsubscribe();
    };
  }, [client]);

  useEffect(() => {
    if (
      typeof window === 'undefined'
      || typeof import.meta === 'undefined'
      || !(import.meta as ImportMeta & { env?: { DEV?: boolean } }).env?.DEV
    ) {
      return undefined;
    }
    const debugWindow = window as RealtimeDebugWindow;
    debugWindow.__CHATOS_REALTIME_DEBUG__ = {
      snapshot: debugSnapshot,
      getSnapshot: () => client.getDebugSnapshot(),
      getTopics: () => client.getTopics(),
      getConnectionState: () => client.getConnectionState(),
      getRecentEvents: () => client.getDebugSnapshot().recentEvents,
    };
    return () => {
      delete debugWindow.__CHATOS_REALTIME_DEBUG__;
    };
  }, [client, debugSnapshot]);

  useEffect(() => () => {
    client.destroy();
  }, [client]);

  return (
    <RealtimeClientContext.Provider value={client}>
      <RealtimeConnectionStateContext.Provider value={connectionState}>
        <RealtimeDebugSnapshotContext.Provider value={debugSnapshot}>
          {children}
        </RealtimeDebugSnapshotContext.Provider>
      </RealtimeConnectionStateContext.Provider>
    </RealtimeClientContext.Provider>
  );
};

const useRealtimeClient = (): RealtimeClient => {
  const client = useContext(RealtimeClientContext);
  if (!client) {
    throw new Error('useRealtimeClient must be used within a RealtimeProvider');
  }
  return client;
};

const useRealtimeConnectionStateContext = (): RealtimeConnectionState => {
  const context = useContext(RealtimeConnectionStateContext);
  if (!context) {
    throw new Error('useRealtimeConnectionState must be used within a RealtimeProvider');
  }
  return context;
};

const useRealtimeDebugSnapshotContext = (): RealtimeDebugSnapshot => {
  const context = useContext(RealtimeDebugSnapshotContext);
  if (!context) {
    throw new Error('useRealtimeDebugSnapshot must be used within a RealtimeProvider');
  }
  return context;
};

export const useRealtimeContext = (): RealtimeContextValue => {
  const client = useRealtimeClient();
  const connectionState = useRealtimeConnectionStateContext();
  const debugSnapshot = useRealtimeDebugSnapshotContext();
  return useMemo(() => ({
    client,
    connectionState,
    debugSnapshot,
  }), [client, connectionState, debugSnapshot]);
};

export const useRealtimeConnectionState = (): RealtimeConnectionState => {
  return useRealtimeConnectionStateContext();
};

export { getRealtimeConnectionStateSnapshot } from './state';

export const useRealtimeDebugSnapshot = (): RealtimeDebugSnapshot => {
  return useRealtimeDebugSnapshotContext();
};

export const useRealtimeEvent = (
  handler: (event: RealtimeEventEnvelope) => void,
): void => {
  const client = useRealtimeClient();
  const handlerRef = useRef(handler);

  useEffect(() => {
    handlerRef.current = handler;
  }, [handler]);

  useEffect(() => {
    return client.subscribe((event) => {
      handlerRef.current(event);
    });
  }, [client]);
};

export const useRealtimeTopic = (
  topic: RealtimeTopic | null | undefined,
  enabled = true,
): void => {
  const client = useRealtimeClient();
  const normalizedTopic = useMemo(() => {
    if (!enabled || !topic) {
      return null;
    }
    const scope = String(topic.scope || '').trim();
    if (!scope) {
      return null;
    }
    const id = typeof topic.id === 'string' ? topic.id.trim() : '';
    return {
      key: `${scope}:${id}`,
      topic: {
        scope: topic.scope,
        id: id || null,
      },
    };
  }, [enabled, topic]);

  useEffect(() => {
    if (!enabled || !normalizedTopic) {
      return undefined;
    }
    return client.subscribeTopic(normalizedTopic.topic);
  }, [client, enabled, normalizedTopic?.key]);
};

export const useRealtimeTopics = (
  topics: Array<RealtimeTopic | null | undefined>,
  enabled = true,
): void => {
  const client = useRealtimeClient();
  const normalizedTopics = useMemo(() => {
    if (!enabled) {
      return {
        signature: '',
        topics: [] as RealtimeTopic[],
      };
    }
    const seen = new Set<string>();
    const out: RealtimeTopic[] = [];
    const keys: string[] = [];
    for (const topic of topics) {
      if (!topic) {
        continue;
      }
      const scope = String(topic.scope || '').trim();
      if (!scope) {
        continue;
      }
      const id = typeof topic.id === 'string' ? topic.id.trim() : '';
      const key = `${scope}:${id}`;
      if (seen.has(key)) {
        continue;
      }
      seen.add(key);
      keys.push(key);
      out.push({
        scope: topic.scope,
        id: id || null,
      });
    }
    return {
      signature: keys.join('|'),
      topics: out,
    };
  }, [enabled, topics]);

  useEffect(() => {
    if (!enabled || normalizedTopics.topics.length === 0) {
      return undefined;
    }
    const unsubscribers = normalizedTopics.topics.map((topic) => client.subscribeTopic(topic));
    return () => {
      unsubscribers.forEach((unsubscribe) => {
        unsubscribe();
      });
    };
  }, [client, enabled, normalizedTopics.signature]);
};
