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
  touchSessionMessagesCacheEntry,
  writeSessionMessagesCache,
} from '../sessionsUtils';
import { applySelectSessionState } from '../sessionsSelectHelpers';
import { recoverStreamingTurnBySnapshot } from '../sendMessage/turnRecovery';
import { createDefaultSessionChatState } from '../sendMessage/sessionState';
import type { SessionActionDeps } from './types';

let latestSelectRequestSeq = 0;
const SESSION_MESSAGES_BACKGROUND_SYNC_MAX_AGE_MS = 30_000;
const RUNNING_SNAPSHOT_STATUSES = new Set(['running', 'in_progress', 'processing']);

const readTrimmedString = (value: unknown): string => (
  typeof value === 'string' ? value.trim() : ''
);

const readStreamingTempUserId = (message: Message | null | undefined): string | null => {
  const linkedUserId = readTrimmedString(message?.metadata?.historyFinalForUserMessageId);
  if (linkedUserId) {
    return linkedUserId;
  }
  const draftUserId = readTrimmedString(message?.metadata?.historyDraftUserMessage?.id);
  return draftUserId || null;
};

export function createSelectSessionActions({
  set,
  get,
  client,
  getSessionParams,
}: SessionActionDeps) {
  const recoverRunningSessionState = async (
    sessionId: string,
    existingMessages: Message[],
  ): Promise<boolean> => {
    if (
      typeof client.getConversationLatestTurnRuntimeContext !== 'function'
      || typeof client.getConversationTurnRuntimeContextByTurn !== 'function'
      || typeof client.getConversationTurnMessagesByTurn !== 'function'
      || typeof client.getConversationTurnMessages !== 'function'
    ) {
      return false;
    }

    const latestSnapshot = await client.getConversationLatestTurnRuntimeContext(sessionId);
    const snapshotStatus = readTrimmedString(latestSnapshot?.status).toLowerCase();
    const turnId = readTrimmedString(latestSnapshot?.turn_id);
    if (!turnId || !RUNNING_SNAPSHOT_STATUSES.has(snapshotStatus)) {
      return false;
    }

    const assistantCandidate = [...existingMessages].reverse().find((message) => (
      message?.role === 'assistant'
      && readTrimmedString(
        message?.metadata?.historyFinalForTurnId
        || message?.metadata?.conversation_turn_id,
      ) === turnId
    )) || null;
    const tempAssistantMessageId = assistantCandidate?.id || `recovered_streaming_${turnId}`;
    const tempUserId = readStreamingTempUserId(assistantCandidate);

    let recovered = false;
    await recoverStreamingTurnBySnapshot({
      apiClient: client,
      set,
      sessionId,
      turnId,
      tempAssistantMessageId,
      tempUserId,
      preferredUserMessageId: tempUserId,
    }).then((result) => {
      recovered = result.recovered;
    }).catch((error) => {
      console.error('Failed to recover running session state during selectSession:', error);
    });

    if (!recovered && assistantCandidate) {
      set((state: ChatStoreDraft) => {
        const prev = state.sessionChatState?.[sessionId] || createDefaultSessionChatState();
        state.sessionChatState[sessionId] = {
          ...prev,
          isLoading: true,
          isStreaming: true,
          isStopping: false,
          activeTurnId: turnId,
          streamingMessageId: assistantCandidate.id,
          streamingPreviewText: typeof assistantCandidate.content === 'string' ? assistantCandidate.content : '',
          streamingTransport: 'realtime',
        };
        if (!state.sessionStreamingMessageDrafts) {
          state.sessionStreamingMessageDrafts = {};
        }
        state.sessionStreamingMessageDrafts[sessionId] = {
          ...assistantCandidate,
          status: 'streaming',
          metadata: {
            ...(assistantCandidate.metadata || {}),
            conversation_turn_id: turnId,
            historyFinalForTurnId: turnId,
            ...(tempUserId ? { historyFinalForUserMessageId: tempUserId } : {}),
          },
        };
        if (state.currentSessionId === sessionId) {
          state.isLoading = true;
          state.isStreaming = true;
          state.streamingMessageId = assistantCandidate.id;
        }
      });
      return true;
    }

    return recovered;
  };

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
        const visibleSnapshot = readVisibleSessionMessagesSnapshot(get(), sessionId);
        const cachedPage = readSessionMessagesCache(get(), sessionId);
        const sessionSnapshot = visibleSnapshot ?? cachedPage;
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
              session: existingSession,
              messages: sessionSnapshot.messages,
              previousSessionId,
              localStreamingMessage,
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
          if (!stateSnapshot.sessionChatState?.[sessionId]?.isStreaming) {
            void recoverRunningSessionState(sessionId, sessionSnapshot.messages);
          }
          const shouldBackgroundSync = (() => {
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
          fetchSessionMessages(client, sessionId, { limit: 50, before: null }),
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
          if (!state.sessionMessagePaginationState) {
            state.sessionMessagePaginationState = {};
          }
          state.sessionMessagePaginationState[sessionId] = {
            nextBefore: effectiveNextBefore,
            loaded: true,
          };
          state.hasMoreMessages = Boolean(effectiveNextBefore);
        });
        if (!get().sessionChatState?.[sessionId]?.isStreaming) {
          void recoverRunningSessionState(sessionId, messages);
        }

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
