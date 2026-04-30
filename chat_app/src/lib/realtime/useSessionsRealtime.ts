import { useEffect, useRef } from 'react';

import { useRealtimeEvent, useRealtimeTopic } from './RealtimeProvider';
import { useRealtimeInvalidationQueue } from './invalidationQueue';
import type {
  RealtimeEventEnvelope,
  RealtimeSessionsUpdatedPayloadWrapper,
} from './types';

interface UseSessionsRealtimeOptions {
  enabled?: boolean;
  onInvalidate: (payload: RealtimeSessionsUpdatedPayloadWrapper) => void | Promise<void>;
}

const isSessionsUpdatedPayload = (
  envelope: RealtimeEventEnvelope,
): envelope is RealtimeEventEnvelope & { payload: RealtimeSessionsUpdatedPayloadWrapper } => {
  return envelope?.payload?.kind === 'sessions_updated';
};

export const useSessionsRealtime = ({
  enabled = true,
  onInvalidate,
}: UseSessionsRealtimeOptions) => {
  const onInvalidateRef = useRef(onInvalidate);

  useEffect(() => {
    onInvalidateRef.current = onInvalidate;
  }, [onInvalidate]);

  const queue = useRealtimeInvalidationQueue<RealtimeSessionsUpdatedPayloadWrapper>({
    delayMs: 250,
    onExecute: (payload) => onInvalidateRef.current(payload),
  });

  useRealtimeTopic({ scope: 'sessions' }, enabled);

  useRealtimeEvent((event) => {
    if (!enabled || event.event !== 'sessions.updated') {
      return;
    }
    if (!isSessionsUpdatedPayload(event)) {
      return;
    }
    queue.run(event.payload);
  });
};
