import type { Message } from '../../../types';
import type ApiClient from '../../api/client';
import {
  applyTurnProcessCache,
  fetchSessionMessages,
  fetchTurnProcessMessages,
  mergeTurnProcessMessages,
  resolveTurnProcessKeyForUserMessage,
  setTurnProcessExpanded,
} from '../helpers/messages';

const cloneStreamingMessageDraft = <T,>(value: T): T => {
  try {
    if (typeof structuredClone === 'function') {
      return structuredClone(value);
    }
  } catch {
    // ignore and fallback to JSON clone
  }

  try {
    return JSON.parse(JSON.stringify(value));
  } catch {
    return value;
  }
};

interface Deps {
  set: any;
  get: any;
  client: ApiClient;
}

type ToggleTurnProcessOptions = {
  forceExpand?: boolean;
  forceCollapse?: boolean;
};

const countLoadedBaseMessages = (messages: any[]): number => (
  (messages || []).filter((message: any) => !message?.metadata?.historyProcessUserMessageId).length
);

const getInlineTurnProcessMessages = (messages: any[], userMessageId: string): Message[] => {
  const userMessage = (messages || []).find((message: any) => (
    message?.id === userMessageId && message?.role === 'user'
  ));
  const inlineMessages = userMessage?.metadata?.historyProcessInlineMessages;
  return Array.isArray(inlineMessages) ? inlineMessages : [];
};

const getTurnIdForUserMessage = (messages: any[], userMessageId: string): string => {
  const userMessage = (messages || []).find((message: any) => (
    message?.id === userMessageId && message?.role === 'user'
  ));
  const turnId = userMessage?.metadata?.conversation_turn_id
    || userMessage?.metadata?.historyProcess?.turnId;
  return typeof turnId === 'string' ? turnId.trim() : '';
};

const hasAssistantProcessFallback = (
  messages: any[],
  userMessageId: string,
  turnId: string,
): boolean => {
  const finalAssistantMessage = (messages || []).find((message: any) => (
    message?.role === 'assistant' && (
      message?.metadata?.historyFinalForUserMessageId === userMessageId
      || (turnId && (
        message?.metadata?.historyFinalForTurnId === turnId
        || message?.metadata?.conversation_turn_id === turnId
      ))
    )
  ));
  if (!finalAssistantMessage) {
    return false;
  }

  const metadata = finalAssistantMessage.metadata || {};
  const segments = Array.isArray(metadata.contentSegments) ? metadata.contentSegments : [];
  const toolCalls = Array.isArray(metadata.toolCalls) ? metadata.toolCalls : [];
  return segments.some((segment: any) => (
    segment?.type === 'thinking'
    || (segment?.type === 'tool_call' && Boolean(segment?.toolCallId))
  )) || toolCalls.length > 0;
};

const hasTurnProcessInMemory = (messages: any[], userMessageId: string): boolean => {
  if (!userMessageId) {
    return false;
  }

  const turnId = getTurnIdForUserMessage(messages, userMessageId);
  if (getInlineTurnProcessMessages(messages, userMessageId).length > 0) {
    return true;
  }

  const hasPersistedProcessMessages = (messages || []).some((message: any) => (
    (
      message?.metadata?.historyProcessUserMessageId === userMessageId
      || (turnId && message?.metadata?.historyProcessTurnId === turnId)
    )
    && message?.metadata?.historyProcessPlaceholder !== true
  ));
  if (hasPersistedProcessMessages) {
    return true;
  }

  return hasAssistantProcessFallback(messages, userMessageId, turnId);
};

const readTurnProcessState = (
  sessionState: Record<string, { expanded: boolean; loaded: boolean; loading: boolean }> | undefined,
  processKey: string,
  userMessageId: string,
) => {
  if (!sessionState) {
    return undefined;
  }
  if (processKey && sessionState[processKey]) {
    return sessionState[processKey];
  }
  if (userMessageId && sessionState[userMessageId]) {
    return sessionState[userMessageId];
  }
  return undefined;
};

const writeTurnProcessState = (
  sessionState: Record<string, { expanded: boolean; loaded: boolean; loading: boolean }>,
  processKey: string,
  userMessageId: string,
  value: { expanded: boolean; loaded: boolean; loading: boolean },
) => {
  const key = processKey || userMessageId;
  sessionState[key] = value;
  if (userMessageId && key !== userMessageId && userMessageId in sessionState) {
    delete sessionState[userMessageId];
  }
};

const writeTurnProcessCache = (
  sessionCache: Record<string, Message[]>,
  processKey: string,
  userMessageId: string,
  value: Message[],
) => {
  const key = processKey || userMessageId;
  sessionCache[key] = value;
  if (userMessageId && key !== userMessageId && userMessageId in sessionCache) {
    delete sessionCache[userMessageId];
  }
};

const ensureSessionTurnMaps = (state: any, sessionId: string) => {
  if (!state.sessionTurnProcessState) {
    state.sessionTurnProcessState = {};
  }
  if (!state.sessionTurnProcessState[sessionId]) {
    state.sessionTurnProcessState[sessionId] = {};
  }

  if (!state.sessionTurnProcessCache) {
    state.sessionTurnProcessCache = {};
  }
  if (!state.sessionTurnProcessCache[sessionId]) {
    state.sessionTurnProcessCache[sessionId] = {};
  }
};

export function createMessageActions({ set, get, client }: Deps) {
  return {
    loadMessages: async (sessionId: string) => {
      try {
        set((state: any) => {
          state.isLoading = true;
          state.error = null;
        });

        const messages = await fetchSessionMessages(client, sessionId, { limit: 50, offset: 0 });

        set((state: any) => {
          ensureSessionTurnMaps(state, sessionId);

          const chatState = state.sessionChatState?.[sessionId];
          const draftMessage = state.sessionStreamingMessageDrafts?.[sessionId];
          let nextMessages = messages;

          if (chatState?.isStreaming && chatState.streamingMessageId) {
            const hasStreamingMessage = nextMessages.some((m: any) => m.id === chatState.streamingMessageId);
            if (!hasStreamingMessage && draftMessage && typeof draftMessage === 'object') {
              nextMessages = [...nextMessages, cloneStreamingMessageDraft(draftMessage)];
            }
          }

          nextMessages = applyTurnProcessCache(
            nextMessages,
            state.sessionTurnProcessCache?.[sessionId],
            state.sessionTurnProcessState?.[sessionId],
          );

          state.messages = nextMessages;
          state.isLoading = false;
          state.hasMoreMessages = messages.length >= 50;
        });
      } catch (error) {
        console.error('Failed to load messages:', error);
        set((state: any) => {
          state.error = error instanceof Error ? error.message : 'Failed to load messages';
          state.isLoading = false;
        });
      }
    },

    loadMoreMessages: async (sessionId: string) => {
      try {
        const current = get();
        const offset = countLoadedBaseMessages(current.messages);
        const page = await fetchSessionMessages(client, sessionId, { limit: 50, offset });
        set((state: any) => {
          ensureSessionTurnMaps(state, sessionId);

          const existingIds = new Set(state.messages.map((m: any) => m.id));
          const older = page.filter((m: any) => !existingIds.has(m.id));
          const merged = [...older, ...state.messages];

          state.messages = applyTurnProcessCache(
            merged,
            state.sessionTurnProcessCache?.[sessionId],
            state.sessionTurnProcessState?.[sessionId],
          );
          state.hasMoreMessages = page.length >= 50;
        });
      } catch (error) {
        console.error('Failed to load more messages:', error);
        set((state: any) => {
          state.error = error instanceof Error ? error.message : 'Failed to load more messages';
        });
      }
    },

    toggleTurnProcess: async (
      userMessageId: string,
      options: ToggleTurnProcessOptions = {},
    ) => {
      const snapshot = get();
      const sessionId = snapshot.currentSessionId;
      if (!sessionId || !userMessageId) {
        return;
      }

      const processKey = resolveTurnProcessKeyForUserMessage(snapshot.messages, userMessageId) || userMessageId;
      const currentState = readTurnProcessState(
        snapshot.sessionTurnProcessState?.[sessionId],
        processKey,
        userMessageId,
      ) || {
        expanded: false,
        loaded: false,
        loading: false,
      };
      const turnId = getTurnIdForUserMessage(snapshot.messages, userMessageId);
      const hasProcessInMemory = hasTurnProcessInMemory(snapshot.messages, userMessageId);
      const isLocalOnlyUserMessage = userMessageId.startsWith('temp_user_');
      const nextExpanded = options.forceCollapse
        ? false
        : options.forceExpand
          ? true
          : !currentState.expanded;
      if (nextExpanded && isLocalOnlyUserMessage) {
        set((state: any) => {
          ensureSessionTurnMaps(state, sessionId);
          writeTurnProcessState(
            state.sessionTurnProcessState[sessionId],
            processKey,
            userMessageId,
            {
              expanded: true,
              loaded: hasTurnProcessInMemory(state.messages, userMessageId),
              loading: false,
            },
          );
          state.messages = applyTurnProcessCache(
            setTurnProcessExpanded(state.messages, userMessageId, true, { processKey }),
            state.sessionTurnProcessCache?.[sessionId],
            state.sessionTurnProcessState?.[sessionId],
          );
        });
        return;
      }
      const shouldLoadProcess = nextExpanded
        && !currentState.loading
        && (!currentState.loaded || !hasProcessInMemory);

      if (shouldLoadProcess) {
        const inlineProcessMessages = getInlineTurnProcessMessages(snapshot.messages, userMessageId);
        if (inlineProcessMessages.length > 0) {
          set((state: any) => {
            ensureSessionTurnMaps(state, sessionId);
            writeTurnProcessCache(
              state.sessionTurnProcessCache[sessionId],
              processKey,
              userMessageId,
              inlineProcessMessages,
            );
            writeTurnProcessState(
              state.sessionTurnProcessState[sessionId],
              processKey,
              userMessageId,
              {
                expanded: true,
                loaded: true,
                loading: false,
              },
            );

            state.messages = mergeTurnProcessMessages(
              state.messages,
              userMessageId,
              inlineProcessMessages,
              true,
              { processKey },
            );
          });
          return;
        }

        set((state: any) => {
          ensureSessionTurnMaps(state, sessionId);
          writeTurnProcessState(
            state.sessionTurnProcessState[sessionId],
            processKey,
            userMessageId,
            {
              expanded: true,
              loaded: false,
              loading: true,
            },
          );
          state.messages = applyTurnProcessCache(
            state.messages,
            state.sessionTurnProcessCache?.[sessionId],
            state.sessionTurnProcessState?.[sessionId],
          );
        });

        try {
          const processMessages = await fetchTurnProcessMessages(
            client,
            sessionId,
            userMessageId,
            { turnId },
          );
          set((state: any) => {
            ensureSessionTurnMaps(state, sessionId);
            const isStreaming = state.sessionChatState?.[sessionId]?.isStreaming === true;
            const shouldRetryLater = isStreaming && processMessages.length === 0;
            writeTurnProcessCache(
              state.sessionTurnProcessCache[sessionId],
              processKey,
              userMessageId,
              processMessages,
            );

            if (shouldRetryLater) {
              writeTurnProcessState(
                state.sessionTurnProcessState[sessionId],
                processKey,
                userMessageId,
                {
                  expanded: true,
                  loaded: false,
                  loading: false,
                },
              );
              state.messages = applyTurnProcessCache(
                setTurnProcessExpanded(state.messages, userMessageId, true, { processKey }),
                state.sessionTurnProcessCache?.[sessionId],
                state.sessionTurnProcessState?.[sessionId],
              );
              return;
            }

            writeTurnProcessState(
              state.sessionTurnProcessState[sessionId],
              processKey,
              userMessageId,
              {
                expanded: true,
                loaded: true,
                loading: false,
              },
            );

            state.messages = mergeTurnProcessMessages(
              state.messages,
              userMessageId,
              processMessages,
              true,
              { processKey },
            );
          });
        } catch (error) {
          console.error('Failed to load turn process messages:', error);
          set((state: any) => {
            ensureSessionTurnMaps(state, sessionId);
            writeTurnProcessState(
              state.sessionTurnProcessState[sessionId],
              processKey,
              userMessageId,
              {
                expanded: false,
                loaded: false,
                loading: false,
              },
            );
            state.messages = applyTurnProcessCache(
              setTurnProcessExpanded(state.messages, userMessageId, false, { processKey }),
              state.sessionTurnProcessCache?.[sessionId],
              state.sessionTurnProcessState?.[sessionId],
            );
            state.error = error instanceof Error ? error.message : 'Failed to load turn process messages';
          });
        }

        return;
      }

      set((state: any) => {
        ensureSessionTurnMaps(state, sessionId);
        writeTurnProcessState(
          state.sessionTurnProcessState[sessionId],
          processKey,
          userMessageId,
          {
            expanded: nextExpanded,
            loaded: currentState.loaded,
            loading: false,
          },
        );

        const toggled = setTurnProcessExpanded(
          state.messages,
          userMessageId,
          nextExpanded,
          { processKey },
        );
        state.messages = applyTurnProcessCache(
          toggled,
          state.sessionTurnProcessCache?.[sessionId],
          state.sessionTurnProcessState?.[sessionId],
        );
      });
    },

    updateMessage: async (messageId: string, _updates: Partial<Message>) => {
      try {
        console.warn('updateMessage not implemented yet');
        const updatedMessage = null;

        set((state: any) => {
          const index = state.messages.findIndex((m: any) => m.id === messageId);
          if (index !== -1 && updatedMessage) {
            state.messages[index] = updatedMessage;
          }
        });
      } catch (error) {
        console.error('Failed to update message:', error);
        set((state: any) => {
          state.error = error instanceof Error ? error.message : 'Failed to update message';
        });
      }
    },

    deleteMessage: async (messageId: string) => {
      try {
        console.warn('deleteMessage not implemented yet');

        set((state: any) => {
          state.messages = state.messages.filter((m: any) => m.id !== messageId);
        });
      } catch (error) {
        console.error('Failed to delete message:', error);
        set((state: any) => {
          state.error = error instanceof Error ? error.message : 'Failed to delete message';
        });
      }
    },
  };
}
