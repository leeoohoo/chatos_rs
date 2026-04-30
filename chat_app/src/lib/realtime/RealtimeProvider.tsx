import React, { createContext, useContext, useEffect, useMemo, useRef, useState } from 'react';

import { apiClient } from '../api/client';
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
const RealtimeStateContext = createContext<RealtimeStateContextValue | null>(null);

interface RealtimeProviderProps {
  children: React.ReactNode;
  accessToken?: string | null;
}

export const RealtimeProvider: React.FC<RealtimeProviderProps> = ({
  children,
  accessToken,
}) => {
  const clientRef = useRef<RealtimeClient | null>(null);
  if (!clientRef.current) {
    clientRef.current = new RealtimeClient(apiClient.getBaseUrl());
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
  }, [client]);

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

  const stateValue = useMemo<RealtimeStateContextValue>(() => ({
    connectionState,
    debugSnapshot,
  }), [connectionState, debugSnapshot]);

  return (
    <RealtimeClientContext.Provider value={client}>
      <RealtimeStateContext.Provider value={stateValue}>
        {children}
      </RealtimeStateContext.Provider>
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

const useRealtimeStateContext = (): RealtimeStateContextValue => {
  const context = useContext(RealtimeStateContext);
  if (!context) {
    throw new Error('useRealtimeStateContext must be used within a RealtimeProvider');
  }
  return context;
};

export const useRealtimeContext = (): RealtimeContextValue => {
  const client = useRealtimeClient();
  const context = useRealtimeStateContext();
  return {
    client,
    ...context,
  };
};

export const useRealtimeConnectionState = (): RealtimeConnectionState => {
  return useRealtimeStateContext().connectionState;
};

export { getRealtimeConnectionStateSnapshot } from './state';

export const useRealtimeDebugSnapshot = (): RealtimeDebugSnapshot => {
  return useRealtimeStateContext().debugSnapshot;
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
