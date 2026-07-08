// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useEffect, useRef } from 'react';

import { useRealtimeEvent, useRealtimeTopic } from './RealtimeProvider';
import { useRealtimeInvalidationQueue } from './invalidationQueue';
import type {
  RealtimeEventEnvelope,
  RealtimeConversationSummariesUpdatedPayloadWrapper,
} from './types';

interface UseConversationSummariesRealtimeOptions {
  sessionId?: string | null;
  enabled?: boolean;
  onEvent: (payload: RealtimeConversationSummariesUpdatedPayloadWrapper) => void | Promise<void>;
}

const isConversationSummariesUpdatedPayload = (
  envelope: RealtimeEventEnvelope,
): envelope is RealtimeEventEnvelope & { payload: RealtimeConversationSummariesUpdatedPayloadWrapper } => (
  envelope?.payload?.kind === 'conversation_summaries_updated'
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

  const queue = useRealtimeInvalidationQueue<RealtimeConversationSummariesUpdatedPayloadWrapper>({
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
    if (!isConversationSummariesUpdatedPayload(event)) {
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
