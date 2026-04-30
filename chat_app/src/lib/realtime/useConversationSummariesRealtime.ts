import { useEffect, useRef } from 'react';

import { useRealtimeEvent, useRealtimeTopic } from './RealtimeProvider';
import { useRealtimeInvalidationQueue } from './invalidationQueue';
import type {
  RealtimeEventEnvelope,
  ReviewRepairRealtimePayload,
} from './types';

interface UseConversationSummariesRealtimeOptions {
  sessionId?: string | null;
  enabled?: boolean;
  onEvent: (payload: ReviewRepairRealtimePayload) => void | Promise<void>;
}

const isReviewRepairPayload = (
  envelope: RealtimeEventEnvelope,
): envelope is RealtimeEventEnvelope & { payload: ReviewRepairRealtimePayload & { kind: 'review_repair' } } => (
  envelope?.payload?.kind === 'review_repair'
);

export const useConversationSummariesRealtime = ({
  sessionId,
  enabled = true,
  onEvent,
}: UseConversationSummariesRealtimeOptions) => {
  const onEventRef = useRef(onEvent);

  useEffect(() => {
    onEventRef.current = onEvent;
  }, [onEvent]);

  const queue = useRealtimeInvalidationQueue<ReviewRepairRealtimePayload>({
    delayMs: 200,
    onExecute: (payload) => onEventRef.current(payload),
  });

  useRealtimeTopic(
    sessionId ? { scope: 'conversation', id: sessionId } : null,
    enabled && Boolean(sessionId),
  );

  useRealtimeEvent((event) => {
    if (!enabled || !sessionId || event.event !== 'conversation.summaries.updated') {
      return;
    }
    if (!isReviewRepairPayload(event)) {
      return;
    }
    const payloadSessionId = String(
      event.conversation_id
      || event.payload.conversation_id
      || '',
    ).trim();
    if (!payloadSessionId || payloadSessionId !== sessionId) {
      return;
    }
    queue.run(event.payload);
  });
};
