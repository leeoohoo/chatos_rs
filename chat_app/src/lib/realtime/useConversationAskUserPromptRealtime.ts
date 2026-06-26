import { useEffect, useRef } from 'react';

import { useRealtimeEvent, useRealtimeTopics } from './RealtimeProvider';
import type {
  RealtimeEventEnvelope,
  RealtimeAskUserPromptPayloadWrapper,
} from './types';

interface UseConversationAskUserPromptRealtimeOptions {
  sessionId?: string | null;
  projectId?: string | null;
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
  projectId,
  enabled = true,
  onEvent,
}: UseConversationAskUserPromptRealtimeOptions) => {
  const onEventRef = useRef(onEvent);

  useEffect(() => {
    onEventRef.current = onEvent;
  }, [onEvent]);

  useRealtimeTopics(
    [
      sessionId ? { scope: 'conversation', id: sessionId } : null,
      projectId ? { scope: 'project', id: projectId } : null,
    ],
    enabled && (Boolean(sessionId) || Boolean(projectId)),
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
    if (projectId) {
      const payloadProjectId = String(
        event.project_id
        || event.payload.project_id
        || '',
      ).trim();
      if (payloadProjectId && payloadProjectId !== projectId) {
        return;
      }
    }
    void onEventRef.current(event.payload);
  });
};
