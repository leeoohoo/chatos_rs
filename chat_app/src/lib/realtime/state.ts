import type { RealtimeConnectionState } from './types';

let realtimeConnectionStateSnapshot: RealtimeConnectionState = 'idle';

export const getRealtimeConnectionStateSnapshot = (): RealtimeConnectionState => {
  return realtimeConnectionStateSnapshot;
};

export const setRealtimeConnectionStateSnapshot = (
  state: RealtimeConnectionState,
): void => {
  realtimeConnectionStateSnapshot = state;
};
