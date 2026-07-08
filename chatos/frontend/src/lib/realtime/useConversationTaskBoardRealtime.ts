// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useEffect, useRef } from 'react';

import { useRealtimeEvent, useRealtimeTopic } from './RealtimeProvider';
import type {
  RealtimeEventEnvelope,
  RealtimeTaskBoardPayloadWrapper,
} from './types';

interface UseConversationTaskBoardRealtimeOptions {
  sessionId?: string | null;
  enabled?: boolean;
  onEvent: (payload: RealtimeTaskBoardPayloadWrapper) => void | Promise<void>;
}

const isTaskBoardPayload = (
  envelope: RealtimeEventEnvelope,
): envelope is RealtimeEventEnvelope & { payload: RealtimeTaskBoardPayloadWrapper } => (
  envelope?.payload?.kind === 'task_board'
);

export const useConversationTaskBoardRealtime = ({
  sessionId,
  enabled = true,
  onEvent,
}: UseConversationTaskBoardRealtimeOptions) => {
  const onEventRef = useRef(onEvent);

  useEffect(() => {
    onEventRef.current = onEvent;
  }, [onEvent]);

  useRealtimeTopic(
    sessionId ? { scope: 'conversation', id: sessionId } : null,
    enabled && Boolean(sessionId),
  );

  useRealtimeEvent((event) => {
    if (!enabled || !sessionId || event.event !== 'conversation.task_board.updated') {
      return;
    }
    if (!isTaskBoardPayload(event)) {
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
