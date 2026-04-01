import type { Message } from '../../../types';
import type ApiClient from '../../api/client';
import {
  applyTurnProcessCache,
  fetchTurnProcessMessages,
  mergeTurnProcessMessages,
  resolveTurnProcessKeyForUserMessage,
  setTurnProcessExpanded,
} from '../helpers/messages';
import type {
  ChatStoreDraft,
  ChatStoreGet,
  ChatStoreSet,
} from '../types';
import {
  ensureSessionTurnMaps,
  readTurnProcessState,
  type TurnProcessMapValue,
  writeTurnProcessCache,
  writeTurnProcessState,
} from './messagesState';

type ToggleTurnProcessOptions = {
  forceExpand?: boolean;
  forceCollapse?: boolean;
};

const turnProcessLoadInFlight = new Map<string, Promise<void>>();

const getInlineTurnProcessMessages = (
  messages: Message[],
  userMessageId: string,
): Message[] => {
  const userMessage = messages.find((message) => (
    message?.id === userMessageId && message?.role === 'user'
  ));
  const inlineMessages = userMessage?.metadata?.historyProcessInlineMessages;
  return Array.isArray(inlineMessages) ? inlineMessages : [];
};

const getTurnIdForUserMessage = (messages: Message[], userMessageId: string): string => {
  const userMessage = messages.find((message) => (
    message?.id === userMessageId && message?.role === 'user'
  ));
  const turnId = userMessage?.metadata?.conversation_turn_id
    || userMessage?.metadata?.historyProcess?.turnId;
  return typeof turnId === 'string' ? turnId.trim() : '';
};

const hasAssistantProcessFallback = (
  messages: Message[],
  userMessageId: string,
  turnId: string,
): boolean => {
  const finalAssistantMessage = messages.find((message) => (
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
  return segments.some((segment) => (
    segment?.type === 'thinking'
    || (segment?.type === 'tool_call' && Boolean(segment?.toolCallId))
  )) || toolCalls.length > 0;
};

const hasTurnProcessInMemory = (messages: Message[], userMessageId: string): boolean => {
  if (!userMessageId) {
    return false;
  }

  const turnId = getTurnIdForUserMessage(messages, userMessageId);
  if (getInlineTurnProcessMessages(messages, userMessageId).length > 0) {
    return true;
  }

  const hasPersistedProcessMessages = messages.some((message) => (
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

const updateTurnProcessUiState = (
  state: ChatStoreDraft,
  sessionId: string,
  processKey: string,
  userMessageId: string,
  value: TurnProcessMapValue,
) => {
  ensureSessionTurnMaps(state, sessionId);
  writeTurnProcessState(
    state.sessionTurnProcessState[sessionId],
    processKey,
    userMessageId,
    value,
  );
};

interface TurnProcessDeps {
  set: ChatStoreSet;
  get: ChatStoreGet;
  client: ApiClient;
}

export function createTurnProcessActions({ set, get, client }: TurnProcessDeps) {
  return {
    toggleTurnProcess: async (
      userMessageId: string,
      options: ToggleTurnProcessOptions = {},
    ) => {
      const snapshot = get();
      const sessionId = snapshot.currentSessionId;
      if (!sessionId || !userMessageId) {
        return;
      }

      const processKey = resolveTurnProcessKeyForUserMessage(snapshot.messages, userMessageId)
        || userMessageId;
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
      const shouldRetryExpandedLoad = (
        currentState.expanded
        && !currentState.loaded
        && !currentState.loading
        && !hasProcessInMemory
        && !options.forceCollapse
        && !options.forceExpand
      );
      const nextExpanded = options.forceCollapse
        ? false
        : options.forceExpand
          ? true
          : shouldRetryExpandedLoad
            ? true
            : !currentState.expanded;
      if (nextExpanded && isLocalOnlyUserMessage && !turnId) {
        set((state) => {
          updateTurnProcessUiState(state, sessionId, processKey, userMessageId, {
            expanded: true,
            loaded: hasTurnProcessInMemory(state.messages, userMessageId),
            loading: false,
          });
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
          set((state) => {
            ensureSessionTurnMaps(state, sessionId);
            writeTurnProcessCache(
              state.sessionTurnProcessCache[sessionId],
              processKey,
              userMessageId,
              inlineProcessMessages,
            );
            updateTurnProcessUiState(state, sessionId, processKey, userMessageId, {
              expanded: true,
              loaded: true,
              loading: false,
            });

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

        const inFlightKey = `${sessionId}::${processKey}`;
        const existingLoad = turnProcessLoadInFlight.get(inFlightKey);
        if (existingLoad) {
          set((state) => {
            updateTurnProcessUiState(state, sessionId, processKey, userMessageId, {
              expanded: true,
              loaded: false,
              loading: true,
            });
            state.messages = applyTurnProcessCache(
              setTurnProcessExpanded(state.messages, userMessageId, true, { processKey }),
              state.sessionTurnProcessCache?.[sessionId],
              state.sessionTurnProcessState?.[sessionId],
            );
          });
          await existingLoad;
          return;
        }

        set((state) => {
          updateTurnProcessUiState(state, sessionId, processKey, userMessageId, {
            expanded: true,
            loaded: false,
            loading: true,
          });
          state.messages = applyTurnProcessCache(
            state.messages,
            state.sessionTurnProcessCache?.[sessionId],
            state.sessionTurnProcessState?.[sessionId],
          );
        });

        const loadPromise = (async () => {
          try {
            const processMessages = await fetchTurnProcessMessages(
              client,
              sessionId,
              userMessageId,
              { turnId },
            );
            set((state) => {
              ensureSessionTurnMaps(state, sessionId);
              const latestTurnState = readTurnProcessState(
                state.sessionTurnProcessState?.[sessionId],
                processKey,
                userMessageId,
              );
              const shouldKeepExpanded = latestTurnState?.expanded === true;
              const isStreaming = state.sessionChatState?.[sessionId]?.isStreaming === true;
              const shouldRetryLater = isStreaming && processMessages.length === 0;
              writeTurnProcessCache(
                state.sessionTurnProcessCache[sessionId],
                processKey,
                userMessageId,
                processMessages,
              );

              if (shouldRetryLater) {
                updateTurnProcessUiState(state, sessionId, processKey, userMessageId, {
                  expanded: shouldKeepExpanded,
                  loaded: false,
                  loading: false,
                });
                state.messages = applyTurnProcessCache(
                  setTurnProcessExpanded(
                    state.messages,
                    userMessageId,
                    shouldKeepExpanded,
                    { processKey },
                  ),
                  state.sessionTurnProcessCache?.[sessionId],
                  state.sessionTurnProcessState?.[sessionId],
                );
                return;
              }

              updateTurnProcessUiState(state, sessionId, processKey, userMessageId, {
                expanded: shouldKeepExpanded,
                loaded: true,
                loading: false,
              });

              state.messages = mergeTurnProcessMessages(
                state.messages,
                userMessageId,
                processMessages,
                shouldKeepExpanded,
                { processKey },
              );
            });
          } catch (error) {
            console.error('Failed to load turn process messages:', error);
            set((state) => {
              updateTurnProcessUiState(state, sessionId, processKey, userMessageId, {
                expanded: false,
                loaded: false,
                loading: false,
              });
              state.messages = applyTurnProcessCache(
                setTurnProcessExpanded(state.messages, userMessageId, false, { processKey }),
                state.sessionTurnProcessCache?.[sessionId],
                state.sessionTurnProcessState?.[sessionId],
              );
              state.error = error instanceof Error
                ? error.message
                : 'Failed to load turn process messages';
            });
          }
        })();

        turnProcessLoadInFlight.set(inFlightKey, loadPromise);
        try {
          await loadPromise;
        } finally {
          if (turnProcessLoadInFlight.get(inFlightKey) === loadPromise) {
            turnProcessLoadInFlight.delete(inFlightKey);
          }
        }

        return;
      }

      set((state) => {
        updateTurnProcessUiState(state, sessionId, processKey, userMessageId, {
          expanded: nextExpanded,
          loaded: currentState.loaded,
          loading: false,
        });

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
  };
}
