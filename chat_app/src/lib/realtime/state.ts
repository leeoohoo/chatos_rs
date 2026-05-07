import type { RealtimeConnectionState } from './types';

type RealtimeConnectionStateListener = (state: RealtimeConnectionState) => void;

let realtimeConnectionStateSnapshot: RealtimeConnectionState = 'idle';
const realtimeConnectionStateListeners = new Set<RealtimeConnectionStateListener>();
const WAITABLE_REALTIME_CONNECTION_STATES = new Set<RealtimeConnectionState>([
  'idle',
  'connecting',
  'disconnected',
  'error',
]);

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
  realtimeConnectionStateSnapshot = state;
  realtimeConnectionStateListeners.forEach((listener) => {
    try {
      listener(state);
    } catch (error) {
      console.error('Realtime connection state snapshot listener failed:', error);
    }
  });
};
