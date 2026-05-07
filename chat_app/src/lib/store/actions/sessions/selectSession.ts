import type { Message, Session } from '../../../../types';
import { debugLog } from '@/lib/utils';
import { fetchSession } from '../../helpers/sessions';
import { fetchSessionMessages } from '../../helpers/messages';
import { readSessionAiSelectionFromMetadata } from '../../helpers/sessionAiSelection';
import type {
  ChatStoreDraft,
  SessionSelectOptions,
} from '../../types';
import {
  createPerfMeasureStopper,
  writeSessionMessagesCache,
} from '../sessionsUtils';
import { applySelectSessionState } from '../sessionsSelectHelpers';
import type { SessionActionDeps } from './types';

export function createSelectSessionActions({
  set,
  get,
  client,
  getSessionParams,
}: SessionActionDeps) {
  return {
    selectSession: async (
      sessionId: string,
      options: SessionSelectOptions = {},
    ) => {
      const selectStartedAt = Date.now();
      const stopPerfMeasure = createPerfMeasureStopper(`store.selectSession.${sessionId}.${selectStartedAt}`);
      const beforeSelect = get();
      const previousSessionId = beforeSelect.currentSessionId;
      const sameSessionState = beforeSelect.sessionChatState?.[sessionId];
      if (beforeSelect.currentSessionId === sessionId && sameSessionState?.isStreaming) {
        // 同一会话流式过程中仍允许切回聊天面板，避免在项目/终端面板点击会话无响应
        if (!options.keepActivePanel && beforeSelect.activePanel !== 'chat') {
          set((state: ChatStoreDraft) => {
            state.activePanel = 'chat';
          });
        }
        debugLog('🔍 当前会话正在流式中，忽略重复切换请求:', sessionId);
        return;
      }

      try {
        set((state: ChatStoreDraft) => {
          state.isLoading = true;
          state.error = null;
        });

        const existingSession = (beforeSelect.sessions || []).find((item: Session) => item.id === sessionId) || null;
        const [session, messages] = await Promise.all([
          existingSession ? Promise.resolve(existingSession) : fetchSession(client, sessionId),
          fetchSessionMessages(client, sessionId, { limit: 50, offset: 0 }),
        ]);
        writeSessionMessagesCache(sessionId, messages);
        const sessionAiSelectionFromMetadata = readSessionAiSelectionFromMetadata(session?.metadata);
        const stateSnapshot = get();
        const snapshotChatState = stateSnapshot.sessionChatState?.[sessionId];
        const localStreamingMessage = snapshotChatState?.streamingMessageId
          ? stateSnapshot.messages.find((message: Message) => (
            message.id === snapshotChatState.streamingMessageId && message.sessionId === sessionId
          )) ?? null
          : null;

        set((state: ChatStoreDraft) => {
          applySelectSessionState({
            state,
            sessionId,
            session,
            messages,
            previousSessionId,
            localStreamingMessage,
            sessionAiSelectionFromMetadata,
            keepActivePanel: options.keepActivePanel,
          });
        });

        if (session) {
          const { userId, projectId } = getSessionParams();
          localStorage.setItem(`lastSessionId_${userId}_${projectId}`, sessionId);
          debugLog('🔍 保存会话ID到 localStorage:', sessionId);
        }
        const latestMessagesForSession = (get().messages || []).filter((message: Message) => message?.sessionId === sessionId);
        if (latestMessagesForSession.length > 0) {
          writeSessionMessagesCache(sessionId, latestMessagesForSession);
        } else {
          writeSessionMessagesCache(sessionId, messages);
        }
        debugLog('[Store] selectSession completed', {
          sessionId,
          previousSessionId,
          messageCount: messages.length,
          cacheHit: false,
          perfMs: stopPerfMeasure() ?? null,
          elapsedMs: Date.now() - selectStartedAt,
        });
      } catch (error) {
        console.error('Failed to select session:', error);
        debugLog('[Store] selectSession failed', {
          sessionId,
          previousSessionId,
          perfMs: stopPerfMeasure() ?? null,
          elapsedMs: Date.now() - selectStartedAt,
          error: error instanceof Error ? error.message : String(error),
        });
        set((state: ChatStoreDraft) => {
          state.error = error instanceof Error ? error.message : 'Failed to select session';
          state.isLoading = false;
        });
      }
    },
  };
}
