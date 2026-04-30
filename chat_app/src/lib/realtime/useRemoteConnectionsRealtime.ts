import { useEffect, useRef } from 'react';

import { useRealtimeEvent, useRealtimeTopic } from './RealtimeProvider';
import { useRealtimeInvalidationQueue } from './invalidationQueue';
import type {
  RealtimeEventEnvelope,
  RealtimeRemoteConnectionsUpdatedPayloadWrapper,
} from './types';

interface UseRemoteConnectionsRealtimeOptions {
  enabled?: boolean;
  onInvalidate: (payload: RealtimeRemoteConnectionsUpdatedPayloadWrapper) => void | Promise<void>;
}

const isRemoteConnectionsUpdatedPayload = (
  envelope: RealtimeEventEnvelope,
): envelope is RealtimeEventEnvelope & { payload: RealtimeRemoteConnectionsUpdatedPayloadWrapper } => {
  return envelope?.payload?.kind === 'remote_connections_updated';
};

export const useRemoteConnectionsRealtime = ({
  enabled = true,
  onInvalidate,
}: UseRemoteConnectionsRealtimeOptions) => {
  const onInvalidateRef = useRef(onInvalidate);

  useEffect(() => {
    onInvalidateRef.current = onInvalidate;
  }, [onInvalidate]);

  const queue = useRealtimeInvalidationQueue<RealtimeRemoteConnectionsUpdatedPayloadWrapper>({
    delayMs: 250,
    onExecute: (payload) => onInvalidateRef.current(payload),
  });

  useRealtimeTopic({ scope: 'remote_connections' }, enabled);

  useRealtimeEvent((event) => {
    if (!enabled || event.event !== 'remote_connections.updated') {
      return;
    }
    if (!isRemoteConnectionsUpdatedPayload(event)) {
      return;
    }
    queue.run(event.payload);
  });
};
