import { useEffect, useRef } from 'react';

import { useRealtimeEvent, useRealtimeTopic } from './RealtimeProvider';
import { useRealtimeInvalidationQueue } from './invalidationQueue';
import type {
  RealtimeEventEnvelope,
  RealtimeProjectsUpdatedPayloadWrapper,
} from './types';

interface UseProjectsRealtimeOptions {
  enabled?: boolean;
  onInvalidate: (payload: RealtimeProjectsUpdatedPayloadWrapper) => void | Promise<void>;
}

const isProjectsUpdatedPayload = (
  envelope: RealtimeEventEnvelope,
): envelope is RealtimeEventEnvelope & { payload: RealtimeProjectsUpdatedPayloadWrapper } => {
  return envelope?.payload?.kind === 'projects_updated';
};

export const useProjectsRealtime = ({
  enabled = true,
  onInvalidate,
}: UseProjectsRealtimeOptions) => {
  const onInvalidateRef = useRef(onInvalidate);

  useEffect(() => {
    onInvalidateRef.current = onInvalidate;
  }, [onInvalidate]);

  const queue = useRealtimeInvalidationQueue<RealtimeProjectsUpdatedPayloadWrapper>({
    delayMs: 250,
    onExecute: (payload) => onInvalidateRef.current(payload),
  });

  useRealtimeTopic({ scope: 'projects' }, enabled);

  useRealtimeEvent((event) => {
    if (!enabled || event.event !== 'projects.updated') {
      return;
    }
    if (!isProjectsUpdatedPayload(event)) {
      return;
    }
    queue.run(event.payload);
  });
};
