// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { Session } from '../../../../types';
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
  SESSION_MESSAGES_INITIAL_PAGE_SIZE,
  syncCurrentProjectFromSession,
} from '../sessionsUtils';
import { applySelectSessionState } from '../sessionsSelectHelpers';
import { restoreSessionRuntimeState } from './runtimeRecovery';
import type { SessionActionDeps } from './types';

let latestSelectRequestSeq = 0;

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
      const requestSeq = ++latestSelectRequestSeq;
      const selectStartedAt = Date.now();
      const stopPerfMeasure = createPerfMeasureStopper(`store.selectSession.${sessionId}.${selectStartedAt}`);
      const beforeSelect = get();
      const previousSessionId = beforeSelect.currentSessionId;
      const requestedInitialPageSize = Number.isFinite(options.initialPageSize)
        ? Math.max(1, Math.floor(options.initialPageSize as number))
        : SESSION_MESSAGES_INITIAL_PAGE_SIZE;
      const sameSessionState = beforeSelect.sessionChatState?.[sessionId];
      if (beforeSelect.currentSessionId === sessionId && sameSessionState?.isStreaming) {
        if (!options.keepActivePanel && beforeSelect.activePanel !== 'chat') {
          set((state: ChatStoreDraft) => {
            state.activePanel = 'chat';
          });
        }
        debugLog('🔍 当前会话正在流式中，忽略重复切换请求:', sessionId);
        return;
      }

      try {
        const existingSession = (beforeSelect.sessions || [])
          .find((item: Session) => item.id === sessionId) || null;

        set((state: ChatStoreDraft) => {
          state.isLoading = true;
          state.error = null;
          state.messages = [];
          state.hasMoreMessages = false;
          if (!state.sessionMessagePaginationState) {
            state.sessionMessagePaginationState = {};
          }
          state.sessionMessagePaginationState[sessionId] = {
            nextBefore: null,
            loaded: false,
          };
          if (existingSession) {
            state.currentSessionId = sessionId;
            state.currentSession = existingSession;
            syncCurrentProjectFromSession(state, existingSession);
            if (!options.keepActivePanel) {
              state.activePanel = 'chat';
            }
          }
          const previousChatState = state.sessionChatState[sessionId];
          state.sessionChatState[sessionId] = {
            isLoading: true,
            isStreaming: previousChatState?.isStreaming ?? false,
            isStopping: previousChatState?.isStopping ?? false,
            streamingPhase: previousChatState?.streamingPhase ?? null,
            streamingMessageId: previousChatState?.streamingMessageId ?? null,
            activeTurnId: previousChatState?.activeTurnId ?? null,
            streamingPreviewText: previousChatState?.streamingPreviewText ?? '',
            streamingTransport: previousChatState?.streamingTransport ?? null,
            runtimeContextRefreshNonce: previousChatState?.runtimeContextRefreshNonce ?? 0,
          };
        });

        const [session, messageResult] = await Promise.all([
          existingSession ? Promise.resolve(existingSession) : fetchSession(client, sessionId),
          fetchSessionMessages(client, sessionId, {
            limit: requestedInitialPageSize,
            before: null,
          }),
        ]);

        if (requestSeq !== latestSelectRequestSeq) {
          debugLog('[Store] selectSession ignored stale result', {
            sessionId,
            previousSessionId,
            elapsedMs: Date.now() - selectStartedAt,
          });
          return;
        }

        const sessionAiSelectionFromMetadata = readSessionAiSelectionFromMetadata(session?.metadata);
        set((state: ChatStoreDraft) => {
          const currentChatState = state.sessionChatState[sessionId];
          state.sessionChatState[sessionId] = {
            ...currentChatState,
            isLoading: Boolean(currentChatState?.isStreaming || currentChatState?.isStopping),
          };
          applySelectSessionState({
            state,
            sessionId,
            session,
            messages: messageResult.messages,
            previousSessionId,
            sessionAiSelectionFromMetadata,
            keepActivePanel: options.keepActivePanel,
          });
          state.sessionMessagePaginationState[sessionId] = {
            nextBefore: messageResult.nextBefore,
            loaded: true,
          };
          state.hasMoreMessages = Boolean(messageResult.nextBefore);
        });
        void restoreSessionRuntimeState({ client, set, get, sessionId });

        if (session) {
          const { userId, projectId } = getSessionParams();
          if (typeof localStorage !== 'undefined') {
            localStorage.setItem(`lastSessionId_${userId}_${projectId}`, sessionId);
            debugLog('🔍 保存会话ID到 localStorage:', sessionId);
          }
        }
        debugLog('[Store] selectSession completed without message cache', {
          sessionId,
          previousSessionId,
          messageCount: messageResult.messages.length,
          perfMs: stopPerfMeasure() ?? null,
          elapsedMs: Date.now() - selectStartedAt,
        });
      } catch (error) {
        if (requestSeq !== latestSelectRequestSeq) {
          return;
        }
        console.error('Failed to select session:', error);
        debugLog('[Store] selectSession failed', {
          sessionId,
          previousSessionId,
          perfMs: stopPerfMeasure() ?? null,
          elapsedMs: Date.now() - selectStartedAt,
          error: error instanceof Error ? error.message : String(error),
        });
        set((state: ChatStoreDraft) => {
          const currentChatState = state.sessionChatState?.[sessionId];
          if (currentChatState) {
            state.sessionChatState[sessionId] = {
              ...currentChatState,
              isLoading: false,
            };
          }
          if (state.currentSessionId === sessionId || !state.currentSessionId) {
            state.error = error instanceof Error ? error.message : 'Failed to select session';
            state.isLoading = false;
          }
        });
      }
    },
  };
}
