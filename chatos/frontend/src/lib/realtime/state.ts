// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { RealtimeConnectionState, RealtimeTopic } from './types';

type RealtimeConnectionStateListener = (state: RealtimeConnectionState) => void;

let realtimeConnectionStateSnapshot: RealtimeConnectionState = 'idle';
const realtimeConnectionStateListeners = new Set<RealtimeConnectionStateListener>();
const realtimeTopicAckListeners = new Set<() => void>();
const acknowledgedRealtimeTopicKeys = new Set<string>();
const WAITABLE_REALTIME_CONNECTION_STATES = new Set<RealtimeConnectionState>([
  'idle',
  'connecting',
  'disconnected',
  'error',
]);

const realtimeTopicKey = (topic: RealtimeTopic): string => {
  const scope = String(topic.scope || '').trim();
  const id = typeof topic.id === 'string' ? topic.id.trim() : '';
  return `${scope}:${id}`;
};

const notifyRealtimeTopicAckListeners = () => {
  realtimeTopicAckListeners.forEach((listener) => {
    try {
      listener();
    } catch (error) {
      console.error('Realtime topic acknowledgement listener failed:', error);
    }
  });
};

export const applyRealtimeTopicAckSnapshot = (
  acked: 'subscribe' | 'unsubscribe',
  topics: RealtimeTopic[],
): void => {
  topics.forEach((topic) => {
    const key = realtimeTopicKey(topic);
    if (acked === 'subscribe') {
      acknowledgedRealtimeTopicKeys.add(key);
    } else {
      acknowledgedRealtimeTopicKeys.delete(key);
    }
  });
  notifyRealtimeTopicAckListeners();
};

export const clearRealtimeTopicAckSnapshot = (): void => {
  if (acknowledgedRealtimeTopicKeys.size === 0) {
    return;
  }
  acknowledgedRealtimeTopicKeys.clear();
  notifyRealtimeTopicAckListeners();
};

export const hasRealtimeTopicAckSnapshot = (topic: RealtimeTopic): boolean => (
  acknowledgedRealtimeTopicKeys.has(realtimeTopicKey(topic))
);

export const waitForRealtimeTopicAckSnapshot = async (
  topic: RealtimeTopic,
  timeoutMs: number,
): Promise<boolean> => {
  if (hasRealtimeTopicAckSnapshot(topic)) {
    return true;
  }
  if (timeoutMs <= 0) {
    return false;
  }
  return new Promise<boolean>((resolve) => {
    let settled = false;
    let unsubscribe = () => {};
    let timer: ReturnType<typeof setTimeout> | null = null;
    const settle = (value: boolean) => {
      if (settled) {
        return;
      }
      settled = true;
      if (timer) {
        clearTimeout(timer);
        timer = null;
      }
      unsubscribe();
      resolve(value);
    };
    const listener = () => {
      if (hasRealtimeTopicAckSnapshot(topic)) {
        settle(true);
      }
    };
    realtimeTopicAckListeners.add(listener);
    unsubscribe = () => {
      realtimeTopicAckListeners.delete(listener);
    };
    timer = setTimeout(() => {
      settle(hasRealtimeTopicAckSnapshot(topic));
    }, timeoutMs);
    listener();
  });
};

export const getRealtimeConnectionStateSnapshot = (): RealtimeConnectionState => {
  return realtimeConnectionStateSnapshot;
};

export const subscribeRealtimeConnectionStateSnapshot = (
  listener: RealtimeConnectionStateListener,
): (() => void) => {
  realtimeConnectionStateListeners.add(listener);
  return () => {
    realtimeConnectionStateListeners.delete(listener);
  };
};

export const waitForRealtimeConnectedSnapshot = async (
  timeoutMs: number,
): Promise<boolean> => {
  const currentState = getRealtimeConnectionStateSnapshot();
  if (currentState === 'connected') {
    return true;
  }
  if (timeoutMs <= 0 || !WAITABLE_REALTIME_CONNECTION_STATES.has(currentState)) {
    return false;
  }

  return new Promise<boolean>((resolve) => {
    let settled = false;
    let timer: ReturnType<typeof setTimeout> | null = null;

    const settle = (value: boolean) => {
      if (settled) {
        return;
      }
      settled = true;
      if (timer) {
        clearTimeout(timer);
        timer = null;
      }
      unsubscribe();
      resolve(value);
    };

    const unsubscribe = subscribeRealtimeConnectionStateSnapshot((state) => {
      if (state === 'connected') {
        settle(true);
        return;
      }
      if (!WAITABLE_REALTIME_CONNECTION_STATES.has(state)) {
        settle(false);
      }
    });

    const latestState = getRealtimeConnectionStateSnapshot();
    if (latestState === 'connected') {
      settle(true);
      return;
    }
    if (!WAITABLE_REALTIME_CONNECTION_STATES.has(latestState)) {
      settle(false);
      return;
    }

    timer = setTimeout(() => {
      settle(getRealtimeConnectionStateSnapshot() === 'connected');
    }, timeoutMs);
  });
};

export const setRealtimeConnectionStateSnapshot = (
  state: RealtimeConnectionState,
): void => {
  if (state !== 'connected') {
    clearRealtimeTopicAckSnapshot();
  }
  realtimeConnectionStateSnapshot = state;
  realtimeConnectionStateListeners.forEach((listener) => {
    try {
      listener(state);
    } catch (error) {
      console.error('Realtime connection state snapshot listener failed:', error);
    }
  });
};
