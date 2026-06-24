import { useEffect, useRef } from 'react';

import { useRealtimeEvent, useRealtimeTopic } from './RealtimeProvider';
import type {
  RealtimeEventEnvelope,
  RealtimeAskUserPromptPayloadWrapper,
} from './types';

interface UseConversationAskUserPromptRealtimeOptions {
  sessionId?: string | null;
  enabled?: boolean;
  onEvent: (payload: RealtimeAskUserPromptPayloadWrapper) => void | Promise<void>;
}

const isAskUserPromptPayload = (
  envelope: RealtimeEventEnvelope,
): envelope is RealtimeEventEnvelope & { payload: RealtimeAskUserPromptPayloadWrapper } => (
  envelope?.payload?.kind === 'ask_user_prompt'
);

export const useConversationAskUserPromptRealtime = ({
  sessionId,
  enabled = true,
  onEvent,
}: UseConversationAskUserPromptRealtimeOptions) => {
  const onEventRef = useRef(onEvent);

  useEffect(() => {
    onEventRef.current = onEvent;
  }, [onEvent]);

  useRealtimeTopic(
    sessionId ? { scope: 'conversation', id: sessionId } : null,
    enabled && Boolean(sessionId),
  );

  useRealtimeEvent((event) => {
    if (!enabled || !sessionId || event.event !== 'conversation.ask_user_prompt.updated') {
      return;
    }
    if (!isAskUserPromptPayload(event)) {
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
