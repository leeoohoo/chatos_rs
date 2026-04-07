import { useMemo } from 'react';

import { readSessionImConversationId } from '../../lib/store/helpers/sessionRuntime';
import type { ImConversationRuntimeState } from '../../lib/store/types';

export type ImRuntimeState = ImConversationRuntimeState;

interface SessionLike {
  id: string;
  metadata?: unknown;
}

interface UseContactImRuntimeStateOptions {
  sessions: SessionLike[];
  displaySessions: SessionLike[];
  displayBackingSessionIdMap?: Record<string, string>;
  isCollapsed: boolean;
  imConversationRuntimeByConversationId: Record<string, ImConversationRuntimeState | undefined>;
}

const normalizeDisplaySessionRefs = (
  displaySessions: SessionLike[],
  sessions: SessionLike[],
  displayBackingSessionIdMap?: Record<string, string>,
): Array<{ displaySessionId: string; conversationId: string }> => {
  return (displaySessions || [])
    .map((displaySession) => {
      const displaySessionId = typeof displaySession?.id === 'string'
        ? displaySession.id.trim()
        : '';
      if (!displaySessionId) {
        return null;
      }
      const runtimeSessionId = typeof displayBackingSessionIdMap?.[displaySessionId] === 'string'
        ? displayBackingSessionIdMap[displaySessionId].trim()
        : '';
      const runtimeSession = runtimeSessionId
        ? sessions.find((session) => session.id === runtimeSessionId) || null
        : null;
      const conversationId = readSessionImConversationId(displaySession?.metadata)
        || readSessionImConversationId(runtimeSession?.metadata);
      if (!conversationId) {
        return null;
      }
      return {
        displaySessionId,
        conversationId,
      };
    })
    .filter(Boolean) as Array<{ displaySessionId: string; conversationId: string }>;
};

export const useContactImRuntimeState = ({
  sessions,
  displaySessions,
  displayBackingSessionIdMap,
  isCollapsed,
  imConversationRuntimeByConversationId,
}: UseContactImRuntimeStateOptions) => {
  const contactRuntimeSessionRefs = useMemo(
    () => normalizeDisplaySessionRefs(displaySessions, sessions, displayBackingSessionIdMap),
    [displayBackingSessionIdMap, displaySessions, sessions],
  );

  const imRuntimeStateByRuntimeSessionId = useMemo(() => {
    if (isCollapsed || contactRuntimeSessionRefs.length === 0) {
      return {};
    }

    return Object.fromEntries(
      contactRuntimeSessionRefs.map((ref) => ([
        ref.displaySessionId,
        imConversationRuntimeByConversationId[ref.conversationId] || {
          busy: false,
          unreadCount: 0,
          latestRunStatus: null,
          lastMessagePreview: null,
          lastMessageAt: null,
        },
      ])),
    );
  }, [
    contactRuntimeSessionRefs,
    imConversationRuntimeByConversationId,
    isCollapsed,
  ]);

  return {
    imRuntimeStateByRuntimeSessionId,
  };
};
