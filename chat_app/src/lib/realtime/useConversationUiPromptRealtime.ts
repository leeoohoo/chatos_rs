import { useEffect, useRef } from 'react';

import { useRealtimeEvent, useRealtimeTopic } from './RealtimeProvider';
import type {
  RealtimeEventEnvelope,
  RealtimeUiPromptPayloadWrapper,
} from './types';

interface UseConversationUiPromptRealtimeOptions {
  sessionId?: string | null;
  enabled?: boolean;
  onEvent: (payload: RealtimeUiPromptPayloadWrapper) => void | Promise<void>;
}

const isUiPromptPayload = (
  envelope: RealtimeEventEnvelope,
): envelope is RealtimeEventEnvelope & { payload: RealtimeUiPromptPayloadWrapper } => (
  envelope?.payload?.kind === 'ui_prompt'
);

export const useConversationUiPromptRealtime = ({
  sessionId,
  enabled = true,
  onEvent,
}: UseConversationUiPromptRealtimeOptions) => {
  const onEventRef = useRef(onEvent);

  useEffect(() => {
    onEventRef.current = onEvent;
  }, [onEvent]);

  useRealtimeTopic(
    sessionId ? { scope: 'conversation', id: sessionId } : null,
    enabled && Boolean(sessionId),
  );

  useRealtimeEvent((event) => {
    if (!enabled || !sessionId || event.event !== 'conversation.ui_prompt.updated') {
      return;
    }
    if (!isUiPromptPayload(event)) {
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
    void onEventRef.current(event.payload);
  });
};
