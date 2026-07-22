// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { Message, Session } from '../../../../types';
import { debugLog } from '@/lib/utils';
import { getRealtimeConnectionStateSnapshot } from '../../../realtime/state';
import { fetchSession } from '../../helpers/sessions';
import { fetchSessionMessages } from '../../helpers/messages';
import { readSessionAiSelectionFromMetadata } from '../../helpers/sessionAiSelection';
import type {
  ChatStoreDraft,
  SessionSelectOptions,
} from '../../types';
import {
  createPerfMeasureStopper,
  extractCompactHistoryMessages,
  mergeLatestCompactHistorySnapshot,
  isSessionMessagesCacheFresh,
  readSessionMessagesCache,
  readVisibleSessionMessagesSnapshot,
  resolveSessionTimestamp,
  resolveSessionProjectScopeId,
  SESSION_MESSAGES_INITIAL_PAGE_SIZE,
  touchSessionMessagesCacheEntry,
  trimCompactHistorySnapshotToRecent,
  writeSessionMessagesCache,
} from '../sessionsUtils';
import { applySelectSessionState } from '../sessionsSelectHelpers';
import { restoreSessionRuntimeState } from './runtimeRecovery';
import type { SessionActionDeps } from './types';

let latestSelectRequestSeq = 0;
const SESSION_MESSAGES_BACKGROUND_SYNC_MAX_AGE_MS = 30_000;

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
      if (previousSessionId && previousSessionId !== sessionId) {
        const previousVisibleSnapshot = readVisibleSessionMessagesSnapshot(beforeSelect, previousSessionId);
        if (previousVisibleSnapshot) {
          set((state: ChatStoreDraft) => {
            writeSessionMessagesCache(state, previousSessionId, previousVisibleSnapshot);
          });
        }
      }
      const requestedInitialPageSize = Number.isFinite(options.initialPageSize)
        ? Math.max(1, Math.floor(options.initialPageSize as number))
        : SESSION_MESSAGES_INITIAL_PAGE_SIZE;
      const forceRefreshMessages = options.forceRefreshMessages === true;
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
        const existingSession = (beforeSelect.sessions || []).find((item: Session) => item.id === sessionId) || null;
        const visibleSnapshot = forceRefreshMessages ? null : readVisibleSessionMessagesSnapshot(get(), sessionId);
        const cachedPage = forceRefreshMessages ? null : readSessionMessagesCache(get(), sessionId);
        const sessionSnapshot = trimCompactHistorySnapshotToRecent(
          visibleSnapshot ?? cachedPage,
          requestedInitialPageSize,
        );
        const hasImmediateSnapshot = Boolean(existingSession && sessionSnapshot);

        set((state: ChatStoreDraft) => {
          state.isLoading = !hasImmediateSnapshot;
          state.error = null;
        });

        if (existingSession) {
          const sessionProjectId = resolveSessionProjectScopeId(existingSession);
          set((state: ChatStoreDraft) => {
            state.currentSessionId = sessionId;
            state.currentSession = existingSession;
            if (!options.keepActivePanel) {
              state.activePanel = 'chat';
            }
            if (!state.sessionChatState[sessionId]) {
              state.sessionChatState[sessionId] = {
                isLoading: !hasImmediateSnapshot,
                isStreaming: false,
                isStopping: false,
                streamingMessageId: null,
                activeTurnId: null,
                streamingPreviewText: '',
                streamingTransport: null,
                runtimeContextRefreshNonce: 0,
              };
            } else {
              state.sessionChatState[sessionId] = {
                ...state.sessionChatState[sessionId],
                isLoading: !hasImmediateSnapshot,
              };
            }

            if (sessionProjectId === '0') {
              state.currentProjectId = null;
              state.currentProject = null;
            } else if (sessionProjectId) {
              state.currentProjectId = sessionProjectId;
              const matchedProject = (state.projects || []).find((project) => project.id === sessionProjectId) || null;
              state.currentProject = matchedProject;
            }
          });
        }
        if (!sessionSnapshot && existingSession) {
          set((state: ChatStoreDraft) => {
            state.messages = [];
            state.hasMoreMessages = false;
            state.isStreaming = state.sessionChatState?.[sessionId]?.isStreaming ?? false;
            state.streamingMessageId = state.sessionChatState?.[sessionId]?.streamingMessageId ?? null;
            if (!state.sessionMessagePaginationState) {
              state.sessionMessagePaginationState = {};
            }
            state.sessionMessagePaginationState[sessionId] = {
              nextBefore: null,
              loaded: false,
            };
          });
        }
        if (sessionSnapshot && existingSession) {
          if (!visibleSnapshot && cachedPage) {
            set((state: ChatStoreDraft) => {
              touchSessionMessagesCacheEntry(state, sessionId);
            });
          }
          const cachedSessionAiSelectionFromMetadata = readSessionAiSelectionFromMetadata(existingSession?.metadata);

          set((state: ChatStoreDraft) => {
            applySelectSessionState({
              state,
              sessionId,
              session: existingSession,
              messages: sessionSnapshot.messages,
              previousSessionId,
              sessionAiSelectionFromMetadata: cachedSessionAiSelectionFromMetadata,
              keepActivePanel: options.keepActivePanel,
            });
            if (!state.sessionMessagePaginationState) {
              state.sessionMessagePaginationState = {};
            }
            state.sessionMessagePaginationState[sessionId] = {
              nextBefore: sessionSnapshot.nextBefore,
              loaded: sessionSnapshot.loaded,
            };
            state.hasMoreMessages = Boolean(sessionSnapshot.nextBefore);
            const currentChatState = state.sessionChatState?.[sessionId];
            if (currentChatState) {
              state.sessionChatState[sessionId] = {
                ...currentChatState,
                isLoading: Boolean(currentChatState.isStreaming || currentChatState.isStopping),
              };
            }
            state.isLoading = false;
          });
          const shouldBackgroundSync = (() => {
            if (options.skipBackgroundSync) {
              return false;
            }
            if (getRealtimeConnectionStateSnapshot() !== 'connected') {
              return true;
            }
            const sessionUpdatedAt = resolveSessionTimestamp(existingSession);
            return !isSessionMessagesCacheFresh(get(), sessionId, {
              minFetchedAt: sessionUpdatedAt,
              maxAgeMs: SESSION_MESSAGES_BACKGROUND_SYNC_MAX_AGE_MS,
            });
          })();
          if (shouldBackgroundSync) {
            void get().syncSessionMessagesInBackground(sessionId);
          }
          void restoreSessionRuntimeState({ client, set, get, sessionId });
          debugLog('[Store] selectSession served from cache', {
            sessionId,
            previousSessionId,
            messageCount: sessionSnapshot.messages.length,
            nextBefore: sessionSnapshot.nextBefore,
            backgroundSync: shouldBackgroundSync,
          });
          return;
        }

        const [session, messageResult] = await Promise.all([
          existingSession ? Promise.resolve(existingSession) : fetchSession(client, sessionId),
          fetchSessionMessages(client, sessionId, {
            limit: requestedInitialPageSize,
            before: null,
          }),
        ]);
        const mergedSnapshot = mergeLatestCompactHistorySnapshot(
          messageResult.messages,
          messageResult.nextBefore,
          sessionSnapshot,
        );
        const messages = mergedSnapshot.messages;
        const effectiveNextBefore = mergedSnapshot.nextBefore;
        if (requestSeq !== latestSelectRequestSeq) {
          debugLog('[Store] selectSession ignored stale result', {
            sessionId,
            previousSessionId,
            elapsedMs: Date.now() - selectStartedAt,
          });
          return;
        }
        set((state) => {
          writeSessionMessagesCache(state, sessionId, {
            messages,
            nextBefore: effectiveNextBefore,
            loaded: true,
          });
        });
        const sessionAiSelectionFromMetadata = readSessionAiSelectionFromMetadata(session?.metadata);
        const stateSnapshot = get();
        const selectionChatState = stateSnapshot.sessionChatState?.[sessionId];
        if (selectionChatState) {
          set((state: ChatStoreDraft) => {
            const currentChatState = state.sessionChatState?.[sessionId];
            if (!currentChatState) {
              return;
            }
            state.sessionChatState[sessionId] = {
              ...currentChatState,
              isLoading: Boolean(currentChatState.isStreaming || currentChatState.isStopping),
            };
          });
        }

        set((state: ChatStoreDraft) => {
          applySelectSessionState({
            state,
            sessionId,
            session,
            messages,
            previousSessionId,
            sessionAiSelectionFromMetadata,
            keepActivePanel: options.keepActivePanel,
          });
          if (!state.sessionMessagePaginationState) {
            state.sessionMessagePaginationState = {};
          }
          state.sessionMessagePaginationState[sessionId] = {
            nextBefore: effectiveNextBefore,
            loaded: true,
          };
          state.hasMoreMessages = Boolean(effectiveNextBefore);
        });
        void restoreSessionRuntimeState({ client, set, get, sessionId });

        if (session) {
          const { userId, projectId } = getSessionParams();
          if (typeof localStorage !== 'undefined') {
            localStorage.setItem(`lastSessionId_${userId}_${projectId}`, sessionId);
            debugLog('🔍 保存会话ID到 localStorage:', sessionId);
          }
        }
        const latestMessagesForSession = (get().messages || []).filter((message: Message) => message?.sessionId === sessionId);
        const latestCompactMessagesForSession = extractCompactHistoryMessages(latestMessagesForSession);
        set((state) => {
          writeSessionMessagesCache(state, sessionId, {
            messages: latestCompactMessagesForSession.length > 0
              ? latestCompactMessagesForSession
              : messages,
            nextBefore: effectiveNextBefore,
            loaded: true,
          });
        });
        debugLog('[Store] selectSession completed', {
          sessionId,
          previousSessionId,
          messageCount: messages.length,
          cacheHit: Boolean(sessionSnapshot),
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
          state.error = error instanceof Error ? error.message : 'Failed to select session';
          state.isLoading = false;
        });
      }
    },
  };
}
