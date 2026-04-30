import { useEffect, useRef } from 'react';

import { useRealtimeEvent, useRealtimeTopic } from './RealtimeProvider';
import type {
  RealtimeChatStreamPayloadWrapper,
  RealtimeEventEnvelope,
} from './types';

interface UseConversationChatStreamRealtimeOptions {
  sessionId?: string | null;
  enabled?: boolean;
  onEvent: (
    payload: RealtimeChatStreamPayloadWrapper,
    eventName: string,
  ) => void | Promise<void>;
}

const isChatStreamPayload = (
  envelope: RealtimeEventEnvelope,
): envelope is RealtimeEventEnvelope & { payload: RealtimeChatStreamPayloadWrapper } => (
  envelope?.payload?.kind === 'chat_stream'
);

export const useConversationChatStreamRealtime = ({
  sessionId,
  enabled = true,
  onEvent,
}: UseConversationChatStreamRealtimeOptions) => {
  const onEventRef = useRef(onEvent);

  useEffect(() => {
    onEventRef.current = onEvent;
  }, [onEvent]);

  useRealtimeTopic(
    sessionId ? { scope: 'conversation', id: sessionId } : null,
    enabled && Boolean(sessionId),
  );

  useRealtimeEvent((event) => {
    if (!enabled) {
      return;
    }
    if (!String(event.event || '').startsWith('chat.')) {
      return;
    }
    if (!isChatStreamPayload(event)) {
      return;
    }
    const payloadSessionId = String(
      event.conversation_id
      || event.payload.conversation_id
      || '',
    ).trim();
    if (!payloadSessionId) {
      return;
    }
    if (sessionId && payloadSessionId !== sessionId) {
      return;
    }
    void onEventRef.current(event.payload, event.event);
  });
};
