import { useEffect, useRef } from 'react';

import { useRealtimeEvent, useRealtimeTopic } from './RealtimeProvider';
import { useRealtimeInvalidationQueue } from './invalidationQueue';
import type {
  RealtimeEventEnvelope,
  RealtimeNotepadUpdatedPayloadWrapper,
} from './types';

interface UseNotepadRealtimeOptions {
  enabled?: boolean;
  onInvalidate: (payload: RealtimeNotepadUpdatedPayloadWrapper) => void | Promise<void>;
}

const isNotepadUpdatedPayload = (
  envelope: RealtimeEventEnvelope,
): envelope is RealtimeEventEnvelope & { payload: RealtimeNotepadUpdatedPayloadWrapper } => (
  envelope?.payload?.kind === 'notepad_updated'
);

export const useNotepadRealtime = ({
  enabled = true,
  onInvalidate,
}: UseNotepadRealtimeOptions) => {
  const onInvalidateRef = useRef(onInvalidate);

  useEffect(() => {
    onInvalidateRef.current = onInvalidate;
  }, [onInvalidate]);

  const queue = useRealtimeInvalidationQueue<RealtimeNotepadUpdatedPayloadWrapper>({
    delayMs: 150,
    onExecute: (payload) => onInvalidateRef.current(payload),
  });

  useRealtimeTopic({ scope: 'notepad' }, enabled);

  useRealtimeEvent((event) => {
    if (!enabled || event.event !== 'notepad.updated') {
      return;
    }
    if (!isNotepadUpdatedPayload(event)) {
      return;
    }
    queue.run(event.payload);
  });
};
